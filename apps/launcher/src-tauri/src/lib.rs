mod auth;
mod config_presets;
mod groups;
mod migration;
mod minecraft_files;
mod pack;
mod pack_cache;
mod settings;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
pub use auth::{AccountData, AccountInfo};

// ── Types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GtnhVersion {
    pub title: String,
    pub description: String,
    #[serde(rename = "releaseDate")]
    pub release_date: String,
    #[serde(rename = "maxJavaVersion")]
    pub max_java_version: u32,
    pub mmc: MmcDownloads,
    pub client: ClientDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmcDownloads {
    #[serde(rename = "java8Url")]
    pub java8_url: String,
    #[serde(rename = "java17_2XUrl")]
    pub java17_2x_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDownload {
    #[serde(rename = "java8Url")]
    pub java8_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: String,
    pub installed: bool,
    pub size_bytes: u64,
    pub settings: InstanceSettings,
    pub group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub name: String,
    #[serde(default)]
    pub pack_version: String,
    #[serde(default = "default_pack_java_type")]
    pub pack_java_type: String,
    pub java_path: Option<String>,
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    #[serde(default = "default_perm_gen_mb")]
    pub perm_gen_mb: u32,
    pub jvm_args: String,
    pub auth_mode: String,
    pub username: String,
    #[serde(default)]
    pub offline_username_confirmed: bool,

    #[serde(default)]
    pub override_window: bool,
    #[serde(default)]
    pub launch_maximized: bool,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    #[serde(default)]
    pub close_after_launch: bool,
    #[serde(default)]
    pub quit_after_game_stop: bool,

    #[serde(default)]
    pub override_console: bool,
    #[serde(default)]
    pub show_console_on_launch: bool,
    #[serde(default = "default_true")]
    pub show_console_on_error: bool,
    #[serde(default)]
    pub auto_close_console: bool,

    #[serde(default)]
    pub override_game_time: bool,
    #[serde(default = "default_true")]
    pub show_game_time: bool,
    #[serde(default = "default_true")]
    pub record_game_time: bool,
    #[serde(default)]
    pub total_play_seconds: u64,

    #[serde(default)]
    pub override_account: bool,
    #[serde(default)]
    pub account_id: Option<String>,

    #[serde(default)]
    pub join_server_on_launch: bool,
    #[serde(default)]
    pub join_server_address: String,

    #[serde(default)]
    pub override_java_location: bool,
    #[serde(default)]
    pub skip_java_compat: bool,
    #[serde(default)]
    pub override_memory: bool,
    #[serde(default)]
    pub override_java_args: bool,

    #[serde(default)]
    pub override_commands: bool,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub wrapper_command: String,
    #[serde(default)]
    pub post_exit_command: String,

    #[serde(default)]
    pub override_env: bool,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,

    /// Cached on disk so instance lists load without walking the full pack tree.
    #[serde(default)]
    pub cached_size_bytes: u64,

    /// Filename of a custom icon stored in the instance directory (e.g. `instance-icon.png`).
    #[serde(default)]
    pub custom_icon: Option<String>,
}

fn default_perm_gen_mb() -> u32 {
    128
}

fn default_window_width() -> u32 {
    854
}

fn default_window_height() -> u32 {
    480
}

fn default_true() -> bool {
    true
}

fn default_pack_java_type() -> String {
    "java17+".to_string()
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            pack_version: String::new(),
            pack_java_type: default_pack_java_type(),
            java_path: None,
            min_ram_mb: 4096,
            max_ram_mb: 6144,
            perm_gen_mb: default_perm_gen_mb(),
            jvm_args: String::new(),
            auth_mode: "offline".into(),
            username: String::new(),
            offline_username_confirmed: false,
            override_window: false,
            launch_maximized: false,
            window_width: default_window_width(),
            window_height: default_window_height(),
            close_after_launch: false,
            quit_after_game_stop: false,
            override_console: false,
            show_console_on_launch: false,
            show_console_on_error: true,
            auto_close_console: false,
            override_game_time: false,
            show_game_time: true,
            record_game_time: true,
            total_play_seconds: 0,
            override_account: false,
            account_id: None,
            join_server_on_launch: false,
            join_server_address: String::new(),
            override_java_location: false,
            skip_java_compat: false,
            override_memory: false,
            override_java_args: false,
            override_commands: false,
            pre_launch_command: String::new(),
            wrapper_command: String::new(),
            post_exit_command: String::new(),
            override_env: false,
            env_vars: HashMap::new(),
            cached_size_bytes: 0,
            custom_icon: None,
        }
    }
}

impl InstanceSettings {
    fn effective_min_ram_mb(&self) -> u32 {
        if self.override_memory {
            self.min_ram_mb
        } else {
            4096
        }
    }

    fn effective_max_ram_mb(&self) -> u32 {
        if self.override_memory {
            self.max_ram_mb
        } else {
            6144
        }
    }

