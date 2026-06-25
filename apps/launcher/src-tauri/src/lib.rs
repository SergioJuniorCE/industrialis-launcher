mod auth;
mod groups;
mod settings;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::io::AsyncWriteExt;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub name: String,
    #[serde(default)]
    pub pack_version: String,
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

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            pack_version: String::new(),
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
    #[serde(rename = "-args")]
    minus_args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatchAssetIndex {
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

fn instance_dir(id: &str) -> PathBuf {
    instances_dir().join(sanitize_name(id))
}

fn sanitize_name(s: &str) -> String {
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

/// Offline-mode UUID — matches Minecraft's `OfflinePlayer:<name>` convention.
fn offline_player_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{username}"));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).hyphenated().to_string()
}

fn resolve_offline_identity(settings: &InstanceSettings) -> Result<(String, String, String), String> {
    let trimmed = settings.username.trim();
    let username = if trimmed.is_empty() { "Player" } else { trimmed };
    if username.len() > 16 {
        return Err("Offline username must be 16 characters or fewer.".into());
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(
            "Offline username may only contain letters, numbers, and underscores.".into(),
        );
    }
    Ok((
        username.to_string(),
        "0".into(),
        offline_player_uuid(username),
    ))
}

fn settings_path(id: &str) -> PathBuf {
    instance_dir(id).join("instance.json")
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

fn should_use_microsoft(settings: &InstanceSettings, accounts: &[AccountData]) -> bool {
    if accounts.is_empty() {
        return false;
    }
    if settings.override_account {
        if let Some(ref id) = settings.account_id {
            return accounts.iter().any(|a| &a.id == id);
        }
    }
    settings.auth_mode == "microsoft"
}

fn pick_microsoft_account<'a>(
    settings: &InstanceSettings,
    accounts: &'a [AccountData],
) -> Option<&'a AccountData> {
    if settings.override_account {
        if let Some(ref id) = settings.account_id {
            return accounts.iter().find(|a| &a.id == id);
        }
    }
    accounts.first()
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
        let inst_dir = entry.path();
        if !known_ids.contains(&id) {
            continue;
        }
        let _ = flatten_nested_pack(&inst_dir);
        let size = dir_size(&inst_dir);
        let settings = load_settings(&id).unwrap_or_default();
        let group = groups::get_instance_group(&dir, &id, &known_ids);
        instances.push(InstanceInfo {
            id,
            installed: true,
            size_bytes: size,
            settings,
            group,
        });
    }
    Ok(instances)
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
            "pct": pct,
            "id": id,
        }),
    );
}

#[tauri::command]
async fn delete_instance(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = instance_dir(&id);
    let mut paths = Vec::new();
    if dir.exists() {
        collect_delete_paths(&dir, &mut paths)?;
    }

    let total = paths.len().max(1);
    let mut last_emitted_pct = -1i32;
    emit_delete_progress(&app, &id, 0.0);

    for (i, path) in paths.iter().enumerate() {
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
            emit_delete_progress(&app, &id, pct);
            last_emitted_pct = pct_ui;
        }
    }

    let instances_root = instances_dir();
    let known_ids = list_instance_ids()?;
    groups::remove_instance_from_groups(&instances_root, &id, &known_ids)?;

    emit(
        &app,
        "dl-progress",
        &serde_json::json!({ "stage": "done", "pct": 1.0, "id": id }),
    );
    Ok(())
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
    let id = sanitize_name(id.trim());
    if id.is_empty() {
        return Err("instance id cannot be empty".into());
    }
    let known_ids = list_instance_ids()?;
    if known_ids.contains(&id) {
        return Err("an instance with that id already exists".into());
    }

    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);
    let url = format!(
        "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json"
    );
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let versions: HashMap<String, GtnhVersion> = resp.json().await.map_err(|e| e.to_string())?;
    let v = versions
        .get(&pack_version)
        .ok_or("pack version not found")?;
    let dl_url = if java_type == "java8" {
        v.mmc.java8_url.clone()
    } else {
        v.mmc.java17_2x_url.clone()
    };

    let inst_dir = instance_dir(&id);
    fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;
    let zip_path = inst_dir.join("pack.zip");

    // Download
    emit(
        &app,
        "dl-progress",
        &serde_json::json!({"stage": "downloading", "pct": 0.0}),
    );
    let resp = client
        .get(&dl_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&zip_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = downloaded as f64 / total as f64;
            if (pct * 100.0) as u32 % 5 == 0 {
                emit(
                    &app,
                    "dl-progress",
                    &serde_json::json!({"stage": "downloading", "pct": pct}),
                );
            }
        }
    }
    drop(file);

    // Extract
    emit(
        &app,
        "dl-progress",
        &serde_json::json!({"stage": "extracting", "pct": 0.0}),
    );
    let zip_file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;
    let total_files = archive.len();
    for i in 0..total_files {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let out_path = inst_dir.join(entry.name());
        if entry.name().ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
        let pct = (i as f64 + 1.0) / total_files as f64;
        if (pct * 100.0) as u32 % 10 == 0 {
            emit(
                &app,
                "dl-progress",
                &serde_json::json!({"stage": "extracting", "pct": pct}),
            );
        }
    }

    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, true)?;

    // Cleanup zip
    fs::remove_file(&zip_path).map_err(|e| e.to_string())?;

    let display_name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("GTNH {pack_version}"));
    let settings = InstanceSettings {
        name: display_name,
        pack_version: pack_version.clone(),
        ..Default::default()
    };
    save_settings_file(&id, &settings)?;

    if let Some(group) = group {
        let instances_root = instances_dir();
        let known_ids = list_instance_ids()?;
        groups::set_instance_group(&instances_root, &id, &group, &known_ids)?;
    }

    emit(
        &app,
        "dl-progress",
        &serde_json::json!({"stage": "done", "pct": 1.0, "id": id}),
    );
    Ok(())
}

