use crate::{emit, flatten_nested_pack, GtnhVersion};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

pub const VERSIONS_JSON_URL: &str =
    "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json";

pub fn emit_dl_progress(
    app: &AppHandle,
    stage: &str,
    pct: f64,
    operation: &str,
    id: Option<&str>,
    log_line: Option<&str>,
) {
    emit_dl_progress_with_stats(app, stage, pct, operation, id, log_line, None, None, None);
}

pub fn emit_dl_progress_with_stats(
    app: &AppHandle,
    stage: &str,
    pct: f64,
    operation: &str,
    id: Option<&str>,
    log_line: Option<&str>,
    speed_mbps: Option<f64>,
    downloaded_mb: Option<f64>,
    total_mb: Option<f64>,
) {
    let mut payload = serde_json::json!({
        "stage": stage,
        "pct": pct,
        "operation": operation,
        "id": id,
    });
    if let Some(line) = log_line {
        payload["log_line"] = serde_json::Value::String(line.to_string());
    }
    if let Some(speed) = speed_mbps {
        payload["speed_mbps"] = serde_json::json!(speed);
    }
    if let Some(downloaded) = downloaded_mb {
        payload["downloaded_mb"] = serde_json::json!(downloaded);
    }
    if let Some(total) = total_mb {
        payload["total_mb"] = serde_json::json!(total);
    }
    emit(app, "dl-progress", &payload);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub filename: String,
    pub identity: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModPreviewEntry {
    pub identity: String,
    pub filename: String,
    pub size_bytes: u64,
    pub in_persistent_overlay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModPreview {
    pub current_pack_version: String,
    pub target_pack_version: String,
    pub custom_mods: Vec<ModPreviewEntry>,
    pub new_pack_mods_count: u32,
    pub updated_pack_mods_count: u32,
    pub removed_from_pack_count: u32,
}

#[derive(Debug, Clone)]
pub struct ModClassification {
    pub custom_candidates: Vec<ModEntry>,
    pub updated_count: u32,
    pub new_count: u32,
    pub removed_count: u32,
}

fn segment_looks_like_version(segment: &str) -> bool {
    let s = segment.to_lowercase();
    if s.is_empty() {
        return false;
    }
    if s.starts_with('v') && s.len() > 1 && s[1..].chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    if s.starts_with("rv") && s.len() > 2 {
        return true;
    }
    if s.contains("beta") || s.contains("alpha") || s.contains("snapshot") || s.contains("pre") {
        return true;
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Strip trailing version/build segments from a mod jar basename.
pub fn mod_identity_from_filename(filename: &str) -> String {
    let mut name = filename.to_lowercase();
    for ext in [".jar", ".zip"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            name = stripped.to_string();
        }
    }
    for suffix in ["-client", "-universal", "-dev", "-sources"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped.to_string();
        }
    }

    let mut parts: Vec<&str> = name.split(&['-', '_'][..]).collect();
    while parts.len() > 1 {
        let last = parts[parts.len() - 1];
        if segment_looks_like_version(last) {
            parts.pop();
        } else {
            break;
        }
    }
    parts.join("-")
}

pub fn list_mods_in_dir(dir: &Path) -> Result<Vec<ModEntry>, String> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut mods = vec![];
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        let lower = filename.to_lowercase();
        if !lower.ends_with(".jar") && !lower.ends_with(".zip") {
            continue;
        }
        let size_bytes = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        mods.push(ModEntry {
            identity: mod_identity_from_filename(&filename),
            filename,
            size_bytes,
        });
    }
    mods.sort_by(|a, b| a.identity.cmp(&b.identity));
    Ok(mods)
}

pub fn resolve_mods_dir(inst_dir: &Path) -> PathBuf {
    let direct = inst_dir.join(".minecraft").join("mods");
    if direct.is_dir() {
        return direct;
    }
    if let Ok(entries) = fs::read_dir(inst_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let nested = path.join(".minecraft").join("mods");
                if nested.is_dir() {
                    return nested;
                }
            }
        }
    }
    direct
}

