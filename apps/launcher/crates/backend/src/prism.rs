//! Prism Launcher import: detection, discovery, and a copy-only importer.
//!
//! Nothing here ever writes to the Prism installation. Instances are copied
//! into a staging directory first and renamed into place only after the copy
//! and metadata writes succeeded, so a cancelled or failed import leaves no
//! half-imported instance behind.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use industrialis_core::instance_settings::InstanceSettings;

use crate::fs_utils::{exists, read_json_value, remove_if_exists, write_json};
use crate::pack_files::{EmitProgress, PackProgress};
use crate::paths::{validate_instance_id, LauncherPaths};

pub const IMPORT_CANCELLED: &str = "Import cancelled; the partial copy was removed.";

const ICON_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"];

/// A detected Prism Launcher data directory plus the instances directory it
/// actually uses (honoring a custom `InstanceDir` in `prismlauncher.cfg`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismInstall {
    pub data_dir: PathBuf,
    pub instances_dir: PathBuf,
}

#[cfg(unix)]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Per-OS default locations, plus an `INDUSTRIALIS_PRISM_DIR` override used by
/// tests and portable setups. Order matters: the first readable install wins.
pub fn candidate_data_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(explicit) = std::env::var_os("INDUSTRIALIS_PRISM_DIR") {
        dirs.push(PathBuf::from(explicit));
    }
    #[cfg(windows)]
    {
        for key in ["APPDATA", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(key) {
                dirs.push(PathBuf::from(base).join("PrismLauncher"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            dirs.push(home.join("Library/Application Support/PrismLauncher"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local/share")));
        if let Some(data_home) = data_home {
            dirs.push(data_home.join("PrismLauncher"));
        }
        if let Some(home) = home_dir() {
            dirs.push(home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"));
        }
    }
    dirs
}

/// Parse the INI-ish documents Prism writes (`prismlauncher.cfg`,
/// `instance.cfg`). Section headers are ignored; every `key=value` is kept.
fn parse_ini(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('!')
            || line.starts_with('[')
        {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }
    values
}

async fn read_ini(path: &Path) -> HashMap<String, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => parse_ini(&text),
        Err(_) => HashMap::new(),
    }
}

fn ini_bool(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn ini_u32(values: &HashMap<String, String>, key: &str) -> Option<u32> {
    values
        .get(key)
        .and_then(|v| v.parse().ok())
        .filter(|v| *v > 0)
}

/// Qt may serialize path-like settings as `@ByteArray(...)`; both forms are
/// accepted, and relative values resolve against the data directory.
fn instance_dir_value(values: &HashMap<String, String>) -> Option<PathBuf> {
    let raw = values.get("InstanceDir")?.trim();
    let raw = raw
        .strip_prefix("@ByteArray(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(raw)
        .trim();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// Resolve one candidate data directory into an install. Returns `None` when
/// neither the configured nor the default instances directory exists.
pub async fn resolve_install(data_dir: &Path) -> Option<PrismInstall> {
    if !exists(data_dir).await {
        return None;
    }
    let cfg = read_ini(&data_dir.join("prismlauncher.cfg")).await;
    let configured = instance_dir_value(&cfg);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = configured {
        candidates.push(if path.is_absolute() {
            path
        } else {
            data_dir.join(path)
        });
    }
    candidates.push(data_dir.join("instances"));
    for instances_dir in candidates {
        if exists(&instances_dir).await {
            return Some(PrismInstall {
                data_dir: data_dir.to_path_buf(),
                instances_dir,
            });
        }
    }
    None
}

/// Auto-detect the first existing Prism installation.
pub async fn detect() -> Option<PrismInstall> {
    for candidate in candidate_data_dirs() {
        if let Some(install) = resolve_install(&candidate).await {
            return Some(install);
        }
    }
    None
}

/// Resolve a user-supplied folder. Accepts either a Prism data directory
/// (with an `instances` child or a configured `InstanceDir`) or the instances
/// directory itself.
pub async fn resolve_manual(path: &Path) -> Option<PrismInstall> {
    if let Some(install) = resolve_install(path).await {
        return Some(install);
    }
    if !exists(path).await {
        return None;
    }
    let mut entries = tokio::fs::read_dir(path).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        if tokio::fs::symlink_metadata(entry.path())
            .await
            .map(|meta| meta.is_dir())
            .unwrap_or(false)
            && exists(&entry.path().join("mmc-pack.json")).await
        {
            return Some(PrismInstall {
                data_dir: path.parent().unwrap_or(path).to_path_buf(),
                instances_dir: path.to_path_buf(),
            });
        }
    }
    None
}

/// Best-effort check for a running Prism process. The UI only warns.
pub fn is_prism_process_running() -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    system.processes().values().any(|process| {
        process.exe().is_some_and(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|stem| stem.contains("prismlauncher"))
        })
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackSummary {
    pub minecraft: String,
    pub loader: String,
    pub gtnh_component: bool,
}

/// Read `mmc-pack.json`: Minecraft version, primary loader, GTNH component.
pub async fn read_mmc_pack(path: &Path) -> PackSummary {
    let mut summary = PackSummary::default();
    let Some(value) = read_json_value(path).await else {
        return summary;
    };
    let Some(components) = value.get("components").and_then(|v| v.as_array()) else {
        return summary;
    };
    for component in components {
        let uid = component
            .get("uid")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let version = component
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if uid == "net.minecraft" {
            summary.minecraft = version.to_string();
        } else if uid.contains("neoforge") {
            summary.loader = format!("NeoForge {version}");
        } else if uid.contains("forge") {
            summary.loader = format!("Forge {version}");
        } else if uid.contains("fabric") {
            summary.loader = format!("Fabric {version}");
        } else if uid.contains("quilt") {
            summary.loader = format!("Quilt {version}");
        }
        if uid.contains("gtnh") || uid.contains("gtnewhorizons") {
            summary.gtnh_component = true;
        }
    }
    summary
}

/// `instgroups.json` maps group → instance folders; invert it to folder → group.
pub async fn instance_groups(instances_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(value) = read_json_value(&instances_dir.join("instgroups.json")).await else {
        return map;
    };
    let Some(groups) = value.get("groups").and_then(|v| v.as_object()) else {
        return map;
    };
    for (group, payload) in groups {
        let Some(folders) = payload.get("instances").and_then(|v| v.as_array()) else {
            continue;
        };
        for folder in folders.iter().filter_map(|v| v.as_str()) {
            map.insert(folder.to_string(), group.clone());
        }
    }
    map
}

/// True when an instance folder looks like GTNH: display name/folder mention
/// GTNH or GregTech, the pack carries a GTNH component, or the GregTech config
/// directory exists.
fn is_gtnh(name: &str, folder: &str, path: &Path, pack: &PackSummary) -> bool {
    if pack.gtnh_component {
        return true;
    }
    let label = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        folder.to_ascii_lowercase()
    );
    if label.contains("gtnh") || label.contains("gregtech") {
        return true;
    }
    path.join(".minecraft")
        .join("config")
        .join("GregTech")
        .is_dir()
}

/// Resolve the icon file to copy: an instance-local `instance-icon.*` first,
/// then Prism's shared `icons/<iconKey>` files.
async fn icon_source(
    instance: &Path,
    install: &PrismInstall,
    icon_key: Option<&String>,
) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(instance).await.ok()?;
    while let Some(entry) = entries.next_entry().await.ok().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("instance-icon") {
            continue;
        }
        let ext = name
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ICON_EXTENSIONS.contains(&ext.as_str()) {
            return Some(entry.path());
        }
    }
    let key = icon_key?.trim();
    let key = Path::new(key)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if key.is_empty() {
        return None;
    }
    let icons = install.data_dir.join("icons");
    for ext in ICON_EXTENSIONS {
        let candidate = icons.join(format!("{key}.{ext}"));
        if exists(&candidate).await {
            return Some(candidate);
        }
    }
    None
}

fn boundary(character: Option<char>) -> bool {
    character.map_or(true, |c| !c.is_ascii_alphanumeric() && c != '.')
}

/// Token match with boundaries; allows the conventional `v` prefix, so
/// `v2.8.0` matches the catalog key `2.8.0` while `2.8.01` does not.
fn contains_bounded(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let begin = start + offset;
        let end = begin + needle.len();
        let before = haystack[..begin].chars().next_back();
        let before_ok = boundary(before)
            || (before == Some('v')
                && boundary(haystack[..begin.saturating_sub(1)].chars().next_back()));
        if before_ok && boundary(haystack[end..].chars().next()) {
            return true;
        }
        if end >= haystack.len() {
            break;
        }
        start = end;
    }
    false
}

/// Longest official GTNH version mentioned in a display name, if any.
pub fn suggest_gtnh_version(name: &str, versions: &[String]) -> Option<String> {
    let haystack = name.to_ascii_lowercase();
    let mut candidates: Vec<&String> = versions
        .iter()
        .filter(|version| !version.trim().is_empty())
        .collect();
    candidates.sort_by_key(|version| std::cmp::Reverse(version.len()));
    for candidate in candidates {
        if contains_bounded(&haystack, &candidate.to_ascii_lowercase()) {
            return Some(candidate.clone());
        }
    }
    None
}

/// One discoverable Prism instance. `eligible` only requires `mmc-pack.json`;
/// non-GTNH instances are importable but flagged so the UI can warn.
#[derive(Debug, Clone, PartialEq)]
pub struct PrismInstance {
    pub folder: String,
    pub path: PathBuf,
    pub name: String,
    pub group: String,
    pub minecraft_version: String,
    pub loader: String,
    pub gtnh: bool,
    pub eligible: bool,
    pub suggested_version: Option<String>,
    pub icon_source: Option<PathBuf>,
    pub java_path: Option<String>,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    pub jvm_args: String,
}

/// List instances in a Prism instances directory. GTNH instances sort first,
/// then by display name.
pub async fn discover(
    install: &PrismInstall,
    catalog_versions: &[String],
) -> anyhow::Result<Vec<PrismInstance>> {
    let groups = instance_groups(&install.instances_dir).await;
    let mut entries = match tokio::fs::read_dir(&install.instances_dir).await {
        Ok(entries) => entries,
        Err(error) => return Err(error.into()),
    };
    let mut list = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !tokio::fs::symlink_metadata(&path)
            .await
            .map(|meta| meta.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        let eligible = exists(&path.join("mmc-pack.json")).await;
        let cfg = read_ini(&path.join("instance.cfg")).await;
        let name = cfg
            .get("name")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| folder.clone());
        let pack = read_mmc_pack(&path.join("mmc-pack.json")).await;
        let gtnh = is_gtnh(&name, &folder, &path, &pack);
        list.push(PrismInstance {
            group: groups.get(&folder).cloned().unwrap_or_default(),
            suggested_version: if gtnh {
                suggest_gtnh_version(&name, catalog_versions)
            } else {
                None
            },
            icon_source: icon_source(&path, install, cfg.get("iconKey")).await,
            java_path: cfg
                .get("JavaPath")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            min_ram_mb: ini_u32(&cfg, "MinMemAlloc"),
            max_ram_mb: ini_u32(&cfg, "MaxMemAlloc"),
            jvm_args: cfg
                .get("JvmArgs")
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            folder,
            path,
            name,
            minecraft_version: pack.minecraft,
            loader: pack.loader,
            gtnh,
            eligible,
        });
    }
    list.sort_by(|a, b| {
        b.gtnh.cmp(&a.gtnh).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    Ok(list)
}

/// Resolve a free instance id for an imported copy: sanitized base name plus
/// an always-increasing numeric suffix on collision. Never overwrites. The
/// `reserved` set covers ids chosen earlier in the same import batch, before
/// their directories exist on disk.
pub fn unique_target_id(
    paths: &LauncherPaths,
    desired: &str,
    reserved: &std::collections::HashSet<String>,
) -> anyhow::Result<String> {
    let raw = if desired.trim().is_empty() {
        "imported-instance"
    } else {
        desired
    };
    let base = validate_instance_id(raw)?;
    // `.import-*` is reserved for in-progress staging directories, which are
    // hidden from the instance list. Never produce a real id in that namespace.
    let base = if base.starts_with(".import-") {
        format!("imported-{}", &base[1..])
    } else {
        base
    };
    let instances = paths.instances_dir();
    let mut candidate = base.clone();
    let mut suffix = 1u32;
    while reserved.contains(&candidate) || instances.join(&candidate).exists() {
        suffix += 1;
        if suffix > 10_000 {
            anyhow::bail!("too many instances share that name");
        }
        candidate = format!("{base}-{suffix}");
    }
    Ok(candidate)
}

/// Everything the importer needs for one instance, decided by the UI after
/// the user confirmed the selection and version choices.
#[derive(Debug, Clone)]
pub struct PrismImportItem {
    pub source_dir: PathBuf,
    pub target_id: String,
    pub name: String,
    pub icon_source: Option<PathBuf>,
    /// When no icon exists and this is set, install the bundled GTNH logo.
    pub install_default_icon: bool,
    /// Assigned GTNH version. `None` keeps the imported copy frozen.
    pub version: Option<String>,
    /// Detected as a GTNH pack. Only GTNH copies can receive a GTNH version.
    pub gtnh: bool,
    pub java_type: Option<String>,
    pub group: Option<String>,
}

fn report(
    emit: &EmitProgress,
    id: &str,
    copied: u64,
    total: u64,
    pct: Option<f64>,
    log_line: Option<String>,
    started: Instant,
) {
    let pct = pct.unwrap_or_else(|| {
        if total == 0 {
            0.0
        } else {
            copied as f64 / total as f64
        }
    });
    let mb = copied as f64 / 1_048_576.0;
    let speed = mb / started.elapsed().as_secs_f64().max(0.001);
    emit(PackProgress {
        stage: "importing".into(),
        pct,
        operation: "import".into(),
        id: Some(id.to_string()),
        name: None,
        log_line,
        speed_mbps: Some(speed),
        downloaded_mb: Some(mb),
        total_mb: Some(total as f64 / 1_048_576.0),
    });
}

/// Copy a tree with cancellation checks between entries. Symlinks are skipped
/// so the copy cannot leave the source tree.
async fn copy_tree_cancellable(
    source: &Path,
    destination: &Path,
    emit: &EmitProgress,
    id: &str,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    let emit = emit.clone();
    let cancel = cancel.clone();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let entries = walkdir::WalkDir::new(&source)
            .follow_links(false)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let total = entries
            .iter()
            .filter(|entry| entry.file_type().is_file())
            .try_fold(0u64, |sum, entry| entry.metadata().map(|m| sum + m.len()))?;
        let started = Instant::now();
        let mut copied = 0u64;
        let mut last_report = Instant::now() - Duration::from_secs(1);
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                anyhow::bail!(IMPORT_CANCELLED);
            }
            let to = destination.join(entry.path().strip_prefix(&source)?);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&to)?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                copied += std::fs::copy(entry.path(), to)?;
                if last_report.elapsed() >= Duration::from_millis(100) {
                    last_report = Instant::now();
                    report(&emit, &id, copied, total, None, None, started);
                }
            }
        }
        report(&emit, &id, copied, total, Some(1.0), None, started);
        Ok(())
    })
    .await?
}

