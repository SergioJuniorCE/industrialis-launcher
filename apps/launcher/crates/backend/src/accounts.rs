//! Port of `apps/launcher/electron/backend/auth.ts` (accounts + Microsoft login).

use base64::Engine as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

use crate::fs_utils::write_json;
use crate::http::{
    format_http_result, http_client, is_transient_http_status, request_json, retry_with_backoff,
    DEFAULT_REQUEST_TIMEOUT,
};
use crate::paths::LauncherPaths;

// ── Stored shapes (field names match the TS `types.ts` on-disk JSON) ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MsaToken {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredToken {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub token: String,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSkin {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinecraftProfile {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skins: Option<Vec<ProfileSkin>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinecraftEntitlement {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub owns_minecraft: bool,
    pub can_play_minecraft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub format_version: u32,
    pub account_type: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msa_token: Option<MsaToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_token: Option<StoredToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mojangservices_token: Option<StoredToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yggdrasil_token: Option<StoredToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_profile: Option<MinecraftProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_entitlement: Option<MinecraftEntitlement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_png_base64: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub account_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_png_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owns_minecraft: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_play_minecraft: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
}

// ── Constants ──

/// Prism Launcher's public Microsoft application ID (same as the TS backend).
const MSA_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";
const MSA_SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";
const MSA_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MSA_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const REFRESH_WINDOW_SECONDS: i64 = 12 * 60 * 60;

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Account store ──

fn parse_account(value: &serde_json::Value) -> Option<AccountData> {
    if value.get("format_version").is_some() {
        let account: AccountData = serde_json::from_value(value.clone()).ok()?;
        return (!account.id.is_empty()
            && account.format_version == 3
            && matches!(account.account_type.as_str(), "msa" | "offline"))
        .then_some(account);
    }
    let object = value.as_object()?;
    let id = object.get("id")?.as_str()?.to_string();
    if id.is_empty() {
        return None;
    }
    let mut extra_fields = object.clone();
    for key in [
        "id",
        "username",
        "uuid",
        "access_token",
        "refresh_token",
        "expires_at",
    ] {
        extra_fields.remove(key);
    }
    Some(AccountData {
        extra_fields,
        format_version: 3,
        account_type: "msa".into(),
        id,
        msa_token: Some(MsaToken {
            extra_fields: Default::default(),
            access_token: String::new(),
            refresh_token: object.get("refresh_token")?.as_str()?.into(),
            expires_at: object.get("expires_at")?.as_i64()?,
        }),
        yggdrasil_token: Some(StoredToken {
            extra_fields: Default::default(),
            token: object.get("access_token")?.as_str()?.into(),
            expires_at: 0,
            extra: None,
        }),
        minecraft_profile: Some(MinecraftProfile {
            extra_fields: Default::default(),
            id: object.get("uuid")?.as_str()?.into(),
            name: object.get("username")?.as_str()?.into(),
            skins: None,
        }),
        user_token: None,
        mojangservices_token: None,
        minecraft_entitlement: None,
        skin_png_base64: None,
    })
}

async fn account_rows(paths: &LauncherPaths) -> anyhow::Result<Vec<serde_json::Value>> {
    match crate::profile_store::read_document(&paths.accounts_path()).await? {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(rows)) => Ok(rows),
        Some(_) => anyhow::bail!(
            "Expected an array in {}. The original file has been kept.",
            paths.accounts_path().display()
        ),
    }
}

/// Isolate malformed/unsupported rows; valid current and legacy accounts still
/// load. Mutations retain every unrecognized row verbatim for later recovery.
pub async fn load_accounts(paths: &LauncherPaths) -> Result<Vec<AccountData>, anyhow::Error> {
    Ok(load_accounts_with_issues(paths).await?.0)
}

pub async fn load_accounts_with_issues(
    paths: &LauncherPaths,
) -> anyhow::Result<(Vec<AccountData>, usize)> {
    let rows = account_rows(paths).await?;
    let accounts: Vec<_> = rows.iter().filter_map(parse_account).collect();
    let skipped = rows.len() - accounts.len();
    Ok((accounts, skipped))
}

async fn save_accounts_unlocked(
    paths: &LauncherPaths,
    accounts: &[AccountData],
) -> anyhow::Result<()> {
    let original = account_rows(paths).await?;
    let mut rows = Vec::new();
    let mut retained = std::collections::HashSet::new();
    for row in &original {
        match parse_account(row) {
            None => rows.push(row.clone()),
            Some(previous) => {
                if let Some(account) = accounts.iter().find(|account| account.id == previous.id) {
                    retained.insert(account.id.as_str());
                    // Preserve legacy shapes and optional nulls for untouched accounts.
                    rows.push(if account == &previous {
                        row.clone()
                    } else {
                        serde_json::to_value(account)?
                    });
                }
            }
        }
    }
    for account in accounts {
        if retained.insert(account.id.as_str()) {
            rows.push(serde_json::to_value(account)?);
        }
    }
    if rows == original {
        return Ok(());
    }
    write_json(&paths.accounts_path(), &serde_json::Value::Array(rows)).await
}

