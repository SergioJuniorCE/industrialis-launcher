//! Pack acquisition helpers: version catalog, mod inventory, zip extraction,
//! staging, and update previews. Ports the corresponding parts of
//! `electron/backend/pack.ts` (install/update orchestration lives in
//! [`crate::install`]).

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::fs_utils::{copy_tree, exists, remove_if_exists};
use crate::pack_cache::{copy_cached_pack_to_staging, lookup_pack_cache, store_pack_cache};
use crate::pack_prep::{flatten_nested_pack, persistent_custom_mods_dir};
use crate::paths::{sanitize_name, LauncherPaths};

pub const VERSIONS_URL: &str =
    "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GtnhVersion {
    pub title: String,
    pub description: String,
    pub release_date: String,
    pub max_java_version: u32,
    pub mmc: GtnhMmcUrls,
    pub client: GtnhClientUrls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GtnhMmcUrls {
    pub java8_url: String,
    #[serde(rename = "java17_2XUrl")]
    pub java17_2x_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GtnhClientUrls {
    pub java8_url: String,
}

pub async fn fetch_gtnh_versions(
) -> Result<std::collections::HashMap<String, GtnhVersion>, anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("IndustrialisLauncher/0.1")
        .build()?;
    let response = client
        .get(VERSIONS_URL)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch pack versions: {e}"))?;
    if !response.status().is_success() {
        anyhow::bail!("failed to fetch pack versions: HTTP {}", response.status());
    }
    Ok(response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("invalid pack versions catalog: {e}"))?)
}

pub fn resolve_pack_download_url(version: &GtnhVersion, java_type: &str) -> String {
    if java_type == "java8" {
        version.mmc.java8_url.clone()
    } else {
        version.mmc.java17_2x_url.clone()
    }
}

// ── Mod inventory ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModEntry {
    pub filename: String,
    pub identity: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModPreviewEntry {
    #[serde(flatten)]
    pub entry: ModEntry,
    pub in_persistent_overlay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateModPreview {
    pub current_pack_version: String,
    pub target_pack_version: String,
    pub custom_mods: Vec<ModPreviewEntry>,
    pub new_pack_mods_count: usize,
    pub updated_pack_mods_count: usize,
    pub removed_from_pack_count: usize,
    /// Mods in the target pack that the instance excludes.
    pub kept_out_count: usize,
}

fn mod_patterns() -> &'static (Regex, Regex, Regex, Regex) {
    static CELL: OnceLock<(Regex, Regex, Regex, Regex)> = OnceLock::new();
    CELL.get_or_init(|| {
        (
            Regex::new(r"\.(jar|zip)$").unwrap(),
            Regex::new(r"^v\d").unwrap(),
            Regex::new(r"^rv.+").unwrap(),
            Regex::new(r"beta|alpha|snapshot|pre").unwrap(),
        )
    })
}

/// Mirrors `modIdentityFromFilename`.
pub fn mod_identity_from_filename(filename: &str) -> String {
    let (ext, v_prefix, rv_prefix, prerelease) = mod_patterns();
    let mut name = ext
        .replace(filename.to_lowercase().as_str(), "")
        .into_owned();
    for suffix in ["-client", "-universal", "-dev", "-sources"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped.to_string();
        }
    }
    let looks_like_version = |segment: &str| {
        if segment.is_empty() {
            return false;
        }
        if v_prefix.is_match(segment) || rv_prefix.is_match(segment) {
            return true;
        }
        if prerelease.is_match(segment) {
            return true;
        }
        segment.starts_with(|c: char| c.is_ascii_digit())
    };
    let mut parts: Vec<&str> = name.split(['-', '_']).collect();
    while parts.len() > 1 && looks_like_version(parts[parts.len() - 1]) {
        parts.pop();
    }
    parts.join("-")
}

/// Mirrors `listMods` (jar/zip files, sorted by identity).
pub async fn list_mods(dir: &Path) -> Result<Vec<ModEntry>, anyhow::Error> {
    if !exists(dir).await {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut mods = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        if !tokio::fs::symlink_metadata(entry.path())
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
            continue;
        }
        let size = tokio::fs::metadata(entry.path())
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        mods.push(ModEntry {
            identity: mod_identity_from_filename(&name),
            filename: name,
            size_bytes: size,
        });
    }
    mods.sort_by(|a, b| a.identity.cmp(&b.identity));
    Ok(mods)
}

