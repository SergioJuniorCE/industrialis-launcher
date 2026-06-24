use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::watch;
use uuid::Uuid;

const MSA_AUTH_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const MSA_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MSA_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MSA_SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";
const OAUTH_REDIRECT_URI: &str = "industrialislauncher://oauth/microsoft";
const OAUTH_TIMEOUT_SECS: u64 = 5 * 60;
const REFRESH_WINDOW_SECS: u64 = 12 * 60 * 60;

struct OAuthPending {
    expected_state: String,
    sender: tokio::sync::oneshot::Sender<Result<String, String>>,
}

static OAUTH_PENDING: Mutex<Option<OAuthPending>> = Mutex::new(None);

// ── Token / account types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub token: String,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsaToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skins: Option<Vec<ProfileSkin>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSkin {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub variant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinecraftEntitlement {
    #[serde(default)]
    pub owns_minecraft: bool,
    #[serde(default)]
    pub can_play_minecraft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    #[serde(default = "default_account_type")]
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

fn default_format_version() -> u32 {
    3
}

fn default_account_type() -> String {
    "msa".into()
}

impl AccountData {
    pub fn access_token(&self) -> String {
        self.yggdrasil_token
            .as_ref()
            .map(|t| t.token.clone())
            .unwrap_or_default()
    }

    pub fn profile_name(&self) -> String {
        self.minecraft_profile
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    pub fn profile_id(&self) -> String {
        self.minecraft_profile
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_default()
    }

    pub fn uhs(&self) -> Option<String> {
        self.user_token
            .as_ref()
            .and_then(token_uhs)
            .or_else(|| self.mojangservices_token.as_ref().and_then(token_uhs))
    }

    pub fn needs_refresh(&self) -> bool {
        let now = unix_now();
        if let Some(msa) = &self.msa_token {
            if msa.expires_at.saturating_sub(now) <= REFRESH_WINDOW_SECS {
                return true;
            }
        }
        if let Some(ygg) = &self.yggdrasil_token {
            if ygg.expires_at > 0 && ygg.expires_at.saturating_sub(now) <= 60 {
                return true;
            }
        }
        false
    }
}

/// Legacy on-disk format (pre–format-version-3).
#[derive(Debug, Clone, Deserialize)]
struct LegacyMinecraftAccount {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub username: String,
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_png_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owns_minecraft: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    OAuthCode,
    DeviceCode,
    Refresh,
}

// ── Persistence ──

pub fn accounts_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("accounts.json")
}

pub fn load_accounts(data_dir: &PathBuf) -> Vec<AccountData> {
    let path = accounts_path(data_dir);
    let raw = match fs_read(&path) {
        Some(s) => s,
        None => return Vec::new(),
    };
    if let Ok(accounts) = serde_json::from_str::<Vec<AccountData>>(&raw) {
        return accounts;
    }
    if let Ok(legacy) = serde_json::from_str::<Vec<LegacyMinecraftAccount>>(&raw) {
        return legacy.into_iter().map(legacy_to_account).collect();
    }
    Vec::new()
}