fn emit<T: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: &T) {
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

fn prepare_instance_configs(inst_dir: &Path, overwrite_pack_configs: bool) -> Result<(), String> {
    seed_pack_configs(inst_dir, overwrite_pack_configs)?;
    apply_gtnh_config_patches(inst_dir)?;
    Ok(())
}

fn flatten_nested_pack(inst_dir: &Path) -> Result<(), String> {
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
        guard.http.clone()
    };

    let inst_dir = instance_dir(&id);
    if !inst_dir.exists() {
        return Err("instance not installed".into());
    }
    flatten_nested_pack(&inst_dir)?;
    prepare_instance_configs(&inst_dir, false)?;

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

    // Auth: Microsoft when linked, otherwise offline (no sign-in required at launch)
    let accounts = load_accounts();
    let use_microsoft = should_use_microsoft(&settings, &accounts);
    let (username, access_token, uuid, user_type) = if use_microsoft {
        let acc = pick_microsoft_account(&settings, &accounts)
            .ok_or("No Microsoft account configured. Add one in Accounts.")?;
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
    } else {
        let (username, token, uuid) = resolve_offline_identity(&settings)?;
        (username, token, uuid, "legacy".to_string())
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
    cmd.current_dir(&pack_dir)
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
    let _running_guard = RunningInstanceGuard {
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

    emit(
        &app,
        "instance-stopped",
        &serde_json::json!({ "id": id, "exit_code": exit_code }),
    );

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
        guard
            .running_processes
            .get(&id)
            .cloned()
            .ok_or_else(|| "Instance is not running".to_string())?
    };
    let mut child = child_arc
        .lock()
        .map_err(|e| format!("process lock failed: {e}"))?;
    child
        .kill()
        .map_err(|e| format!("failed to stop process: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn get_accounts() -> Result<Vec<AccountInfo>, String> {
    let accounts = load_accounts();
    Ok(accounts
        .into_iter()
        .map(|a| AccountInfo {
            id: a.id.clone(),
            username: a.profile_name(),
            uuid: a.profile_id(),
            skin_png_base64: a.skin_png_base64,
            owns_minecraft: a.minecraft_entitlement.as_ref().map(|e| e.owns_minecraft),
        })
        .collect())
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
    fn offline_player_uuid_matches_minecraft_convention() {
        assert_eq!(
            offline_player_uuid("Steve"),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
    }

    #[test]
    fn resolve_offline_identity_defaults_empty_username() {
        let settings = InstanceSettings {
            username: "  ".into(),
            ..Default::default()
        };
        let (name, token, uuid) = resolve_offline_identity(&settings).unwrap();
        assert_eq!(name, "Player");
        assert_eq!(token, "0");
        assert_eq!(uuid, offline_player_uuid("Player"));
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
        }))
        .invoke_handler(tauri::generate_handler![
            get_versions,
            get_instances,
            get_instance_groups,
            set_instance_group,
            rename_group,
            delete_group,
            set_group_collapsed,
            delete_instance,
            open_instance_folder,
            save_settings,
            get_settings,
            download_install,
            detect_java,
            launch_instance,
            browse_java_executable,
            test_java,
            exit_launcher,
            kill_instance,
            get_instance_console_log,
            clear_instance_console_log,
            get_accounts,
            remove_account,
            start_microsoft_login,
            get_launcher_settings,
            save_launcher_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