    fn effective_jvm_args(&self) -> &str {
        if self.override_java_args {
            &self.jvm_args
        } else {
            ""
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaInfo {
    pub path: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub main_class: String,
    pub minecraft_version: String,
    pub libraries: Vec<String>,
    pub game_dir: String,
    pub assets_dir: String,
    pub jvm_args: Vec<String>,
    pub program_args: Vec<String>,
    pub minecraft_arguments_template: Option<String>,
    pub asset_index: Option<PatchAssetIndex>,
}

pub use settings::LauncherSettings;

// ── MMC Pack structures ──

#[derive(Debug, Deserialize)]
struct MmcPackJson {
    components: Vec<MmComponent>,
    #[allow(dead_code)]
    format_version: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MmComponent {
    uid: String,
    version: Option<String>,
    #[serde(rename = "cachedVersion")]
    cached_version: Option<String>,
}

impl MmComponent {
    fn effective_version(&self) -> &str {
        self.version
            .as_deref()
            .or(self.cached_version.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Deserialize)]
struct PatchJson {
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    uid: Option<String>,
    #[allow(dead_code)]
    version: Option<String>,
    order: Option<i32>,
    #[serde(rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    #[serde(rename = "assetIndex")]
    asset_index: Option<PatchAssetIndex>,
    #[serde(rename = "mainClass")]
    main_class: Option<String>,
    #[serde(rename = "+mainClass")]
    plus_main_class: Option<String>,
    #[serde(rename = "mainJar")]
    main_jar: Option<PatchMainJar>,
    libraries: Option<Vec<PatchLibraryEntry>>,
    #[serde(rename = "+jvmArgs")]
    plus_jvm_args: Option<Vec<String>>,
    #[serde(rename = "+args")]
    plus_args: Option<Vec<String>>,
    #[serde(rename = "+tweakers")]
    plus_tweakers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchAssetIndex {
    id: String,
    sha1: Option<String>,
    size: Option<u64>,
    #[serde(rename = "totalSize")]
    total_size: Option<u64>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndexFile {
    objects: HashMap<String, AssetIndexObject>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndexObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct PatchMainJar {
    name: String,
    downloads: PatchDownloads,
}

#[derive(Debug, Deserialize)]
struct PatchLibraryEntry {
    name: String,
    /// Maven repository base URL (MultiMC / legacy version.json format).
    url: Option<String>,
    #[serde(rename = "MMC-hint")]
    mmc_hint: Option<String>,
    #[serde(rename = "MMC-absoluteUrl")]
    mmc_absolute_url: Option<String>,
    downloads: Option<PatchDownloads>,
    rules: Option<Vec<PatchRule>>,
    natives: Option<serde_json::Value>,
}

const DEFAULT_LIBRARY_REPO: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Clone, Deserialize)]
struct PatchDownloads {
    artifact: Option<PatchArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct PatchArtifact {
    url: String,
}

#[derive(Debug, Deserialize)]
struct PatchRule {
    action: String,
    os: Option<PatchRuleOs>,
}

#[derive(Debug, Deserialize)]
struct PatchRuleOs {
    name: Option<String>,
}

#[derive(Debug, Clone)]
struct GradleSpec {
    group: String,
    artifact: String,
    version: String,
    classifier: Option<String>,
    extension: String,
}

impl GradleSpec {
    fn filename(&self) -> String {
        let mut name = format!("{}-{}", self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            name.push('-');
            name.push_str(classifier);
        }
        name.push('.');
        name.push_str(&self.extension);
        name
    }

    fn storage_path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.filename()
        )
    }
}

fn library_coord_key(spec: &GradleSpec) -> String {
    format!("{}:{}", spec.group, spec.artifact)
}

fn version_is_newer(candidate: &str, existing: &str) -> bool {
    candidate.cmp(existing) == std::cmp::Ordering::Greater
}

struct ResolvedLibrary {
    path: String,
    version: String,
}

fn upsert_library(
    libraries: &mut Vec<ResolvedLibrary>,
    index: &mut HashMap<String, usize>,
    spec: &GradleSpec,
    path: String,
) {
    let key = library_coord_key(spec);
    if let Some(&idx) = index.get(&key) {
        if version_is_newer(&spec.version, &libraries[idx].version) {
            libraries[idx] = ResolvedLibrary {
                path,
                version: spec.version.clone(),
            };
        }
        return;
    }
    index.insert(key, libraries.len());
    libraries.push(ResolvedLibrary {
        path,
        version: spec.version.clone(),
    });
}

fn parse_gradle_spec(value: &str) -> Option<GradleSpec> {
    let (coords, extension) = match value.split_once('@') {
        Some((c, e)) => (c, e.to_string()),
        None => (value, "jar".to_string()),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    Some(GradleSpec {
        group: parts[0].to_string(),
        artifact: parts[1].to_string(),
        version: parts[2].to_string(),
        classifier: parts.get(3).map(|s| s.to_string()),
        extension,
    })
}

fn current_os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn library_allowed(rules: Option<&[PatchRule]>) -> bool {
    let rules = match rules {
        Some(r) if !r.is_empty() => r,
        _ => return true,
    };
    let mut allowed = false;
    for rule in rules {
        let applies = rule
            .os
            .as_ref()
            .map_or(true, |os| os.name.as_deref() == Some(current_os_name()));
        if applies && rule.action != "defer" {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn is_native_only(entry: &PatchLibraryEntry) -> bool {
    entry.natives.is_some()
        && entry
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .is_none()
}

fn library_paths(pack_dir: &Path, entry: &PatchLibraryEntry) -> Vec<PathBuf> {
    let Some(spec) = parse_gradle_spec(&entry.name) else {
        return Vec::new();
    };
    let lib_root = pack_dir.join("libraries");
    let mut paths = vec![lib_root.join(spec.storage_path())];
    if entry.mmc_hint.as_deref() == Some("local") {
        paths.insert(0, lib_root.join(spec.filename()));
    }
    paths
}

fn find_library(pack_dir: &Path, entry: &PatchLibraryEntry) -> Option<PathBuf> {
    library_paths(pack_dir, entry)
        .into_iter()
        .find(|path| path.exists())
}

fn maven_repo_url(base: &str, storage_path: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{storage_path}")
    } else {
        format!("{base}/{storage_path}")
    }
}

fn resolve_library_download_url(entry: &PatchLibraryEntry, spec: &GradleSpec) -> Option<String> {
    if let Some(abs) = entry.mmc_absolute_url.as_deref() {
        let trimmed = abs.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(downloads) = &entry.downloads {
        if let Some(artifact) = downloads.artifact.as_ref() {
            let trimmed = artifact.url.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let repo = entry
        .url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(DEFAULT_LIBRARY_REPO);
    Some(maven_repo_url(repo, &spec.storage_path()))
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed ({url}): HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    fs::write(dest, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn file_sha1(path: &Path) -> Result<String, String> {
    use sha1::{Digest, Sha1};
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha1::digest(bytes)))
}

fn asset_object_digest(hash: &str) -> &str {
    hash.strip_prefix("sha1:").unwrap_or(hash)
}

fn asset_object_path(assets_dir: &Path, hash: &str) -> PathBuf {
    let digest = asset_object_digest(hash);
    assets_dir
        .join("objects")
        .join(&digest[..2])
        .join(digest)
}

fn asset_object_url(hash: &str) -> String {
    let digest = asset_object_digest(hash);
    format!(
        "https://resources.download.minecraft.net/{}/{}",
        &digest[..2],
        digest
    )
}

fn asset_object_present(assets_dir: &Path, obj: &AssetIndexObject) -> bool {
    let path = asset_object_path(assets_dir, &obj.hash);
    fs::metadata(&path)
        .map(|meta| meta.len() == obj.size)
        .unwrap_or(false)
}

async fn ensure_asset_index(
    client: &reqwest::Client,
    assets_dir: &Path,
    index: &PatchAssetIndex,
) -> Result<PathBuf, String> {
    let indexes_dir = assets_dir.join("indexes");
    fs::create_dir_all(&indexes_dir).map_err(|e| e.to_string())?;
    let index_path = indexes_dir.join(format!("{}.json", index.id));

    if index_path.exists() {
        if let Some(expected) = &index.sha1 {
            if file_sha1(&index_path).ok().as_deref() == Some(expected.as_str()) {
                return Ok(index_path);
            }
        } else {
            return Ok(index_path);
        }
    }

    let url = index
        .url
        .as_deref()
        .ok_or_else(|| format!("asset index {} has no download URL", index.id))?;
    download_file(client, url, &index_path).await?;

    if let Some(expected) = &index.sha1 {
        let actual = file_sha1(&index_path)?;
        if actual != *expected {
            let _ = fs::remove_file(&index_path);
            return Err(format!(
                "asset index {} checksum mismatch (expected {expected}, got {actual})",
                index.id
            ));
        }
    }

    Ok(index_path)
}

async fn ensure_assets(
    client: &reqwest::Client,
    assets_dir: &Path,
    index: &PatchAssetIndex,
    app: &tauri::AppHandle,
    version: &str,
) -> Result<(), String> {
    emit_launch_log(
        app,
        version,
        "system",
        &format!("Syncing Minecraft assets ({})…", index.id),
    );

    let index_path = ensure_asset_index(client, assets_dir, index).await?;
    let raw = fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
    let index_file: AssetIndexFile =
        serde_json::from_str(&raw).map_err(|e| format!("bad asset index {}: {e}", index.id))?;

    let objects: Vec<AssetIndexObject> = index_file.objects.into_values().collect();
    let missing: Vec<AssetIndexObject> = objects
        .iter()
        .filter(|obj| !asset_object_present(assets_dir, obj))
        .cloned()
        .collect();

    if missing.is_empty() {
        emit_launch_log(
            app,
            version,
            "system",
            &format!("Assets up to date ({} objects)", objects.len()),
        );
        return Ok(());
    }

    emit_launch_log(
        app,
        version,
        "system",
        &format!(
            "Downloading {} of {} asset objects…",
            missing.len(),
            objects.len()
        ),
    );

    let results: Vec<Result<(), String>> = futures::stream::iter(missing.into_iter().map(|obj| {
        let client = client.clone();
        let assets_dir = assets_dir.to_path_buf();
        async move {
            let dest = asset_object_path(&assets_dir, &obj.hash);
            let url = asset_object_url(&obj.hash);
            download_file(&client, &url, &dest).await?;
            if !asset_object_present(&assets_dir, &obj) {
                return Err(format!(
                    "asset object {} did not verify after download",
                    asset_object_digest(&obj.hash)
                ));
            }
            Ok(())
        }
    }))
    .buffer_unordered(8)
    .collect()
    .await;

    for result in results {
        result?;
    }

    emit_launch_log(app, version, "system", "Asset sync complete");
    Ok(())
}

async fn ensure_library(
    client: &reqwest::Client,
    pack_dir: &Path,
    entry: &PatchLibraryEntry,
    app: &tauri::AppHandle,
    version: &str,
) -> Result<Option<PathBuf>, String> {
    if !library_allowed(entry.rules.as_deref()) || is_native_only(entry) {
        return Ok(None);
    }
    if let Some(path) = find_library(pack_dir, entry) {
        return Ok(Some(path));
    }
    let spec = parse_gradle_spec(&entry.name)
        .ok_or_else(|| format!("bad library name: {}", entry.name))?;
    let url = resolve_library_download_url(entry, &spec)
        .ok_or_else(|| format!("no download URL for {}", entry.name))?;
    let dest = if entry.mmc_hint.as_deref() == Some("local") {
        pack_dir.join("libraries").join(spec.filename())
    } else {
        pack_dir.join("libraries").join(spec.storage_path())
    };
    emit_launch_log(
        app,
        version,
        "system",
        &format!("Downloading {}", entry.name),
    );
    download_file(client, &url, &dest).await?;
    Ok(Some(dest))
}

async fn ensure_main_jar(
    client: &reqwest::Client,
    pack_dir: &Path,
    main_jar: &PatchMainJar,
    mc_version: &str,
    app: &tauri::AppHandle,
    version: &str,
) -> Result<Option<PathBuf>, String> {
    let dest = pack_dir
        .join(".minecraft")
        .join("versions")
        .join(mc_version)
        .join(format!("{mc_version}.jar"));
    if dest.exists() {
        return Ok(Some(dest));
    }
    let url = main_jar
        .downloads
        .artifact
        .as_ref()
        .map(|a| a.url.as_str())
        .ok_or("minecraft mainJar has no download URL")?;
    emit_launch_log(
        app,
        version,
        "system",
        &format!("Downloading Minecraft {mc_version}"),
    );
    download_file(client, url, &dest).await?;
    Ok(Some(dest))
}

// ── App State ──

struct AppState {
    http: reqwest::Client,
    running_processes: HashMap<String, Arc<Mutex<Child>>>,
    delete_cancel: HashMap<String, Arc<AtomicBool>>,
    update_in_progress: HashSet<String>,
    reinstall_in_progress: HashSet<String>,
    copy_in_progress: HashSet<String>,
}

struct RunningInstanceGuard<'a> {
    state: &'a State<'a, Mutex<AppState>>,
    id: String,
}

impl Drop for RunningInstanceGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.running_processes.remove(&self.id);
        }
    }
}

// ── Helpers ──

fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("industrialis-launcher")
}

fn instances_dir() -> PathBuf {
    data_dir().join("instances")
}

fn is_valid_instance_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    path.join("mmc-pack.json").exists() || nested_pack_dir(path).is_some()
}

fn list_instance_ids() -> Result<HashSet<String>, String> {
    let dir = instances_dir();
    if !dir.exists() {
        return Ok(HashSet::new());
    }
    let mut ids = HashSet::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if is_valid_instance_dir(&path) {
            ids.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(ids)
}

pub(crate) fn instance_dir(id: &str) -> PathBuf {
    instances_dir().join(sanitize_name(id))
}

pub(crate) fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_whitespace() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn validate_instance_id(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("instance id cannot be empty".into());
    }
    let id = sanitize_name(trimmed);
    if id.is_empty() {
        return Err("instance id cannot be empty".into());
    }
    Ok(id)
}

/// Offline-mode UUID — matches Minecraft's `OfflinePlayer:<name>` convention.
fn pick_launch_account<'a>(
    settings: &InstanceSettings,
    accounts: &'a [AccountData],
    default_account_id: Option<&str>,
) -> Result<&'a AccountData, String> {
    if accounts.is_empty() {
        return Err("Add an account in Accounts before launching.".into());
    }
    if settings.override_account {
        if let Some(id) = settings.account_id.as_ref() {
            return accounts
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| "The account selected in instance settings was not found.".into());
        }
    }
    if let Some(id) = default_account_id {
        if let Some(acc) = accounts.iter().find(|a| a.id == id) {
            return Ok(acc);
        }
    }
    if accounts.len() == 1 {
        return Ok(&accounts[0]);
    }
    Err("Set a default account in Accounts before launching.".into())
}

fn settings_path(id: &str) -> PathBuf {
    instance_dir(id).join("instance.json")
}

const INSTANCE_ICON_BASENAME: &str = "instance-icon";
const MAX_INSTANCE_ICON_BYTES: u64 = 4 * 1024 * 1024;

fn allowed_icon_extensions() -> &'static [&'static str] {
    &["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"]
}

fn resolve_instance_icon_path(id: &str, settings: &InstanceSettings) -> Option<String> {
    let dir = instance_dir(id);
    if let Some(filename) = settings.custom_icon.as_ref().filter(|name| !name.is_empty()) {
        let path = dir.join(filename);
        if path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    if !dir.exists() {
        return None;
    }
    let entries = fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(INSTANCE_ICON_BASENAME) {
            continue;
        }
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())?;
        if allowed_icon_extensions().contains(&ext.as_str()) {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

fn validate_icon_source(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err("image file not found".into());
    }
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_INSTANCE_ICON_BYTES {
        return Err(format!(
            "image must be under {} MB",
            MAX_INSTANCE_ICON_BYTES / 1024 / 1024
        ));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or("image must have a file extension")?;
    if !allowed_icon_extensions().contains(&ext.as_str()) {
        return Err(format!(
            "unsupported image type (.{ext}); use PNG, JPG, WebP, GIF, BMP, or ICO"
        ));
    }
    Ok(ext)
}

fn clear_instance_icon_files(id: &str) {
    let dir = instance_dir(id);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(INSTANCE_ICON_BASENAME) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn console_log_path(id: &str) -> PathBuf {
    instance_dir(id).join("console.log")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchLogLine {
    pub stream: String,
    pub line: String,
}

fn java_from_home(home: &str) -> Option<String> {
    let home = home.trim();
    if home.is_empty() {
        return None;
    }
    let bin = PathBuf::from(home).join("bin");
    let candidates = if cfg!(windows) {
        vec![bin.join("java.exe"), bin.join("java")]
    } else {
        vec![bin.join("java")]
    };
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn java_from_path() -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        let candidates = if cfg!(windows) {
            vec![
                PathBuf::from(dir).join("java.exe"),
                PathBuf::from(dir).join("java"),
            ]
        } else {
            vec![PathBuf::from(dir).join("java")]
        };
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn java_path() -> Option<String> {
    std::env::var("JAVA_HOME")
        .ok()
        .and_then(|jh| java_from_home(&jh))
        .or_else(java_from_path)
}

fn resolve_java(settings: &InstanceSettings) -> Result<String, String> {
    if settings.override_java_location {
        if let Some(ref custom) = settings.java_path {
            let custom = custom.trim();
            if !custom.is_empty() {
                let path = PathBuf::from(custom);
                if path.exists() {
                    return Ok(path.to_string_lossy().to_string());
                }
                return Err(format!("configured Java not found: {custom}"));
            }
        }
    }
    java_path().ok_or_else(|| {
        "no Java configured or found — set JAVA_HOME or pick a Java in instance settings".into()
    })
}

fn substitute_command_vars(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("${key}"), value);
    }
    out
}

fn split_command_args(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in command.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

fn instance_command_vars(
    version: &str,
    settings: &InstanceSettings,
    inst_dir: &Path,
    java: &str,
) -> HashMap<&'static str, String> {
    let mc_dir = inst_dir.join(".minecraft");
    HashMap::from([
        ("INST_NAME", settings.name.clone()),
        ("INST_ID", version.to_string()),
        ("INST_DIR", inst_dir.to_string_lossy().to_string()),
        ("INST_MC_DIR", mc_dir.to_string_lossy().to_string()),
        ("INST_JAVA", java.to_string()),
    ])
}



fn record_play_time(version: &str, elapsed_secs: u64) {
    if elapsed_secs == 0 {
        return;
    }
    let Some(mut settings) = load_settings(version) else {
        return;
    };
    if !(settings.override_game_time && settings.record_game_time) {
        return;
    }
    settings.total_play_seconds = settings.total_play_seconds.saturating_add(elapsed_secs);
    let _ = save_settings_file(version, &settings);
}

fn run_shell_command(command: &str, work_dir: &Path, extra_env: &HashMap<String, String>) -> Result<(), String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", trimmed]);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", trimmed]);
        c
    };
    cmd.current_dir(work_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run command ({trimmed}): {e}"))?;
    if !status.success() {
        return Err(format!("command failed ({trimmed}): exit {status}"));
    }
    Ok(())
}

#[tauri::command]
fn browse_java_executable() -> Result<Option<String>, String> {
    let mut dialog = rfd::FileDialog::new();
    #[cfg(windows)]
    {
        dialog = dialog.add_filter("Java executable", &["exe"]);
    }
    Ok(dialog
        .pick_file()
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
fn test_java(path_override: Option<String>) -> Result<String, String> {
    let java = if let Some(path) = path_override.filter(|p| !p.trim().is_empty()) {
        path
    } else {
        java_path().ok_or("no Java configured or found — set JAVA_HOME or pick a Java path")?
    };
    let output = std::process::Command::new(&java)
        .arg("-version")
        .output()
        .map_err(|e| format!("failed to run Java ({java}): {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.to_string()
    } else {
        format!("{stderr}{stdout}")
    };
    if !output.status.success() {
        return Err(format!("Java test failed ({java}):\n{combined}"));
    }
    Ok(format!("OK — {java}\n{combined}"))
}

// ── Commands ──

#[tauri::command]
async fn get_versions(
    state: State<'_, Mutex<AppState>>,
) -> Result<HashMap<String, GtnhVersion>, String> {
    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);
    let url = "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json";
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let versions: HashMap<String, GtnhVersion> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(versions)
}

#[tauri::command]
async fn get_instances() -> Result<Vec<InstanceInfo>, String> {
    let dir = instances_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let known_ids = list_instance_ids()?;
    let mut instances = vec![];
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let id = entry.file_name().to_string_lossy().to_string();
        if !known_ids.contains(&id) {
            continue;
        }
        let settings = load_settings(&id).unwrap_or_default();
        let group = groups::get_instance_group(&dir, &id, &known_ids);
        let icon_path = resolve_instance_icon_path(&id, &settings);
        instances.push(InstanceInfo {
            id,
            installed: true,
            size_bytes: settings.cached_size_bytes,
            settings,
            group,
            icon_path,
        });
    }
    Ok(instances)
}

fn refresh_cached_size(id: &str) -> Result<u64, String> {
    let inst_dir = instance_dir(id);
    if !inst_dir.exists() {
        return Ok(0);
    }
    let size = dir_size(&inst_dir);
    if let Some(mut settings) = load_settings(id) {
        settings.cached_size_bytes = size;
        save_settings_file(id, &settings)?;
    }
    Ok(size)
}

#[tauri::command]
async fn refresh_instance_sizes(ids: Option<Vec<String>>) -> Result<HashMap<String, u64>, String> {
    let target_ids: Vec<String> = match ids {
        Some(list) => list
            .into_iter()
            .map(|id| sanitize_name(id.trim()))
            .collect(),
        None => list_instance_ids()?.into_iter().collect(),
    };
    tokio::task::spawn_blocking(move || {
        let mut sizes = HashMap::new();
        for id in target_ids {
            if let Ok(size) = refresh_cached_size(&id) {
                sizes.insert(id, size);
            }
        }
        sizes
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_instance_groups() -> Result<groups::InstanceGroupsState, String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    Ok(groups::get_groups_state(&dir, &known_ids))
}

#[tauri::command]
async fn set_instance_group(id: String, group: String) -> Result<(), String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::set_instance_group(&dir, &id, &group, &known_ids)
}

#[tauri::command]
async fn rename_group(old_name: String, new_name: String) -> Result<(), String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::rename_group(&dir, &old_name, &new_name, &known_ids)
}

#[tauri::command]
async fn delete_group(name: String) -> Result<(), String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::delete_group(&dir, &name, &known_ids)
}

#[tauri::command]
async fn move_instance_in_group(id: String, direction: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::move_instance_in_group(&dir, &id, &direction, &known_ids)
}

#[tauri::command]
async fn set_group_instance_order(group: String, order: Vec<String>) -> Result<(), String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::set_group_instance_order(&dir, &group, &order, &known_ids)
}