/// Map Prism `instance.cfg` values onto launcher settings. Prism override
/// flags are respected: values only apply when Prism says they override the
/// global defaults.
fn imported_settings(
    item: &PrismImportItem,
    cfg: &HashMap<String, String>,
    id: &str,
) -> InstanceSettings {
    let mut settings = InstanceSettings::default();
    settings.name = if item.name.trim().is_empty() {
        id.to_string()
    } else {
        item.name.trim().to_string()
    };
    settings.imported = true;
    settings.imported_gtnh = item.gtnh;
    settings.pack_version = item.version.clone().unwrap_or_default();
    settings.pack_java_type = item
        .java_type
        .clone()
        .filter(|java| !java.trim().is_empty())
        .unwrap_or_else(|| settings.pack_java_type.clone());
    if ini_bool(cfg, "OverrideJavaLocation") {
        if let Some(java) = cfg
            .get("JavaPath")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            settings.java_path = Some(java);
            settings.override_java_location = true;
        }
    }
    if ini_bool(cfg, "OverrideMemory") {
        if let Some(max) = ini_u32(cfg, "MaxMemAlloc") {
            settings.max_ram_mb = max;
            settings.min_ram_mb = ini_u32(cfg, "MinMemAlloc")
                .filter(|min| *min <= max)
                .unwrap_or_else(|| 512.min(max));
            settings.override_memory = true;
        }
    }
    if let Some(perm_gen) = ini_u32(cfg, "PermGen") {
        settings.perm_gen_mb = perm_gen;
    }
    if ini_bool(cfg, "OverrideJavaArgs") {
        if let Some(args) = cfg
            .get("JvmArgs")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            settings.jvm_args = args;
            settings.override_java_args = true;
        }
    }
    if ini_bool(cfg, "OverrideCommands") {
        let mut overridden = false;
        for (key, target) in [
            ("PreLaunchCommand", &mut settings.pre_launch_command),
            ("WrapperCommand", &mut settings.wrapper_command),
            ("PostExitCommand", &mut settings.post_exit_command),
        ] {
            if let Some(value) = cfg
                .get(key)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                *target = value;
                overridden = true;
            }
        }
        settings.override_commands = overridden;
    }
    settings.launch_maximized = ini_bool(cfg, "LaunchMaximized");
    settings.join_server_on_launch = ini_bool(cfg, "JoinServerOnLaunch");
    if settings.join_server_on_launch {
        settings.join_server_address = cfg
            .get("JoinServerOnLaunchAddress")
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
    }
    settings.auth_mode = "offline".to_string();
    settings
}