pub fn save_accounts(data_dir: &PathBuf, accounts: &[AccountData]) -> Result<(), String> {
    let path = accounts_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

fn fs_read(path: &PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn legacy_to_account(legacy: LegacyMinecraftAccount) -> AccountData {
    AccountData {
        format_version: 3,
        account_type: "msa".into(),
        id: legacy.id,
        msa_token: Some(MsaToken {
            access_token: String::new(),
            refresh_token: legacy.refresh_token,
            expires_at: legacy.expires_at,
        }),
        user_token: None,
        mojangservices_token: None,
        yggdrasil_token: Some(StoredToken {
            token: legacy.access_token,
            expires_at: 0,
            extra: None,
        }),
        minecraft_profile: Some(MinecraftProfile {
            id: legacy.uuid,
            name: legacy.username,
            skins: None,
        }),
        minecraft_entitlement: None,
        skin_png_base64: None,
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn expires_at_from_secs(secs: u64) -> u64 {
    unix_now() + secs
}

// ── Public entry points ──

pub fn embedded_microsoft_client_id() -> Result<&'static str, String> {
    const CID: &str = env!("MICROSOFT_CLIENT_ID");
    if CID.is_empty() {
        Err(
            "Microsoft login is not available in this build (no client ID configured at build time)."
                .into(),
        )
    } else {
        Ok(CID)
    }
}

pub async fn login_microsoft_account(
    client: &Client,
    app: &AppHandle,
    data_dir: &PathBuf,
) -> Result<AccountInfo, String> {
    let client_id = embedded_microsoft_client_id()?;
    let (cancel_tx, cancel_rx) = watch::channel(false);

    let oauth_client = client.clone();
    let oauth_app = app.clone();
    let oauth_cid = client_id.to_string();
    let oauth_cancel = cancel_rx.clone();
    let oauth_task = tokio::spawn(async move {
        oauth_code_flow(&oauth_client, &oauth_app, &oauth_cid, oauth_cancel).await
    });

    let device_client = client.clone();
    let device_app = app.clone();
    let device_cid = client_id.to_string();
    let device_cancel = cancel_rx;
    let device_task = tokio::spawn(async move {
        device_code_flow(&device_client, &device_app, &device_cid, device_cancel).await
    });

    let msa = race_oauth_and_device_code(oauth_task, device_task, cancel_tx).await?;

    let mut account = AccountData {
        format_version: 3,
        account_type: "msa".into(),
        id: Uuid::new_v4().to_string(),
        msa_token: None,
        user_token: None,
        mojangservices_token: None,
        yggdrasil_token: None,
        minecraft_profile: None,
        minecraft_entitlement: None,
        skin_png_base64: None,
    };

    run_pipeline(client, &mut account, msa, AuthMethod::OAuthCode).await?;
    upsert_account(data_dir, &mut account)?;
    Ok(account_to_info(&account))
}

pub async fn ensure_fresh_token(
    client: &Client,
    data_dir: &PathBuf,
    account: &AccountData,
) -> Result<String, String> {
    if !account.needs_refresh() {
        return Ok(account.access_token());
    }
    let msa = account
        .msa_token
        .as_ref()
        .ok_or("account has no MSA refresh token")?;
    let refreshed =
        refresh_msa_token(client, embedded_microsoft_client_id()?, &msa.refresh_token).await?;
    let mut updated = account.clone();
    run_pipeline(client, &mut updated, refreshed, AuthMethod::Refresh).await?;
    upsert_account(data_dir, &mut updated)?;
    Ok(updated.access_token())
}

fn upsert_account(data_dir: &PathBuf, account: &mut AccountData) -> Result<(), String> {
    let mut accounts = load_accounts(data_dir);
    let profile_id = account.profile_id();
    if !profile_id.is_empty() {
        accounts.retain(|a| a.profile_id() != profile_id);
    }
    accounts.retain(|a| a.id != account.id);
    accounts.push(account.clone());
    save_accounts(data_dir, &accounts)
}

fn account_to_info(account: &AccountData) -> AccountInfo {
    AccountInfo {
        id: account.id.clone(),
        username: account.profile_name(),
        uuid: account.profile_id(),
        skin_png_base64: account.skin_png_base64.clone(),
        owns_minecraft: account
            .minecraft_entitlement
            .as_ref()
            .map(|e| e.owns_minecraft),
    }
}

// ── Step 1: Microsoft OAuth ──

fn begin_oauth_wait(expected_state: String) -> tokio::sync::oneshot::Receiver<Result<String, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut guard = OAUTH_PENDING.lock().unwrap();
    *guard = Some(OAuthPending {
        expected_state,
        sender: tx,
    });
    rx
}

fn cancel_oauth_wait() {
    let mut guard = OAUTH_PENDING.lock().unwrap();
    *guard = None;
}

fn is_oauth_redirect_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "industrialislauncher"
        && parsed.host_str() == Some("oauth")
        && matches!(parsed.path(), "/microsoft" | "/microsoft/")
}

pub fn handle_oauth_callback(url: &str) -> Result<(), String> {
    if !is_oauth_redirect_url(url) {
        return Err("unexpected OAuth callback URL".into());
    }

    let result = parse_oauth_callback_url(url);
    let mut guard = OAUTH_PENDING.lock().unwrap();
    let pending = guard.take().ok_or("no OAuth flow in progress")?;

    match result {
        Ok((code, state)) => {
            if state != pending.expected_state {
                let _ = pending
                    .sender
                    .send(Err("OAuth state mismatch".into()));
                return Err("OAuth state mismatch".into());
            }
            let _ = pending.sender.send(Ok(code));
            Ok(())
        }
        Err(e) => {
            let _ = pending.sender.send(Err(e.clone()));
            Err(e)
        }
    }
}

async fn wait_for_oauth_cancel(cancel: watch::Receiver<bool>) {
    let mut cancel = cancel;
    loop {
        if *cancel.borrow() {
            break;
        }
        if cancel.changed().await.is_err() {
            break;
        }
    }
}

async fn oauth_code_flow(
    client: &Client,
    app: &AppHandle,
    client_id: &str,
    cancel: watch::Receiver<bool>,
) -> Result<MsaToken, String> {
    if *cancel.borrow() {
        return Err("login cancelled".into());
    }

    let state = Uuid::new_v4().to_string();
    let rx = begin_oauth_wait(state.clone());

    let auth_url = format!(
        "{MSA_AUTH_URL}?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}&response_mode=query&prompt=select_account",
        urlencoding::encode(client_id),
        urlencoding::encode(OAUTH_REDIRECT_URI),
        urlencoding::encode(MSA_SCOPES),
        urlencoding::encode(&state),
    );

    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("failed to open browser: {e}"))?;

    let code = tokio::select! {
        result = rx => {
            match result {
                Ok(Ok(code)) => code,
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err("OAuth flow cancelled".into()),
            }
        }
        _ = wait_for_oauth_cancel(cancel) => {
            cancel_oauth_wait();
            return Err("login cancelled".into());
        }
        _ = tokio::time::sleep(Duration::from_secs(OAUTH_TIMEOUT_SECS)) => {
            cancel_oauth_wait();
            return Err("OAuth login timed out".into());
        }
    };

    exchange_auth_code(client, client_id, &code, OAUTH_REDIRECT_URI).await
}