#[tauri::command]
async fn set_group_collapsed(group: String, collapsed: bool) -> Result<(), String> {
    let dir = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::set_group_collapsed(&dir, &group, collapsed, &known_ids)
}

fn dir_size(path: &Path) -> u64 {
    fn walk(p: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(p) {
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    total += walk(&path);
                } else if let Ok(meta) = fs::metadata(&path) {
                    total += meta.len();
                }
            }
        }
        total
    }
    walk(path)
}

fn load_settings(id: &str) -> Option<InstanceSettings> {
    let path = settings_path(id);
    let mut settings: InstanceSettings = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())?;
    if settings.pack_version.is_empty() {
        settings.pack_version = id.to_string();
        let _ = save_settings_file(id, &settings);
    }
    Some(settings)
}

fn save_settings_file(id: &str, settings: &InstanceSettings) -> Result<(), String> {
    let path = settings_path(id);
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_accounts() -> Vec<AccountData> {
    auth::load_accounts(&data_dir())
}

fn save_accounts(accounts: &[AccountData]) -> Result<(), String> {
    auth::save_accounts(&data_dir(), accounts)
}

fn load_launcher_settings() -> LauncherSettings {
    settings::load_launcher_settings(&data_dir())
}

fn write_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    settings::write_launcher_settings(&data_dir(), settings)
}

/// Moves per-instance `username` values into launcher accounts and sets instance overrides.
fn migrate_legacy_instance_usernames() {
    let instances_root = instances_dir();
    if !instances_root.exists() {
        return;
    }
    let Ok(known_ids) = list_instance_ids() else {
        return;
    };

    let data = data_dir();

    for id in known_ids {
        let Some(mut settings) = load_settings(&id) else {
            continue;
        };
        let username = settings.username.trim();
        if username.is_empty() {
            continue;
        }

        let Ok(account) = auth::find_or_create_offline_account(&data, username) else {
            continue;
        };

        settings.override_account = true;
        settings.account_id = Some(account.id);
        settings.username.clear();
        settings.offline_username_confirmed = false;
        let _ = save_settings_file(&id, &settings);
    }

    let mut launcher_settings = load_launcher_settings();
    if launcher_settings.default_account_id.is_none() {
        let accounts = load_accounts();
        if accounts.len() == 1 {
            launcher_settings.default_account_id = Some(accounts[0].id.clone());
            let _ = write_launcher_settings(&launcher_settings);
        }
    }
}

#[tauri::command]
async fn save_settings(id: String, settings: InstanceSettings) -> Result<(), String> {
    save_settings_file(&id, &settings)
}

#[tauri::command]
async fn get_settings(id: String) -> Result<InstanceSettings, String> {
    Ok(load_settings(&id).unwrap_or_default())
}

fn collect_delete_paths(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            collect_delete_paths(&entry.path(), out)?;
        }
    }
    out.push(path.to_path_buf());
    Ok(())
}

fn emit_delete_progress(app: &tauri::AppHandle, id: &str, pct: f64) {
    emit(
        app,
        "dl-progress",
        &serde_json::json!({
            "stage": "deleting",
            "operation": "delete",
            "pct": pct,
            "id": id,
        }),
    );
}

fn run_delete_instance(
    app: &tauri::AppHandle,
    id: &str,
    cancel_flag: &AtomicBool,
) -> Result<(), String> {
    let dir = instance_dir(id);
    let mut paths = Vec::new();
    if dir.exists() {
        collect_delete_paths(&dir, &mut paths)?;
    }

    let total = paths.len().max(1);
    let mut last_emitted_pct = -1i32;
    emit_delete_progress(app, id, 0.0);

    for (i, path) in paths.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err("deletion cancelled".into());
        }

        if path.is_dir() {
            fs::remove_dir(path).map_err(|e| {
                format!("failed to remove directory {}: {e}", path.display())
            })?;
        } else {
            fs::remove_file(path).map_err(|e| {
                format!("failed to remove file {}: {e}", path.display())
            })?;
        }

        let pct = (i + 1) as f64 / total as f64;
        let pct_ui = (pct * 100.0) as i32;
        if i + 1 == total || pct_ui / 5 > last_emitted_pct / 5 {
            emit_delete_progress(app, id, pct);
            last_emitted_pct = pct_ui;
        }
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Err("deletion cancelled".into());
    }

    let instances_root = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::remove_instance_from_groups(&instances_root, id, &known_ids)?;

    emit(
        app,
        "dl-progress",
        &serde_json::json!({
            "stage": "done",
            "operation": "delete",
            "pct": 1.0,
            "id": id,
        }),
    );
    Ok(())
}

#[tauri::command]
async fn delete_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.delete_cancel.insert(id.clone(), cancel_flag.clone());
    }

    let app_bg = app.clone();
    let id_bg = id.clone();
    let result = tokio::task::spawn_blocking(move || run_delete_instance(&app_bg, &id_bg, &cancel_flag))
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.delete_cancel.remove(&id);
    }

    result
}

#[tauri::command]
fn cancel_delete_instance(state: State<'_, Mutex<AppState>>, id: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let guard = state.lock().map_err(|e| e.to_string())?;
    let flag = guard
        .delete_cancel
        .get(&id)
        .ok_or("no deletion in progress for this instance")?;
    flag.store(true, Ordering::Relaxed);
    Ok(())
}

fn collect_copy_files(base: &Path, path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            collect_copy_files(base, &entry.path(), out)?;
        }
    } else {
        out.push(
            path.strip_prefix(base)
                .map_err(|_| "failed to resolve copy path".to_string())?
                .to_path_buf(),
        );
    }
    Ok(())
}

fn emit_copy_progress(app: &tauri::AppHandle, id: &str, name: &str, pct: f64) {
    emit(
        app,
        "dl-progress",
        &serde_json::json!({
            "stage": "copying",
            "operation": "copy",
            "pct": pct,
            "id": id,
            "name": name,
        }),
    );
}

