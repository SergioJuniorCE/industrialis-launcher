//! Port of the config/download half of `electron/backend/launch.ts`.
//!
//! Reads `mmc-pack.json` + `patches/*.json`, resolves the library set,
//! downloads missing libraries/assets, and builds JVM + program arguments.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::fs_utils::exists;

const DEFAULT_LIBRARY_REPO: &str = "https://libraries.minecraft.net/";
const DOWNLOAD_ATTEMPTS: u32 = 3;
const ASSET_DOWNLOAD_ATTEMPTS: u32 = 4;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// `(id, stream, line)` — owned strings so it can cross spawned tasks.
pub type EmitLaunchLog = Arc<dyn Fn(String, String, String) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConfig {
    pub main_class: String,
    pub minecraft_version: String,
    pub libraries: Vec<String>,
    pub game_dir: String,
    pub assets_dir: String,
    pub jvm_args: Vec<String>,
    pub program_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<PatchAssetIndex>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchAssetIndex {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GradleSpec {
    group: String,
    artifact: String,
    version: String,
    classifier: Option<String>,
    extension: String,
}

fn parse_gradle_spec(value: &str) -> Option<GradleSpec> {
    let (coordinates, extension) = match value.split_once('@') {
        Some((c, e)) => (c, e.to_string()),
        None => (value, "jar".to_string()),
    };
    let parts: Vec<&str> = coordinates.split(':').collect();
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

fn spec_filename(spec: &GradleSpec) -> String {
    match &spec.classifier {
        Some(c) => format!(
            "{}-{}-{}.{}",
            spec.artifact, spec.version, c, spec.extension
        ),
        None => format!("{}-{}.{}", spec.artifact, spec.version, spec.extension),
    }
}

fn spec_storage_path(spec: &GradleSpec) -> String {
    format!(
        "{}/{}/{}/{}",
        spec.group.replace('.', "/"),
        spec.artifact,
        spec.version,
        spec_filename(spec)
    )
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

#[derive(Debug, Clone, Deserialize)]
struct LibraryRule {
    action: String,
    os: Option<RuleOs>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PatchLibraryEntry {
    name: String,
    url: Option<String>,
    #[serde(rename = "MMC-hint")]
    mmc_hint: Option<String>,
    #[serde(rename = "MMC-absoluteUrl")]
    mmc_absolute_url: Option<String>,
    downloads: Option<EntryDownloads>,
    rules: Option<Vec<LibraryRule>>,
    natives: Option<serde_json::Value>,
    extract: Option<NativeExtract>,
}

#[derive(Debug, Clone, Deserialize)]
struct NativeExtract {
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EntryDownloads {
    artifact: Option<ArtifactDownload>,
    #[serde(default)]
    classifiers: HashMap<String, ArtifactDownload>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArtifactDownload {
    url: Option<String>,
    sha1: Option<String>,
    size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PatchJson {
    order: Option<f64>,
    #[serde(rename = "mainClass")]
    main_class: Option<String>,
    #[serde(rename = "+mainClass")]
    plus_main_class: Option<String>,
    #[serde(rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    #[serde(rename = "assetIndex")]
    asset_index: Option<PatchAssetIndex>,
    #[serde(rename = "mainJar")]
    main_jar: Option<MainJar>,
    libraries: Option<Vec<PatchLibraryEntry>>,
    #[serde(rename = "+jvmArgs")]
    plus_jvm_args: Option<Vec<String>>,
    #[serde(rename = "+args")]
    plus_args: Option<Vec<String>>,
    #[serde(rename = "+tweakers")]
    plus_tweakers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct MainJar {
    name: String,
    downloads: EntryDownloads,
}

#[derive(Debug, Clone, Deserialize)]
struct MmcComponent {
    uid: String,
    version: Option<String>,
    #[serde(rename = "cachedVersion")]
    cached_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MmcPackJson {
    components: Vec<MmcComponent>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndexFile {
    objects: Option<HashMap<String, AssetObject>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

fn library_allowed(rules: Option<&[LibraryRule]>) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let applies = rule
            .os
            .as_ref()
            .and_then(|o| o.name.as_deref())
            .map(|n| n == current_os_name())
            .unwrap_or(true);
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

fn resolve_library_url(entry: &PatchLibraryEntry, spec: &GradleSpec) -> String {
    if let Some(absolute) = entry
        .mmc_absolute_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return absolute.to_string();
    }
    if let Some(url) = entry
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .and_then(|a| a.url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return url.to_string();
    }
    let repo = entry
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_LIBRARY_REPO);
    format!(
        "{}/{}",
        repo.trim_end_matches('/'),
        spec_storage_path(spec).replace('\\', "/")
    )
}

fn sha1_hex(content: &[u8]) -> String {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(content);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent("IndustrialisLauncher/0.1")
        .build()
        .expect("reqwest client builds")
}

async fn fetch_buffer(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, anyhow::Error> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("download failed ({url}): {e}"))?;
    if !response.status().is_success() {
        anyhow::bail!("download failed ({url}): HTTP {}", response.status());
    }
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| anyhow::anyhow!("download failed ({url}): {e}"))
}

async fn write_file_atomic(destination: &Path, data: &[u8]) -> Result<(), anyhow::Error> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = destination.with_extension(format!("tmp-{}-{}", std::process::id(), chrono_suffix()));
    let result = async {
        tokio::fs::write(&tmp, data).await?;
        tokio::fs::rename(&tmp, destination).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    tokio::fs::remove_file(&tmp).await.ok();
    result
}

fn chrono_suffix() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), anyhow::Error> {
    let mut last_error = String::new();
    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        match fetch_buffer(client, url).await {
            Ok(data) => {
                if let Err(e) = write_file_atomic(destination, &data).await {
                    last_error = e.to_string();
                } else {
                    return Ok(());
                }
            }
            Err(e) => last_error = e.to_string(),
        }
        tokio::fs::remove_file(destination).await.ok();
        if attempt < DOWNLOAD_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
        }
    }
    anyhow::bail!("download failed ({url}): {last_error}")
}

fn asset_digest(hash: &str) -> String {
    hash.strip_prefix("sha1:").unwrap_or(hash).to_string()
}

fn asset_path(assets_dir: &Path, hash: &str) -> PathBuf {
    let digest = asset_digest(hash);
    assets_dir
        .join("objects")
        .join(&digest[..2.min(digest.len())])
        .join(&digest)
}

fn asset_url(hash: &str) -> String {
    let digest = asset_digest(hash);
    format!(
        "https://resources.download.minecraft.net/{}/{}",
        &digest[..2.min(digest.len())],
        digest
    )
}

async fn download_asset_object(
    client: &reqwest::Client,
    object: &AssetObject,
    target: &Path,
) -> Result<(), anyhow::Error> {
    let digest = asset_digest(&object.hash);
    let url = asset_url(&object.hash);
    let mut last_error = String::new();
    for attempt in 1..=ASSET_DOWNLOAD_ATTEMPTS {
        let step: Result<(), anyhow::Error> = async {
            let data = fetch_buffer(client, &url).await?;
            if data.len() as u64 != object.size {
                anyhow::bail!(
                    "size mismatch (expected {} bytes, got {} bytes)",
                    object.size,
                    data.len()
                );
            }
            if sha1_hex(&data) != digest {
                anyhow::bail!("checksum mismatch (expected sha1 {digest})");
            }
            write_file_atomic(target, &data).await?;
            let written = tokio::fs::metadata(target)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if written != object.size {
                anyhow::bail!(
                    "size mismatch after write (expected {} bytes, got {written} bytes)",
                    object.size
                );
            }
            Ok(())
        }
        .await;
        match step {
            Ok(()) => return Ok(()),
            Err(e) => last_error = e.to_string(),
        }
        tokio::fs::remove_file(target).await.ok();
        if attempt < ASSET_DOWNLOAD_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
        }
    }
    anyhow::bail!("asset object {digest} did not verify after download (expected {} bytes). {last_error}. Check your connection and disk space, then try launching again", object.size)
}

async fn ensure_assets(
    client: &reqwest::Client,
    assets_dir: &Path,
    index: &PatchAssetIndex,
    id: &str,
    emit: &EmitLaunchLog,
) -> Result<(), anyhow::Error> {
    emit(
        id.to_string(),
        "system".to_string(),
        format!("Syncing Minecraft assets ({})…", index.id),
    );
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", index.id));
    let mut index_bytes = tokio::fs::read(&index_path).await.ok();
    if let (Some(bytes), Some(expected)) = (index_bytes.as_ref(), index.sha1.as_deref()) {
        if sha1_hex(bytes) != expected {
            index_bytes = None;
        }
    }
    if index_bytes.is_none() {
        let url = index
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("asset index {} has no download URL", index.id))?;
        download_to_file(client, url, &index_path).await?;
        let bytes = tokio::fs::read(&index_path).await?;
        if let Some(expected) = index.sha1.as_deref() {
            if sha1_hex(&bytes) != expected {
                tokio::fs::remove_file(&index_path).await.ok();
                anyhow::bail!("asset index {} checksum mismatch", index.id);
            }
        }
        index_bytes = Some(bytes);
    }
    let parsed: AssetIndexFile = serde_json::from_slice(&index_bytes.unwrap_or_default())
        .map_err(|e| anyhow::anyhow!("invalid asset index {}: {e}", index.id))?;
    let objects: Vec<AssetObject> = parsed.objects.unwrap_or_default().into_values().collect();
    let mut missing = Vec::new();
    for object in &objects {
        let target = asset_path(assets_dir, &object.hash);
        let size = tokio::fs::metadata(&target)
            .await
            .map(|m| m.len())
            .unwrap_or(u64::MAX);
        if size != object.size {
            missing.push(object.clone());
        }
    }
    if missing.is_empty() {
        emit(
            id.to_string(),
            "system".to_string(),
            format!("Assets up to date ({} objects)", objects.len()),
        );
        return Ok(());
    }
    emit(
        id.to_string(),
        "system".to_string(),
        format!(
            "Downloading {} of {} asset objects…",
            missing.len(),
            objects.len()
        ),
    );
    let mut set = tokio::task::JoinSet::new();
    for object in missing {
        let client = client.clone();
        let target = asset_path(assets_dir, &object.hash);
        set.spawn(async move { download_asset_object(&client, &object, &target).await });
    }
    while let Some(result) = set.join_next().await {
        result.map_err(|e| anyhow::anyhow!("asset download task failed: {e}"))??;
    }
    emit(
        id.to_string(),
        "system".to_string(),
        "Asset sync complete".to_string(),
    );
    Ok(())
}

async fn ensure_library(
    client: &reqwest::Client,
    pack_dir: &Path,
    entry: &PatchLibraryEntry,
    id: &str,
    emit: &EmitLaunchLog,
) -> Result<Option<PathBuf>, anyhow::Error> {
    if !library_allowed(entry.rules.as_deref()) || is_native_only(entry) {
        return Ok(None);
    }
    let spec = parse_gradle_spec(&entry.name)
        .ok_or_else(|| anyhow::anyhow!("bad library name: {}", entry.name))?;
    let root = pack_dir.join("libraries");
    let mut candidates = vec![root.join(spec_storage_path(&spec))];
    if entry.mmc_hint.as_deref() == Some("local") {
        candidates.insert(0, root.join(spec_filename(&spec)));
    }
    for candidate in &candidates {
        if exists(candidate).await {
            return Ok(Some(candidate.clone()));
        }
    }
    let destination = if entry.mmc_hint.as_deref() == Some("local") {
        root.join(spec_filename(&spec))
    } else {
        root.join(spec_storage_path(&spec))
    };
    emit(
        id.to_string(),
        "system".to_string(),
        format!("Downloading {}", entry.name),
    );
    download_to_file(client, &resolve_library_url(entry, &spec), &destination).await?;
    Ok(Some(destination))
}

async fn ensure_main_jar(
    client: &reqwest::Client,
    pack_dir: &Path,
    jar: &MainJar,
    minecraft_version: &str,
    id: &str,
    emit: &EmitLaunchLog,
) -> Result<PathBuf, anyhow::Error> {
    let destination = pack_dir
        .join(".minecraft")
        .join("versions")
        .join(minecraft_version)
        .join(format!("{minecraft_version}.jar"));
    if exists(&destination).await {
        return Ok(destination);
    }
    let url = jar
        .downloads
        .artifact
        .as_ref()
        .and_then(|a| a.url.as_deref())
        .ok_or_else(|| anyhow::anyhow!("minecraft mainJar has no download URL"))?;
    emit(
        id.to_string(),
        "system".to_string(),
        format!("Downloading Minecraft {minecraft_version}"),
    );
    download_to_file(client, url, &destination).await?;
    Ok(destination)
}

async fn ensure_legacy_natives(
    client: &reqwest::Client,
    instance: &Path,
    entry: &PatchLibraryEntry,
) -> anyhow::Result<()> {
    if !library_allowed(entry.rules.as_deref()) {
        return Ok(());
    }
    let Some(natives) = entry.natives.as_ref() else {
        return Ok(());
    };
    let arm_key = format!("{}-arm64", current_os_name());
    let classifier = (if cfg!(target_arch = "aarch64") {
        natives.get(&arm_key)
    } else {
        None
    })
    .or_else(|| natives.get(current_os_name()))
    .and_then(|value| value.as_str())
    .ok_or_else(|| {
        anyhow::anyhow!(
            "No native platform for {} on {}",
            entry.name,
            current_os_name()
        )
    })?
    .replace(
        "${arch}",
        if cfg!(target_pointer_width = "64") {
            "64"
        } else {
            "32"
        },
    );
    let artifact = entry
        .downloads
        .as_ref()
        .and_then(|d| d.classifiers.get(&classifier))
        .ok_or_else(|| {
            anyhow::anyhow!("Missing native classifier {classifier} for {}", entry.name)
        })?;
    let mut spec = parse_gradle_spec(&entry.name)
        .ok_or_else(|| anyhow::anyhow!("Invalid native library name"))?;
    spec.classifier = Some(classifier);
    let jar = instance.join("libraries").join(spec_storage_path(&spec));
    let valid = |raw: &[u8]| {
        artifact.size.map_or(true, |size| raw.len() as u64 == size)
            && artifact
                .sha1
                .as_deref()
                .map_or(true, |hash| sha1_hex(raw) == hash)
    };
    let cached = tokio::fs::read(&jar).await.ok();
    if !cached.as_deref().is_some_and(valid) {
        let url = artifact
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Native classifier has no URL"))?;
        let mut response = client.get(url).send().await?.error_for_status()?;
        const NATIVE_LIMIT: usize = 64 * 1024 * 1024;
        anyhow::ensure!(
            response
                .content_length()
                .map_or(true, |size| size <= NATIVE_LIMIT as u64),
            "Native library exceeds 64 MB"
        );
        let mut raw = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                raw.len() + chunk.len() <= NATIVE_LIMIT,
                "Native library exceeds 64 MB"
            );
            raw.extend_from_slice(&chunk);
        }
        anyhow::ensure!(
            valid(&raw),
            "Native library checksum/size mismatch for {}",
            entry.name
        );
        let target = jar.clone();
        tokio::task::spawn_blocking(move || crate::fs_utils::write_atomic(&target, &raw)).await??;
    }
    let directory = instance.join("natives");
    let excludes = entry
        .extract
        .as_ref()
        .map(|v| v.exclude.clone())
        .unwrap_or_default();
    tokio::task::spawn_blocking(move || {
        crate::native_libraries::extract(&jar, &directory, &excludes)
    })
    .await?
}

fn effective_version(component: &MmcComponent) -> String {
    component
        .version
        .clone()
        .or(component.cached_version.clone())
        .unwrap_or_default()
}

/// Mirrors `buildLaunchConfig`.
pub async fn build_launch_config(
    instance: &Path,
    id: &str,
    emit: &EmitLaunchLog,
) -> Result<LaunchConfig, anyhow::Error> {
    let client = http_client();
    let mmc_path = instance.join("mmc-pack.json");
    let mmc_raw = tokio::fs::read_to_string(&mmc_path)
        .await
        .map_err(|_| anyhow::anyhow!("invalid Minecraft pack metadata: {}", mmc_path.display()))?;
    let mmc: MmcPackJson = serde_json::from_str(&mmc_raw)
        .map_err(|_| anyhow::anyhow!("invalid Minecraft pack metadata: {}", mmc_path.display()))?;
    let mut patches: Vec<(f64, String, PatchJson)> = Vec::new();
    for component in &mmc.components {
        let raw = crate::component_metadata::load(
            &client,
            instance,
            &component.uid,
            &effective_version(component),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Resolve {} component metadata: {e:#}", component.uid))?;
        let patch: PatchJson = serde_json::from_slice(&raw).map_err(|e| {
            anyhow::anyhow!(
                "Invalid Minecraft patch metadata for {}: {e}",
                component.uid
            )
        })?;
        patches.push((patch.order.unwrap_or(0.0), component.uid.clone(), patch));
    }
    patches.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut main_class = "net.minecraft.launchwrapper.Launch".to_string();
    let mut minecraft_version = "1.12.2".to_string();
    let mut jvm_args = vec!["-Duser.language=en".to_string()];
    let mut program_args: Vec<String> = Vec::new();
    let mut minecraft_arguments_template: Option<String> = None;
    let mut asset_index: Option<PatchAssetIndex> = None;
    let mut main_jar: Option<MainJar> = None;
    let mut library_entries: Vec<PatchLibraryEntry> = Vec::new();

    for (_, uid, patch) in &patches {
        if let Some(plus) = patch
            .plus_main_class
            .as_deref()
            .or(patch.main_class.as_deref())
        {
            main_class = plus.to_string();
        }
        jvm_args.extend(patch.plus_jvm_args.clone().unwrap_or_default());
        for tweaker in patch.plus_tweakers.clone().unwrap_or_default() {
            program_args.push("--tweakClass".to_string());
            program_args.push(tweaker);
        }
        let extra = patch.plus_args.clone().unwrap_or_default();
        let mut i = 0;
        while i < extra.len() {
            if extra[i] == "--tweakClass" && i + 1 < extra.len() {
                program_args.push("--tweakClass".to_string());
                program_args.push(extra[i + 1].clone());
                i += 2;
            } else if extra[i].starts_with("--tweakClass=") {
                program_args.push(extra[i].clone());
                i += 1;
            } else {
                jvm_args.push(extra[i].clone());
                i += 1;
            }
        }
        if uid == "net.minecraft" {
            if let Some(component) = mmc.components.iter().find(|c| &c.uid == uid) {
                let v = effective_version(component);
                if !v.is_empty() {
                    minecraft_version = v;
                }
            }
            main_jar = patch.main_jar.clone();
            minecraft_arguments_template = patch.minecraft_arguments.clone();
            asset_index = patch.asset_index.clone();
        }
    }

    for (_, _, patch) in &patches {
        library_entries.extend(patch.libraries.clone().unwrap_or_default());
    }
    for entry in &library_entries {
        ensure_legacy_natives(&client, instance, entry).await?;
    }
    // Deduped concurrent resolution (same key shares one download).
    let mut seen: HashMap<String, PatchLibraryEntry> = HashMap::new();
    for entry in library_entries {
        let key = match parse_gradle_spec(&entry.name) {
            Some(spec) if entry.mmc_hint.as_deref() == Some("local") => spec_filename(&spec),
            Some(spec) => spec_storage_path(&spec),
            None => entry.name.clone(),
        };
        seen.entry(key).or_insert(entry);
    }
    let mut set = tokio::task::JoinSet::new();
    for entry in seen.into_values() {
        let client = client.clone();
        let pack_dir = instance.to_path_buf();
        let id = id.to_string();
        let emit = emit.clone();
        set.spawn(async move {
            ensure_library(&client, &pack_dir, &entry, &id, &emit)
                .await
                .map(|resolved| (entry, resolved))
        });
    }
    let mut libraries: Vec<(String, String, String)> = Vec::new(); // (path, version, key)
    let mut index: HashMap<String, usize> = HashMap::new();
    while let Some(result) = set.join_next().await {
        let (entry, resolved) =
            result.map_err(|e| anyhow::anyhow!("library task failed: {e}"))??;
        let (Some(resolved), Some(spec)) = (resolved, parse_gradle_spec(&entry.name)) else {
            continue;
        };
        let key = format!("{}:{}", spec.group, spec.artifact);
        match index.get(&key) {
            None => {
                index.insert(key.clone(), libraries.len());
                libraries.push((resolved.to_string_lossy().into_owned(), spec.version, key));
            }
            Some(&previous) => {
                if spec.version > libraries[previous].1 {
                    libraries[previous] =
                        (resolved.to_string_lossy().into_owned(), spec.version, key);
                }
            }
        }
    }
    if let Some(jar) = main_jar {
        let resolved =
            ensure_main_jar(&client, instance, &jar, &minecraft_version, id, emit).await?;
        match parse_gradle_spec(&jar.name) {
            Some(spec) => {
                let key = format!("{}:{}", spec.group, spec.artifact);
                match index.get(&key) {
                    None => {
                        index.insert(key.clone(), libraries.len());
                        libraries.push((
                            resolved.to_string_lossy().into_owned(),
                            minecraft_version.clone(),
                            key,
                        ));
                    }
                    Some(&previous) => {
                        libraries[previous] = (
                            resolved.to_string_lossy().into_owned(),
                            minecraft_version.clone(),
                            key,
                        );
                    }
                }
            }
            None => libraries.push((
                resolved.to_string_lossy().into_owned(),
                minecraft_version.clone(),
                jar.name,
            )),
        }
    }
    if libraries.is_empty() {
        anyhow::bail!("no libraries resolved for launch");
    }
    let game_dir = instance.join(".minecraft");
    let assets_dir = game_dir.join("assets");
    let natives_dir = instance.join("natives");
    tokio::fs::create_dir_all(&natives_dir).await?;
    jvm_args.push(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));
    Ok(LaunchConfig {
        main_class,
        minecraft_version,
        libraries: libraries.into_iter().map(|(path, _, _)| path).collect(),
        game_dir: game_dir.to_string_lossy().into_owned(),
        assets_dir: assets_dir.to_string_lossy().into_owned(),
        jvm_args,
        program_args,
        minecraft_arguments_template,
        asset_index,
    })
}

/// Mirrors `syncAssets`.
pub async fn sync_assets(
    config: &LaunchConfig,
    id: &str,
    emit: &EmitLaunchLog,
) -> Result<(), anyhow::Error> {
    if let Some(index) = config.asset_index.as_ref() {
        let client = http_client();
        ensure_assets(&client, Path::new(&config.assets_dir), index, id, emit).await?;
    }
    Ok(())
}

/// Mirrors `buildClasspath`.
pub fn build_classpath(libraries: &[String]) -> String {
    libraries.join(if cfg!(windows) { ";" } else { ":" })
}

fn template_pattern() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").unwrap())
}

/// Mirrors `expandMinecraftArguments` (empty parts are dropped *before*
/// substitution, so a token that expands to nothing stays in the list).
pub fn expand_minecraft_arguments(template: &str, tokens: &HashMap<String, String>) -> Vec<String> {
    template
        .split_whitespace()
        .map(|part| {
            template_pattern()
                .replace_all(part, |caps: &regex::Captures| {
                    tokens.get(&caps[1]).cloned().unwrap_or_default()
                })
                .into_owned()
        })
        .collect()
}

/// Mirrors `splitCommandArgs` (double-quote grouping, no escaping).
pub fn split_command_args(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in command.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Mirrors `substituteCommandVars` (`$KEY` replacement).
pub fn substitute_command_vars(template: &str, vars: &HashMap<String, String>) -> String {
    let mut value = template.to_string();
    for (key, replacement) in vars {
        value = value.replace(&format!("${key}"), replacement);
    }
    value
}

/// Mirrors `writeLaunchArgfile` (one JSON string per line).
pub async fn write_launch_argfile(target: &Path, args: &[String]) -> Result<(), anyhow::Error> {
    let mut raw = String::new();
    for arg in args {
        raw.push_str(&serde_json::to_string(arg)?);
        raw.push('\n');
    }
    tokio::fs::write(target, raw).await?;
    Ok(())
}

/// Mirrors `runShellCommand`.
pub async fn run_shell_command(
    command: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<(), anyhow::Error> {
    let command = command.trim().to_string();
    #[cfg(windows)]
    let mut child_cmd = {
        let mut c = tokio::process::Command::new("cmd.exe");
        c.args(["/C", &command]);
        c
    };
    #[cfg(not(windows))]
    let mut child_cmd = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.args(["-c", &command]);
        c
    };
    let status = child_cmd
        .current_dir(cwd)
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| anyhow::anyhow!("failed to run command ({command}): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "command failed ({command}): exit {}",
            status.code().unwrap_or(-1)
        )
    }
}

/// Mirrors `instanceCommandVars`.
pub fn instance_command_vars(
    id: &str,
    name: &str,
    instance: &Path,
    java: &str,
) -> HashMap<String, String> {
    HashMap::from([
        ("INST_NAME".to_string(), name.to_string()),
        ("INST_ID".to_string(), id.to_string()),
        (
            "INST_DIR".to_string(),
            instance.to_string_lossy().into_owned(),
        ),
        (
            "INST_MC_DIR".to_string(),
            instance.join(".minecraft").to_string_lossy().into_owned(),
        ),
        ("INST_JAVA".to_string(), java.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gradle_specs() {
        let spec = parse_gradle_spec("com.google.guava:guava:21.0").unwrap();
        assert_eq!(spec.group, "com.google.guava");
        assert_eq!(spec_filename(&spec), "guava-21.0.jar");
        assert_eq!(
            spec_storage_path(&spec),
            "com/google/guava/guava/21.0/guava-21.0.jar"
        );
        let native = parse_gradle_spec(
            "org.lwjgl:lwjgl-platform:2.9.4-nightly-20150209:natives-windows@jar",
        )
        .unwrap();
        assert_eq!(native.classifier.as_deref(), Some("natives-windows"));
        assert!(parse_gradle_spec("bad-name").is_none());
    }

    #[test]
    fn resolves_library_urls() {
        let spec = parse_gradle_spec("com.google.guava:guava:21.0").unwrap();
        let plain = PatchLibraryEntry {
            name: "com.google.guava:guava:21.0".to_string(),
            url: None,
            mmc_hint: None,
            mmc_absolute_url: None,
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert_eq!(
            resolve_library_url(&plain, &spec),
            "https://libraries.minecraft.net/com/google/guava/guava/21.0/guava-21.0.jar"
        );
    }

    #[test]
    fn filters_libraries_by_os_rules() {
        let allow_all = PatchLibraryEntry {
            name: "x".to_string(),
            url: None,
            mmc_hint: None,
            mmc_absolute_url: None,
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert!(library_allowed(allow_all.rules.as_deref()));
        let windows_only = vec![LibraryRule {
            action: "allow".to_string(),
            os: Some(RuleOs {
                name: Some("windows".to_string()),
            }),
        }];
        assert_eq!(library_allowed(Some(&windows_only)), cfg!(windows));
    }

    #[test]
    fn expands_minecraft_arguments() {
        let tokens = HashMap::from([
            ("auth_player_name".to_string(), "Steve".to_string()),
            ("version_name".to_string(), "1.12.2".to_string()),
        ]);
        assert_eq!(
            expand_minecraft_arguments(
                "--username ${auth_player_name} --version ${version_name} --missing ${nope}",
                &tokens
            ),
            vec![
                "--username",
                "Steve",
                "--version",
                "1.12.2",
                "--missing",
                ""
            ]
        );
    }

    #[test]
    fn splits_and_substitutes_commands() {
        assert_eq!(
            split_command_args(r#"java -Xmx4G "my game""#),
            vec!["java", "-Xmx4G", "my game"]
        );
        let vars = HashMap::from([("INST_ID".to_string(), "gtnh".to_string())]);
        assert_eq!(
            substitute_command_vars("echo $INST_ID/$INST_ID", &vars),
            "echo gtnh/gtnh"
        );
    }

    #[tokio::test]
    async fn resolves_reference_only_pack_components_from_versioned_metadata_cache() {
        let root = std::env::temp_dir().join(format!(
            "gpui-component-resolution-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(root.join("component-cache/net.minecraft"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("libraries"))
            .await
            .unwrap();
        tokio::fs::write(
            root.join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.7.10"}]}"#,
        )
        .await
        .unwrap();
        tokio::fs::write(root.join("component-cache/net.minecraft/1.7.10.json"), r#"{"uid":"net.minecraft","version":"1.7.10","mainClass":"net.minecraft.client.main.Main","order":-2,"libraries":[{"name":"fixture:game:1","MMC-hint":"local"}]}"#).await.unwrap();
        tokio::fs::write(root.join("libraries/game-1.jar"), b"fixture")
            .await
            .unwrap();
        let emit: EmitLaunchLog = Arc::new(|_, _, _| {});
        let result = build_launch_config(&root, "reference-only", &emit).await;
        tokio::fs::remove_dir_all(&root).await.unwrap();
        let config = result.expect("Reference-only packs must resolve their component metadata");
        assert_eq!(config.minecraft_version, "1.7.10");
        assert_eq!(config.main_class, "net.minecraft.client.main.Main");
        assert_eq!(config.libraries.len(), 1);
    }

    #[tokio::test]
    async fn extracts_legacy_native_classifier_without_adding_it_to_classpath() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("gpui-native-resolution-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(root.join("patches"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("libraries/fixture/platform/1"))
            .await
            .unwrap();
        tokio::fs::write(
            root.join("mmc-pack.json"),
            r#"{"components":[{"uid":"fixture"}]}"#,
        )
        .await
        .unwrap();
        let classifier = format!("natives-{}-64", current_os_name());
        let metadata = serde_json::json!({"libraries":[{"name":"fixture:platform:1",
            "natives":{current_os_name():format!("natives-{}-${{arch}}", current_os_name())},
            "downloads":{"classifiers":{classifier.clone():{"url":"http://127.0.0.1:1/not-needed"}}}}]});
        tokio::fs::write(root.join("patches/fixture.json"), metadata.to_string())
            .await
            .unwrap();
        // Keep one regular library in the patch; native-only entries are not Java jars.
        let mut metadata = metadata;
        metadata["libraries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name":"fixture:game:1","MMC-hint":"local"}));
        tokio::fs::write(root.join("patches/fixture.json"), metadata.to_string())
            .await
            .unwrap();
        tokio::fs::write(root.join("libraries/game-1.jar"), b"fixture")
            .await
            .unwrap();
        let jar = root.join(format!(
            "libraries/fixture/platform/1/platform-1-{classifier}.jar"
        ));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(jar).unwrap());
        zip.start_file(
            "native-fixture.bin",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"native bytes").unwrap();
        zip.start_file(
            "META-INF/signature",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"ignored").unwrap();
        zip.finish().unwrap();
        let emit: EmitLaunchLog = Arc::new(|_, _, _| {});
        let config = build_launch_config(&root, "native", &emit).await.unwrap();
        let extracted = tokio::fs::read(root.join("natives/native-fixture.bin")).await;
        let metadata_extracted = root.join("natives/META-INF/signature").exists();
        tokio::fs::remove_dir_all(root).await.unwrap();
        assert_eq!(extracted.unwrap(), b"native bytes");
        assert!(!metadata_extracted);
        assert_eq!(config.libraries.len(), 1);
    }

    #[test]
    fn multiline_jvm_arguments_split_between_lines_and_preserve_quoted_spaces() {
        assert_eq!(
            split_command_args("-Dfirst=one\r\n-Dsecond=two\n\"-Dpath=my game\""),
            vec!["-Dfirst=one", "-Dsecond=two", "-Dpath=my game"]
        );
    }

    #[test]
    fn builds_classpath_with_platform_delimiter() {
        assert_eq!(
            build_classpath(&["a".to_string(), "b".to_string()]),
            if cfg!(windows) { "a;b" } else { "a:b" }
        );
    }
}