pub async fn save_accounts(
    paths: &LauncherPaths,
    accounts: &[AccountData],
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let accounts = accounts.to_vec();
    crate::profile_store::mutate(async move { save_accounts_unlocked(&paths, &accounts).await })
        .await
}

/// Persist the launch default, retiring the legacy fallback at the same time.
pub async fn set_default_account(
    paths: &LauncherPaths,
    id: Option<&str>,
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let id = id.map(str::to_string);
    crate::profile_store::mutate(async move {
        if let Some(id) = id.as_deref() {
            if !load_accounts(&paths)
                .await?
                .iter()
                .any(|account| account.id == id)
            {
                anyhow::bail!("The selected account was not found.");
            }
        }
        let mut settings = crate::settings::load_launcher_settings(&paths).await?;
        settings.default_account_id = id;
        settings.active_account_id = None;
        crate::settings::save_launcher_settings_unlocked(&paths, &settings).await
    })
    .await
}

/// Mirrors Electron's remove-account action and clears a removed default.
pub async fn remove_account(paths: &LauncherPaths, id: &str) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let mut accounts = load_accounts(&paths).await?;
        // Read preferences before changing either file; corrupt preferences must
        // not cause a partially completed removal.
        let mut settings = crate::settings::load_launcher_settings(&paths).await?;
        accounts.retain(|account| account.id != id);
        let default_removed = settings.default_account_id.as_deref() == Some(id.as_str());
        let legacy_removed = settings.active_account_id.as_deref() == Some(id.as_str());
        if default_removed || legacy_removed {
            if default_removed {
                settings.default_account_id = None;
                // A removed explicit default must not resurrect the legacy one.
                settings.active_account_id = None;
            } else {
                settings.active_account_id = None;
            }
            crate::settings::save_launcher_settings_unlocked(&paths, &settings).await?;
        }
        // Clear the default before deleting credentials. A failure in the
        // second file leaves an existing account with no default, never a
        // dangling default pointing at an already removed account.
        save_accounts_unlocked(&paths, &accounts).await?;
        Ok(())
    })
    .await
}

fn profile_id(account: &AccountData) -> &str {
    account
        .minecraft_profile
        .as_ref()
        .map(|p| p.id.as_str())
        .unwrap_or("")
}

fn profile_name(account: &AccountData) -> &str {
    account
        .minecraft_profile
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("")
}

fn access_token(account: &AccountData) -> &str {
    account
        .yggdrasil_token
        .as_ref()
        .map(|t| t.token.as_str())
        .unwrap_or("")
}

/// Profile identity used for launch arguments. Mirrors the TS `launchAuth`
/// profile reads (offline and MSA alike).
pub fn profile_identity(account: &AccountData) -> (String, String) {
    (
        profile_name(account).to_string(),
        profile_id(account).to_string(),
    )
}

/// Mirrors the TS `chooseAccount`: explicit instance override, then the
/// launcher default, then the lone account, else an error.
pub fn choose_launch_account(
    settings: &industrialis_core::instance_settings::InstanceSettings,
    accounts: &[AccountData],
    default_account_id: Option<&str>,
) -> Result<AccountData, anyhow::Error> {
    if accounts.is_empty() {
        anyhow::bail!("Add an account in Accounts before launching.");
    }
    if settings.override_account {
        if let Some(wanted) = settings.account_id.as_deref() {
            return accounts
                .iter()
                .find(|entry| entry.id == wanted)
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("The account selected in instance settings was not found.")
                });
        }
    }
    if let Some(default) = default_account_id {
        if let Some(account) = accounts.iter().find(|entry| entry.id == default) {
            return Ok(account.clone());
        }
    }
    if accounts.len() == 1 {
        return Ok(accounts[0].clone());
    }
    anyhow::bail!("Set a default account in Accounts before launching.")
}

pub fn account_to_info(account: &AccountData) -> AccountInfo {
    AccountInfo {
        id: account.id.clone(),
        username: profile_name(account).to_string(),
        uuid: profile_id(account).to_string(),
        account_type: account.account_type.clone(),
        skin_png_base64: account.skin_png_base64.clone(),
        owns_minecraft: account
            .minecraft_entitlement
            .as_ref()
            .map(|e| e.owns_minecraft),
        can_play_minecraft: account
            .minecraft_entitlement
            .as_ref()
            .map(|e| e.can_play_minecraft),
    }
}