fn run_copy_instance(
    app: &tauri::AppHandle,
    source_id: &str,
    new_id: &str,
    new_name: &str,
) -> Result<(), String> {
    let src_dir = instance_dir(source_id);
    if !is_valid_instance_dir(&src_dir) {
        return Err("source instance not found".into());
    }

    let dest_dir = instance_dir(new_id);
    if dest_dir.exists() {
        return Err("an instance with that id already exists".into());
    }

    let mut files = Vec::new();
    collect_copy_files(&src_dir, &src_dir, &mut files)?;

    let total = files.len().max(1);
    let mut last_emitted_pct = -1i32;
    emit_copy_progress(app, new_id, new_name, 0.0);

    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    for (i, rel) in files.iter().enumerate() {
        let src = src_dir.join(rel);
        let dest = dest_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&src, &dest).map_err(|e| {
            format!(
                "failed to copy {} -> {}: {e}",
                src.display(),
                dest.display()
            )
        })?;

        let pct = (i + 1) as f64 / total as f64;
        let pct_ui = (pct * 100.0) as i32;
        if i + 1 == total || pct_ui / 5 > last_emitted_pct / 5 {
            emit_copy_progress(app, new_id, new_name, pct);
            last_emitted_pct = pct_ui;
        }
    }

    let mut settings = load_settings(new_id).ok_or("copied instance settings missing")?;
    settings.name = new_name.to_string();
    settings.cached_size_bytes = dir_size(&dest_dir);
    save_settings_file(new_id, &settings)?;

    let instances_root = instances_dir();
    let known_ids = list_instance_ids()?;
    let source_group = groups::get_instance_group(&instances_root, source_id, &known_ids);
    if !source_group.is_empty() {
        groups::set_instance_group(&instances_root, new_id, &source_group, &known_ids)?;
    }

    emit(
        app,
        "dl-progress",
        &serde_json::json!({
            "stage": "done",
            "operation": "copy",
            "pct": 1.0,
            "id": new_id,
            "name": new_name,
        }),
    );
    Ok(())
}

#[tauri::command]
async fn copy_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    source_id: String,
    new_id: String,
    new_name: String,
) -> Result<(), String> {
    let source_id = sanitize_name(source_id.trim());
    let new_id = validate_instance_id(&new_id)?;
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("instance name cannot be empty".into());
    }
    if source_id == new_id {
        return Err("new instance id must differ from the source".into());
    }

    {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running_processes.contains_key(&source_id) {
            return Err("cannot copy while instance is running".into());
        }
        if guard.copy_in_progress.contains(&source_id) {
            return Err("copy already in progress for this instance".into());
        }
    }

    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&source_id) {
        return Err("source instance not found".into());
    }
    if known_ids.contains(&new_id) {
        return Err("an instance with that id already exists".into());
    }

    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.copy_in_progress.insert(source_id.clone());
    }

    let app_bg = app.clone();
    let source_bg = source_id.clone();
    let new_id_bg = new_id.clone();
    let new_name_bg = new_name.clone();
    let copy_result = tokio::task::spawn_blocking(move || {
        run_copy_instance(&app_bg, &source_bg, &new_id_bg, &new_name_bg)
    })
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.copy_in_progress.remove(&source_id);
    }

    if copy_result.is_err() {
        let partial = instance_dir(&new_id);
        if partial.exists() {
            let _ = fs::remove_dir_all(&partial);
        }
    }

    copy_result
}

#[tauri::command]
fn open_instance_folder(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let inst_dir = instance_dir(&id);
    if !inst_dir.exists() {
        return Err("instance not installed".into());
    }
    flatten_nested_pack(&inst_dir)?;
    let dir = mmc_pack_dir(&inst_dir);
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("failed to open instance folder: {e}"))
}

#[tauri::command]
async fn download_install(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
    pack_version: String,
    java_type: String,
    group: Option<String>,
    name: Option<String>,
) -> Result<(), String> {
    let id = validate_instance_id(&id)?;
    let known_ids = list_instance_ids()?;
    if known_ids.contains(&id) {
        return Err("an instance with that id already exists".into());
    }

    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);

    let inst_dir = instance_dir(&id);
    fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;

    pack::download_and_extract_to_staging(
        &app,
        &client,
        &pack_version,
        &java_type,
        &inst_dir,
        "install",
        Some(&id),
    )
    .await?;

    let staging = inst_dir.join("staging");
    for entry in fs::read_dir(&staging).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = inst_dir.join(entry.file_name());
        if dest.exists() {
            if dest.is_dir() {
                fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&dest).map_err(|e| e.to_string())?;
            }
        }
        if entry.path().is_dir() {
            pack::copy_tree_merge(&entry.path(), &dest)?;
        } else {
            pack::copy_file_create_parent(&entry.path(), &dest)?;
        }
    }
    fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;

    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, true)?;

    let display_name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("GTNH {pack_version}"));
    let settings = InstanceSettings {
        name: display_name,
        pack_version: pack_version.clone(),
        pack_java_type: java_type.clone(),
        ..Default::default()
    };
    save_settings_file(&id, &settings)?;
    let _ = refresh_cached_size(&id);

    if let Some(group) = group {
        let instances_root = instances_dir();
        let known_ids = list_instance_ids()?;
        groups::set_instance_group(&instances_root, &id, &group, &known_ids)?;
    }

    emit(
        &app,
        "dl-progress",
        &serde_json::json!({"stage": "done", "pct": 1.0, "id": id, "operation": "install"}),
    );
    Ok(())
}

#[tauri::command]
async fn preview_update_mods(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
    pack_version: String,
    java_type: String,
) -> Result<pack::UpdateModPreview, String> {
    let id = sanitize_name(id.trim());
    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&id) {
        return Err("instance not found".into());
    }

    let inst_dir = instance_dir(&id);
    let settings = load_settings(&id).unwrap_or_default();
    let current = if settings.pack_version.is_empty() {
        id.clone()
    } else {
        settings.pack_version.clone()
    };

    let client = state.lock().map_err(|e| e.to_string())?.http.clone();

    let preview_dir = inst_dir.join(".update-preview");
    if preview_dir.exists() {
        fs::remove_dir_all(&preview_dir).map_err(|e| e.to_string())?;
    }

    pack::emit_dl_progress(
        &app,
        "preview",
        0.0,
        "preview",
        Some(&id),
        Some(&format!(
            "Preparing mod analysis: {current} → {pack_version}"
        )),
    );

    let staging = pack::download_and_extract_to_staging(
        &app,
        &client,
        &pack_version,
        &java_type,
        &preview_dir,
        "preview",
        Some(&id),
    )
    .await?;

    pack::emit_dl_progress(
        &app,
        "preview",
        0.85,
        "preview",
        Some(&id),
        Some("Scanning mods in current instance…"),
    );
    let old_mods = pack::list_mods_in_dir(&pack::resolve_mods_dir(&inst_dir))?;
    pack::emit_dl_progress(
        &app,
        "preview",
        0.88,
        "preview",
        Some(&id),
        Some(&format!("Found {} mod(s) in current instance", old_mods.len())),
    );

    pack::emit_dl_progress(
        &app,
        "preview",
        0.9,
        "preview",
        Some(&id),
        Some("Scanning mods in target pack…"),
    );
    let new_mods = pack::list_mods_in_dir(&pack::resolve_mods_dir(&staging))?;
    pack::emit_dl_progress(
        &app,
        "preview",
        0.93,
        "preview",
        Some(&id),
        Some(&format!("Found {} mod(s) in target pack", new_mods.len())),
    );

    let persistent_mods = inst_dir.join("persistent-minecraft").join("mods");

    pack::emit_dl_progress(
        &app,
        "preview",
        0.96,
        "preview",
        Some(&id),
        Some("Comparing mod lists and detecting custom mods…"),
    );
    let preview = pack::build_update_preview(
        &old_mods,
        &new_mods,
        &persistent_mods,
        &current,
        &pack_version,
    );

    pack::emit_dl_progress(
        &app,
        "preview",
        1.0,
        "preview",
        Some(&id),
        Some(&format!(
            "Analysis complete: {} custom, {} updated, {} new from pack",
            preview.custom_mods.len(),
            preview.updated_pack_mods_count,
            preview.new_pack_mods_count
        )),
    );

    fs::remove_dir_all(&preview_dir).ok();
    Ok(preview)
}

async fn run_update_instance(
    app: tauri::AppHandle,
    client: reqwest::Client,
    id: String,
    pack_version: String,
    java_type: String,
    keep_mod_identities: Vec<String>,
) -> Result<(), String> {
    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&id) {
        return Err("instance not found".into());
    }

    let inst_dir = instance_dir(&id);
    let preserve_dir = inst_dir.join(migration::preserve_dir_name());
    let keep_set: HashSet<String> = keep_mod_identities.into_iter().collect();

    pack::emit_dl_progress(
        &app,
        "updating",
        0.02,
        "update-pack",
        Some(&id),
        Some(&format!("Starting pack update to {pack_version}")),
    );

    let persistent_mods = pack::persistent_custom_mods_dir(&inst_dir);
    let removed_custom = pack::remove_custom_mods_except(&persistent_mods, &keep_set)?;
    if removed_custom > 0 {
        pack::emit_dl_progress(
            &app,
            "updating",
            0.05,
            "update-pack",
            Some(&id),
            Some(&format!(
                "Removed {removed_custom} custom mod(s) not selected to keep"
            )),
        );
    }

    pack::emit_dl_progress(
        &app,
        "updating",
        0.08,
        "update-pack",
        Some(&id),
        Some("Backing up saves, JourneyMap, and player settings"),
    );
    migration::backup_player_data(&inst_dir, &preserve_dir)?;

    pack::emit_dl_progress(
        &app,
        "updating",
        0.12,
        "update-pack",
        Some(&id),
        Some("Removing old pack files"),
    );
    migration::wipe_instance_for_reinstall(&inst_dir, &preserve_dir)?;
    fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;

    let staging = pack::download_and_extract_to_staging(
        &app,
        &client,
        &pack_version,
        &java_type,
        &inst_dir,
        "update-pack",
        Some(&id),
    )
    .await?;

    pack::emit_dl_progress(
        &app,
        "updating",
        0.75,
        "update-pack",
        Some(&id),
        Some("Installing fresh pack files"),
    );
    pack::install_staging_contents(&staging, &inst_dir)?;
    fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;

    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, true)?;

    pack::emit_dl_progress(
        &app,
        "updating",
        0.88,
        "update-pack",
        Some(&id),
        Some("Restoring saves and player data"),
    );
    migration::restore_player_data(&inst_dir, &preserve_dir)?;

    if let Some(mut settings) = load_settings(&id) {
        settings.pack_version = pack_version;
        settings.pack_java_type = java_type;
        save_settings_file(&id, &settings)?;
    }

    pack::apply_persistent_custom_mods(&inst_dir)?;
    minecraft_files::apply_persistent_minecraft(&inst_dir)?;
    let _ = refresh_cached_size(&id);

    fs::remove_dir_all(&preserve_dir).ok();

    pack::emit_dl_progress(
        &app,
        "done",
        1.0,
        "update-pack",
        Some(&id),
        Some("Update complete"),
    );
    Ok(())
}