/// Split a mod list into deployable mods and excluded-identity matches.
pub fn split_excluded_mods(
    mods: Vec<ModEntry>,
    excluded: &HashSet<String>,
) -> (Vec<ModEntry>, usize) {
    let mut kept = Vec::with_capacity(mods.len());
    let mut kept_out = 0;
    for entry in mods {
        if excluded.contains(&entry.identity) {
            kept_out += 1;
        } else {
            kept.push(entry);
        }
    }
    (kept, kept_out)
}

/// Mirrors `classifyModUpdates` → (updated, added, removed).
pub fn classify_mod_updates(old_mods: &[ModEntry], new_mods: &[ModEntry]) -> (usize, usize, usize) {
    let old_ids: HashSet<&str> = old_mods.iter().map(|m| m.identity.as_str()).collect();
    let new_ids: HashSet<&str> = new_mods.iter().map(|m| m.identity.as_str()).collect();
    (
        old_mods
            .iter()
            .filter(|m| new_ids.contains(m.identity.as_str()))
            .count(),
        new_mods
            .iter()
            .filter(|m| !old_ids.contains(m.identity.as_str()))
            .count(),
        old_mods
            .iter()
            .filter(|m| !new_ids.contains(m.identity.as_str()))
            .count(),
    )
}

pub async fn list_custom_mods(instance: &Path) -> Result<Vec<ModEntry>, anyhow::Error> {
    list_mods(&persistent_custom_mods_dir(instance)).await
}

pub async fn add_custom_mod(instance: &Path, source: &Path) -> Result<ModEntry, anyhow::Error> {
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("only .jar and .zip mods are supported"))?;
    let lower = filename.to_lowercase();
    if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
        anyhow::bail!("only .jar and .zip mods are supported");
    }
    let meta = tokio::fs::metadata(source)
        .await
        .map_err(|_| anyhow::anyhow!("mod file not found"))?;
    if !meta.is_file() {
        anyhow::bail!("mod file not found");
    }
    let persistent = persistent_custom_mods_dir(instance);
    let active = instance.join(".minecraft").join("mods");
    tokio::fs::create_dir_all(&persistent).await?;
    tokio::fs::create_dir_all(&active).await?;
    tokio::fs::copy(source, persistent.join(filename)).await?;
    tokio::fs::copy(source, active.join(filename)).await?;
    let size = tokio::fs::metadata(persistent.join(filename))
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(ModEntry {
        filename: filename.to_string(),
        identity: mod_identity_from_filename(filename),
        size_bytes: size,
    })
}

pub async fn remove_custom_mod(instance: &Path, identity: &str) -> Result<(), anyhow::Error> {
    let normalized = identity.trim().to_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("mod identity is required");
    }
    let mut removed = 0;
    for dir in [
        persistent_custom_mods_dir(instance),
        instance.join(".minecraft").join("mods"),
    ] {
        for entry in list_mods(&dir).await? {
            if entry.identity == normalized {
                tokio::fs::remove_file(dir.join(&entry.filename)).await?;
                removed += 1;
            }
        }
    }
    if removed == 0 {
        anyhow::bail!("custom mod not found");
    }
    Ok(())
}

/// Mirrors `removeCustomModsExcept` (persistent overlay only). Returns removed count.
pub async fn remove_custom_mods_except(
    dir: &Path,
    keep: &HashSet<String>,
) -> Result<usize, anyhow::Error> {
    let mut removed = 0;
    for entry in list_mods(dir).await? {
        if !keep.contains(&entry.identity) {
            tokio::fs::remove_file(dir.join(&entry.filename)).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Mirrors `restorePersistentCustomMods`. Returns restored count.
pub async fn restore_persistent_custom_mods(
    persistent: &Path,
    active: &Path,
) -> Result<usize, anyhow::Error> {
    if !exists(persistent).await {
        return Ok(0);
    }
    tokio::fs::create_dir_all(active).await?;
    let mut restored = 0;
    let mut entries = tokio::fs::read_dir(persistent).await?;
    while let Some(entry) = entries.next_entry().await? {
        if tokio::fs::symlink_metadata(entry.path())
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            tokio::fs::copy(entry.path(), active.join(entry.file_name())).await?;
            restored += 1;
        }
    }
    Ok(restored)
}

pub async fn resolve_mods_dir(instance: &Path) -> PathBuf {
    let direct = instance.join(".minecraft").join("mods");
    if exists(&direct).await {
        return direct;
    }
    if let Ok(mut entries) = tokio::fs::read_dir(instance).await {
        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let nested = entry.path().join(".minecraft").join("mods");
            if tokio::fs::symlink_metadata(entry.path())
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false)
                && exists(&nested).await
            {
                return nested;
            }
        }
    }
    direct
}