/// Wait for the first successful OAuth or device-code flow. A failure in one path
/// does not cancel the other — browser OAuth can still complete after device code
/// fails (e.g. Azure app not marked as a mobile/public client).
async fn race_oauth_and_device_code(
    oauth_task: tokio::task::JoinHandle<Result<MsaToken, String>>,
    device_task: tokio::task::JoinHandle<Result<MsaToken, String>>,
    cancel_tx: watch::Sender<bool>,
) -> Result<MsaToken, String> {
    let oauth = async { oauth_task.await.map_err(|e| e.to_string())? };
    let device = async { device_task.await.map_err(|e| e.to_string())? };

    tokio::pin!(oauth);
    tokio::pin!(device);

    let mut oauth_err = None;
    let mut device_err = None;
    let mut oauth_pending = true;
    let mut device_pending = true;

    while oauth_pending || device_pending {
        tokio::select! {
            result = &mut oauth, if oauth_pending => {
                oauth_pending = false;
                match result {
                    Ok(msa) => {
                        let _ = cancel_tx.send(true);
                        return Ok(msa);
                    }
                    Err(e) => oauth_err = Some(e),
                }
            }
            result = &mut device, if device_pending => {
                device_pending = false;
                match result {
                    Ok(msa) => {
                        let _ = cancel_tx.send(true);
                        return Ok(msa);
                    }
                    Err(e) => device_err = Some(e),
                }
            }
        }
    }

    Err(oauth_err.unwrap_or_else(|| {
        device_err.unwrap_or_else(|| "Microsoft login failed".into())
    }))
}

