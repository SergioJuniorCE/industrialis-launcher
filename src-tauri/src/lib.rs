use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Stdio};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tokio::io::AsyncWriteExt;

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
    pub version: String,
    pub installed: bool,
    pub size_bytes: u64,
    pub settings: InstanceSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub name: String,
    pub java_path: Option<String>,
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub jvm_args: String,
    pub auth_mode: String,
    pub username: String,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            java_path: None,
            min_ram_mb: 4096,
            max_ram_mb: 6144,
            jvm_args: String::new(),
            auth_mode: "offline".into(),
            username: "Player".into(),
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
    pub asset_index_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftAccount {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    pub microsoft_client_id: String,
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub theme_overrides: ThemeOverrides,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            microsoft_client_id: String::new(),
            theme_mode: ThemeMode::Dark,
            theme_overrides: ThemeOverrides::default(),
        }
    }
}

fn validate_theme_override_value(value: &str) -> Result<(), String> {
    if value.len() > 32 {
        return Err("theme override value too long".into());
    }
    Ok(())
}

fn validate_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    for value in [
        settings.theme_overrides.background.as_deref(),
        settings.theme_overrides.foreground.as_deref(),
        settings.theme_overrides.primary.as_deref(),
        settings.theme_overrides.card.as_deref(),
        settings.theme_overrides.border.as_deref(),
        settings.theme_overrides.muted.as_deref(),
        settings.theme_overrides.muted_foreground.as_deref(),
        settings.theme_overrides.radius.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_theme_override_value(value)?;
    }
    Ok(())
}

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

#[derive(Debug, Deserialize)]
struct PatchAssetIndex {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PatchMainJar {
    name: String,
    downloads: PatchDownloads,
}

#[derive(Debug, Deserialize)]
struct PatchLibraryEntry {
    name: String,
    #[serde(rename = "MMC-hint")]
    mmc_hint: Option<String>,
    downloads: Option<PatchDownloads>,
    rules: Option<Vec<PatchRule>>,
    natives: Option<serde_json::Value>,
}

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
        let applies = rule.os.as_ref().map_or(true, |os| {
            os.name.as_deref() == Some(current_os_name())
        });
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

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), String> {
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
    let url = entry
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .map(|a| a.url.as_str())
        .ok_or_else(|| format!("no download URL for {}", entry.name))?;
    let spec = parse_gradle_spec(&entry.name).ok_or_else(|| format!("bad library name: {}", entry.name))?;
    let dest = if entry.mmc_hint.as_deref() == Some("local") {
        pack_dir.join("libraries").join(spec.filename())
    } else {
        pack_dir.join("libraries").join(spec.storage_path())
    };
    emit_launch_log(app, version, "system", &format!("Downloading {}", entry.name));
    download_file(client, url, &dest).await?;
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
    emit_launch_log(app, version, "system", &format!("Downloading Minecraft {mc_version}"));
    download_file(client, url, &dest).await?;
    Ok(Some(dest))
}

// ── App State ──

struct AppState {
    http: reqwest::Client,
    running_instances: HashSet<String>,
}

struct RunningInstanceGuard<'a> {
    state: &'a State<'a, Mutex<AppState>>,
    version: String,
}

impl Drop for RunningInstanceGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.running_instances.remove(&self.version);
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

fn instance_dir(version: &str) -> PathBuf {
    instances_dir().join(sanitize_name(version))
}