fn upsert_account_into(accounts: &mut Vec<AccountData>, mut account: AccountData) {
    let profile = profile_id(&account).to_string();
    if let Some(old) = accounts
        .iter()
        .find(|a| a.id == account.id || (!profile.is_empty() && profile_id(a) == profile))
    {
        let mut extras = old.extra_fields.clone();
        extras.extend(account.extra_fields);
        account.extra_fields = extras;
        macro_rules! keep_extensions {
            ($($field:ident),*) => { $(
                if let (Some(old), Some(new)) = (&old.$field, &mut account.$field) {
                    let mut extras = old.extra_fields.clone();
                    extras.extend(std::mem::take(&mut new.extra_fields));
                    new.extra_fields = extras;
                }
            )* };
        }
        keep_extensions!(
            msa_token,
            user_token,
            mojangservices_token,
            yggdrasil_token,
            minecraft_profile,
            minecraft_entitlement
        );
        if let (Some(old_profile), Some(new_profile)) =
            (&old.minecraft_profile, &mut account.minecraft_profile)
        {
            if let (Some(old_skins), Some(new_skins)) = (&old_profile.skins, &mut new_profile.skins)
            {
                for skin in new_skins {
                    if let Some(old) = old_skins.iter().find(|old| old.id == skin.id) {
                        let mut extras = old.extra_fields.clone();
                        extras.extend(std::mem::take(&mut skin.extra_fields));
                        skin.extra_fields = extras;
                    }
                }
            }
        }
    }
    accounts.retain(|entry| {
        entry.id != account.id && (profile.is_empty() || profile_id(entry) != profile)
    });
    accounts.push(account);
}

pub async fn upsert_account(
    paths: &LauncherPaths,
    account: AccountData,
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let mut accounts = load_accounts(&paths).await?;
        upsert_account_into(&mut accounts, account);
        save_accounts_unlocked(&paths, &accounts).await
    })
    .await
}

async fn persist_refreshed_account(
    paths: &LauncherPaths,
    account: AccountData,
) -> anyhow::Result<()> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let mut accounts = load_accounts(&paths).await?;
        if !accounts.iter().any(|old| old.id == account.id) {
            anyhow::bail!("The account was removed while signing in. Add it again in Accounts.");
        }
        upsert_account_into(&mut accounts, account);
        save_accounts_unlocked(&paths, &accounts).await
    })
    .await
}

// ── Offline accounts ──

fn username_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^[A-Za-z0-9_]+$").unwrap())
}

pub fn validate_offline_username(value: &str) -> Result<String, anyhow::Error> {
    let username = value.trim();
    if username.is_empty() {
        anyhow::bail!("username cannot be empty");
    }
    if username.len() > 16 {
        anyhow::bail!("username must be 16 characters or fewer");
    }
    if !username_pattern().is_match(username) {
        anyhow::bail!("username may only contain letters, numbers, and underscores");
    }
    Ok(username.to_string())
}

/// Offline-mode player UUID (MD5 of `OfflinePlayer:{name}` with v3 bits).
pub fn offline_player_uuid(username: &str) -> String {
    let mut digest = md5::compute(format!("OfflinePlayer:{username}")).0;
    digest[6] = (digest[6] & 0x0f) | 0x30;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

pub async fn create_offline_account(
    paths: &LauncherPaths,
    username_raw: &str,
) -> Result<AccountInfo, anyhow::Error> {
    let username = validate_offline_username(username_raw)?;
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let mut accounts = load_accounts(&paths).await?;
        if accounts
            .iter()
            .any(|a| a.account_type == "offline" && profile_name(a).eq_ignore_ascii_case(&username))
        {
            anyhow::bail!("an offline account with this username already exists");
        }
        let account = AccountData {
            extra_fields: Default::default(),
            format_version: 3,
            account_type: "offline".to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            minecraft_profile: Some(MinecraftProfile {
                extra_fields: Default::default(),
                id: offline_player_uuid(&username),
                name: username,
                skins: None,
            }),
            msa_token: None,
            user_token: None,
            mojangservices_token: None,
            yggdrasil_token: None,
            minecraft_entitlement: None,
            skin_png_base64: None,
        };
        let info = account_to_info(&account);
        upsert_account_into(&mut accounts, account);
        save_accounts_unlocked(&paths, &accounts).await?;
        Ok(info)
    })
    .await
}

// ── Microsoft device-code flow ──

#[derive(Debug, Clone, Deserialize)]
struct DeviceCodeResponse {
    device_code: Option<String>,
    user_code: Option<serde_json::Value>,
    verification_uri: Option<String>,
    verification_uri_complete: Option<String>,
    message: Option<String>,
    expires_in: Option<serde_json::Value>,
    interval: Option<serde_json::Value>,
    error: Option<String>,
    error_description: Option<String>,
}