fn icon_file_name(source: &Path) -> String {
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.starts_with("instance-icon") {
        return name;
    }
    let ext = source
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .filter(|ext| ICON_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or_else(|| "png".to_string());
    format!("instance-icon.{ext}")
}

/// Import one Prism instance: copy into staging, write mapped metadata and
/// the icon, then rename into place. Cancellation and failures remove staging.
pub async fn import_instance(
    paths: &LauncherPaths,
    item: &PrismImportItem,
    emit: EmitProgress,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let id = validate_instance_id(&item.target_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    let destination = paths.instance_dir(&id)?;
    if exists(&destination).await {
        anyhow::bail!("an instance with that id already exists");
    }
    if !exists(&item.source_dir.join("mmc-pack.json")).await {
        anyhow::bail!("not a Prism instance: mmc-pack.json is missing");
    }
    let instances = paths.instances_dir();
    tokio::fs::create_dir_all(&instances).await?;
    let staging = instances.join(format!(".import-{id}-{}", uuid::Uuid::new_v4()));
    let cfg = read_ini(&item.source_dir.join("instance.cfg")).await;
    report(
        &emit,
        &id,
        0,
        0,
        Some(0.0),
        Some(format!("Importing {} as {id}", item.name)),
        Instant::now(),
    );
    let staged = async {
        copy_tree_cancellable(&item.source_dir, &staging, &emit, &id, &cancel).await?;
        let settings = imported_settings(item, &cfg, &id);
        write_json(
            &staging.join("instance.json"),
            &serde_json::to_value(&settings)?,
        )
        .await?;
        if let Some(source) = &item.icon_source {
            tokio::fs::copy(source, staging.join(icon_file_name(source))).await?;
        } else if item.install_default_icon {
            crate::icons::install_default_instance_icon(&staging).await?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = staged {
        let _ = remove_if_exists(&staging).await;
        return Err(error);
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = remove_if_exists(&staging).await;
        anyhow::bail!(IMPORT_CANCELLED);
    }
    if let Err(error) = tokio::fs::rename(&staging, &destination).await {
        let _ = remove_if_exists(&staging).await;
        return Err(error.into());
    }
    report(
        &emit,
        &id,
        0,
        0,
        Some(1.0),
        Some(format!("Finalizing {id}")),
        Instant::now(),
    );
    let result = async {
        let mut settings = crate::settings::load_instance_settings(paths, &id).await?;
        crate::install::save_and_refresh_size(paths, &id, &mut settings).await?;
        if let Some(group) = item
            .group
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
        {
            let known = crate::instances::known_instance_ids(paths).await?;
            crate::groups::set_instance_group(paths, &id, group, &known).await?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = result {
        // The instance was already renamed into place; keep it and report the
        // metadata failure instead of deleting a complete copy.
        report(
            &emit,
            &id,
            0,
            0,
            Some(1.0),
            Some(format!("Imported {id}, but finalizing failed: {error:#}")),
            Instant::now(),
        );
        return Err(error);
    }
    report(
        &emit,
        &id,
        0,
        0,
        Some(1.0),
        Some(format!("Imported {id}")),
        Instant::now(),
    );
    Ok(())
}

/// Assign (or clear) the GTNH version of an imported instance. Assigning
/// enables update checks, updates, and reinstall; clearing freezes it again.
pub async fn assign_pack_version(
    paths: &LauncherPaths,
    id: &str,
    version: Option<&str>,
    java_type: Option<&str>,
) -> anyhow::Result<()> {
    let id = validate_instance_id(id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    let paths = paths.clone();
    let version = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let java_type = java_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    crate::profile_store::mutate(async move {
        let mut settings = crate::settings::load_instance_settings(&paths, &id).await?;
        if version.is_some() && !settings.imported_gtnh {
            anyhow::bail!("GTNH versions can only be assigned to imported GTNH instances");
        }
        match version {
            Some(version) => {
                settings.pack_version = version;
                if let Some(java) = java_type {
                    settings.pack_java_type = java;
                }
            }
            None => settings.pack_version = String::new(),
        }
        crate::settings::save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use industrialis_core::instance_settings::parse_instance_settings_json;
    use std::collections::HashSet;

    struct PrismFixture {
        root: PathBuf,
    }

    impl PrismFixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "industrialis-prism-test-{name}-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(root.join("instances")).unwrap();
            Self { root }
        }

        fn instance(&self, folder: &str, cfg: &str, mmc: &str) -> PathBuf {
            let dir = self.root.join("instances").join(folder);
            std::fs::create_dir_all(dir.join(".minecraft/config")).unwrap();
            std::fs::write(dir.join("instance.cfg"), cfg).unwrap();
            std::fs::write(dir.join("mmc-pack.json"), mmc).unwrap();
            dir
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }

        fn install(&self) -> PrismInstall {
            PrismInstall {
                data_dir: self.root.clone(),
                instances_dir: self.root.join("instances"),
            }
        }

        fn cleanup(&self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-prism-app-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        LauncherPaths::new(dir)
    }

    const GTNH_PACK: &str = r#"{"formatVersion":1,"components":[
        {"uid":"net.minecraft","version":"1.7.10"},
        {"uid":"net.minecraftforge","version":"10.13.4.1614"}
    ]}"#;
    const VANILLA_PACK: &str = r#"{"formatVersion":1,"components":[
        {"uid":"net.minecraft","version":"1.20.1"}
    ]}"#;

    #[test]
    fn parses_ini_documents_with_sections_and_bytearray_paths() {
        let values = parse_ini(
            "[General]\nInstanceDir=@ByteArray(/data/prism/instances)\nConfigVersion=1.3\n# comment\n",
        );
        assert_eq!(
            instance_dir_value(&values),
            Some(PathBuf::from("/data/prism/instances"))
        );
        let plain = parse_ini("InstanceDir=C:\\Prism\\instances\nName=Game\n");
        assert_eq!(
            instance_dir_value(&plain),
            Some(PathBuf::from("C:\\Prism\\instances"))
        );
        assert_eq!(instance_dir_value(&parse_ini("Name=Game\n")), None);
    }

    #[tokio::test]
    async fn resolves_default_and_configured_instance_directories() {
        let fixture = PrismFixture::new("resolve");
        fixture.write("instances/placeholder.txt", "");
        assert_eq!(
            resolve_install(&fixture.root).await,
            Some(fixture.install())
        );

        let custom = fixture.root.join("custom-instances");
        std::fs::create_dir_all(&custom).unwrap();
        fixture.write("prismlauncher.cfg", "InstanceDir=@ByteArray(");
        let cfg = fixture.root.join("prismlauncher.cfg");
        std::fs::write(&cfg, format!("InstanceDir={}\n", custom.display())).unwrap();
        assert_eq!(
            resolve_install(&fixture.root).await.unwrap().instances_dir,
            custom
        );

        fixture.cleanup();
    }

    #[test]
    fn suggests_official_versions_only_on_token_boundaries() {
        let versions = vec!["2.8.0".to_string(), "2.7.4".to_string()];
        assert_eq!(
            suggest_gtnh_version("GTNH 2.8.0 (Java 17)", &versions),
            Some("2.8.0".to_string())
        );
        assert_eq!(
            suggest_gtnh_version("GregTech v2.7.4 pack", &versions),
            Some("2.7.4".to_string())
        );
        assert_eq!(suggest_gtnh_version("2.8.01 experimental", &versions), None);
        assert_eq!(suggest_gtnh_version("My world", &versions), None);
    }

    #[tokio::test]
    async fn discovers_instances_with_groups_icons_and_gtnh_detection() {
        let fixture = PrismFixture::new("discover");
        fixture.instance(
            "GregTech New Horizons",
            "name=GTNH 2.8.0\niconKey=gtnh\n",
            GTNH_PACK,
        );
        std::fs::create_dir_all(
            fixture
                .root
                .join("instances/GregTech New Horizons/.minecraft/config/GregTech"),
        )
        .unwrap();
        fixture.instance("Plain pack", "name=Plain pack\n", VANILLA_PACK);
        std::fs::create_dir_all(fixture.root.join("instances/No pack")).unwrap();
        std::fs::write(
            fixture.root.join("instances/No pack/instance.cfg"),
            "name=No pack\n",
        )
        .unwrap();
        fixture.write(
            "instances/instgroups.json",
            r#"{"formatVersion":1,"groups":{"GTNH":{"hidden":false,"instances":["GregTech New Horizons"]}}}"#,
        );
        fixture.write("icons/gtnh.png", "icon");

        let versions = vec!["2.8.0".to_string()];
        let list = discover(&fixture.install(), &versions).await.unwrap();
        assert_eq!(list.len(), 3);
        let gtnh = &list[0];
        assert_eq!(gtnh.folder, "GregTech New Horizons");
        assert!(gtnh.gtnh);
        assert!(gtnh.eligible);
        assert_eq!(gtnh.minecraft_version, "1.7.10");
        assert_eq!(gtnh.loader, "Forge 10.13.4.1614");
        assert_eq!(gtnh.group, "GTNH");
        assert_eq!(gtnh.suggested_version.as_deref(), Some("2.8.0"));
        assert_eq!(
            gtnh.icon_source.as_deref(),
            Some(fixture.root.join("icons/gtnh.png").as_path())
        );

        let plain = list.iter().find(|i| i.folder == "Plain pack").unwrap();
        assert!(!plain.gtnh);
        assert_eq!(plain.suggested_version, None);
        assert_eq!(plain.minecraft_version, "1.20.1");

        let no_pack = list.iter().find(|i| i.folder == "No pack").unwrap();
        assert!(!no_pack.eligible);

        fixture.cleanup();
    }

    #[tokio::test]
    async fn unique_target_id_appends_suffixes_without_overwriting() {
        let paths = temp_paths("unique");
        let instance = paths.instance_dir("GTNH").unwrap();
        std::fs::create_dir_all(&instance).unwrap();
        let empty = HashSet::new();
        assert_eq!(unique_target_id(&paths, "GTNH", &empty).unwrap(), "GTNH-2");
        assert_eq!(
            unique_target_id(&paths, "GTNH 2.8.0", &empty).unwrap(),
            "GTNH_2.8.0"
        );
        assert_eq!(
            unique_target_id(&paths, "  ", &empty).unwrap(),
            "imported-instance"
        );
        assert_eq!(
            unique_target_id(&paths, ".import-evil", &empty).unwrap(),
            "imported-import-evil"
        );
        let mut reserved = HashSet::new();
        reserved.insert("GTNH-2".to_string());
        assert_eq!(
            unique_target_id(&paths, "GTNH", &reserved).unwrap(),
            "GTNH-3"
        );
        let _ = std::fs::remove_dir_all(paths.base());
    }

    #[test]
    fn imported_settings_accept_an_assigned_version_and_java_type() {
        let cfg = parse_ini("name=GTNH 2.8.0\n");
        let item = PrismImportItem {
            source_dir: PathBuf::from("unused"),
            target_id: "gtnh".to_string(),
            name: "GTNH 2.8.0".to_string(),
            icon_source: None,
            install_default_icon: false,
            version: Some("2.8.0".to_string()),
            gtnh: true,
            java_type: Some("java17+".to_string()),
            group: None,
        };
        let settings = imported_settings(&item, &cfg, "gtnh");
        assert!(settings.imported);
        assert!(settings.imported_gtnh);
        assert_eq!(settings.pack_version, "2.8.0");
        assert_eq!(settings.pack_java_type, "java17+");
    }

    #[tokio::test]
    async fn imports_one_instance_with_mapped_metadata_icon_and_group() {
        let fixture = PrismFixture::new("import");
        let source = fixture.instance(
            "GregTech New Horizons",
            "name=GTNH 2.8.0\nOverrideMemory=true\nMinMemAlloc=512\nMaxMemAlloc=7168\nOverrideJavaLocation=true\nJavaPath=C:\\Java\\8\\bin\\java.exe\nOverrideJavaArgs=true\nJvmArgs=-Xss1M\nLaunchMaximized=true\niconKey=gtnh\n",
            GTNH_PACK,
        );
        fixture.write("icons/gtnh.png", "icon-bytes");

        let paths = temp_paths("import-one");
        let emit: EmitProgress = Arc::new(|_| {});
        let item = PrismImportItem {
            source_dir: source.clone(),
            target_id: "GTNH-copy".to_string(),
            name: "GTNH 2.8.0".to_string(),
            icon_source: Some(fixture.root.join("icons/gtnh.png")),
            install_default_icon: false,
            version: None,
            gtnh: true,
            java_type: None,
            group: Some("GTNH".to_string()),
        };
        import_instance(&paths, &item, emit, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();

        let target = paths.instance_dir("GTNH-copy").unwrap();
        assert!(target.join("mmc-pack.json").exists());
        assert!(source.join("mmc-pack.json").exists(), "source untouched");
        let settings: InstanceSettings = parse_instance_settings_json(
            &std::fs::read_to_string(target.join("instance.json")).unwrap(),
        );
        assert!(settings.imported);
        assert!(settings.imported_gtnh);
        assert_eq!(settings.name, "GTNH 2.8.0");
        assert_eq!(settings.pack_version, "");
        assert!(settings.override_memory);
        assert_eq!(settings.min_ram_mb, 512);
        assert_eq!(settings.max_ram_mb, 7168);
        assert_eq!(
            settings.java_path.as_deref(),
            Some("C:\\Java\\8\\bin\\java.exe")
        );
        assert!(settings.override_java_args);
        assert_eq!(settings.jvm_args, "-Xss1M");
        assert!(settings.launch_maximized);
        assert_eq!(settings.auth_mode, "offline");
        assert!(target.join("instance-icon.png").exists());
        assert!(settings.cached_size_bytes > 0);

        let groups = crate::groups::groups_state(&paths).await.unwrap();
        assert_eq!(
            groups.instance_groups.get("GTNH-copy").map(String::as_str),
            Some("GTNH")
        );

        let _ = std::fs::remove_dir_all(paths.base());
        fixture.cleanup();
    }

    #[tokio::test]
    async fn cancelled_import_removes_staging_and_never_touches_the_source() {
        let fixture = PrismFixture::new("cancelled");
        let source = fixture.instance("Pack", "name=Pack\n", VANILLA_PACK);
        std::fs::write(source.join("payload.bin"), vec![7u8; 512 * 1024]).unwrap();

        let paths = temp_paths("cancelled");
        let emit: EmitProgress = Arc::new(|_| {});
        let cancel = Arc::new(AtomicBool::new(true));
        let item = PrismImportItem {
            source_dir: source.clone(),
            target_id: "pack".to_string(),
            name: "Pack".to_string(),
            icon_source: None,
            install_default_icon: false,
            version: None,
            gtnh: false,
            java_type: None,
            group: None,
        };
        let error = import_instance(&paths, &item, emit, cancel)
            .await
            .unwrap_err();
        assert!(error.to_string().starts_with("Import cancelled;"));
        assert!(!paths.instance_dir("pack").unwrap().exists());
        let leftovers: Vec<_> = std::fs::read_dir(paths.instances_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            leftovers.iter().all(|name| !name.starts_with(".import-")),
            "staging cleaned up: {leftovers:?}"
        );
        assert_eq!(
            std::fs::read(source.join("payload.bin")).unwrap().len(),
            512 * 1024
        );

        let _ = std::fs::remove_dir_all(paths.base());
        fixture.cleanup();
    }

    #[tokio::test]
    async fn assigning_and_clearing_a_pack_version_toggles_frozen_state() {
        let fixture = PrismFixture::new("assign");
        let source = fixture.instance("Pack", "name=GTNH pack\n", GTNH_PACK);
        let paths = temp_paths("assign");
        let emit: EmitProgress = Arc::new(|_| {});
        let item = PrismImportItem {
            source_dir: source,
            target_id: "pack".to_string(),
            name: "GTNH pack".to_string(),
            icon_source: None,
            install_default_icon: false,
            version: None,
            gtnh: true,
            java_type: None,
            group: None,
        };
        import_instance(&paths, &item, emit, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();

        assign_pack_version(&paths, "pack", Some("2.8.0"), Some("java17+"))
            .await
            .unwrap();
        let list = crate::instances::load_instances(&paths).await.unwrap();
        assert_eq!(list[0].settings.pack_version, "2.8.0");
        assert_eq!(list[0].settings.pack_java_type, "java17+");
        assert!(list[0].settings.imported);

        assign_pack_version(&paths, "pack", None, None)
            .await
            .unwrap();
        let list = crate::instances::load_instances(&paths).await.unwrap();
        assert_eq!(list[0].settings.pack_version, "");
        assert!(list[0].settings.imported);

        let _ = std::fs::remove_dir_all(paths.base());
        fixture.cleanup();
    }

    #[tokio::test]
    async fn non_gtnh_imports_stay_frozen_and_reject_version_assignment() {
        let fixture = PrismFixture::new("non-gtnh");
        let source = fixture.instance("Plain", "name=Plain pack\n", VANILLA_PACK);
        let paths = temp_paths("non-gtnh");
        let emit: EmitProgress = Arc::new(|_| {});
        let item = PrismImportItem {
            source_dir: source,
            target_id: "Plain_pack".to_string(),
            name: "Plain pack".to_string(),
            icon_source: None,
            install_default_icon: false,
            version: None,
            gtnh: false,
            java_type: None,
            group: None,
        };
        import_instance(&paths, &item, emit, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();

        let settings = crate::settings::load_instance_settings(&paths, "Plain_pack")
            .await
            .unwrap();
        assert!(settings.imported);
        assert!(!settings.imported_gtnh);
        assert_eq!(settings.pack_version, "");

        let error = assign_pack_version(&paths, "Plain_pack", Some("2.8.0"), None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("imported GTNH"));

        let _ = std::fs::remove_dir_all(paths.base());
        fixture.cleanup();
    }
}