fn sanitize_name(s: &str) -> String {
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

fn settings_path(version: &str) -> PathBuf {
    instance_dir(version).join("instance.json")
}

fn console_log_path(version: &str) -> PathBuf {
    instance_dir(version).join("console.log")
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
            vec![PathBuf::from(dir).join("java.exe"), PathBuf::from(dir).join("java")]
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
    java_path().ok_or_else(|| {
        "no Java configured or found — set JAVA_HOME or pick a Java in instance settings".into()
    })
}

// ── Commands ──

#[tauri::command]
async fn get_versions(state: State<'_, Mutex<AppState>>) -> Result<HashMap<String, GtnhVersion>, String> {
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
    let mut instances = vec![];
    let mut entries: Vec<_> = fs::read_dir(&dir).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let inst_dir = entry.path();
        if !inst_dir.is_dir() { continue; }
        let size = dir_size(&inst_dir);
        let settings = load_settings(&name).unwrap_or_default();
        instances.push(InstanceInfo {
            version: name,
            installed: true,
            size_bytes: size,
            settings,
        });
    }
    Ok(instances)
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

fn load_settings(version: &str) -> Option<InstanceSettings> {
    let path = settings_path(version);
    fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn save_settings_file(version: &str, settings: &InstanceSettings) -> Result<(), String> {
    let path = settings_path(version);
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

fn accounts_path() -> PathBuf {
    data_dir().join("accounts.json")
}

fn load_accounts() -> Vec<MinecraftAccount> {
    fs::read_to_string(accounts_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_accounts(accounts: &[MinecraftAccount]) -> Result<(), String> {
    let path = accounts_path();
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

fn launcher_settings_path() -> PathBuf {
    data_dir().join("launcher-settings.json")
}

fn load_launcher_settings() -> LauncherSettings {
    fs::read_to_string(launcher_settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    validate_launcher_settings(settings)?;
    let path = launcher_settings_path();
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn save_settings(version: String, settings: InstanceSettings) -> Result<(), String> {
    save_settings_file(&version, &settings)
}

#[tauri::command]
async fn get_settings(version: String) -> Result<InstanceSettings, String> {
    Ok(load_settings(&version).unwrap_or_default())
}

#[tauri::command]
async fn delete_instance(version: String) -> Result<(), String> {
    let dir = instance_dir(&version);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn download_install(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    version: String,
    java_type: String,
) -> Result<(), String> {
    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);
    let url = format!(
        "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json"
    );
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let versions: HashMap<String, GtnhVersion> = resp.json().await.map_err(|e| e.to_string())?;
    let v = versions.get(&version).ok_or("version not found")?;
    let dl_url = if java_type == "java8" {
        v.mmc.java8_url.clone()
    } else {
        v.mmc.java17_2x_url.clone()
    };

    let inst_dir = instance_dir(&version);
    fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;
    let zip_path = inst_dir.join("pack.zip");

    // Download
    emit(&app, "dl-progress", &serde_json::json!({"stage": "downloading", "pct": 0.0}));
    let resp = client.get(&dl_url).send().await.map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&zip_path).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = downloaded as f64 / total as f64;
            if (pct * 100.0) as u32 % 5 == 0 {
                emit(&app, "dl-progress", &serde_json::json!({"stage": "downloading", "pct": pct}));
            }
        }
    }
    drop(file);

    // Extract
    emit(&app, "dl-progress", &serde_json::json!({"stage": "extracting", "pct": 0.0}));
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
            emit(&app, "dl-progress", &serde_json::json!({"stage": "extracting", "pct": pct}));
        }
    }

    // Flatten nested directory if mmc-pack.json isn't at root
    if !inst_dir.join("mmc-pack.json").exists() {
        if let Ok(entries) = fs::read_dir(&inst_dir) {
            let subdirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir() && e.path().join("mmc-pack.json").exists())
                .collect();
            if let Some(nested) = subdirs.first() {
                let nested_path = nested.path();
                for entry in fs::read_dir(&nested_path).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let dest = inst_dir.join(entry.file_name());
                    let _ = fs::rename(entry.path(), &dest);
                }
                fs::remove_dir_all(&nested_path).map_err(|e| e.to_string())?;
            }
        }
    }

    // Cleanup zip
    fs::remove_file(&zip_path).map_err(|e| e.to_string())?;

    // Create default settings
    let settings = InstanceSettings {
        name: format!("GTNH {version}"),
        username: version.split('-').next().unwrap_or("Player").to_string(),
        ..Default::default()
    };
    save_settings_file(&version, &settings)?;

    emit(&app, "dl-progress", &serde_json::json!({"stage": "done", "pct": 1.0}));
    Ok(())
}

fn emit<T: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: &T) {
    let _ = app.emit(event, payload);
}

fn persist_console_log(version: &str, stream: &str, line: &str) {
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
        .open(console_log_path(version))
    {
        let _ = writeln!(file, "{json}");
    }
}