async fn request_device_code(client: &Client, client_id: &str) -> Result<DeviceCodeResponse, String> {
    let body: serde_json::Value = client
        .post(MSA_DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", MSA_SCOPES)])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| format!("device code request: {e}"))?;

    if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
        let desc = body["error_description"].as_str().unwrap_or("");
        return Err(format!("device code ({err}): {desc}"));
    }

    serde_json::from_value(body).map_err(|e| format!("device code response: {e}"))
}

async fn device_code_flow(
    client: &Client,
    app: &AppHandle,
    client_id: &str,
    cancel: watch::Receiver<bool>,
) -> Result<MsaToken, String> {
    // Device code is a fallback (see Prism Launcher MSALoginDialog). A setup error here
    // must not block browser OAuth — race_oauth_and_device_code waits for both paths.
    let device = match request_device_code(client, client_id).await {
        Ok(device) => device,
        Err(e) => {
            eprintln!("device code fallback unavailable: {e}");
            return Err(e);
        }
    };

    let _ = app.emit(
        "auth-device-code",
        &DeviceCodeInfo {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            message: device.message.clone(),
        },
    );

    poll_device_code_token(client, client_id, &device, cancel).await
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
    message: String,
}

async fn poll_device_code_token(
    client: &Client,
    client_id: &str,
    device: &DeviceCodeResponse,
    cancel: watch::Receiver<bool>,
) -> Result<MsaToken, String> {
    let deadline = unix_now() + device.expires_in;
    let mut interval = device.interval.max(5);

    loop {
        if *cancel.borrow() {
            return Err("login cancelled".into());
        }
        if unix_now() >= deadline {
            return Err("device code login timed out".into());
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;

        let resp = client
            .post(MSA_TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device.device_code),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

        if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
            let refresh = body["refresh_token"]
                .as_str()
                .ok_or("no refresh_token in device code response")?;
            let expires_in = body["expires_in"].as_u64().unwrap_or(3600);
            return Ok(MsaToken {
                access_token: token.to_string(),
                refresh_token: refresh.to_string(),
                expires_at: expires_at_from_secs(expires_in),
            });
        }

        let error = body["error"].as_str().unwrap_or("");
        match error {
            "authorization_pending" => continue,
            "slow_down" => {
                interval += 5;
                continue;
            }
            "expired_token" => return Err("device code expired — try again".into()),
            "access_denied" => return Err("login denied".into()),
            other => {
                let desc = body["error_description"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(format!("device code poll failed ({other}): {desc}"));
            }
        }
    }
}

async fn exchange_auth_code(
    client: &Client,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<MsaToken, String> {
    let body: serde_json::Value = client
        .post(MSA_TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("scope", MSA_SCOPES),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| format!("token exchange: {e}"))?;

    parse_msa_token_response_with_fallback(&body, None)
}

async fn refresh_msa_token(
    client: &Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<MsaToken, String> {
    let body: serde_json::Value = client
        .post(MSA_TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", MSA_SCOPES),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| format!("MSA refresh: {e}"))?;

    parse_msa_token_response_with_fallback(&body, Some(refresh_token))
}

fn parse_msa_token_response(body: &serde_json::Value) -> Result<MsaToken, String> {
    parse_msa_token_response_with_fallback(body, None)
}

fn parse_msa_token_response_with_fallback(
    body: &serde_json::Value,
    fallback_refresh: Option<&str>,
) -> Result<MsaToken, String> {
    if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
        let desc = body["error_description"].as_str().unwrap_or("");
        return Err(format!("Microsoft auth failed ({err}): {desc}"));
    }
    let access = body["access_token"]
        .as_str()
        .ok_or("no access_token in Microsoft response")?;
    let refresh = body["refresh_token"]
        .as_str()
        .or(fallback_refresh)
        .ok_or("no refresh_token in Microsoft response")?;
    let expires_in = body["expires_in"].as_u64().unwrap_or(3600);
    Ok(MsaToken {
        access_token: access.to_string(),
        refresh_token: refresh.to_string(),
        expires_at: expires_at_from_secs(expires_in),
    })
}

fn parse_oauth_callback_url(url: &str) -> Result<(String, String), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid OAuth callback URL: {e}"))?;
    let query = parsed.query().ok_or("no query in OAuth callback")?;

    if let Some(err) = extract_query_param(query, "error") {
        let desc = extract_query_param(query, "error_description").unwrap_or_default();
        return Err(format!("Microsoft login error ({err}): {desc}"));
    }

    let state = extract_query_param(query, "state").ok_or("missing state in OAuth callback")?;
    let code = extract_query_param(query, "code").ok_or("no auth code in callback")?;
    Ok((code, state))
}

fn extract_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return urlencoding::decode(v).ok().map(|s| s.into_owned());
            }
        }
    }
    None
}