pub fn classify_mod_updates(old_mods: &[ModEntry], new_mods: &[ModEntry]) -> ModClassification {
    let new_ids: HashSet<&str> = new_mods.iter().map(|m| m.identity.as_str()).collect();
    let old_ids: HashSet<&str> = old_mods.iter().map(|m| m.identity.as_str()).collect();

    let mut custom_candidates = vec![];
    for m in old_mods {
        if !new_ids.contains(m.identity.as_str()) {
            custom_candidates.push(m.clone());
        }
    }

    let updated_count = old_mods
        .iter()
        .filter(|m| new_ids.contains(m.identity.as_str()))
        .count() as u32;
    let new_count = new_mods
        .iter()
        .filter(|m| !old_ids.contains(m.identity.as_str()))
        .count() as u32;
    let removed_count = old_mods
        .iter()
        .filter(|m| !new_ids.contains(m.identity.as_str()))
        .count() as u32;

    ModClassification {
        custom_candidates,
        updated_count,
        new_count,
        removed_count,
    }
}

pub async fn fetch_gtnh_versions(
    client: &reqwest::Client,
) -> Result<HashMap<String, GtnhVersion>, String> {
    let resp = client
        .get(VERSIONS_JSON_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

pub fn resolve_pack_download_url(v: &GtnhVersion, java_type: &str) -> String {
    if java_type == "java8" {
        v.mmc.java8_url.clone()
    } else {
        v.mmc.java17_2x_url.clone()
    }
}

pub async fn download_pack_to_file(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    operation: &str,
    id: Option<&str>,
) -> Result<(), String> {
    let log = match operation {
        "update" => Some("Downloading pack archive…"),
        "preview" => Some("Downloading target pack for mod comparison…"),
        "install" => Some("Downloading pack archive…"),
        _ => None,
    };
    emit_dl_progress(app, "downloading", 0.0, operation, id, log);
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let download_start = Instant::now();
    let mut last_emit = Instant::now();
    let mut last_emit_bytes = 0u64;
    let mut last_pct_emit = -1i32;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let now = Instant::now();
        let pct = if total > 0 {
            downloaded as f64 / total as f64
        } else {
            0.0
        };
        let pct_ui = (pct * 100.0) as i32;
        let time_since_emit = now.duration_since(last_emit).as_secs_f64();
        let pct_milestone = total > 0 && pct_ui / 5 > last_pct_emit / 5;
        let periodic = time_since_emit >= 0.5;

        if periodic || pct_milestone {
            let interval = now.duration_since(last_emit).as_secs_f64().max(0.05);
            let bytes_delta = downloaded.saturating_sub(last_emit_bytes);
            let instant_mbps =
                (bytes_delta as f64 / interval) / (1024.0 * 1024.0);
            let elapsed = now.duration_since(download_start).as_secs_f64().max(0.05);
            let avg_mbps = (downloaded as f64 / elapsed) / (1024.0 * 1024.0);
            let speed_mbps = if bytes_delta > 0 && interval >= 0.2 {
                instant_mbps
            } else {
                avg_mbps
            };
            let downloaded_mb = downloaded as f64 / (1024.0 * 1024.0);
            let total_mb = if total > 0 {
                Some(total as f64 / (1024.0 * 1024.0))
            } else {
                None
            };

            emit_dl_progress_with_stats(
                app,
                "downloading",
                pct,
                operation,
                id,
                None,
                Some(speed_mbps),
                Some(downloaded_mb),
                total_mb,
            );

            last_emit = now;
            last_emit_bytes = downloaded;
            if pct_milestone {
                last_pct_emit = pct_ui;
            }
        }
    }

    let downloaded_mb = downloaded as f64 / (1024.0 * 1024.0);
    let elapsed = download_start.elapsed().as_secs_f64().max(0.05);
    let avg_mbps = downloaded_mb / elapsed;
    let total_mb = if total > 0 {
        Some(total as f64 / (1024.0 * 1024.0))
    } else {
        None
    };
    let complete_log = if let Some(total_mb) = total_mb {
        format!(
            "Download complete ({downloaded_mb:.1} / {total_mb:.1} MB, avg {avg_mbps:.1} MB/s)"
        )
    } else {
        format!("Download complete ({downloaded_mb:.1} MB, avg {avg_mbps:.1} MB/s)")
    };
    emit_dl_progress_with_stats(
        app,
        "downloading",
        1.0,
        operation,
        id,
        Some(&complete_log),
        Some(avg_mbps),
        Some(downloaded_mb),
        total_mb,
    );
    Ok(())
}

pub fn extract_pack_zip(
    app: &AppHandle,
    zip_path: &Path,
    dest_dir: &Path,
    operation: &str,
    id: Option<&str>,
) -> Result<(), String> {
    let log = match operation {
        "update" => Some("Extracting pack archive…"),
        "preview" => Some("Extracting pack archive…"),
        _ => None,
    };
    emit_dl_progress(app, "extracting", 0.0, operation, id, log);
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let zip_file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;
    let total_files = archive.len();
    for i in 0..total_files {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let out_path = dest_dir.join(entry.name());
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
            let log = if operation == "preview" && (pct * 100.0) as u32 % 20 == 0 {
                Some(format!(
                    "Extracting files… {}/{}",
                    i + 1,
                    total_files
                ))
            } else {
                None
            };
            emit_dl_progress(
                app,
                "extracting",
                pct,
                operation,
                id,
                log.as_deref(),
            );
        }
    }
    if operation == "preview" {
        emit_dl_progress(
            app,
            "extracting",
            1.0,
            operation,
            id,
            Some(&format!("Extracted {total_files} files")),
        );
    }
    Ok(())
}