fn number_field(value: Option<&serde_json::Value>, fallback: i64) -> i64 {
    match value {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(fallback),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(fallback),
        _ => fallback,
    }
}

fn string_field(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

async fn resilient_request(
    build: impl Fn() -> reqwest::RequestBuilder,
) -> Result<crate::http::JsonResult, anyhow::Error> {
    retry_with_backoff(
        || async { request_json(build()).await },
        3,
        Duration::from_millis(500),
    )
    .await
}

async fn request_device_code(
    client: &reqwest::Client,
) -> Result<DeviceCodeResponse, anyhow::Error> {
    let result = resilient_request(|| {
        client
            .post(MSA_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&[("client_id", MSA_CLIENT_ID), ("scope", MSA_SCOPES)])
    })
    .await?;
    let body: DeviceCodeResponse =
        serde_json::from_value(result.body.clone()).unwrap_or(DeviceCodeResponse {
            device_code: None,
            user_code: None,
            verification_uri: None,
            verification_uri_complete: None,
            message: None,
            expires_in: None,
            interval: None,
            error: Some("bad-response".to_string()),
            error_description: None,
        });
    if let Some(error) = body.error.as_deref() {
        anyhow::bail!(
            "device code ({}): {}",
            error,
            body.error_description.as_deref().unwrap_or("")
        );
    }
    if !(200..300).contains(&result.status) || body.device_code.is_none() {
        anyhow::bail!(
            "device code request failed: {}",
            format_http_result(&result)
        );
    }
    Ok(body)
}

fn device_code_info(device: &DeviceCodeResponse) -> DeviceCodeInfo {
    DeviceCodeInfo {
        user_code: string_field(device.user_code.as_ref()).unwrap_or_default(),
        verification_uri: device
            .verification_uri
            .clone()
            .or(device.verification_uri_complete.clone())
            .unwrap_or_else(|| "https://microsoft.com/devicelogin".to_string()),
        message: device
            .message
            .clone()
            .unwrap_or_else(|| "Complete sign-in in your browser.".to_string()),
    }
}

async fn poll_device_code(
    client: &reqwest::Client,
    device: &DeviceCodeResponse,
) -> Result<MsaToken, anyhow::Error> {
    let device_code = device.device_code.clone().unwrap_or_default();
    let deadline = std::time::Instant::now()
        + Duration::from_secs(number_field(device.expires_in.as_ref(), 900).max(1) as u64);
    let mut interval_secs = number_field(device.interval.as_ref(), 5).max(5) as u64;
    let mut network_failures = 0u32;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        let result = match request_json(client.post(MSA_TOKEN_URL).form(&[
            ("client_id", MSA_CLIENT_ID),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code.as_str()),
        ]))
        .await
        {
            Ok(r) => r,
            Err(error) => {
                network_failures += 1;
                if network_failures >= 3 {
                    return Err(error);
                }
                continue;
            }
        };
        network_failures = 0;
        let body = &result.body;
        if let Some(access) = body.get("access_token").and_then(|v| v.as_str()) {
            return Ok(MsaToken {
                extra_fields: Default::default(),
                access_token: access.to_string(),
                refresh_token: body
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                expires_at: now_epoch() + number_field(body.get("expires_in"), 3600),
            });
        }
        match body.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval_secs += 5;
                continue;
            }
            Some("expired_token") => anyhow::bail!("device code expired — try again"),
            Some("access_denied") => anyhow::bail!("login denied"),
            other => anyhow::bail!(
                "device code poll failed (HTTP {}, {}): {}",
                result.status,
                other.unwrap_or("unknown"),
                body.get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            ),
        }
    }
    anyhow::bail!("device code login timed out")
}

#[derive(Debug, Clone, Deserialize)]
struct XboxAuthBody {
    #[serde(rename = "Token")]
    token: Option<String>,
    #[serde(rename = "NotAfter")]
    not_after: Option<String>,
    #[serde(rename = "DisplayClaims")]
    display_claims: Option<serde_json::Value>,
    #[serde(rename = "XErr")]
    x_err: Option<serde_json::Value>,
}