// ── Progress + downloads ──

/// Mirrors the TS `DownloadProgress` event shape.
#[derive(Debug, Clone, Serialize)]
pub struct PackProgress {
    pub stage: String,
    pub pct: f64,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded_mb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_mb: Option<f64>,
}

pub type EmitProgress = Arc<dyn Fn(PackProgress) + Send + Sync>;

#[derive(Clone)]
pub struct ProgressCtx {
    pub emit: EmitProgress,
    pub operation: String,
    pub id: Option<String>,
}

impl ProgressCtx {
    pub fn progress(&self, stage: &str, pct: f64, extra: impl FnOnce(&mut PackProgress)) {
        let mut payload = PackProgress {
            stage: stage.to_string(),
            pct,
            operation: self.operation.clone(),
            id: self.id.clone(),
            name: None,
            log_line: None,
            speed_mbps: None,
            downloaded_mb: None,
            total_mb: None,
        };
        extra(&mut payload);
        (self.emit)(payload);
    }
}

const DOWNLOAD_ATTEMPTS: u32 = 3;
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_PACK_BYTES: u64 = 4 * 1024 * 1024 * 1024;

async fn download_file(
    ctx: &ProgressCtx,
    url: &str,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .user_agent("IndustrialisLauncher/0.1")
        .build()?;
    download_with_retry(ctx, &client, url, destination).await
}

async fn download_with_retry(
    ctx: &ProgressCtx,
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    let mut last_error = None;
    for attempt in 0..DOWNLOAD_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
            ctx.progress("downloading", 0.0, |p| {
                p.log_line = Some(format!(
                    "Retrying pack download ({}/{DOWNLOAD_ATTEMPTS})",
                    attempt + 1
                ))
            });
        }
        match download_attempt(ctx, client, url, destination).await {
            Ok(()) => return Ok(()),
            Err((error, retryable)) => {
                last_error = Some(error);
                if !retryable {
                    break;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("pack download failed")))
}

fn content_range_total(value: &str) -> Option<u64> {
    value.rsplit_once('/')?.1.parse().ok()
}