// ── Steps 2–7: Xbox → Minecraft pipeline ──

pub async fn run_pipeline(
    client: &Client,
    account: &mut AccountData,
    msa: MsaToken,
    method: AuthMethod,
) -> Result<(), String> {
    account.msa_token = Some(msa.clone());

    // Step 2: Xbox user token
    let xbl = xbox_user_auth(client, &msa.access_token).await?;
    let uhs = xbl_uhs(&xbl)?;
    account.user_token = Some(StoredToken {
        token: xbl["Token"]
            .as_str()
            .ok_or("no Xbox user token")?
            .to_string(),
        expires_at: parse_xbox_expiry(&xbl),
        extra: Some(serde_json::json!({ "uhs": uhs })),
    });

    // Step 3: XSTS for Mojang services
    let xsts = xsts_mojang_auth(client, account.user_token.as_ref().unwrap()).await?;
    let xsts_uhs = xbl_uhs(&xsts)?;
    account.mojangservices_token = Some(StoredToken {
        token: xsts["Token"].as_str().ok_or("no XSTS token")?.to_string(),
        expires_at: parse_xbox_expiry(&xsts),
        extra: Some(serde_json::json!({ "uhs": xsts_uhs })),
    });

    // Step 4: Minecraft launcher login (Prism uses mojangservices uhs + XSTS token)
    let mojang = account.mojangservices_token.as_ref().unwrap();
    let uhs = token_uhs(mojang).ok_or("missing Xbox user hash (uhs) in XSTS token")?;
    let xsts_token = mojang.token.clone();
    let mc = minecraft_launcher_login(client, &uhs, &xsts_token).await?;
    let mc_expires = mc["expires_in"].as_u64().unwrap_or(86400);
    account.yggdrasil_token = Some(StoredToken {
        token: mc["access_token"]
            .as_str()
            .ok_or("no Minecraft access token")?
            .to_string(),
        expires_at: expires_at_from_secs(mc_expires),
        extra: None,
    });

    let mc_token = account.access_token();

    // Step 5: Entitlements
    account.minecraft_entitlement = Some(check_entitlements(client, &mc_token).await?);

    // Step 6: Profile (404 = no username set yet)
    account.minecraft_profile = fetch_profile(client, &mc_token).await?;

    // Step 7: Skin
    account.skin_png_base64 = download_skin(client, account.minecraft_profile.as_ref()).await?;

    let _ = method; // reserved for logging / telemetry
    Ok(())
}

async fn xbox_user_auth(client: &Client, msa_access: &str) -> Result<serde_json::Value, String> {
    let resp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={msa_access}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp.json().await.map_err(|e| format!("xbl auth: {e}"))?;
    if body.get("Token").is_none() {
        return Err(format!("Xbox user auth failed: {body}"));
    }
    Ok(body)
}