fn emit_launch_log(app: &tauri::AppHandle, version: &str, stream: &str, line: &str) {
    persist_console_log(version, stream, line);
    emit(
        app,
        "launch-log",
        &serde_json::json!({ "version": version, "stream": stream, "line": line }),
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

fn wait_for_launch(child: Child, app: tauri::AppHandle, version: String) -> Result<i32, String> {
    let mut child = child;
    if let Some(stdout) = child.stdout.take() {
        pipe_launch_output(stdout, app.clone(), version.clone(), "stdout");
    }
    if let Some(stderr) = child.stderr.take() {
        pipe_launch_output(stderr, app.clone(), version.clone(), "stderr");
    }
    let status = child.wait().map_err(|e| format!("process wait failed: {e}"))?;
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
fn get_instance_console_log(version: String) -> Result<Vec<LaunchLogLine>, String> {
    let path = console_log_path(&version);
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
fn clear_instance_console_log(version: String) -> Result<(), String> {
    let path = console_log_path(&version);
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
    let output = std::process::Command::new(path).arg("-version").output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Parse "openjdk version \"21.0.3\"" or similar
    let version_str = stderr.lines().next()?;
    let v: u32 = version_str.split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .next()?;
    Some(JavaInfo {
        path: path.to_string(),
        version: v,
    })
}

fn mmc_pack_dir(inst_dir: &Path) -> PathBuf {
    let direct = inst_dir.join("mmc-pack.json");
    if direct.exists() {
        return inst_dir.to_path_buf();
    }
    // Check for nested subdirectory containing mmc-pack.json
    if let Ok(entries) = fs::read_dir(inst_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() && entry.path().join("mmc-pack.json").exists() {
                return entry.path();
            }
        }
    }
    inst_dir.to_path_buf()
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
    let mut asset_index_id: Option<String> = None;

    for loaded in &loaded_patches {
        let patch = &loaded.patch;
        let comp = mmc.components.iter().find(|c| c.uid == loaded.uid);

        if let Some(mc) = patch.plus_main_class.clone().or_else(|| patch.main_class.clone()) {
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
            asset_index_id = patch.asset_index.as_ref().map(|a| a.id.clone());
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
        if let Some(path) = ensure_main_jar(client, &pack_dir, &main_jar, &mc_version, app, version).await? {
            let path_str = path.to_string_lossy().to_string();
            if let Some(spec) = parse_gradle_spec(&main_jar.name) {
                upsert_library(
                    &mut resolved_libraries,
                    &mut library_index,
                    &spec,
                    path_str,
                );
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
    let assets_dir = pack_dir.join(".minecraft").join("assets").to_string_lossy().to_string();
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
        asset_index_id,
    })
}

#[tauri::command]
async fn launch_instance(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    version: String,
) -> Result<(), String> {
    let client = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if !guard.running_instances.insert(version.clone()) {
            return Err("Instance is already running".into());
        }
        guard.http.clone()
    };
    let _running_guard = RunningInstanceGuard {
        state: &state,
        version: version.clone(),
    };

    let inst_dir = instance_dir(&version);
    if !inst_dir.exists() {
        return Err("instance not installed".into());
    }

    let settings = load_settings(&version).ok_or("no settings")?;
    let java = resolve_java(&settings)?;

    let config = build_launch_config(&inst_dir, &client, &app, &version).await?;
    let pack_dir = mmc_pack_dir(&inst_dir);

    let classpath = build_classpath(&config.libraries);
    let classpath_len = classpath.len();

    // Build JVM args
    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xms{}M", settings.min_ram_mb));
    args.push(format!("-Xmx{}M", settings.max_ram_mb));
    if !settings.jvm_args.is_empty() {
        args.extend(settings.jvm_args.split_whitespace().map(String::from));
    }
    // Classpath must be set before the custom system classloader initializes.
    args.push("-cp".to_string());
    args.push(classpath);
    args.extend(config.jvm_args);
    let main_class = config.main_class;
    args.push(main_class.clone());
    args.extend(config.program_args);

    // Auth: Microsoft or offline
    let (username, access_token, uuid, user_type) = if settings.auth_mode == "microsoft" {
        let accounts = load_accounts();
        let acc = accounts.first().ok_or("No Microsoft account configured. Add one in Accounts.")?;
        let token = if is_expired(acc.expires_at) {
            // ponytail: refresh inline; token refresh is one HTTP call
            refresh_minecraft_token(acc).await?
        } else {
            acc.access_token.clone()
        };
        (acc.username.clone(), token, acc.uuid.clone(), "ms".to_string())
    } else {
        (settings.username.clone(), "0".into(), "00000000-0000-0000-0000-000000000000".into(), "mojang".into())
    };

    if let Some(template) = &config.minecraft_arguments_template {
        let asset_index = config
            .asset_index_id
            .clone()
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

    emit_launch_log(&app, &version, "system", "──────── Launch ────────");
    emit_launch_log(&app, &version, "system", &format!("Java: {java}"));
    emit_launch_log(&app, &version, "system", &format!("Main class: {main_class}"));
    emit_launch_log(
        &app,
        &version,
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
        &version,
        "system",
        &format!("Launch args saved to {}", argfile_path.display()),
    );

    let child = std::process::Command::new(&java)
        .current_dir(&pack_dir)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("launch failed ({java}): {e}"))?;

    let launch_version = version.clone();
    let exit_code = tokio::task::spawn_blocking(move || wait_for_launch(child, app, launch_version))
        .await
        .map_err(|e| format!("launch task failed: {e}"))??;

    if exit_code != 0 {
        return Err(format!("game exited with code {exit_code}"));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub username: String,
    pub uuid: String,
}

#[tauri::command]
async fn get_accounts() -> Result<Vec<AccountInfo>, String> {
    let accounts = load_accounts();
    Ok(accounts.into_iter().map(|a| AccountInfo {
        id: a.id,
        username: a.username,
        uuid: a.uuid,
    }).collect())
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
    validate_launcher_settings(&settings)?;
    write_launcher_settings(&settings)
}

#[tauri::command]
async fn start_microsoft_login(state: State<'_, Mutex<AppState>>) -> Result<AccountInfo, String> {
    let client = state.lock().map_err(|e| e.to_string())?.http.clone();
    drop(state);

    let ls = load_launcher_settings();
    let cid = ls.microsoft_client_id;
    if cid.is_empty() {
        return Err("No Microsoft client ID configured. Create an Azure app and add the client ID in Accounts -> Settings.".into());
    }

    // Start local server for OAuth callback
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://localhost:{port}");
    let state_str = uuid::Uuid::new_v4().to_string();

    // Open browser
    let auth_url = format!(
        "https://login.live.com/oauth20_authorize.srf?client_id={cid}&response_type=code&\
         redirect_uri={redirect_uri}&scope=XboxLive.signin+offline_access&\
         state={state_str}"
    );
    // ponytail: use opener plugin instead of open crate
    std::process::Command::new("cmd").args(["/C", "start", &auth_url]).spawn().ok();

    // Wait for redirect
    let code = tokio::task::spawn_blocking(move || {
        listener.set_nonblocking(false).ok();
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                return parse_auth_code(&mut s);
            }
        }
        Err("no connection received".to_string())
    }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    // Exchange auth code → Microsoft tokens
    let params: &[(&str, &str)] = &[
        ("client_id", cid.as_str()),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];
    let token_resp: serde_json::Value = client
        .post("https://login.live.com/oauth20_token.srf")
        .form(params)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("token exchange: {e}"))?;

    let ms_access = token_resp["access_token"].as_str().ok_or("no access_token")?.to_string();
    let ms_refresh = token_resp["refresh_token"].as_str().ok_or("no refresh_token")?.to_string();
    let expires_in = token_resp["expires_in"].as_u64().unwrap_or(3600);
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + expires_in;

    // Xbox Live auth
    let xbl: serde_json::Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&serde_json::json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("xbl auth: {e}"))?;
    let xbl_token = xbl["Token"].as_str().ok_or("no xbl token")?.to_string();

    // XSTS auth
    let xsts: serde_json::Value = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("xsts auth: {e}"))?;

    let xsts_token = xsts["Token"].as_str().ok_or("no xsts token")?.to_string();
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("no uhs")?.to_string();

    // Minecraft auth
    let mc: serde_json::Value = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("mc auth: {e}"))?;

    let mc_token = mc["access_token"].as_str().ok_or("no mc token")?.to_string();

    // Get profile
    let profile: serde_json::Value = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .header("Authorization", format!("Bearer {mc_token}"))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("profile: {e}"))?;

    let mc_uuid = profile["id"].as_str().ok_or("no uuid")?.to_string();
    let mc_name = profile["name"].as_str().ok_or("no name")?.to_string();
    let account_id = uuid::Uuid::new_v4().to_string();

    let account = MinecraftAccount {
        id: account_id.clone(),
        username: mc_name.clone(),
        uuid: mc_uuid,
        access_token: mc_token,
        refresh_token: ms_refresh,
        expires_at,
    };

    let mut accounts = load_accounts();
    let uuid_clone = account.uuid.clone();
    accounts.retain(|a| a.uuid != uuid_clone);
    accounts.push(account);
    save_accounts(&accounts)?;

    Ok(AccountInfo { id: account_id, username: mc_name, uuid: uuid_clone })
}