async fn run_reinstall_instance(
    app: tauri::AppHandle,
    client: reqwest::Client,
    id: String,
    pack_version: String,
    java_type: String,
) -> Result<(), String> {
    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&id) {
        return Err("instance not found".into());
    }

    let inst_dir = instance_dir(&id);
    let preserve_dir = inst_dir.join(migration::preserve_dir_name());

    pack::emit_dl_progress(
        &app,
        "reinstalling",
        0.05,
        "reinstall",
        Some(&id),
        Some("Backing up saves, JourneyMap, and player settings"),
    );
    migration::backup_player_data(&inst_dir, &preserve_dir)?;

    pack::emit_dl_progress(
        &app,
        "reinstalling",
        0.1,
        "reinstall",
        Some(&id),
        Some("Removing old pack files"),
    );
    migration::wipe_instance_for_reinstall(&inst_dir, &preserve_dir)?;
    fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;

    let staging = pack::download_and_extract_to_staging(
        &app,
        &client,
        &pack_version,
        &java_type,
        &inst_dir,
        "reinstall",
        Some(&id),
    )
    .await?;

    pack::emit_dl_progress(
        &app,
        "reinstalling",
        0.75,
        "reinstall",
        Some(&id),
        Some("Installing fresh pack files"),
    );
    pack::install_staging_contents(&staging, &inst_dir)?;
    fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;

    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, true)?;

    pack::emit_dl_progress(
        &app,
        "reinstalling",
        0.88,
        "reinstall",
        Some(&id),
        Some("Restoring saves and player data"),
    );
    migration::restore_player_data(&inst_dir, &preserve_dir)?;

    if let Some(mut settings) = load_settings(&id) {
        settings.pack_version = pack_version;
        settings.pack_java_type = java_type;
        save_settings_file(&id, &settings)?;
    }

    pack::apply_persistent_custom_mods(&inst_dir)?;
    minecraft_files::apply_persistent_minecraft(&inst_dir)?;
    let _ = refresh_cached_size(&id);

    fs::remove_dir_all(&preserve_dir).ok();

    pack::emit_dl_progress(
        &app,
        "done",
        1.0,
        "reinstall",
        Some(&id),
        Some("Clean reinstall complete"),
    );
    Ok(())
}

#[tauri::command]
async fn update_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
    pack_version: String,
    java_type: String,
    keep_mod_identities: Vec<String>,
) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let client = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running_processes.contains_key(&id) {
            return Err("cannot update while instance is running".into());
        }
        if guard.update_in_progress.contains(&id) {
            return Err("update already in progress for this instance".into());
        }
        if guard.reinstall_in_progress.contains(&id) {
            return Err("reinstall already in progress for this instance".into());
        }
        guard.update_in_progress.insert(id.clone());
        guard.http.clone()
    };

    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&id) {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.update_in_progress.remove(&id);
        return Err("instance not found".into());
    }

    let app_bg = app.clone();
    let id_bg = id.clone();
    tokio::spawn(async move {
        let result = run_update_instance(
            app_bg.clone(),
            client,
            id_bg.clone(),
            pack_version,
            java_type,
            keep_mod_identities,
        )
        .await;

        if let Ok(mut guard) = app_bg.state::<Mutex<AppState>>().lock() {
            guard.update_in_progress.remove(&id_bg);
        }

        if let Err(e) = result {
            pack::emit_dl_progress(
                &app_bg,
                "failed",
                0.0,
                "update-pack",
                Some(&id_bg),
                Some(&format!("Error: {e}")),
            );
        }
    });

    Ok(())
}

#[tauri::command]
async fn reinstall_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
    pack_version: String,
    java_type: String,
) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let client = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running_processes.contains_key(&id) {
            return Err("cannot reinstall while instance is running".into());
        }
        if guard.update_in_progress.contains(&id) {
            return Err("pack update already in progress for this instance".into());
        }
        if guard.reinstall_in_progress.contains(&id) {
            return Err("clean reinstall already in progress for this instance".into());
        }
        guard.reinstall_in_progress.insert(id.clone());
        guard.http.clone()
    };

    let known_ids = list_instance_ids()?;
    if !known_ids.contains(&id) {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.reinstall_in_progress.remove(&id);
        return Err("instance not found".into());
    }

    let app_bg = app.clone();
    let id_bg = id.clone();
    tokio::spawn(async move {
        let result = run_reinstall_instance(
            app_bg.clone(),
            client,
            id_bg.clone(),
            pack_version,
            java_type,
        )
        .await;

        if let Ok(mut guard) = app_bg.state::<Mutex<AppState>>().lock() {
            guard.reinstall_in_progress.remove(&id_bg);
        }

        if let Err(e) = result {
            pack::emit_dl_progress(
                &app_bg,
                "failed",
                0.0,
                "reinstall",
                Some(&id_bg),
                Some(&format!("Error: {e}")),
            );
        }
    });

    Ok(())
}

#[tauri::command]
fn list_minecraft_entries(id: String, subpath: Option<String>) -> Result<Vec<minecraft_files::MinecraftDirEntry>, String> {
    let id = sanitize_name(id.trim());
    let inst_dir = instance_dir(&id);
    minecraft_files::list_minecraft_entries(&inst_dir, subpath.as_deref().unwrap_or(""))
}

#[tauri::command]
fn read_minecraft_file(id: String, rel_path: String) -> Result<String, String> {
    let id = sanitize_name(id.trim());
    minecraft_files::read_minecraft_file(&instance_dir(&id), &rel_path)
}

#[tauri::command]
fn write_minecraft_file(
    id: String,
    rel_path: String,
    content: String,
    persist: bool,
) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    minecraft_files::write_minecraft_file(&instance_dir(&id), &rel_path, &content, persist)
}

#[tauri::command]
fn delete_persistent_file(id: String, rel_path: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    minecraft_files::delete_persistent_file(&instance_dir(&id), &rel_path)
}

#[tauri::command]
fn list_persistent_files(id: String) -> Result<Vec<String>, String> {
    let id = sanitize_name(id.trim());
    minecraft_files::list_persistent_files(&instance_dir(&id))
}

#[tauri::command]
fn list_custom_mods(id: String) -> Result<Vec<pack::ModEntry>, String> {
    let id = sanitize_name(id.trim());
    pack::list_custom_mods(&instance_dir(&id))
}

#[tauri::command]
fn browse_custom_mod() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .add_filter("Minecraft mods", &["jar", "zip"])
        .pick_file()
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
fn browse_instance_icon_file() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"],
        )
        .pick_file()
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
fn set_instance_icon(id: String, source_path: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    let source = PathBuf::from(source_path.trim());
    let ext = validate_icon_source(&source)?;
    let dest_name = format!("{INSTANCE_ICON_BASENAME}.{ext}");
    let dest = instance_dir(&id).join(&dest_name);
    fs::create_dir_all(instance_dir(&id)).map_err(|e| e.to_string())?;
    clear_instance_icon_files(&id);
    fs::copy(&source, &dest).map_err(|e| format!("failed to copy icon: {e}"))?;
    let mut settings = load_settings(&id).unwrap_or_default();
    settings.custom_icon = Some(dest_name);
    save_settings_file(&id, &settings)
}

#[tauri::command]
fn clear_instance_icon(id: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    clear_instance_icon_files(&id);
    if let Some(mut settings) = load_settings(&id) {
        settings.custom_icon = None;
        save_settings_file(&id, &settings)?;
    }
    Ok(())
}

#[tauri::command]
fn add_custom_mod(id: String, source_path: String) -> Result<pack::ModEntry, String> {
    let id = sanitize_name(id.trim());
    let source = PathBuf::from(source_path.trim());
    if !source.is_file() {
        return Err("mod file not found".into());
    }
    pack::add_custom_mod(&instance_dir(&id), &source)
}

#[tauri::command]
fn remove_custom_mod(id: String, identity: String) -> Result<(), String> {
    let id = sanitize_name(id.trim());
    pack::remove_custom_mod(&instance_dir(&id), &identity)
}

pub(crate) fn emit<T: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: &T) {
    let _ = app.emit(event, payload);
}

fn persist_console_log(id: &str, stream: &str, line: &str) {
    let entry = LaunchLogLine {
        stream: stream.to_string(),
        line: line.to_string(),
    };
    let Ok(json) = serde_json::to_string(&entry) else {
        return;
    };
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(console_log_path(id))
    {
        let _ = writeln!(file, "{json}");
    }
}

fn emit_launch_log(app: &tauri::AppHandle, id: &str, stream: &str, line: &str) {
    persist_console_log(id, stream, line);
    emit(
        app,
        "launch-log",
        &serde_json::json!({ "id": id, "stream": stream, "line": line }),
    );
}

fn pipe_launch_output<R: Read + Send + 'static>(
    reader: R,
    app: tauri::AppHandle,
    version: String,
    stream: &'static str,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            emit_launch_log(&app, &version, stream, &line);
        }
    });
}

fn build_classpath(libraries: &[String]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    libraries.join(sep)
}

fn write_launch_argfile(path: &Path, args: &[String]) -> Result<(), String> {
    let content = args
        .iter()
        .map(|arg| {
            // Classpath is one semicolon-joined string; quote the whole value for argfile readers.
            if arg.contains(';') {
                if arg.contains(' ') || arg.contains('"') {
                    return format!("\"{}\"", arg.replace('"', "\\\""));
                }
                return arg.clone();
            }
            if arg.contains(' ') {
                return format!("\"{}\"", arg.replace('"', "\\\""));
            }
            arg.clone()
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, content).map_err(|e| e.to_string())
}

fn wait_for_launch(
    child: Arc<Mutex<Child>>,
    app: tauri::AppHandle,
    version: String,
) -> Result<i32, String> {
    let mut child = child
        .lock()
        .map_err(|e| format!("process lock failed: {e}"))?;
    if let Some(stdout) = child.stdout.take() {
        pipe_launch_output(stdout, app.clone(), version.clone(), "stdout");
    }
    if let Some(stderr) = child.stderr.take() {
        pipe_launch_output(stderr, app.clone(), version.clone(), "stderr");
    }
    let status = child
        .wait()
        .map_err(|e| format!("process wait failed: {e}"))?;
    let code = status.code().unwrap_or(-1);
    emit_launch_log(
        &app,
        &version,
        "system",
        &format!("Process exited with code {code}"),
    );
    Ok(code)
}

#[tauri::command]
fn get_instance_console_log(id: String) -> Result<Vec<LaunchLogLine>, String> {
    let path = console_log_path(&id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut lines = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<LaunchLogLine>(line) {
            lines.push(entry);
        }
    }
    Ok(lines)
}

#[tauri::command]
fn clear_instance_console_log(id: String) -> Result<(), String> {
    let path = console_log_path(&id);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn detect_java() -> Result<Vec<JavaInfo>, String> {
    let mut found = Vec::new();
    // Try JAVA_HOME
    if let Some(jh) = java_path() {
        if let Some(info) = probe_java(&jh) {
            found.push(info);
        }
    }
    // Try PATH
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(';') {
            let candidate = PathBuf::from(dir).join("java.exe");
            if candidate.exists() {
                if let Some(info) = probe_java(&candidate.to_string_lossy()) {
                    if !found.iter().any(|j: &JavaInfo| j.path == info.path) {
                        found.push(info);
                    }
                }
            }
            let candidate_nix = PathBuf::from(dir).join("java");
            if candidate_nix.exists() {
                let p = candidate_nix.to_string_lossy().to_string();
                if let Some(info) = probe_java(&p) {
                    if !found.iter().any(|j| j.path == info.path) {
                        found.push(info);
                    }
                }
            }
        }
    }
    // Common install paths (Windows)
    for base in [
        "C:\\Program Files\\Java",
        "C:\\Program Files\\Eclipse Adoptium",
        "C:\\Program Files\\Microsoft\\jdk",
    ] {
        if let Ok(entries) = fs::read_dir(base) {
            for e in entries.flatten() {
                let bin = e.path().join("bin").join("java.exe");
                if bin.exists() {
                    let p = bin.to_string_lossy().to_string();
                    if let Some(info) = probe_java(&p) {
                        if !found.iter().any(|j| j.path == info.path) {
                            found.push(info);
                        }
                    }
                }
            }
        }
    }
    // macOS /usr/libexec/java_home
    if cfg!(target_os = "macos") {
        if let Ok(out) = std::process::Command::new("/usr/libexec/java_home").output() {
            if let Ok(home) = String::from_utf8(out.stdout) {
                let p = PathBuf::from(home.trim()).join("bin").join("java");
                let ps = p.to_string_lossy().to_string();
                if let Some(info) = probe_java(&ps) {
                    if !found.iter().any(|j| j.path == info.path) {
                        found.push(info);
                    }
                }
            }
        }
    }
    // Linux /usr/lib/jvm
    if cfg!(target_os = "linux") {
        if let Ok(entries) = fs::read_dir("/usr/lib/jvm") {
            for e in entries.flatten() {
                let bin = e.path().join("bin").join("java");
                if bin.exists() {
                    let p = bin.to_string_lossy().to_string();
                    if let Some(info) = probe_java(&p) {
                        if !found.iter().any(|j| j.path == info.path) {
                            found.push(info);
                        }
                    }
                }
            }
        }
    }
    // ponytail: O(n²) dedup, fine for <20 results
    found.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(found)
}

fn probe_java(path: &str) -> Option<JavaInfo> {
    let output = std::process::Command::new(path)
        .arg("-version")
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Parse "openjdk version \"21.0.3\"" or similar
    let version_str = stderr.lines().next()?;
    let v: u32 = version_str
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .next()?;
    Some(JavaInfo {
        path: path.to_string(),
        version: v,
    })
}

fn nested_pack_dir(inst_dir: &Path) -> Option<PathBuf> {
    if inst_dir.join("mmc-pack.json").exists() {
        return None;
    }
    let mut matches = Vec::new();
    if let Ok(entries) = fs::read_dir(inst_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("mmc-pack.json").exists() {
                matches.push(path);
            }
        }
    }
    if matches.len() > 1 {
        matches.sort_by_key(|path| path.file_name().map(|name| name.to_string_lossy().to_string()));
    }
    matches.into_iter().next()
}