fn token_uhs(token: &StoredToken) -> Option<String> {
    token
        .extra
        .as_ref()
        .and_then(|e| e.get("uhs"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

async fn xsts_mojang_auth(
    client: &Client,
    user_token: &StoredToken,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .header("x-xbl-contract-version", "1")
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [user_token.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("xsts auth: {e}"))?;

    if !status.is_success() || body.get("Token").is_none() {
        return Err(parse_xsts_error(&body));
    }
    Ok(body)
}

fn parse_xsts_error(body: &serde_json::Value) -> String {
    let xerr = body["XErr"]
        .as_u64()
        .or_else(|| body["XErr"].as_str().and_then(|s| s.parse().ok()));
    match xerr {
        Some(2148916233) => {
            "This Microsoft account has no Xbox profile. Create one at https://www.xbox.com first."
                .into()
        }
        Some(2148916238) => "This account is a child account and must be added to a family.".into(),
        Some(2148916235) => "Xbox Live is unavailable in your region.".into(),
        Some(2148916236) | Some(2148916237) => {
            "This account needs adult verification on Xbox Live.".into()
        }
        Some(code) => format!("Xbox authorization failed (XErr {code})"),
        None => format!("Xbox authorization failed: {body}"),
    }
}

fn xbl_uhs(xbl: &serde_json::Value) -> Result<String, String> {
    xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .map(str::to_string)
        .ok_or("no uhs in Xbox user token".into())
}

fn parse_xbox_expiry(body: &serde_json::Value) -> u64 {
    body["NotAfter"]
        .as_str()
        .and_then(parse_rfc3339_to_unix)
        .unwrap_or(0)
}

fn parse_rfc3339_to_unix(value: &str) -> Option<u64> {
    // Minimal RFC3339 parser for Xbox NotAfter timestamps.
    let trimmed = value.trim_end_matches('Z');
    let (date_time, _) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let mut parts = date_time.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;

    let days_from_civil = |y: i32, m: u32, d: u32| -> i64 {
        let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
        let era = if y >= 0 { y / 400 } else { -1 - (-1 - y) / 400 };
        let yoe = y - era * 400;
        let doy = (153 * (m as i64 - 3) + 2) / 5 + d as i64 - 1;
        let yoe = yoe as i64;
        era as i64 * 146097 + yoe * 1461 / 4 + yoe / 4 - yoe / 100 + doy - 719468
    };

    let days = days_from_civil(year, month, day);
    let secs = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    Some(secs as u64)
}

fn format_minecraft_login_error(body: &serde_json::Value) -> String {
    let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    if error.eq_ignore_ascii_case("FORBIDDEN") {
        return "Minecraft API rejected this Azure application (FORBIDDEN). \
                Custom client IDs must be approved by Mojang before they can log in — \
                Prism Launcher ships a pre-approved ID, but your own registration needs review. \
                Submit your Application (client) ID at https://aka.ms/mce-reviewappid, \
                then try again after approval."
            .into();
    }
    format!("Minecraft launcher login failed: {body}")
}

async fn post_minecraft_login(
    client: &Client,
    url: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post(url)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    resp.json()
        .await
        .map_err(|e| format!("minecraft login response: {e}"))
}

async fn minecraft_launcher_login(
    client: &Client,
    uhs: &str,
    xsts_token: &str,
) -> Result<serde_json::Value, String> {
    let identity = format!("XBL3.0 x={uhs};{xsts_token}");

    // Prism Launcher uses /launcher/login; wiki documents /authentication/login_with_xbox.
    let launcher_body = serde_json::json!({
        "xtoken": identity,
        "platform": "PC_LAUNCHER"
    });
    let launcher = post_minecraft_login(
        client,
        "https://api.minecraftservices.com/launcher/login",
        launcher_body,
    )
    .await?;

    if launcher.get("access_token").is_some() {
        return Ok(launcher);
    }

    if launcher.get("error").and_then(|v| v.as_str()) != Some("FORBIDDEN") {
        return Err(format_minecraft_login_error(&launcher));
    }

    let xbox_body = serde_json::json!({ "identityToken": identity });
    let xbox = post_minecraft_login(
        client,
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        xbox_body,
    )
    .await?;

    if xbox.get("access_token").is_some() {
        return Ok(xbox);
    }

    Err(format_minecraft_login_error(&launcher))
}

async fn check_entitlements(
    client: &Client,
    mc_token: &str,
) -> Result<MinecraftEntitlement, String> {
    let resp = client
        .get("https://api.minecraftservices.com/entitlements/license?requestId=00000000-0000-0000-0000-000000000000")
        .header("Authorization", format!("Bearer {mc_token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("entitlements: {e}"))?;

    let items = body["items"].as_array();
    let mut owns = false;
    let mut can_play = false;

    if let Some(items) = items {
        for item in items {
            let name = item["name"].as_str().unwrap_or("");
            if name == "product_minecraft" || name == "game_minecraft" {
                owns = true;
            }
            if name == "product_minecraft"
                || name == "game_minecraft"
                || name == "product_game_pass_pc"
            {
                can_play = true;
            }
        }
    }

    Ok(MinecraftEntitlement {
        owns_minecraft: owns,
        can_play_minecraft: can_play,
    })
}

async fn fetch_profile(
    client: &Client,
    mc_token: &str,
) -> Result<Option<MinecraftProfile>, String> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .header("Authorization", format!("Bearer {mc_token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("profile fetch failed: {body}"));
    }

    let profile: MinecraftProfile = resp
        .json()
        .await
        .map_err(|e| format!("profile parse: {e}"))?;
    Ok(Some(profile))
}

async fn download_skin(
    client: &Client,
    profile: Option<&MinecraftProfile>,
) -> Result<Option<String>, String> {
    let profile = match profile {
        Some(p) => p,
        None => return Ok(None),
    };
    let skin_url = profile
        .skins
        .as_ref()
        .and_then(|skins| skins.iter().find(|s| s.state == "ACTIVE"))
        .map(|s| s.url.as_str())
        .or_else(|| {
            profile
                .skins
                .as_ref()
                .and_then(|s| s.first())
                .map(|s| s.url.as_str())
        });

    let url = match skin_url {
        Some(u) => u,
        None => return Ok(None),
    };

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(Some(BASE64.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_data_access_token_uses_yggdrasil() {
        let account = AccountData {
            format_version: 3,
            account_type: "msa".into(),
            id: "1".into(),
            msa_token: None,
            user_token: None,
            mojangservices_token: None,
            yggdrasil_token: Some(StoredToken {
                token: "mc-token".into(),
                expires_at: 0,
                extra: None,
            }),
            minecraft_profile: None,
            minecraft_entitlement: None,
            skin_png_base64: None,
        };
        assert_eq!(account.access_token(), "mc-token");
    }

    #[test]
    fn needs_refresh_within_twelve_hours() {
        let account = AccountData {
            format_version: 3,
            account_type: "msa".into(),
            id: "1".into(),
            msa_token: Some(MsaToken {
                access_token: String::new(),
                refresh_token: String::new(),
                expires_at: unix_now() + REFRESH_WINDOW_SECS - 10,
            }),
            user_token: None,
            mojangservices_token: None,
            yggdrasil_token: None,
            minecraft_profile: None,
            minecraft_entitlement: None,
            skin_png_base64: None,
        };
        assert!(account.needs_refresh());
    }

    #[test]
    fn oauth_redirect_url_matches_prism_style_callback() {
        assert!(is_oauth_redirect_url(
            "industrialislauncher://oauth/microsoft?code=abc&state=xyz"
        ));
        assert!(is_oauth_redirect_url(
            "industrialislauncher://oauth/microsoft/?code=abc&state=xyz"
        ));
        assert!(!is_oauth_redirect_url("https://example.com/oauth/microsoft?code=abc"));
    }

    #[test]
    fn legacy_account_deserializes() {
        let json = r#"[{
            "id": "a",
            "username": "Steve",
            "uuid": "uuid",
            "access_token": "tok",
            "refresh_token": "ref",
            "expires_at": 9999999999
        }]"#;
        let accounts: Vec<AccountData> = serde_json::from_str::<Vec<LegacyMinecraftAccount>>(json)
            .unwrap()
            .into_iter()
            .map(legacy_to_account)
            .collect();
        assert_eq!(accounts[0].profile_name(), "Steve");
        assert_eq!(accounts[0].access_token(), "tok");
    }
}
