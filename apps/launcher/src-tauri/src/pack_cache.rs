use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PACK_CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackCacheMeta {
    pack_version: String,
    java_type: String,
    download_url: String,
    cached_at_secs: u64,
    last_used_at_secs: u64,
}

fn launcher_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("industrialis-launcher")
}

pub fn pack_cache_root() -> PathBuf {
    launcher_data_dir().join("pack-cache")
}

fn sanitize_cache_segment(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' => '_',
            c => c,
        })
        .collect()
}

pub fn pack_cache_key(pack_version: &str, java_type: &str) -> String {
    format!(
        "{}__{}",
        sanitize_cache_segment(pack_version),
        sanitize_cache_segment(java_type)
    )
}

fn cache_entry_dir(key: &str) -> PathBuf {
    pack_cache_root().join(key)
}

fn cache_pack_dir(entry: &Path) -> PathBuf {
    entry.join("pack")
}

fn cache_meta_path(entry: &Path) -> PathBuf {
    entry.join("meta.json")
}

fn system_time_secs(time: SystemTime) -> Result<u64, String> {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| e.to_string())
}

fn read_meta(entry: &Path) -> Result<Option<PackCacheMeta>, String> {
    let path = cache_meta_path(entry);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("invalid pack cache metadata: {e}"))
}

fn write_meta(entry: &Path, meta: &PackCacheMeta) -> Result<(), String> {
    let path = cache_meta_path(entry);
    let raw = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

fn cache_is_expired(meta: &PackCacheMeta, now_secs: u64) -> bool {
    now_secs.saturating_sub(meta.cached_at_secs) > PACK_CACHE_TTL.as_secs()
}

fn cache_is_valid(entry: &Path, download_url: &str, now_secs: u64) -> Result<bool, String> {
    let Some(meta) = read_meta(entry)? else {
        return Ok(false);
    };
    if meta.download_url != download_url {
        return Ok(false);
    }
    if cache_is_expired(&meta, now_secs) {
        return Ok(false);
    }
    let pack_dir = cache_pack_dir(entry);
    Ok(pack_dir.is_dir())
}

fn touch_cache(entry: &Path, meta: &mut PackCacheMeta) -> Result<(), String> {
    meta.last_used_at_secs = system_time_secs(SystemTime::now())?;
    write_meta(entry, meta)
}

fn remove_cache_entry(entry: &Path) {
    if entry.exists() {
        let _ = fs::remove_dir_all(entry);
    }
}

fn copy_file_create_parent(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

fn copy_tree_merge(src: &Path, dest: &Path) -> Result<(), String> {
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

pub fn evict_expired_pack_cache() -> Result<u32, String> {
    let root = pack_cache_root();
    if !root.is_dir() {
        return Ok(0);
    }

    let now_secs = system_time_secs(SystemTime::now())?;
    let mut removed = 0u32;
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let should_remove = match read_meta(&path)? {
            Some(meta) => cache_is_expired(&meta, now_secs),
            None => true,
        };
        if should_remove {
            remove_cache_entry(&path);
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn lookup_pack_cache(
    pack_version: &str,
    java_type: &str,
    download_url: &str,
) -> Result<Option<PathBuf>, String> {
    evict_expired_pack_cache()?;

    let key = pack_cache_key(pack_version, java_type);
    let entry = cache_entry_dir(&key);
    if !entry.is_dir() {
        return Ok(None);
    }

    let now_secs = system_time_secs(SystemTime::now())?;
    if !cache_is_valid(&entry, download_url, now_secs)? {
        remove_cache_entry(&entry);
        return Ok(None);
    }

    if let Some(mut meta) = read_meta(&entry)? {
        touch_cache(&entry, &mut meta)?;
    }

    Ok(Some(cache_pack_dir(&entry)))
}

pub fn store_pack_cache(
    pack_version: &str,
    java_type: &str,
    download_url: &str,
    staging: &Path,
) -> Result<(), String> {
    if !staging.is_dir() {
        return Err("cannot cache pack: staging directory missing".into());
    }

    let now_secs = system_time_secs(SystemTime::now())?;
    let key = pack_cache_key(pack_version, java_type);
    let entry = cache_entry_dir(&key);

    if cache_is_valid(&entry, download_url, now_secs)? {
        return Ok(());
    }

    let root = pack_cache_root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    let temp = root.join(format!("{key}.tmp"));
    remove_cache_entry(&temp);

    let temp_pack = cache_pack_dir(&temp);
    fs::create_dir_all(&temp_pack).map_err(|e| e.to_string())?;
    copy_tree_merge(staging, &temp_pack)?;

    let meta = PackCacheMeta {
        pack_version: pack_version.to_string(),
        java_type: java_type.to_string(),
        download_url: download_url.to_string(),
        cached_at_secs: now_secs,
        last_used_at_secs: now_secs,
    };
    write_meta(&temp, &meta)?;

    remove_cache_entry(&entry);
    fs::rename(&temp, &entry).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn copy_cached_pack_to_staging(cache_pack: &Path, staging: &Path) -> Result<(), String> {
    if staging.exists() {
        fs::remove_dir_all(staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    copy_tree_merge(cache_pack, staging)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_cache_key_sanitizes_values() {
        let key = pack_cache_key("2.9.0-beta/1", "java17+");
        assert_eq!(key, "2.9.0-beta_1__java17+");
    }

    #[test]
    fn cache_expires_after_ttl() {
        let meta = PackCacheMeta {
            pack_version: "1.0".into(),
            java_type: "java8".into(),
            download_url: "https://example.com/pack.zip".into(),
            cached_at_secs: 0,
            last_used_at_secs: 0,
        };
        assert!(cache_is_expired(
            &meta,
            PACK_CACHE_TTL.as_secs() + 1
        ));
        assert!(!cache_is_expired(&meta, PACK_CACHE_TTL.as_secs()));
    }
}