fn move_into(dest_root: &Path, src: &Path) -> Result<(), String> {
    let file_name = src
        .file_name()
        .ok_or_else(|| format!("missing file name for {}", src.display()))?;
    let dest = dest_root.join(file_name);
    if src == dest {
        return Ok(());
    }
    if !src.exists() {
        return Ok(());
    }
    if dest.exists() {
        if src.is_dir() && dest.is_dir() {
            for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                move_into(&dest, &entry.path())?;
            }
            fs::remove_dir_all(src).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if dest.is_dir() {
            fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&dest).map_err(|e| e.to_string())?;
        }
    } else if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if fs::rename(src, &dest).is_err() {
        if src.is_dir() {
            copy_dir_all(src, &dest)?;
            fs::remove_dir_all(src).map_err(|e| e.to_string())?;
        } else {
            fs::copy(src, &dest).map_err(|e| e.to_string())?;
            fs::remove_file(src).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// GTNH zips ship tuned configs at `<instance>/config/`, but our gameDir is
/// `<instance>/.minecraft`. Seed those files into the active config folder.
fn seed_pack_configs(inst_dir: &Path, overwrite: bool) -> Result<(), String> {
    let src = inst_dir.join("config");
    if !src.is_dir() {
        return Ok(());
    }
    let dest = inst_dir.join(".minecraft").join("config");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    seed_pack_config_tree(&src, &dest, overwrite)
}

fn seed_pack_config_tree(src: &Path, dest: &Path, overwrite: bool) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            seed_pack_config_tree(&path, &target, overwrite)?;
        } else if overwrite || !target.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&path, &target).map_err(|e| {
                format!(
                    "failed to seed config {} -> {}: {e}",
                    path.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

fn replace_in_config_file(path: &Path, from: &str, to: &str) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| {
        format!("failed to read config {}: {e}", path.display())
    })?;
    if !content.contains(from) {
        return Ok(());
    }
    let updated = content.replace(from, to);
    fs::write(path, updated).map_err(|e| {
        format!("failed to write config {}: {e}", path.display())
    })
}

/// Dreamcraft's Gadomancy script expects ANCIENT_STONES research, which Gadomancy
/// only registers when ancientStoneRecipes is enabled.
fn apply_gtnh_config_patches(inst_dir: &Path) -> Result<(), String> {
    const GADOMANCY_FALSE: &str = "B:ancientStoneRecipes=false";
    const GADOMANCY_TRUE: &str = "B:ancientStoneRecipes=true";
    for rel in ["config/gadomancy.cfg", ".minecraft/config/gadomancy.cfg"] {
        replace_in_config_file(&inst_dir.join(rel), GADOMANCY_FALSE, GADOMANCY_TRUE)?;
    }
    Ok(())
}

pub(crate) fn prepare_instance_configs(inst_dir: &Path, overwrite_pack_configs: bool) -> Result<(), String> {
    seed_pack_configs(inst_dir, overwrite_pack_configs)?;
    apply_gtnh_config_patches(inst_dir)?;
    Ok(())
}

pub(crate) fn flatten_nested_pack(inst_dir: &Path) -> Result<(), String> {
    let Some(nested) = nested_pack_dir(inst_dir) else {
        return Ok(());
    };
    for entry in fs::read_dir(&nested).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        move_into(inst_dir, &entry.path())?;
    }
    if nested.exists() {
        fs::remove_dir_all(&nested).map_err(|e| e.to_string())?;
    }
    if !inst_dir.join("mmc-pack.json").exists() {
        return Err(format!(
            "failed to flatten instance pack in {}",
            inst_dir.display()
        ));
    }
    Ok(())
}

fn mmc_pack_dir(inst_dir: &Path) -> PathBuf {
    if inst_dir.join("mmc-pack.json").exists() {
        return inst_dir.to_path_buf();
    }
    nested_pack_dir(inst_dir).unwrap_or_else(|| inst_dir.to_path_buf())
}

/// Minecraft `--gameDir` for GTNH instances. Several mods (e.g. BetterQuesting) resolve
/// `config/...` relative to the process working directory, so launch CWD must match gameDir.
fn minecraft_working_dir(pack_dir: &Path) -> PathBuf {
    pack_dir.join(".minecraft")
}

struct LoadedPatch {
    order: i32,
    uid: String,
    patch: PatchJson,
}

fn expand_minecraft_arguments(template: &str, tokens: &HashMap<&str, String>) -> Vec<String> {
    template
        .split_whitespace()
        .map(|part| {
            let mut result = part.to_string();
            for (key, value) in tokens {
                result = result.replace(&format!("${{{key}}}"), value);
            }
            result
        })
        .collect()
}

async fn build_launch_config(
    inst_dir: &Path,
    client: &reqwest::Client,
    app: &tauri::AppHandle,
    version: &str,
) -> Result<LaunchConfig, String> {
    let pack_dir = mmc_pack_dir(inst_dir);
    let mmc_path = pack_dir.join("mmc-pack.json");
    let mmc: MmcPackJson = serde_json::from_str(
        &fs::read_to_string(&mmc_path).map_err(|e| format!("missing mmc-pack.json: {e}"))?,
    )
    .map_err(|e| format!("bad mmc-pack.json: {e}"))?;

    let mut loaded_patches: Vec<LoadedPatch> = Vec::new();
    for comp in &mmc.components {
        let patch_path = pack_dir.join("patches").join(format!("{}.json", comp.uid));
        if !patch_path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&patch_path).map_err(|e| format!("patch read: {e}"))?;
        let patch: PatchJson =
            serde_json::from_str(&raw).map_err(|e| format!("bad patch {}: {e}", comp.uid))?;
        loaded_patches.push(LoadedPatch {
            order: patch.order.unwrap_or(0),
            uid: comp.uid.clone(),
            patch,
        });
    }
    loaded_patches.sort_by_key(|p| p.order);

    let mut main_class = "net.minecraft.launchwrapper.Launch".to_string();
    let mut jvm_args: Vec<String> = vec!["-Duser.language=en".to_string()];
    let mut program_args: Vec<String> = Vec::new();
    let mut mc_version = "1.12.2".to_string();
    let mut resolved_libraries: Vec<ResolvedLibrary> = Vec::new();
    let mut library_index: HashMap<String, usize> = HashMap::new();
    let mut main_jar: Option<PatchMainJar> = None;
    let mut minecraft_arguments: Option<String> = None;
    let mut asset_index: Option<PatchAssetIndex> = None;

    for loaded in &loaded_patches {
        let patch = &loaded.patch;
        let comp = mmc.components.iter().find(|c| c.uid == loaded.uid);

        if let Some(mc) = patch
            .plus_main_class
            .clone()
            .or_else(|| patch.main_class.clone())
        {
            main_class = mc;
        }
        if let Some(args) = &patch.plus_jvm_args {
            jvm_args.extend(args.iter().cloned());
        }
        if let Some(tweakers) = &patch.plus_tweakers {
            for tweaker in tweakers {
                program_args.push("--tweakClass".to_string());
                program_args.push(tweaker.clone());
            }
        }
        if let Some(args) = &patch.plus_args {
            let mut i = 0;
            while i < args.len() {
                if args[i] == "--tweakClass" && i + 1 < args.len() {
                    program_args.push("--tweakClass".to_string());
                    program_args.push(args[i + 1].clone());
                    i += 2;
                } else if args[i].starts_with("--tweakClass=") {
                    program_args.push(args[i].clone());
                    i += 1;
                } else {
                    jvm_args.push(args[i].clone());
                    i += 1;
                }
            }
        }
        if loaded.uid == "net.minecraft" {
            if let Some(comp) = comp {
                mc_version = comp.effective_version().to_string();
            }
            main_jar = patch.main_jar.clone();
            minecraft_arguments = patch.minecraft_arguments.clone();
            asset_index = patch.asset_index.clone();
        }

        if let Some(entries) = &patch.libraries {
            for entry in entries {
                if let Some(path) = ensure_library(client, &pack_dir, entry, app, version).await? {
                    let Some(spec) = parse_gradle_spec(&entry.name) else {
                        continue;
                    };
                    upsert_library(
                        &mut resolved_libraries,
                        &mut library_index,
                        &spec,
                        path.to_string_lossy().to_string(),
                    );
                }
            }
        }
    }

    if let Some(main_jar) = main_jar {
        if let Some(path) =
            ensure_main_jar(client, &pack_dir, &main_jar, &mc_version, app, version).await?
        {
            let path_str = path.to_string_lossy().to_string();
            if let Some(spec) = parse_gradle_spec(&main_jar.name) {
                upsert_library(&mut resolved_libraries, &mut library_index, &spec, path_str);
            } else {
                resolved_libraries.push(ResolvedLibrary {
                    path: path_str,
                    version: mc_version.clone(),
                });
            }
        }
    }

    let libraries: Vec<String> = resolved_libraries.into_iter().map(|l| l.path).collect();
    if libraries.is_empty() {
        return Err("no libraries resolved for launch".into());
    }

    let game_dir = pack_dir.join(".minecraft").to_string_lossy().to_string();
    let assets_dir = pack_dir
        .join(".minecraft")
        .join("assets")
        .to_string_lossy()
        .to_string();
    let natives_dir = pack_dir.join("natives").to_string_lossy().to_string();
    let _ = fs::create_dir_all(&natives_dir);
    jvm_args.push(format!("-Djava.library.path={natives_dir}"));

    Ok(LaunchConfig {
        main_class,
        minecraft_version: mc_version,
        libraries,
        game_dir,
        assets_dir,
        jvm_args,
        program_args,
        minecraft_arguments_template: minecraft_arguments,
        asset_index,
    })
}