fn xbl_uhs(body: &XboxAuthBody) -> Result<String, anyhow::Error> {
    body.display_claims
        .as_ref()
        .and_then(|c| c.get("xui"))
        .and_then(|x| x.get(0))
        .and_then(|x| x.get("uhs"))
        .and_then(|u| u.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no uhs in Xbox user token"))
}

fn parse_xbox_expiry(value: Option<&str>) -> i64 {
    // ISO-8601 `NotAfter`; fall back to 0 like the TS `Date.parse` path.
    let Some(raw) = value else { return 0 };
    chrono_parse_epoch(raw).unwrap_or(0)
}

/// Minimal ISO-8601 → epoch parser (avoids a chrono dependency).
fn chrono_parse_epoch(raw: &str) -> Option<i64> {
    let (date, time) = raw.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let time = time.trim_end_matches('Z');
    let time = time.split(['+', '-']).next().unwrap_or(time);
    let mut t = time.split(':');
    let (hh, mm, ss): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()
            .and_then(|s| s.split('.').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let days = days_from_civil(y, m as u32, day);
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

fn days_from_civil(year: i64, month: u32, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = ((month as i64) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

async fn xbox_user_auth(
    client: &reqwest::Client,
    msa_access: &str,
) -> Result<XboxAuthBody, anyhow::Error> {
    let result = resilient_request(|| {
        client.post("https://user.auth.xboxlive.com/user/authenticate").json(&serde_json::json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={msa_access}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
    })
    .await?;
    let body: XboxAuthBody = serde_json::from_value(result.body.clone())
        .map_err(|_| anyhow::anyhow!("Xbox user auth failed: {}", format_http_result(&result)))?;
    if body.token.is_none() {
        anyhow::bail!("Xbox user auth failed: {}", format_http_result(&result));
    }
    Ok(body)
}

/// Known XSTS failure codes mapped to user-facing messages (pure for tests).
fn xsts_error_message(x_err: Option<i64>) -> Option<String> {
    match x_err {
        Some(2148916233) => Some(
            "This Microsoft account has no Xbox profile. Create one at https://www.xbox.com first."
                .to_string(),
        ),
        Some(2148916238) => {
            Some("This account is a child account and must be added to a family.".to_string())
        }
        Some(2148916235) => Some("Xbox Live is unavailable in your region.".to_string()),
        Some(2148916236) | Some(2148916237) => {
            Some("This account needs adult verification on Xbox Live.".to_string())
        }
        _ => None,
    }
}

async fn xsts_auth(
    client: &reqwest::Client,
    user_token: &StoredToken,
) -> Result<XboxAuthBody, anyhow::Error> {
    let result = resilient_request(|| {
        client
            .post("https://xsts.auth.xboxlive.com/xsts/authorize")
            .header("x-xbl-contract-version", "1")
            .json(&serde_json::json!({
                "Properties": { "SandboxId": "RETAIL", "UserTokens": [user_token.token] },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT",
            }))
    })
    .await?;
    let body: XboxAuthBody = serde_json::from_value(result.body.clone()).unwrap_or(XboxAuthBody {
        token: None,
        not_after: None,
        display_claims: None,
        x_err: None,
    });
    if (200..300).contains(&result.status) && body.token.is_some() {
        return Ok(body);
    }
    let code = body.x_err.as_ref().and_then(|v| v.as_i64());
    if let Some(message) = xsts_error_message(code) {
        anyhow::bail!("{message}");
    }
    let suffix = match code {
        Some(c) => format!(" (XErr {c})"),
        None => String::new(),
    };
    anyhow::bail!(
        "Xbox authorization failed{suffix}: {}",
        format_http_result(&result)
    )
}

#[derive(Debug, Clone, Deserialize)]
struct MinecraftLoginBody {
    access_token: Option<String>,
    expires_in: Option<serde_json::Value>,
    error: Option<String>,
}

async fn minecraft_login(
    client: &reqwest::Client,
    uhs: &str,
    xsts_token: &str,
) -> Result<MinecraftLoginBody, anyhow::Error> {
    let identity = format!("XBL3.0 x={uhs};{xsts_token}");
    let primary = resilient_request(|| {
        client
            .post("https://api.minecraftservices.com/launcher/login")
            .json(&serde_json::json!({ "xtoken": identity, "platform": "PC_LAUNCHER" }))
    })
    .await?;
    let primary_body: MinecraftLoginBody =
        serde_json::from_value(primary.body.clone()).unwrap_or(MinecraftLoginBody {
            access_token: None,
            expires_in: None,
            error: None,
        });
    if primary_body.access_token.is_some() {
        return Ok(primary_body);
    }
    let fallback_warranted = is_transient_http_status(primary.status)
        || primary_body.error.as_deref() == Some("FORBIDDEN");
    if !fallback_warranted {
        anyhow::bail!(
            "Minecraft launcher login failed: {}",
            format_http_result(&primary)
        );
    }
    let fallback = resilient_request(|| {
        client
            .post("https://api.minecraftservices.com/authentication/login_with_xbox")
            .json(&serde_json::json!({ "identityToken": identity }))
    })
    .await?;
    let fallback_body: MinecraftLoginBody = serde_json::from_value(fallback.body.clone())
        .unwrap_or(MinecraftLoginBody {
            access_token: None,
            expires_in: None,
            error: None,
        });
    if fallback_body.access_token.is_some() {
        return Ok(fallback_body);
    }
    if primary.status == 429 || fallback.status == 429 {
        anyhow::bail!(
            "Minecraft services are rate-limiting logins (HTTP 429). Wait a minute and try again. [launcher/login {}; login_with_xbox {}]",
            format_http_result(&primary),
            format_http_result(&fallback)
        );
    }
    if primary_body.error.as_deref() == Some("FORBIDDEN") {
        anyhow::bail!(
            "Minecraft API rejected this Azure application: {}",
            format_http_result(&primary)
        );
    }
    anyhow::bail!(
        "Minecraft launcher login failed: launcher/login {}; login_with_xbox {}",
        format_http_result(&primary),
        format_http_result(&fallback)
    )
}

fn entitlements_from_items(items: Option<&Vec<serde_json::Value>>) -> MinecraftEntitlement {
    let mut owns = false;
    let mut can_play = false;
    for item in items.map(Vec::as_slice).unwrap_or(&[]) {
        match item.get("name").and_then(|n| n.as_str()) {
            Some("product_minecraft") | Some("game_minecraft") => {
                owns = true;
                can_play = true;
            }
            Some("product_game_pass_pc") => can_play = true,
            _ => {}
        }
    }
    MinecraftEntitlement {
        extra_fields: Default::default(),
        owns_minecraft: owns,
        can_play_minecraft: can_play,
    }
}

async fn check_entitlements(
    client: &reqwest::Client,
    token: &str,
) -> Result<MinecraftEntitlement, anyhow::Error> {
    let result = resilient_request(|| {
        client
            .get("https://api.minecraftservices.com/entitlements/license?requestId=00000000-0000-0000-0000-000000000000")
            .bearer_auth(token)
    })
    .await?;
    if !(200..300).contains(&result.status) {
        anyhow::bail!("entitlements check failed: {}", format_http_result(&result));
    }
    Ok(entitlements_from_items(
        result.body.get("items").and_then(|v| v.as_array()),
    ))
}

async fn fetch_profile(
    client: &reqwest::Client,
    token: &str,
) -> Result<Option<MinecraftProfile>, anyhow::Error> {
    let result = resilient_request(|| {
        client
            .get("https://api.minecraftservices.com/minecraft/profile")
            .bearer_auth(token)
    })
    .await?;
    if result.status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&result.status) {
        anyhow::bail!("profile fetch failed: {}", format_http_result(&result));
    }
    Ok(serde_json::from_value(result.body.clone()).ok())
}

async fn download_skin(
    client: &reqwest::Client,
    profile: Option<&MinecraftProfile>,
) -> Option<String> {
    let skins = profile?.skins.as_ref()?;
    let skin = skins
        .iter()
        .find(|s| s.state == "ACTIVE")
        .or(skins.first())?;
    if skin.url.is_empty() {
        return None;
    }
    // Best effort: a skin failure must never fail the login itself.
    let bytes = client
        .get(&skin.url)
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

async fn run_pipeline(
    client: &reqwest::Client,
    msa: MsaToken,
) -> Result<AccountData, anyhow::Error> {
    let mut account = AccountData {
        extra_fields: Default::default(),
        format_version: 3,
        account_type: "msa".to_string(),
        id: uuid::Uuid::new_v4().to_string(),
        msa_token: Some(msa.clone()),
        user_token: None,
        mojangservices_token: None,
        yggdrasil_token: None,
        minecraft_profile: None,
        minecraft_entitlement: None,
        skin_png_base64: None,
    };
    let xbl = xbox_user_auth(client, &msa.access_token).await?;
    let user_token = StoredToken {
        extra_fields: Default::default(),
        token: xbl
            .token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Xbox user auth returned no token"))?,
        expires_at: parse_xbox_expiry(xbl.not_after.as_deref()),
        extra: Some(serde_json::json!({ "uhs": xbl_uhs(&xbl)? })),
    };
    account.user_token = Some(user_token.clone());
    let xsts = xsts_auth(client, &user_token).await?;
    let mojang_token = StoredToken {
        extra_fields: Default::default(),
        token: xsts
            .token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Xbox authorization returned no token"))?,
        expires_at: parse_xbox_expiry(xsts.not_after.as_deref()),
        extra: Some(serde_json::json!({ "uhs": xbl_uhs(&xsts)? })),
    };
    account.mojangservices_token = Some(mojang_token.clone());
    let uhs = mojang_token
        .extra
        .as_ref()
        .and_then(|e| e.get("uhs"))
        .and_then(|u| u.as_str())
        .unwrap_or_default()
        .to_string();
    let minecraft = minecraft_login(client, &uhs, &mojang_token.token).await?;
    let minecraft_token = minecraft
        .access_token
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Minecraft login returned no access token"))?;
    account.yggdrasil_token = Some(StoredToken {
        extra_fields: Default::default(),
        token: minecraft_token,
        expires_at: now_epoch() + number_field(minecraft.expires_in.as_ref(), 86400),
        extra: None,
    });
    account.minecraft_entitlement = Some(check_entitlements(client, access_token(&account)).await?);
    account.minecraft_profile = fetch_profile(client, access_token(&account)).await?;
    account.skin_png_base64 = download_skin(client, account.minecraft_profile.as_ref()).await;
    Ok(account)
}

/// Mirrors `startMicrosoftLogin`. The callback receives the device-code payload
/// (replaces the `auth-device-code` Electron event); the browser is opened
/// with the OS default handler.
pub async fn start_microsoft_login(
    paths: &LauncherPaths,
    on_device_code: impl Fn(DeviceCodeInfo),
) -> Result<AccountInfo, anyhow::Error> {
    let client = http_client();
    let device = request_device_code(&client).await?;
    let info = device_code_info(&device);
    on_device_code(info.clone());
    let _ = open::that(&info.verification_uri);
    let account = run_pipeline(&client, poll_device_code(&client, &device).await?).await?;
    let result = account_to_info(&account);
    upsert_account(paths, account).await?;
    Ok(result)
}

/// Mirrors `ensureFreshToken`: returns a usable game access token, refreshing
/// the MSA chain when inside the 12h window.
pub async fn ensure_fresh_token(
    paths: &LauncherPaths,
    account: &AccountData,
) -> Result<String, anyhow::Error> {
    let Some(msa) = account.msa_token.as_ref() else {
        return Ok(access_token(account).to_string());
    };
    if msa.expires_at - now_epoch() > REFRESH_WINDOW_SECONDS {
        return Ok(access_token(account).to_string());
    }
    let client = http_client();
    let result = resilient_request(|| {
        client.post(MSA_TOKEN_URL).form(&[
            ("client_id", MSA_CLIENT_ID),
            ("refresh_token", msa.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
            ("scope", MSA_SCOPES),
        ])
    })
    .await?;
    let refreshed_token = result
        .body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("MSA refresh failed: {}", format_http_result(&result)))?;
    let mut refreshed = run_pipeline(
        &client,
        MsaToken {
            extra_fields: Default::default(),
            access_token: refreshed_token.to_string(),
            refresh_token: result
                .body
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or(&msa.refresh_token)
                .to_string(),
            expires_at: now_epoch() + number_field(result.body.get("expires_in"), 3600),
        },
    )
    .await?;
    refreshed.id = account.id.clone();
    let token = access_token(&refreshed).to_string();
    persist_refreshed_account(paths, refreshed).await?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn late_token_refresh_cannot_resurrect_a_removed_account() {
        let paths = temp_paths("removed-during-refresh");
        let info = create_offline_account(&paths, "Alex").await.unwrap();
        let snapshot = load_accounts(&paths).await.unwrap().remove(0);
        remove_account(&paths, &info.id).await.unwrap();
        assert!(persist_refreshed_account(&paths, snapshot).await.is_err());
        assert!(load_accounts(&paths).await.unwrap().is_empty());
    }

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-accounts-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[test]
    fn validates_offline_usernames() {
        assert_eq!(
            validate_offline_username("  Steve_123 ").unwrap(),
            "Steve_123"
        );
        assert!(validate_offline_username("   ").is_err());
        assert!(validate_offline_username("this-name-is-way-too-long").is_err());
        assert!(validate_offline_username("bad name!").is_err());
    }

    #[test]
    fn offline_uuid_matches_java_guava_convention() {
        // Known vector: MD5("OfflinePlayer:Steve") with v3/variant bits.
        let uuid = offline_player_uuid("Steve");
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "3");
        assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(offline_player_uuid("Steve"), uuid);
        assert_ne!(offline_player_uuid("Steve"), offline_player_uuid("Alex"));
    }

    #[test]
    fn maps_known_xsts_failures() {
        assert!(xsts_error_message(Some(2148916233))
            .unwrap()
            .contains("no Xbox profile"));
        assert!(xsts_error_message(Some(2148916238))
            .unwrap()
            .contains("child account"));
        assert!(xsts_error_message(Some(7)).is_none());
    }

    #[test]
    fn parses_xbox_expiry() {
        assert_eq!(parse_xbox_expiry(Some("2026-01-01T00:00:00Z")), 1767225600);
        assert_eq!(parse_xbox_expiry(None), 0);
        assert_eq!(parse_xbox_expiry(Some("not-a-date")), 0);
    }

    #[test]
    fn derives_entitlements_from_items() {
        let items = vec![serde_json::json!({ "name": "product_minecraft" })];
        let ent = entitlements_from_items(Some(&items));
        assert!(ent.owns_minecraft && ent.can_play_minecraft);
        let pass = vec![serde_json::json!({ "name": "product_game_pass_pc" })];
        let ent = entitlements_from_items(Some(&pass));
        assert!(!ent.owns_minecraft && ent.can_play_minecraft);
        let ent = entitlements_from_items(None);
        assert!(!ent.owns_minecraft && !ent.can_play_minecraft);
    }

    #[tokio::test]
    async fn creates_and_dedupes_offline_accounts() {
        let paths = temp_paths("offline");
        let info = create_offline_account(&paths, "Steve").await.unwrap();
        assert_eq!(info.username, "Steve");
        assert_eq!(info.account_type, "offline");
        assert_eq!(info.uuid, offline_player_uuid("Steve"));
        // Case-insensitive duplicate rejected.
        assert!(create_offline_account(&paths, "steve").await.is_err());
        let accounts = load_accounts(&paths).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(account_to_info(&accounts[0]).username, "Steve");
    }

    #[tokio::test]
    async fn account_defaults_and_removal_preserve_other_accounts_and_settings() {
        let paths = temp_paths("lifecycle");
        let steve = create_offline_account(&paths, "Steve").await.unwrap();
        let alex = create_offline_account(&paths, "Alex").await.unwrap();
        let mut settings = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        settings.backup_retention_limit = 25;
        settings.active_account_id = Some(steve.id.clone());
        crate::settings::save_launcher_settings(&paths, &settings)
            .await
            .unwrap();

        set_default_account(&paths, Some(&alex.id)).await.unwrap();
        let saved = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        assert_eq!(saved.default_account_id.as_deref(), Some(alex.id.as_str()));
        assert_eq!(saved.active_account_id, None);
        assert_eq!(saved.backup_retention_limit, 25);
        assert!(set_default_account(&paths, Some("missing")).await.is_err());

        remove_account(&paths, &steve.id).await.unwrap();
        let saved = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        assert_eq!(saved.default_account_id.as_deref(), Some(alex.id.as_str()));
        assert_eq!(load_accounts(&paths).await.unwrap().len(), 1);

        remove_account(&paths, &alex.id).await.unwrap();
        let saved = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        assert_eq!(
            industrialis_core::launcher_settings::resolve_default_account_id(&saved),
            None
        );
        assert_eq!(saved.backup_retention_limit, 25);
        assert!(load_accounts(&paths).await.unwrap().is_empty());
        remove_account(&paths, &alex.id).await.unwrap();
    }

    #[tokio::test]
    async fn removing_a_default_does_not_restore_the_legacy_account() {
        let paths = temp_paths("legacy-default");
        let steve = create_offline_account(&paths, "Steve").await.unwrap();
        let alex = create_offline_account(&paths, "Alex").await.unwrap();
        let mut settings = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        settings.default_account_id = Some(steve.id.clone());
        settings.active_account_id = Some(alex.id.clone());
        crate::settings::save_launcher_settings(&paths, &settings)
            .await
            .unwrap();
        remove_account(&paths, &steve.id).await.unwrap();
        let saved = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        assert_eq!(
            industrialis_core::launcher_settings::resolve_default_account_id(&saved),
            None
        );
        assert_eq!(load_accounts(&paths).await.unwrap()[0].id, alex.id);

        let mut legacy = saved;
        legacy.active_account_id = Some(alex.id.clone());
        crate::settings::save_launcher_settings(&paths, &legacy)
            .await
            .unwrap();
        remove_account(&paths, &alex.id).await.unwrap();
        let saved = crate::settings::load_launcher_settings(&paths)
            .await
            .unwrap();
        assert_eq!(
            industrialis_core::launcher_settings::resolve_default_account_id(&saved),
            None
        );
    }

    #[tokio::test]
    async fn migrates_legacy_account_files() {
        let paths = temp_paths("legacy");
        let target = paths.accounts_path();
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &target,
            serde_json::to_string(&serde_json::json!([{
                "id": "abc", "username": "Steve", "uuid": "u-1",
                "access_token": "ygg", "refresh_token": "ref", "expires_at": 123
            }]))
            .unwrap(),
        )
        .await
        .unwrap();
        let accounts = load_accounts(&paths).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].format_version, 3);
        assert_eq!(accounts[0].msa_token.as_ref().unwrap().refresh_token, "ref");
        assert_eq!(accounts[0].yggdrasil_token.as_ref().unwrap().token, "ygg");
    }
}