pub async fn download_and_extract_staging_silent(
    client: &reqwest::Client,
    pack_version: &str,
    java_type: &str,
    staging_parent: &Path,
) -> Result<PathBuf, String> {
    let versions = fetch_gtnh_versions(client).await?;
    let v = versions
        .get(pack_version)
        .ok_or_else(|| "pack version not found".to_string())?;
    let dl_url = resolve_pack_download_url(v, java_type);

    fs::create_dir_all(staging_parent).map_err(|e| e.to_string())?;
    let zip_path = staging_parent.join("pack.zip");
    let staging = staging_parent.join("staging");

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let resp = client.get(&dl_url).send().await.map_err(|e| e.to_string())?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    fs::write(&zip_path, &bytes).map_err(|e| e.to_string())?;

    let zip_file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let out_path = staging.join(entry.name());
        if entry.name().ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    flatten_nested_pack(&staging)?;
    fs::remove_file(&zip_path).ok();
    Ok(staging)
}

pub async fn download_and_extract_to_staging(
    app: &AppHandle,
    client: &reqwest::Client,
    pack_version: &str,
    java_type: &str,
    staging_parent: &Path,
    operation: &str,
    id: Option<&str>,
) -> Result<PathBuf, String> {
    let versions = fetch_gtnh_versions(client).await?;
    let v = versions
        .get(pack_version)
        .ok_or_else(|| "pack version not found".to_string())?;
    let dl_url = resolve_pack_download_url(v, java_type);

    fs::create_dir_all(staging_parent).map_err(|e| e.to_string())?;
    let zip_path = staging_parent.join("pack.zip");
    let staging = staging_parent.join("staging");

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    download_pack_to_file(app, client, &dl_url, &zip_path, operation, id).await?;
    extract_pack_zip(app, &zip_path, &staging, operation, id)?;
    flatten_nested_pack(&staging)?;
    if operation == "update" {
        emit_dl_progress(
            app,
            "updating",
            0.55,
            operation,
            id,
            Some("Pack downloaded and extracted to staging"),
        );
    }
    fs::remove_file(&zip_path).ok();
    Ok(staging)
}