#[tauri::command]
async fn launch_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: String,
) -> Result<(), String> {
    let client = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running_processes.contains_key(&id) {
            return Err("Instance is already running".into());
        }
        if guard.update_in_progress.contains(&id) {
            return Err("pack update in progress for this instance".into());
        }
        if guard.reinstall_in_progress.contains(&id) {
            return Err("clean reinstall in progress for this instance".into());
        }
        guard.http.clone()
    };

    let inst_dir = instance_dir(&id);
    if !inst_dir.exists() {
        return Err("instance not installed".into());
    }
    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, false)?;
    pack::apply_persistent_custom_mods(&inst_dir)?;

    let settings = load_settings(&id).ok_or("no settings")?;
    let java = resolve_java(&settings)?;

    let config = build_launch_config(&inst_dir, &client, &app, &id).await?;
    if let Some(asset_index) = &config.asset_index {
        ensure_assets(
            &client,
            Path::new(&config.assets_dir),
            asset_index,
            &app,
            &id,
        )
        .await?;
    }
    let pack_dir = mmc_pack_dir(&inst_dir);

    let classpath = build_classpath(&config.libraries);
    let classpath_len = classpath.len();
    let command_vars = instance_command_vars(&id, &settings, &inst_dir, &java);

    if settings.override_commands {
        let pre = substitute_command_vars(settings.pre_launch_command.trim(), &command_vars);
        if !pre.is_empty() {
            let env = if settings.override_env {
                &settings.env_vars
            } else {
                &HashMap::new()
            };
            run_shell_command(&pre, &pack_dir, env)?;
            emit_launch_log(&app, &id, "system", "Pre-launch command finished");
        }
    }

    // Build JVM args
    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xms{}M", settings.effective_min_ram_mb()));
    args.push(format!("-Xmx{}M", settings.effective_max_ram_mb()));
    if settings.override_memory && settings.perm_gen_mb > 0 {
        args.push(format!("-XX:PermSize={}M", settings.perm_gen_mb));
        args.push(format!("-XX:MaxPermSize={}M", settings.perm_gen_mb));
    }
    let extra_jvm = settings.effective_jvm_args();
    if !extra_jvm.is_empty() {
        args.extend(extra_jvm.split_whitespace().map(String::from));
    }
    // Classpath must be set before the custom system classloader initializes.
    args.push("-cp".to_string());
    args.push(classpath);
    args.extend(config.jvm_args);
    let main_class = config.main_class;
    args.push(main_class.clone());
    args.extend(config.program_args);

    if settings.override_window && !settings.launch_maximized {
        args.push("--width".to_string());
        args.push(settings.window_width.to_string());
        args.push("--height".to_string());
        args.push(settings.window_height.to_string());
    }
    if settings.join_server_on_launch {
        let server = settings.join_server_address.trim();
        if !server.is_empty() {
            args.push("--server".to_string());
            args.push(server.to_string());
        }
    }

    let accounts = load_accounts();
    let launcher_settings = load_launcher_settings();
    let acc = pick_launch_account(
        &settings,
        &accounts,
        launcher_settings.default_account_id.as_deref(),
    )?;
    let (username, access_token, uuid, user_type) = if acc.is_offline() {
        let username = acc.profile_name();
        let uuid = acc.profile_id();
        if username.is_empty() || uuid.is_empty() {
            return Err("Offline account is missing a username.".into());
        }
        (username, "0".into(), uuid, "legacy".to_string())
    } else {
        if let Some(ent) = &acc.minecraft_entitlement {
            if !ent.can_play_minecraft {
                return Err("This Microsoft account does not own Minecraft Java Edition.".into());
            }
        }
        let token = auth::ensure_fresh_token(&client, &data_dir(), acc).await?;
        let username = acc.profile_name();
        let uuid = acc.profile_id();
        if username.is_empty() || uuid.is_empty() {
            return Err(
                "This Microsoft account has no Minecraft profile yet. Set a username in the official launcher first.".into(),
            );
        }
        (username, token, uuid, "msa".to_string())
    };

    if let Some(template) = &config.minecraft_arguments_template {
        let asset_index = config
            .asset_index
            .as_ref()
            .map(|a| a.id.clone())
            .unwrap_or_else(|| config.minecraft_version.clone());
        let tokens: HashMap<&str, String> = HashMap::from([
            ("auth_player_name", username.clone()),
            ("version_name", config.minecraft_version.clone()),
            ("game_directory", config.game_dir.clone()),
            ("assets_root", config.assets_dir.clone()),
            ("assets_index_name", asset_index),
            ("auth_uuid", uuid.clone()),
            ("auth_access_token", access_token.clone()),
            ("user_properties", "{}".to_string()),
            ("user_type", user_type.clone()),
        ]);
        args.extend(expand_minecraft_arguments(template, &tokens));
    } else {
        args.push("--username".to_string());
        args.push(username);
        args.push("--version".to_string());
        args.push(config.minecraft_version);
        args.push("--gameDir".to_string());
        args.push(config.game_dir);
        args.push("--assetsDir".to_string());
        args.push(config.assets_dir);
        args.push("--accessToken".to_string());
        args.push(access_token);
        args.push("--uuid".to_string());
        args.push(uuid);
        args.push("--userType".to_string());
        args.push(user_type);
    }

    emit_launch_log(&app, &id, "system", "──────── Launch ────────");
    emit_launch_log(&app, &id, "system", &format!("Java: {java}"));
    emit_launch_log(
        &app,
        &id,
        "system",
        &format!("Main class: {main_class}"),
    );
    emit_launch_log(
        &app,
        &id,
        "system",
        &format!(
            "Classpath: {} libraries ({} chars)",
            config.libraries.len(),
            classpath_len
        ),
    );

    // Write launch.arg for debugging; Java @argfiles mangle long classpaths on Windows.
    let argfile_path = inst_dir.join("launch.arg");
    write_launch_argfile(&argfile_path, &args).ok();
    emit_launch_log(
        &app,
        &id,
        "system",
        &format!("Launch args saved to {}", argfile_path.display()),
    );

    let (cmd_executable, cmd_args) = if settings.override_commands {
        let wrapper = settings.wrapper_command.trim();
        if wrapper.is_empty() {
            (java.clone(), args)
        } else {
            let substituted = substitute_command_vars(wrapper, &command_vars);
            let parts = split_command_args(&substituted);
            let wrapper_exe = parts
                .first()
                .cloned()
                .ok_or("wrapper command is empty")?;
            let mut wrapper_args = parts[1..].to_vec();
            wrapper_args.push(java.clone());
            wrapper_args.extend(args);
            emit_launch_log(
                &app,
                &id,
                "system",
                &format!("Wrapper: {substituted}"),
            );
            (wrapper_exe, wrapper_args)
        }
    } else {
        (java.clone(), args)
    };

    let mut cmd = std::process::Command::new(&cmd_executable);
    let game_work_dir = minecraft_working_dir(&pack_dir);
    fs::create_dir_all(&game_work_dir).map_err(|e| e.to_string())?;
    cmd.current_dir(&game_work_dir)
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if settings.override_env {
        for (key, value) in &settings.env_vars {
            cmd.env(key, value);
        }
    }

    let play_start = std::time::Instant::now();
    let child = cmd
        .spawn()
        .map_err(|e| format!("launch failed ({cmd_executable}): {e}"))?;

    emit(
        &app,
        "instance-started",
        &serde_json::json!({ "id": id }),
    );

    let child_handle = Arc::new(Mutex::new(child));
    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard
            .running_processes
            .insert(id.clone(), child_handle.clone());
    }
    let running_guard = RunningInstanceGuard {
        state: &state,
        id: id.clone(),
    };

    let launch_id = id.clone();
    let app_for_wait = app.clone();
    let exit_code = tokio::task::spawn_blocking(move || {
        wait_for_launch(child_handle, app_for_wait, launch_id)
    })
    .await
    .map_err(|e| format!("launch task failed: {e}"))??;

    record_play_time(&id, play_start.elapsed().as_secs());

    // Release the running slot as soon as the game process exits so Stop/relaunch
    // work immediately, even while post-exit shell commands are still running.
    drop(running_guard);
    emit(
        &app,
        "instance-stopped",
        &serde_json::json!({ "id": id, "exit_code": exit_code }),
    );

    if settings.override_commands {
        let post = substitute_command_vars(settings.post_exit_command.trim(), &command_vars);
        if !post.is_empty() {
            let env = if settings.override_env {
                settings.env_vars.clone()
            } else {
                HashMap::new()
            };
            run_shell_command(&post, &pack_dir, &env)?;
            emit_launch_log(&app, &id, "system", "Post-exit command finished");
        }
    }

    if exit_code != 0 {
        return Err(format!("game exited with code {exit_code}"));
    }
    Ok(())
}

