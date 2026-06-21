use std::collections::HashMap;
use std::fs;
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
    // ponytail: no GC tuning; add -XX:G1GC etc when 8GB+ needed
    if !settings.jvm_args.is_empty() {
        args.extend(settings.jvm_args.split_whitespace().map(String::from));
    }
    args.extend(config.jvm_args);
    args.push("-cp".to_string());
    args.push(classpath);
    args.push(config.main_class);

    // ponytail: offline auth only; Microsoft auth slot open
    let username = if settings.auth_mode == "offline" {
        settings.username.clone()
    } else {
        settings.username.clone()
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
    args.push("0".to_string()); // ponytail: offline token, real token when Microsoft auth added
    args.push("--uuid".to_string());
    args.push("00000000-0000-0000-0000-000000000000".into());
    args.push("--userType".to_string());
    args.push("mojang".to_string());

    // Spawn
    let child = std::process::Command::new(&java)
        .args(&args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| format!("launch failed: {e}"))?;

    // TODO: capture output and emit events
    let _ = child.wait_with_output();

    Ok(())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