pub fn wipe_dir_contents(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn copy_file_create_parent(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn copy_tree_merge(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if src.is_file() {
        return copy_file_create_parent(src, dest);
    }
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        copy_tree_merge(&entry.path(), &dest.join(entry.file_name()))?;
    }
    Ok(())
}

const PACK_ROOT_ENTRIES: &[&str] = &[
    "mmc-pack.json",
    "patches",
    "config",
    "libraries",
    "natives",
    "scripts",
    "servers",
];

pub fn apply_pack_tree_from_staging(staging: &Path, inst_dir: &Path) -> Result<(), String> {
    for name in PACK_ROOT_ENTRIES {
        let src = staging.join(name);
        if src.exists() {
            let dest = inst_dir.join(name);
            if src.is_dir() {
                if dest.exists() {
                    fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
                }
                copy_tree_merge(&src, &dest)?;
            } else {
                copy_file_create_parent(&src, &dest)?;
            }
        }
    }

    let staging_mods = resolve_mods_dir(staging);
    let inst_mods = inst_dir.join(".minecraft").join("mods");
    fs::create_dir_all(inst_mods.parent().unwrap()).map_err(|e| e.to_string())?;
    wipe_dir_contents(&inst_mods)?;
    if staging_mods.is_dir() {
        copy_tree_merge(&staging_mods, &inst_mods)?;
    }

    Ok(())
}

pub fn persistent_custom_mods_dir(inst_dir: &Path) -> PathBuf {
    inst_dir.join("persistent-minecraft").join("mods")
}

pub fn list_custom_mods(inst_dir: &Path) -> Result<Vec<ModEntry>, String> {
    list_mods_in_dir(&persistent_custom_mods_dir(inst_dir))
}

pub fn add_custom_mod(inst_dir: &Path, source: &Path) -> Result<ModEntry, String> {
    let filename = source
        .file_name()
        .ok_or("invalid mod path")?
        .to_string_lossy()
        .to_string();
    let lower = filename.to_lowercase();
    if !lower.ends_with(".jar") && !lower.ends_with(".zip") {
        return Err("only .jar and .zip mods are supported".into());
    }
    if !source.is_file() {
        return Err("mod file not found".into());
    }

    let persistent_mods = persistent_custom_mods_dir(inst_dir);
    let inst_mods = inst_dir.join(".minecraft").join("mods");
    fs::create_dir_all(&persistent_mods).map_err(|e| e.to_string())?;
    fs::create_dir_all(&inst_mods).map_err(|e| e.to_string())?;

    let dest_persistent = persistent_mods.join(&filename);
    let dest_inst = inst_mods.join(&filename);
    fs::copy(source, &dest_persistent).map_err(|e| e.to_string())?;
    fs::copy(source, &dest_inst).map_err(|e| e.to_string())?;

    Ok(ModEntry {
        identity: mod_identity_from_filename(&filename),
        filename,
        size_bytes: fs::metadata(&dest_persistent).map_err(|e| e.to_string())?.len(),
    })
}

pub fn remove_custom_mod(inst_dir: &Path, identity: &str) -> Result<(), String> {
    let identity = identity.trim().to_lowercase();
    if identity.is_empty() {
        return Err("mod identity is required".into());
    }

    let removed_persistent =
        remove_mod_files_by_identity(&persistent_custom_mods_dir(inst_dir), &identity)?;
    let removed_inst = remove_mod_files_by_identity(
        &inst_dir.join(".minecraft").join("mods"),
        &identity,
    )?;

    if removed_persistent == 0 && removed_inst == 0 {
        return Err("custom mod not found".into());
    }
    Ok(())
}

fn remove_mod_files_by_identity(dir: &Path, identity: &str) -> Result<u32, String> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut removed = 0u32;
    for entry in list_mods_in_dir(dir)? {
        if entry.identity == identity {
            fs::remove_file(dir.join(&entry.filename)).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn remove_custom_mods_except(
    persistent_mods: &Path,
    keep_identities: &HashSet<String>,
) -> Result<u32, String> {
    if !persistent_mods.is_dir() {
        return Ok(0);
    }
    let mut removed = 0u32;
    for entry in list_mods_in_dir(persistent_mods)? {
        if !keep_identities.contains(&entry.identity) {
            fs::remove_file(persistent_mods.join(&entry.filename)).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn restore_persistent_custom_mods(persistent_mods: &Path, inst_mods: &Path) -> Result<u32, String> {
    if !persistent_mods.is_dir() {
        return Ok(0);
    }
    fs::create_dir_all(inst_mods).map_err(|e| e.to_string())?;
    let mut restored = 0u32;
    for entry in fs::read_dir(persistent_mods).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        copy_file_create_parent(&entry.path(), &inst_mods.join(&filename))?;
        restored += 1;
    }
    Ok(restored)
}

pub fn apply_persistent_custom_mods(inst_dir: &Path) -> Result<u32, String> {
    restore_persistent_custom_mods(
        &persistent_custom_mods_dir(inst_dir),
        &inst_dir.join(".minecraft").join("mods"),
    )
}

pub fn build_update_preview(
    _old_mods: &[ModEntry],
    new_mods: &[ModEntry],
    persistent_mods_dir: &Path,
    current_pack_version: &str,
    target_pack_version: &str,
) -> UpdateModPreview {
    let classification = classify_mod_updates(_old_mods, new_mods);
    let custom_mods = list_mods_in_dir(persistent_mods_dir)
        .unwrap_or_default()
        .into_iter()
        .map(|m| ModPreviewEntry {
            identity: m.identity,
            filename: m.filename,
            size_bytes: m.size_bytes,
            in_persistent_overlay: true,
        })
        .collect();

    UpdateModPreview {
        current_pack_version: current_pack_version.to_string(),
        target_pack_version: target_pack_version.to_string(),
        custom_mods,
        new_pack_mods_count: classification.new_count,
        updated_pack_mods_count: classification.updated_count,
        removed_from_pack_count: classification.removed_count,
    }
}

pub fn backup_paths_for_update(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(preserve_dir).map_err(|e| e.to_string())?;

    let copies: [(&Path, &str); 4] = [
        (&inst_dir.join("instance.json"), "instance.json"),
        (&inst_dir.join("console.log"), "console.log"),
        (&inst_dir.join("persistent-minecraft"), "persistent-minecraft"),
        (&inst_dir.join(".minecraft").join("saves"), "saves"),
    ];

    for (src, rel) in copies {
        if !src.exists() {
            continue;
        }
        let dest = preserve_dir.join(rel);
        if src.is_dir() {
            if dest.exists() {
                fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
            }
            copy_tree_merge(src, &dest)?;
        } else {
            copy_file_create_parent(src, &dest)?;
        }
    }

    let mc = inst_dir.join(".minecraft");
    if mc.is_dir() {
        for name in ["options.txt", "optionsof.txt", "optionsshaders.txt"] {
            let src = mc.join(name);
            if src.is_file() {
                copy_file_create_parent(&src, &preserve_dir.join(name))?;
            }
        }
        let shots = mc.join("screenshots");
        if shots.is_dir() {
            let dest = preserve_dir.join("screenshots");
            if dest.exists() {
                fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
            }
            copy_tree_merge(&shots, &dest)?;
        }
    }

    Ok(())
}

pub fn restore_paths_from_update_backup(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    let restores: [(&str, &Path); 4] = [
        ("instance.json", &inst_dir.join("instance.json")),
        ("console.log", &inst_dir.join("console.log")),
        (
            "persistent-minecraft",
            &inst_dir.join("persistent-minecraft"),
        ),
        ("saves", &inst_dir.join(".minecraft").join("saves")),
    ];

    for (rel, dest) in restores {
        let src = preserve_dir.join(rel);
        if !src.exists() {
            continue;
        }
        if src.is_dir() {
            if dest.exists() {
                fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
            }
            copy_tree_merge(&src, dest)?;
        } else {
            copy_file_create_parent(&src, dest)?;
        }
    }

    let mc = inst_dir.join(".minecraft");
    fs::create_dir_all(&mc).map_err(|e| e.to_string())?;
    for name in ["options.txt", "optionsof.txt", "optionsshaders.txt"] {
        let src = preserve_dir.join(name);
        if src.is_file() {
            copy_file_create_parent(&src, &mc.join(name))?;
        }
    }
    let shots_src = preserve_dir.join("screenshots");
    if shots_src.is_dir() {
        copy_tree_merge(&shots_src, &mc.join("screenshots"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_identity_strips_version_suffix() {
        let a = mod_identity_from_filename("appliedenergistics2-rv3-beta-6.jar");
        let b = mod_identity_from_filename("appliedenergistics2-rv4-beta-1.jar");
        assert_eq!(a, b);
        assert_eq!(a, "appliedenergistics2");
    }

    #[test]
    fn mod_identity_strips_numeric_version() {
        let a = mod_identity_from_filename("SomeMod-1.2.3.jar");
        let b = mod_identity_from_filename("SomeMod-2.0.0.jar");
        assert_eq!(a, b);
        assert_eq!(a, "somemod");
    }

    #[test]
    fn classify_custom_candidate() {
        let old = vec![ModEntry {
            filename: "MyCustom-1.0.jar".into(),
            identity: "mycustom".into(),
            size_bytes: 100,
        }];
        let new = vec![ModEntry {
            filename: "OtherMod-2.0.jar".into(),
            identity: "othermod".into(),
            size_bytes: 200,
        }];
        let c = classify_mod_updates(&old, &new);
        assert_eq!(c.custom_candidates.len(), 1);
        assert_eq!(c.custom_candidates[0].identity, "mycustom");
        assert_eq!(c.new_count, 1);
    }

    #[test]
    fn classify_pack_updated() {
        let old = vec![ModEntry {
            filename: "Foo-1.0.jar".into(),
            identity: "foo".into(),
            size_bytes: 1,
        }];
        let new = vec![ModEntry {
            filename: "Foo-2.0.jar".into(),
            identity: "foo".into(),
            size_bytes: 2,
        }];
        let c = classify_mod_updates(&old, &new);
        assert!(c.custom_candidates.is_empty());
        assert_eq!(c.updated_count, 1);
        assert_eq!(c.new_count, 0);
    }

    #[test]
    fn preview_lists_only_persistent_custom_mods() {
        let temp = std::env::temp_dir().join(format!("pack-preview-{}", uuid::Uuid::new_v4()));
        let persistent = temp.join("persistent-minecraft").join("mods");
        std::fs::create_dir_all(&persistent).unwrap();
        std::fs::write(persistent.join("MyAddon-1.0.jar"), b"x").unwrap();

        let old = vec![ModEntry {
            filename: "notenoughIDs-2.1.10.jar".into(),
            identity: "notenoughids".into(),
            size_bytes: 1,
        }];
        let new = vec![ModEntry {
            filename: "endlessids-mc1.7.10-1.7.3.jar".into(),
            identity: "endlessids-mc".into(),
            size_bytes: 2,
        }];

        let preview = build_update_preview(&old, &new, &persistent, "2.7.0", "2.8.0");
        assert_eq!(preview.custom_mods.len(), 1);
        assert_eq!(preview.custom_mods[0].identity, "myaddon");
        assert_eq!(preview.removed_from_pack_count, 1);

        let _ = std::fs::remove_dir_all(temp);
    }
}