async fn download_attempt(
    ctx: &ProgressCtx,
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), (anyhow::Error, bool)> {
    let existing = tokio::fs::metadata(destination)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = tokio::time::timeout(DOWNLOAD_IDLE_TIMEOUT, request.send())
        .await
        .map_err(|_| (anyhow::anyhow!("pack download timed out while connecting"), true))?
        .map_err(|error| (anyhow::anyhow!("pack download failed: {error}"), true))?;
    let status = response.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && existing > 0 {
        let _ = tokio::fs::remove_file(destination).await;
        return Err((
            anyhow::anyhow!("pack download resume point was rejected"),
            true,
        ));
    }
    if !status.is_success() {
        let retryable = status.is_server_error()
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
        return Err((
            anyhow::anyhow!("pack download failed: HTTP {status}"),
            retryable,
        ));
    }
    let resuming = status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0;
    if status == reqwest::StatusCode::PARTIAL_CONTENT && !resuming {
        return Err((
            anyhow::anyhow!("pack server returned an unexpected partial response"),
            true,
        ));
    }
    let total = if resuming {
        response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(content_range_total)
            .or_else(|| response.content_length().map(|length| existing + length))
    } else {
        response.content_length()
    };
    if total.is_some_and(|total| total > MAX_PACK_BYTES) {
        return Err((
            anyhow::anyhow!("pack archive is larger than the supported limit"),
            false,
        ));
    }
    let mut response = response;
    let mut downloaded = if resuming { existing } else { 0 };
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            (
                anyhow::anyhow!("could not create the pack download directory: {error}"),
                false,
            )
        })?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(destination)
        .await
        .map_err(|error| {
            (
                anyhow::anyhow!("could not open the pack download: {error}"),
                false,
            )
        })?;
    let pct = total
        .filter(|total| *total > 0)
        .map(|total| downloaded as f64 / total as f64)
        .unwrap_or(0.0);
    ctx.progress("downloading", pct, |p| {
        p.log_line = Some(if resuming {
            format!("Resuming pack archive at {downloaded} bytes…")
        } else {
            "Downloading pack archive…".to_string()
        });
        p.downloaded_mb = Some(downloaded as f64 / 1024.0 / 1024.0);
        p.total_mb = total.map(|total| total as f64 / 1024.0 / 1024.0);
    });
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    loop {
        let chunk = tokio::time::timeout(DOWNLOAD_IDLE_TIMEOUT, response.chunk())
            .await
            .map_err(|_| (anyhow::anyhow!("pack download stalled"), true))?
            .map_err(|error| (anyhow::anyhow!("pack download failed: {error}"), true))?;
        let Some(bytes) = chunk else { break };
        if downloaded + bytes.len() as u64 > MAX_PACK_BYTES {
            return Err((
                anyhow::anyhow!("pack download exceeded the supported size limit"),
                false,
            ));
        }
        tokio::io::AsyncWriteExt::write_all(&mut file, &bytes)
            .await
            .map_err(|error| {
                (
                    anyhow::anyhow!("could not write the pack download: {error}"),
                    false,
                )
            })?;
        downloaded += bytes.len() as u64;
        let pct = total
            .filter(|total| *total > 0)
            .map(|total| downloaded as f64 / total as f64)
            .unwrap_or(0.0);
        let now = Instant::now();
        if now - last_emit >= Duration::from_millis(500) || (pct * 100.0).floor() as u64 % 5 == 0 {
            let elapsed = now.duration_since(started).as_secs_f64().max(0.05);
            let transferred = downloaded.saturating_sub(if resuming { existing } else { 0 });
            let speed = transferred as f64 / elapsed / 1024.0 / 1024.0;
            ctx.progress("downloading", pct, |p| {
                p.speed_mbps = Some(speed);
                p.downloaded_mb = Some(downloaded as f64 / 1024.0 / 1024.0);
                p.total_mb = total.map(|total| total as f64 / 1024.0 / 1024.0);
            });
            last_emit = now;
        }
    }
    // Extraction opens a new handle immediately after this function returns.
    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|error| {
            (
                anyhow::anyhow!("could not flush the pack download: {error}"),
                false,
            )
        })?;
    drop(file);
    if let Some(total) = total {
        if downloaded != total {
            return Err((
                anyhow::anyhow!("pack download stopped at {downloaded} of {total} bytes"),
                true,
            ));
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(0.05);
    let mb = downloaded as f64 / 1024.0 / 1024.0;
    ctx.progress("downloading", 1.0, |p| {
        p.log_line = Some(format!(
            "Download complete ({mb:.1} MB, avg {:.1} MB/s)",
            downloaded as f64 / elapsed / 1024.0 / 1024.0
        ));
        p.speed_mbps = Some(downloaded as f64 / elapsed / 1024.0 / 1024.0);
        p.downloaded_mb = Some(mb);
        p.total_mb = total.map(|total| total as f64 / 1024.0 / 1024.0);
    });
    Ok(())
}

/// Zip extraction runs on a blocking thread; progress crosses via callback.
/// `enclosed_name` rejects zip-slip entries (matches the TS `..`/absolute skips).
pub async fn extract_pack_zip(
    ctx: &ProgressCtx,
    zip_path: &Path,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    ctx.progress("extracting", 0.0, |p| {
        p.log_line = Some("Extracting pack archive…".to_string())
    });
    let zip_path = zip_path.to_path_buf();
    let destination = destination.to_path_buf();
    let progress = {
        let ctx = ctx.clone();
        Arc::new(move |done: usize, total: usize| {
            let pct = if total > 0 {
                done as f64 / total as f64
            } else {
                0.0
            };
            if (pct * 100.0).floor() as usize % 10 == 0 || done == total {
                ctx.progress("extracting", pct, |_| {});
            }
        }) as Arc<dyn Fn(usize, usize) + Send + Sync>
    };
    tokio::task::spawn_blocking(move || extract_zip_blocking(&zip_path, &destination, &progress))
        .await?
}

fn extract_zip_blocking(
    zip_path: &Path,
    destination: &Path,
    progress: &Arc<dyn Fn(usize, usize) + Send + Sync>,
) -> Result<(), anyhow::Error> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))?;
    let total = archive.len();
    for index in 0..total {
        let mut entry = archive.by_index(index)?;
        let Some(safe_path) = entry.enclosed_name() else {
            progress(index + 1, total);
            continue;
        };
        let target = destination.join(safe_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        progress(index + 1, total);
    }
    Ok(())
}

