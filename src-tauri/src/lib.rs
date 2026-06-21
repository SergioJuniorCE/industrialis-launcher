use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
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
pub struct LauncherSettings {
    pub microsoft_client_id: String,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            microsoft_client_id: String::new(),
        }
    }
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
    version: String,
}

#[derive(Debug, Deserialize)]
struct PatchJson {
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    uid: Option<String>,
    #[allow(dead_code)]
    version: Option<String>,
    #[serde(rename = "mainClass")]
    main_class: Option<String>,
    #[serde(rename = "+mainClass")]
    plus_main_class: Option<String>,
    libraries: Option<Vec<PatchLibrary>>,
    #[serde(rename = "+args")]
    plus_args: Option<Vec<String>>,
    #[serde(rename = "-args")]
    minus_args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct PatchLibrary {
    name: String,
}

// ── App State ──

struct AppState {
    http: reqwest::Client,
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

fn java_path() -> Option<String> {
    std::env::var("JAVA_HOME").ok().map(|jh| {
        let p = PathBuf::from(&jh).join("bin").join("java");
        p.to_string_lossy().to_string()
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

fn parse_mmc_pack(inst_dir: &Path) -> Result<LaunchConfig, String> {
    let pack_dir = mmc_pack_dir(inst_dir);
    let mmc_path = pack_dir.join("mmc-pack.json");
    let mmc: MmcPackJson = serde_json::from_str(
        &fs::read_to_string(&mmc_path).map_err(|e| format!("missing mmc-pack.json: {e}"))?
    ).map_err(|e| format!("bad mmc-pack.json: {e}"))?;

    let mut main_class = "net.minecraft.launchwrapper.Launch".to_string();
    let mut extra_jvm_args: Vec<String> = Vec::new();
    let mut mc_version = "1.12.2".to_string();

    for comp in &mmc.components {
        let patch_path = pack_dir.join("patches").join(format!("{}.json", comp.uid));
        if patch_path.exists() {
            let raw = fs::read_to_string(&patch_path).map_err(|e| format!("patch read: {e}"))?;
            if let Ok(patch) = serde_json::from_str::<PatchJson>(&raw) {
                if let Some(mc) = patch.plus_main_class.or(patch.main_class) {
                    main_class = mc;
                }
                if let Some(args) = patch.plus_args {
                    extra_jvm_args.extend(args);
                }
            }
        }
        if comp.uid == "net.minecraft" {
            mc_version = comp.version.clone();
        }
    }

    // Gather all library jars
    let lib_dir = pack_dir.join("libraries");
    let mut libraries = Vec::new();
    if lib_dir.exists() {
        collect_jars(&lib_dir, &mut libraries);
    }

    // Also include Minecraft jar
    let mc_jar = pack_dir.join(".minecraft").join("versions").join(&mc_version).join(format!("{mc_version}.jar"));
    if mc_jar.exists() {
        libraries.push(mc_jar.to_string_lossy().to_string());
    }

    // Handle lwjgl3ify natives if present
    let natives_dir = pack_dir.join(".minecraft").join("bin");
    if natives_dir.exists() {
        collect_jars(&natives_dir, &mut libraries);
    }

    let game_dir = pack_dir.join(".minecraft").to_string_lossy().to_string();
    let assets_dir = pack_dir.join(".minecraft").join("assets").to_string_lossy().to_string();

    // Remove --tweakClass from extra_jvm_args; they go in program args
    let mut program_args = Vec::new();
    let mut i = 0;
    while i < extra_jvm_args.len() {
        if extra_jvm_args[i] == "--tweakClass" && i + 1 < extra_jvm_args.len() {
            program_args.push("--tweakClass".to_string());
            program_args.push(extra_jvm_args[i + 1].clone());
            i += 2;
        } else if extra_jvm_args[i].starts_with("--tweakClass=") {
            program_args.push(extra_jvm_args[i].clone());
            i += 1;
        } else {
            i += 1;
        }
    }

    let mut jvm_args: Vec<String> = extra_jvm_args.iter()
        .filter(|a| !a.starts_with("--tweakClass"))
        .cloned()
        .collect();
    // ponytail: all patch args kept; some may be noise, trim when launch fails
    jvm_args.extend(program_args.iter().cloned());

    Ok(LaunchConfig {
        main_class,
        minecraft_version: mc_version,
        libraries,
        game_dir,
        assets_dir,
        jvm_args,
    })
}

fn collect_jars(dir: &Path, jars: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_jars(&path, jars);
            } else if path.extension().is_some_and(|e| e == "jar") {
                jars.push(path.to_string_lossy().to_string());
            }
        }
    }
}

#[tauri::command]
async fn launch_instance(version: String) -> Result<(), String> {
    let inst_dir = instance_dir(&version);
    if !inst_dir.exists() {
        return Err("instance not installed".into());
    }

    let settings = load_settings(&version).ok_or("no settings")?;
    let java = settings.java_path.clone()
        .or_else(java_path)
        .ok_or("no Java configured or found")?;

    let config = parse_mmc_pack(&inst_dir)?;

    // Build classpath
    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = config.libraries.join(sep);

    // Build JVM args
    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xms{}M", settings.min_ram_mb));
    args.push(format!("-Xmx{}M", settings.max_ram_mb));
    if !settings.jvm_args.is_empty() {
        args.extend(settings.jvm_args.split_whitespace().map(String::from));
    }
    args.extend(config.jvm_args);
    args.push("-cp".to_string());
    args.push(classpath);
    args.push(config.main_class);

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

    // Minecraft program args
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

    // Spawn
    let child = std::process::Command::new(&java)
        .args(&args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| format!("launch failed: {e}"))?;

    let _ = child.wait_with_output();
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

// ── App Entry ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(AppState {
            http: reqwest::Client::new(),
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
            get_accounts,
            remove_account,
            start_microsoft_login,
            get_launcher_settings,
            save_launcher_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