fn parse_auth_code(stream: &mut TcpStream) -> Result<String, String> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let code = request.split("?code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or("no auth code")?;
    let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>You can close this window.</h1></body></html>";
    stream.write_all(resp.as_bytes()).ok();
    Ok(code.to_string())
}

fn is_expired(expires_at: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    now + 60 > expires_at // 1 min buffer
}

async fn refresh_minecraft_token(acc: &MinecraftAccount) -> Result<String, String> {
    let client = reqwest::Client::new();
    let ls = load_launcher_settings();
    let params: &[(&str, &str)] = &[
        ("client_id", ls.microsoft_client_id.as_str()),
        ("refresh_token", acc.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
        ("redirect_uri", "http://localhost:0"),
    ];
    let resp: serde_json::Value = client
        .post("https://login.live.com/oauth20_token.srf")
        .form(params)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("refresh: {e}"))?;

    let ms_access = resp["access_token"].as_str().ok_or("no access_token")?.to_string();
    let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + expires_in;

    // Re-do Xbox → Minecraft chain with new Microsoft token
    let xbl: serde_json::Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&serde_json::json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("xbl refresh: {e}"))?;
    let xbl_token = xbl["Token"].as_str().ok_or("no xbl token")?.to_string();

    let xsts: serde_json::Value = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("xsts refresh: {e}"))?;
    let xsts_token = xsts["Token"].as_str().ok_or("no xsts token")?.to_string();
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("no uhs")?.to_string();

    let mc: serde_json::Value = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| format!("mc refresh: {e}"))?;
    let mc_token = mc["access_token"].as_str().ok_or("no mc token")?.to_string();

    // Update stored account
    let mut accounts = load_accounts();
    if let Some(stored) = accounts.iter_mut().find(|a| a.id == acc.id) {
        stored.access_token = mc_token.clone();
        stored.expires_at = expires_at;
        stored.refresh_token = resp["refresh_token"].as_str().unwrap_or(&acc.refresh_token).to_string();
    }
    save_accounts(&accounts).ok();

    Ok(mc_token)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let home = format!(" {}\\Program Files\\Java\\jdk-21", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()));
        if PathBuf::from(home.trim()).join("bin").join("java.exe").exists() {
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
    fn expand_minecraft_arguments_preserves_paths_with_spaces() {
        let template = "--username ${auth_player_name} --gameDir ${game_directory} --assetsDir ${assets_root}";
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
    fn deserialize_legacy_launcher_settings() {
        let json = r#"{ "microsoft_client_id": "abc" }"#;
        let settings: LauncherSettings =
            serde_json::from_str(json).expect("legacy launcher settings should parse");
        assert_eq!(settings.microsoft_client_id, "abc");
        assert!(matches!(settings.theme_mode, ThemeMode::Dark));
        assert!(settings.theme_overrides.background.is_none());
    }

    #[test]
    fn deserialize_full_launcher_settings() {
        let json = r##"{
            "microsoft_client_id": "abc",
            "theme_mode": "light",
            "theme_overrides": { "muted_foreground": "#b0b0b0" }
        }"##;
        let settings: LauncherSettings =
            serde_json::from_str(json).expect("full launcher settings should parse");
        assert!(matches!(settings.theme_mode, ThemeMode::Light));
        assert_eq!(
            settings.theme_overrides.muted_foreground.as_deref(),
            Some("#b0b0b0")
        );
    }

    #[test]
    fn reject_oversized_override() {
        let settings = LauncherSettings {
            microsoft_client_id: "x".into(),
            theme_mode: ThemeMode::Dark,
            theme_overrides: ThemeOverrides {
                background: Some("x".repeat(33)),
                ..ThemeOverrides::default()
            },
        };
        assert!(validate_launcher_settings(&settings).is_err());
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
}

// ── App Entry ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(AppState {
            http: reqwest::Client::new(),
            running_instances: HashSet::new(),
        }))
        .invoke_handler(tauri::generate_handler![
            get_versions,
            get_instances,
            delete_instance,
            save_settings,
            get_settings,
            download_install,
            detect_java,
            launch_instance,
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