pub async fn install_staging_contents(
    staging: &Path,
    instance: &Path,
) -> Result<(), anyhow::Error> {
    let mut entries = tokio::fs::read_dir(staging).await?;
    while let Some(entry) = entries.next_entry().await? {
        let target = instance.join(entry.file_name());
        remove_if_exists(&target).await?;
        copy_tree(&entry.path(), &target).await?;
    }
    Ok(())
}

/// Mirrors `downloadAndExtractToStaging` (cache → download → extract → cache).
/// Returns the staging dir. `staging_parent` is the instance dir for
/// installs/updates, or the `.update-preview` dir for previews.
pub async fn download_and_extract_to_staging(
    paths: &LauncherPaths,
    ctx: &ProgressCtx,
    pack_version: &str,
    java_type: &str,
    staging_parent: &Path,
    operation: &str,
) -> Result<PathBuf, anyhow::Error> {
    ctx.progress("downloading", 0.0, |p| {
        p.log_line = Some("Fetching the GTNH pack catalog…".to_string())
    });
    let versions = fetch_gtnh_versions().await?;
    let version = versions
        .get(pack_version)
        .ok_or_else(|| anyhow::anyhow!("pack version not found"))?;
    let url = resolve_pack_download_url(version, java_type);
    ctx.progress("downloading", 0.0, |p| {
        p.log_line = Some(format!("Resolved pack {pack_version} ({java_type})"))
    });
    let staging = staging_parent.join("staging");
    let cached = match lookup_pack_cache(paths, pack_version, java_type, &url).await {
        Ok(cached) => cached,
        Err(error) => {
            ctx.progress("cached", 0.0, |p| {
                p.log_line = Some(format!("Local pack cache is unavailable: {error:#}"))
            });
            None
        }
    };
    if let Some(cached) = cached {
        ctx.progress("cached", 1.0, |p| {
            p.log_line = Some(format!(
                "Using cached pack {pack_version} ({java_type}) — skipping download"
            ))
        });
        copy_cached_pack_to_staging(&cached, &staging).await?;
        ctx.progress("extracting", 1.0, |p| {
            p.log_line = Some("Cached pack ready".to_string())
        });
        if operation == "update-pack" {
            ctx.progress("updating", 0.55, |p| {
                p.log_line = Some("Pack ready from cache".to_string())
            });
        }
        return Ok(staging);
    }
    let url_hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    let zip_path = staging_parent.join(format!(
        "pack-{}-{}-{}.zip",
        sanitize_name(pack_version),
        sanitize_name(java_type),
        &url_hash[..12]
    ));
    remove_if_exists(&staging).await?;
    tokio::fs::create_dir_all(&staging).await?;
    download_file(ctx, &url, &zip_path).await?;
    if let Err(error) = extract_pack_zip(ctx, &zip_path, &staging).await {
        let _ = tokio::fs::remove_file(&zip_path).await;
        return Err(error);
    }
    flatten_nested_pack(&staging).await?;
    // The cache is an optimization; a full disk must not discard a good pack.
    match store_pack_cache(paths, pack_version, java_type, &url, &staging).await {
        Ok(()) => ctx.progress("extracting", 1.0, |p| {
            p.log_line = Some(format!("Pack {pack_version} cached for future installs"))
        }),
        Err(error) => ctx.progress("cached", 0.0, |p| {
            p.log_line = Some(format!("Pack cache could not be updated: {error:#}"))
        }),
    }
    if operation == "update-pack" {
        ctx.progress("updating", 0.55, |p| {
            p.log_line = Some("Pack downloaded and extracted to staging".to_string())
        });
    }
    tokio::fs::remove_file(&zip_path).await.ok();
    Ok(staging)
}

