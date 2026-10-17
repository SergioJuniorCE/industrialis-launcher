//! Pack acquisition helpers: version catalog, mod inventory, zip extraction,
//! staging, and update previews. Ports the corresponding parts of
//! `electron/backend/pack.ts` (install/update orchestration lives in
//! [`crate::install`]).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::fs_utils::{copy_tree, exists, remove_if_exists};
use crate::pack_cache::{copy_cached_pack_to_staging, lookup_pack_cache, store_pack_cache};
use crate::pack_prep::{flatten_nested_pack, persistent_custom_mods_dir};
use crate::paths::LauncherPaths;

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

async fn download_file(
    ctx: &ProgressCtx,
    url: &str,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    ctx.progress("downloading", 0.0, |p| {
        p.log_line = Some("Downloading pack archive…".to_string())
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("IndustrialisLauncher/0.1")
        .build()?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("pack download failed: {e}"))?;
    if !response.status().is_success() {
        anyhow::bail!("pack download failed: HTTP {}", response.status());
    }
    let total = response.content_length().unwrap_or(0);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::File::create(destination).await?;
    let started = Instant::now();
    let mut downloaded = 0u64;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|e| anyhow::anyhow!("pack download failed: {e}"))?;
        let Some(bytes) = chunk else { break };
        tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await?;
        downloaded += bytes.len() as u64;
        let pct = if total > 0 {
            downloaded as f64 / total as f64
        } else {
            0.0
        };
        let now = Instant::now();
        if now - last_emit >= Duration::from_millis(500) || (pct * 100.0).floor() as u64 % 5 == 0 {
            let elapsed = now.duration_since(started).as_secs_f64().max(0.05);
            let speed = downloaded as f64 / elapsed / 1024.0 / 1024.0;
            let mb = downloaded as f64 / 1024.0 / 1024.0;
            ctx.progress("downloading", pct, |p| {
                p.speed_mbps = Some(speed);
                p.downloaded_mb = Some(mb);
                p.total_mb = if total > 0 {
                    Some(total as f64 / 1024.0 / 1024.0)
                } else {
                    None
                };
            });
            last_emit = now;
        }
    }
    // Extraction opens a new handle immediately after this function returns.
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);
    let elapsed = started.elapsed().as_secs_f64().max(0.05);
    let mb = downloaded as f64 / 1024.0 / 1024.0;
    ctx.progress("downloading", 1.0, |p| {
        p.log_line = Some(format!(
            "Download complete ({mb:.1} MB, avg {:.1} MB/s)",
            downloaded as f64 / elapsed / 1024.0 / 1024.0
        ));
        p.speed_mbps = Some(downloaded as f64 / elapsed / 1024.0 / 1024.0);
        p.downloaded_mb = Some(mb);
        p.total_mb = if total > 0 {
            Some(total as f64 / 1024.0 / 1024.0)
        } else {
            None
        };
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
    let versions = fetch_gtnh_versions().await?;
    let version = versions
        .get(pack_version)
        .ok_or_else(|| anyhow::anyhow!("pack version not found"))?;
    let url = resolve_pack_download_url(version, java_type);
    let staging = staging_parent.join("staging");
    if let Some(cached) = lookup_pack_cache(paths, pack_version, java_type, &url).await? {
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
    let zip_path = staging_parent.join("pack.zip");
    remove_if_exists(&staging).await?;
    tokio::fs::create_dir_all(&staging).await?;
    download_file(ctx, &url, &zip_path).await?;
    extract_pack_zip(ctx, &zip_path, &staging).await?;
    flatten_nested_pack(&staging).await?;
    store_pack_cache(paths, pack_version, java_type, &url, &staging).await?;
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
    let current_version: String = serde_json::from_str::<serde_json::Value>(&settings_raw)
        .ok()
        .and_then(|v| {
            v.get("pack_version")
                .and_then(|p| p.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
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
    let (updated, added, removed) = classify_mod_updates(&old_mods, &new_mods);
    let custom_mods = list_custom_mods(instance)
        .await?
        .into_iter()
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
}