#[tauri::command]
fn exit_launcher(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn kill_instance(state: State<'_, Mutex<AppState>>, id: String) -> Result<(), String> {
    let child_arc = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.running_processes.get(&id).cloned()
    };
    let Some(child_arc) = child_arc else {
        return Ok(());
    };
    let mut child = child_arc
        .lock()
        .map_err(|e| format!("process lock failed: {e}"))?;
    if child.try_wait().ok().flatten().is_some() {
        return Ok(());
    }
    child
        .kill()
        .map_err(|e| format!("failed to stop process: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn get_accounts() -> Result<Vec<AccountInfo>, String> {
    Ok(load_accounts()
        .into_iter()
        .map(|a| auth::account_to_info(&a))
        .collect())
}

#[tauri::command]
fn add_offline_account(username: String) -> Result<AccountInfo, String> {
    auth::create_offline_account(&data_dir(), &username)
}

#[tauri::command]
async fn remove_account(id: String) -> Result<(), String> {
    let mut accounts = load_accounts();
    accounts.retain(|a| a.id != id);
    save_accounts(&accounts)
}

#[tauri::command]
async fn get_launcher_settings() -> Result<LauncherSettings, String> {
    Ok(load_launcher_settings())
}

#[tauri::command]
async fn save_launcher_settings(settings: LauncherSettings) -> Result<(), String> {
    write_launcher_settings(&settings)
}

#[tauri::command]
async fn start_microsoft_login(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<AccountInfo, String> {
    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);

    auth::login_microsoft_account(&client, &app, &data_dir()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_object_digest_strips_sha1_prefix() {
        assert_eq!(
            asset_object_digest("sha1:1863782e33ce7b584fc45b037325a1964e095d3e"),
            "1863782e33ce7b584fc45b037325a1964e095d3e"
        );
        assert_eq!(
            asset_object_digest("1863782e33ce7b584fc45b037325a1964e095d3e"),
            "1863782e33ce7b584fc45b037325a1964e095d3e"
        );
    }

    #[test]
    fn asset_object_path_uses_two_char_prefix() {
        let path = asset_object_path(
            Path::new(r"C:\game\assets"),
            "sha1:1863782e33ce7b584fc45b037325a1964e095d3e",
        );
        assert_eq!(
            path,
            PathBuf::from(
                r"C:\game\assets\objects\18\1863782e33ce7b584fc45b037325a1964e095d3e"
            )
        );
    }

    #[test]
    fn validate_instance_id_sanitizes_spaces() {
        assert_eq!(validate_instance_id("GTNH 2.8.0").unwrap(), "GTNH_2.8.0");
        assert_eq!(validate_instance_id("GTNH-2.8.0").unwrap(), "GTNH-2.8.0");
    }

    #[test]
    fn sanitize_name_replaces_whitespace() {
        assert_eq!(sanitize_name("foo bar"), "foo_bar");
    }

    #[test]
    fn argfile_quotes_classpath_with_spaces() {
        let path = std::env::temp_dir().join("industrialis-test-launch.arg");
        let cp = r"C:\GT New Horizons\a.jar;C:\GT New Horizons\b.jar".to_string();
        write_launch_argfile(&path, &["-cp".into(), cp.clone()]).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#""C:\GT New Horizons\a.jar;C:\GT New Horizons\b.jar""#));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn classpath_joins_without_per_jar_quotes() {
        let cp = build_classpath(&[
            r"C:\Games\GT New Horizons\lib\a.jar".to_string(),
            r"C:\Games\GT New Horizons\lib\b.jar".to_string(),
        ]);
        assert_eq!(
            cp,
            r"C:\Games\GT New Horizons\lib\a.jar;C:\Games\GT New Horizons\lib\b.jar"
        );
    }

    #[test]
    fn gradle_spec_resolves_log4j_path() {
        let spec = parse_gradle_spec("org.apache.logging.log4j:log4j-api:2.0-beta9-fixed").unwrap();
        assert_eq!(
            spec.storage_path(),
            "org/apache/logging/log4j/log4j-api/2.0-beta9-fixed/log4j-api-2.0-beta9-fixed.jar"
        );
    }

    #[test]
    fn java_from_home_trims_whitespace() {
        let home = format!(
            " {}\\Program Files\\Java\\jdk-21",
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
        );
        if PathBuf::from(home.trim())
            .join("bin")
            .join("java.exe")
            .exists()
        {
            let resolved = java_from_home(&home).expect("java should resolve after trim");
            assert!(resolved.ends_with("java.exe") || resolved.ends_with("java"));
        }
    }

    #[test]
    fn library_dedup_keeps_newer_guava() {
        let mut libs = Vec::new();
        let mut index = HashMap::new();
        let old = parse_gradle_spec("com.google.guava:guava:15.0").unwrap();
        let new = parse_gradle_spec("com.google.guava:guava:17.0").unwrap();
        upsert_library(&mut libs, &mut index, &old, "guava-15.jar".into());
        upsert_library(&mut libs, &mut index, &new, "guava-17.jar".into());
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].path, "guava-17.jar");
        assert_eq!(libs[0].version, "17.0");
    }

    #[test]
    fn resolve_forge_library_url_from_maven_repo() {
        let entry = PatchLibraryEntry {
            name: "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10:universal".into(),
            url: Some("https://maven.minecraftforge.net/".into()),
            mmc_hint: None,
            mmc_absolute_url: None,
            downloads: None,
            rules: None,
            natives: None,
        };
        let spec = parse_gradle_spec(&entry.name).unwrap();
        let url = resolve_library_download_url(&entry, &spec).unwrap();
        assert_eq!(
            url,
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
        );
    }

    #[test]
    fn expand_minecraft_arguments_preserves_paths_with_spaces() {
        let template =
            "--username ${auth_player_name} --gameDir ${game_directory} --assetsDir ${assets_root}";
        let tokens = HashMap::from([
            ("auth_player_name", "Player".to_string()),
            (
                "game_directory",
                r"C:\instances\GT New Horizons 2.9.0-beta-1\.minecraft".to_string(),
            ),
            (
                "assets_root",
                r"C:\instances\GT New Horizons 2.9.0-beta-1\.minecraft\assets".to_string(),
            ),
        ]);
        let args = expand_minecraft_arguments(template, &tokens);
        assert_eq!(
            args,
            vec![
                "--username",
                "Player",
                "--gameDir",
                r"C:\instances\GT New Horizons 2.9.0-beta-1\.minecraft",
                "--assetsDir",
                r"C:\instances\GT New Horizons 2.9.0-beta-1\.minecraft\assets",
            ]
        );
    }

    #[test]
    fn parses_mmc_pack_components_without_version_field() {
        let json = r#"{
            "components": [
                {"uid": "net.minecraft", "version": "1.7.10"},
                {"uid": "me.eigenraven.lwjgl3ify.forgepatches", "cachedVersion": "3.0.23"}
            ],
            "formatVersion": 1
        }"#;
        let mmc: MmcPackJson = serde_json::from_str(json).expect("mmc-pack.json should parse");
        let forgepatches = mmc
            .components
            .iter()
            .find(|c| c.uid == "me.eigenraven.lwjgl3ify.forgepatches")
            .expect("forgepatches component");
        assert_eq!(forgepatches.version, None);
        assert_eq!(forgepatches.effective_version(), "3.0.23");
        let minecraft = mmc
            .components
            .iter()
            .find(|c| c.uid == "net.minecraft")
            .expect("minecraft component");
        assert_eq!(minecraft.effective_version(), "1.7.10");
    }

    #[test]
    fn collect_delete_paths_orders_children_before_parents() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-delete-paths-test-{}",
            uuid::Uuid::new_v4()
        ));
        let nested = root.join("nested");
        fs::create_dir_all(nested.join("child")).unwrap();
        fs::write(nested.join("child/file.txt"), b"x").unwrap();

        let mut paths = Vec::new();
        collect_delete_paths(&root, &mut paths).expect("collect paths");

        let file_idx = paths
            .iter()
            .position(|p| p.ends_with("file.txt"))
            .expect("file path");
        let nested_idx = paths
            .iter()
            .position(|p| p == &nested)
            .expect("nested dir");
        let root_idx = paths.iter().position(|p| p == &root).expect("root dir");
        assert!(file_idx < nested_idx);
        assert!(nested_idx < root_idx);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_pack_configs_copies_into_minecraft_config() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-seed-config-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("config/nested")).unwrap();
        fs::write(root.join("config/gadomancy.cfg"), "B:ancientStoneRecipes=false\n").unwrap();
        fs::write(root.join("config/nested/child.cfg"), "flag=true\n").unwrap();

        seed_pack_configs(&root, true).expect("seed should succeed");

        let seeded = root.join(".minecraft/config/gadomancy.cfg");
        let child = root.join(".minecraft/config/nested/child.cfg");
        assert!(seeded.exists());
        assert!(child.exists());
        assert_eq!(
            fs::read_to_string(&seeded).unwrap(),
            "B:ancientStoneRecipes=false\n"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_pack_configs_skips_existing_files_when_not_overwriting() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-seed-config-skip-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join(".minecraft/config")).unwrap();
        fs::write(root.join("config/gadomancy.cfg"), "from-pack\n").unwrap();
        fs::write(root.join(".minecraft/config/gadomancy.cfg"), "user-edit\n").unwrap();

        seed_pack_configs(&root, false).expect("seed should succeed");

        assert_eq!(
            fs::read_to_string(root.join(".minecraft/config/gadomancy.cfg")).unwrap(),
            "user-edit\n"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_gtnh_config_patches_enables_gadomancy_ancient_stone_recipes() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-gtnh-patch-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join(".minecraft/config")).unwrap();
        fs::write(
            root.join(".minecraft/config/gadomancy.cfg"),
            "skyblock {\n    B:ancientStoneRecipes=false\n}\n",
        )
        .unwrap();

        apply_gtnh_config_patches(&root).expect("patch should succeed");

        let content = fs::read_to_string(root.join(".minecraft/config/gadomancy.cfg")).unwrap();
        assert!(content.contains("B:ancientStoneRecipes=true"));
        assert!(!content.contains("B:ancientStoneRecipes=false"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn flatten_nested_pack_moves_mmc_contents_to_instance_root() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-flatten-test-{}",
            uuid::Uuid::new_v4()
        ));
        let nested = root.join("GT New Horizons 2.9.0-beta-1");
        fs::create_dir_all(nested.join(".minecraft/mods")).unwrap();
        fs::write(nested.join("mmc-pack.json"), r#"{"components":[],"formatVersion":1}"#).unwrap();
        fs::write(nested.join(".minecraft/mods/test.jar"), b"mod").unwrap();
        fs::write(root.join("instance.json"), r#"{"name":"test"}"#).unwrap();

        flatten_nested_pack(&root).expect("flatten should succeed");

        assert!(root.join("mmc-pack.json").exists());
        assert!(root.join(".minecraft/mods/test.jar").exists());
        assert!(!nested.exists());
        let _ = fs::remove_dir_all(&root);
    }
}

// ── App Entry ──

fn handle_oauth_deep_links(urls: &[url::Url]) {
    for url in urls {
        if let Err(e) = auth::handle_oauth_callback(url.as_str()) {
            eprintln!("OAuth deep link: {e}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            use tauri_plugin_deep_link::DeepLinkExt;

            migrate_legacy_instance_usernames();
            let _ = pack_cache::evict_expired_pack_cache();

            #[cfg(any(windows, target_os = "linux"))]
            {
                app.deep_link().register_all()?;
            }

            if let Some(urls) = app.deep_link().get_current()? {
                handle_oauth_deep_links(&urls);
            }

            app.deep_link().on_open_url(|event| {
                handle_oauth_deep_links(&event.urls());
            });

            Ok(())
        })
        .manage(Mutex::new(AppState {
            http: reqwest::Client::new(),
            running_processes: HashMap::new(),
            delete_cancel: HashMap::new(),
            update_in_progress: HashSet::new(),
            reinstall_in_progress: HashSet::new(),
            copy_in_progress: HashSet::new(),
        }))
        .invoke_handler(tauri::generate_handler![
            get_versions,
            get_instances,
            refresh_instance_sizes,
            get_instance_groups,
            set_instance_group,
            rename_group,
            delete_group,
            set_group_collapsed,
            move_instance_in_group,
            set_group_instance_order,
            delete_instance,
            cancel_delete_instance,
            copy_instance,
            open_instance_folder,
            save_settings,
            get_settings,
            download_install,
            preview_update_mods,
            update_instance,
            reinstall_instance,
            list_minecraft_entries,
            read_minecraft_file,
            write_minecraft_file,
            delete_persistent_file,
            config_presets::apply_config_preset,
            config_presets::get_config_preset_status,
            list_persistent_files,
            list_custom_mods,
            browse_custom_mod,
            browse_instance_icon_file,
            set_instance_icon,
            clear_instance_icon,
            add_custom_mod,
            remove_custom_mod,
            detect_java,
            launch_instance,
            browse_java_executable,
            test_java,
            exit_launcher,
            kill_instance,
            get_instance_console_log,
            clear_instance_console_log,
            get_accounts,
            add_offline_account,
            remove_account,
            start_microsoft_login,
            get_launcher_settings,
            save_launcher_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