/// Mirrors `buildUpdatePreview`.
pub async fn build_update_preview(
    paths: &LauncherPaths,
    ctx: &ProgressCtx,
    instance: &Path,
    target_version: &str,
    java_type: &str,
) -> Result<UpdateModPreview, anyhow::Error> {
    let settings_raw = tokio::fs::read_to_string(instance.join("instance.json"))
        .await
        .map_err(|_| anyhow::anyhow!("instance settings are invalid"))?;
    let settings =
        industrialis_core::instance_settings::parse_instance_settings_json(&settings_raw);
    let current_version = settings.pack_version.clone();
    let excluded = crate::mod_exclusions::excluded_identities(&settings.removed_mods);
    let old_mods = list_mods(&resolve_mods_dir(instance).await).await?;
    let staging_parent = instance.join(".update-preview");
    let staging = download_and_extract_to_staging(
        paths,
        ctx,
        target_version,
        java_type,
        &staging_parent,
        "preview",
    )
    .await?;
    let new_mods = list_mods(&resolve_mods_dir(&staging).await).await?;
    let (new_mods, kept_out_count) = split_excluded_mods(new_mods, &excluded);
    let (updated, added, removed) = classify_mod_updates(&old_mods, &new_mods);
    let custom_mods = list_custom_mods(instance)
        .await?
        .into_iter()
        .filter(|entry| !excluded.contains(&entry.identity))
        .map(|entry| ModPreviewEntry {
            entry,
            in_persistent_overlay: true,
        })
        .collect();
    Ok(UpdateModPreview {
        current_pack_version: current_version,
        target_pack_version: target_version.to_string(),
        custom_mods,
        new_pack_mods_count: added,
        updated_pack_mods_count: updated,
        removed_from_pack_count: removed,
        kept_out_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_mod_identities() {
        assert_eq!(
            mod_identity_from_filename("AppleCore-1.12.2-3.4.0.jar"),
            "applecore"
        );
        assert_eq!(
            mod_identity_from_filename("ThaumicTinkerer-2.3-1.7.10-164347588.jar"),
            "thaumictinkerer"
        );
        assert_eq!(
            mod_identity_from_filename("mymod-client-1.0.jar"),
            "mymod-client"
        );
        assert_eq!(
            mod_identity_from_filename("journeymap-1.12.2-5.7.1.zip"),
            "journeymap"
        );
        assert_eq!(mod_identity_from_filename("GTNH-2.8.0"), "gtnh");
    }

    #[test]
    fn classifies_mod_updates() {
        let entry = |identity: &str| ModEntry {
            filename: format!("{identity}.jar"),
            identity: identity.to_string(),
            size_bytes: 1,
        };
        let old = vec![entry("a"), entry("b"), entry("gone")];
        let new = vec![entry("a"), entry("b"), entry("fresh")];
        assert_eq!(classify_mod_updates(&old, &new), (2, 1, 1));
    }

    #[test]
    fn splits_excluded_mods_by_identity() {
        let entry = |identity: &str| ModEntry {
            filename: format!("{identity}.jar"),
            identity: identity.to_string(),
            size_bytes: 1,
        };
        let excluded = HashSet::from(["defaultserverlist".to_string()]);
        let (kept, count) =
            split_excluded_mods(vec![entry("defaultserverlist"), entry("keep")], &excluded);
        assert_eq!(count, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].identity, "keep");
    }

    #[test]
    fn resolves_java8_vs_modern_urls() {
        let version = GtnhVersion {
            title: "t".to_string(),
            description: "d".to_string(),
            release_date: "2025/01/01".to_string(),
            max_java_version: 25,
            mmc: GtnhMmcUrls {
                java8_url: "https://x/8.zip".to_string(),
                java17_2x_url: "https://x/17.zip".to_string(),
            },
            client: GtnhClientUrls {
                java8_url: "https://x/c.zip".to_string(),
            },
        };
        assert_eq!(
            resolve_pack_download_url(&version, "java8"),
            "https://x/8.zip"
        );
        assert_eq!(
            resolve_pack_download_url(&version, "java17+"),
            "https://x/17.zip"
        );
    }

    #[tokio::test]
    async fn round_trips_custom_mods() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-mods-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let instance = dir.join("inst");
        let source = dir.join("source.jar");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(&source, b"fake-mod").await.unwrap();
        let added = add_custom_mod(&instance, &source).await.unwrap();
        assert_eq!(added.filename, "source.jar");
        assert_eq!(list_custom_mods(&instance).await.unwrap().len(), 1);
        assert!(instance
            .join(".minecraft")
            .join("mods")
            .join("source.jar")
            .exists());
        remove_custom_mod(&instance, &added.identity).await.unwrap();
        assert!(list_custom_mods(&instance).await.unwrap().is_empty());
        assert!(remove_custom_mod(&instance, &added.identity).await.is_err());
    }

    #[tokio::test]
    async fn extracts_zip_safely() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        // Build a zip with a normal file, a nested file, and a zip-slip attempt.
        let zip_path = dir.join("pack.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .start_file("mmc-pack.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut archive, b"{}").unwrap();
            archive
                .start_file(
                    "patches/net.minecraft.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            std::io::Write::write_all(&mut archive, b"{}").unwrap();
            archive
                .start_file("../../evil.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut archive, b"evil").unwrap();
            archive.finish().unwrap();
        }
        let events: Arc<std::sync::Mutex<Vec<PackProgress>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = events.clone();
        let ctx = ProgressCtx {
            emit: Arc::new(move |p| capture.lock().unwrap().push(p)),
            operation: "install".to_string(),
            id: Some("t".to_string()),
        };
        let dest = dir.join("staging");
        extract_pack_zip(&ctx, &zip_path, &dest).await.unwrap();
        assert!(dest.join("mmc-pack.json").exists());
        assert!(dest.join("patches").join("net.minecraft.json").exists());
        assert!(!dir.join("evil.txt").exists());
        assert!(!dest.join("evil.txt").exists());
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.stage == "extracting" && p.pct == 1.0));
    }

    fn serve_download(
        body: Vec<u8>,
        honor_range: bool,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let read = std::io::Read::read(&mut socket, &mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request).into_owned();
            let start = request
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if !name.eq_ignore_ascii_case("range") {
                        return None;
                    }
                    value
                        .trim()
                        .strip_prefix("bytes=")?
                        .strip_suffix('-')?
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            if honor_range && start > 0 && start < body.len() {
                let remaining = &body[start..];
                let header = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                    remaining.len(),
                    start,
                    body.len() - 1,
                    body.len()
                );
                std::io::Write::write_all(&mut socket, header.as_bytes()).unwrap();
                std::io::Write::write_all(&mut socket, remaining).unwrap();
            } else {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                std::io::Write::write_all(&mut socket, header.as_bytes()).unwrap();
                std::io::Write::write_all(&mut socket, &body).unwrap();
            }
            request
        });
        (address, handle)
    }

    #[tokio::test]
    async fn resumes_a_partial_pack_download() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-resume-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let destination = dir.join("pack.zip");
        let body = b"hello world".to_vec();
        tokio::fs::write(&destination, b"hello ").await.unwrap();
        let (address, server) = serve_download(body.clone(), true);
        let ctx = ProgressCtx {
            emit: Arc::new(|_| {}),
            operation: "install".to_string(),
            id: None,
        };
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        download_with_retry(
            &ctx,
            &client,
            &format!("http://{address}/pack.zip"),
            &destination,
        )
        .await
        .unwrap();
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), body);
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("range: bytes=6-"), "{request}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restarts_when_the_server_ignores_the_resume_range() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-restart-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let destination = dir.join("pack.zip");
        let body = b"complete pack".to_vec();
        tokio::fs::write(&destination, b"partial").await.unwrap();
        let (address, server) = serve_download(body.clone(), false);
        let ctx = ProgressCtx {
            emit: Arc::new(|_| {}),
            operation: "install".to_string(),
            id: None,
        };
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        download_with_retry(
            &ctx,
            &client,
            &format!("http://{address}/pack.zip"),
            &destination,
        )
        .await
        .unwrap();
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), body);
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
