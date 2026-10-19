//! Port of `electron/backend/pack-cache.ts`: TTL pack archive cache.
//!
//! The TS backend resolves the cache root globally; here every function takes
//! the staging parent (install/update) or an explicit root so tests stay
//! hermetic. Layout: `<root>/pack-cache/<version>__<javaType>/{meta.json,pack/}`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fs_utils::{copy_tree, exists, read_json_value, remove_if_exists, write_json};
use crate::paths::{sanitize_name, LauncherPaths};

const TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

pub fn pack_cache_key(version: &str, java_type: &str) -> String {
    format!("{}__{}", sanitize_name(version), sanitize_name(java_type))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMeta {
    pack_version: String,
    java_type: String,
    download_url: String,
    cached_at_secs: i64,
    last_used_at_secs: i64,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cache_root(paths: &LauncherPaths) -> PathBuf {
    paths.pack_cache_root()
}

fn entry_dir(paths: &LauncherPaths, key: &str) -> PathBuf {
    cache_root(paths).join(key)
}

async fn entry_valid(entry: &Path, url: &str) -> bool {
    let Some(meta_value) = read_json_value(&entry.join("meta.json")).await else {
        return false;
    };
    let Ok(meta): Result<CacheMeta, _> = serde_json::from_value(meta_value) else {
        return false;
    };
    meta.download_url == url
        && now_secs() - meta.cached_at_secs <= TTL_SECONDS
        && exists(&entry.join("pack")).await
}

pub async fn evict_expired_pack_cache(paths: &LauncherPaths) -> Result<(), anyhow::Error> {
    let root = cache_root(paths);
    if !exists(&root).await {
        return Ok(());
    }
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !tokio::fs::symlink_metadata(entry.path())
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let target = entry.path();
        let expired = match read_json_value(&target.join("meta.json")).await {
            Some(v) => serde_json::from_value::<CacheMeta>(v)
                .map(|m| now_secs() - m.cached_at_secs > TTL_SECONDS)
                .unwrap_or(true),
            None => true,
        };
        if expired {
            remove_if_exists(&target).await?;
        }
    }
    Ok(())
}

pub async fn lookup_pack_cache(
    paths: &LauncherPaths,
    version: &str,
    java_type: &str,
    url: &str,
) -> Result<Option<PathBuf>, anyhow::Error> {
    evict_expired_pack_cache(paths).await?;
    let entry = entry_dir(paths, &pack_cache_key(version, java_type));
    if !entry_valid(&entry, url).await {
        if exists(&entry).await {
            remove_if_exists(&entry).await?;
        }
        return Ok(None);
    }
    if let Some(meta_value) = read_json_value(&entry.join("meta.json")).await {
        if let Ok(mut meta) = serde_json::from_value::<CacheMeta>(meta_value) {
            meta.last_used_at_secs = now_secs();
            write_json(&entry.join("meta.json"), &serde_json::to_value(&meta)?).await?;
        }
    }
    Ok(Some(entry.join("pack")))
}

pub async fn store_pack_cache(
    paths: &LauncherPaths,
    version: &str,
    java_type: &str,
    url: &str,
    staging: &Path,
) -> Result<(), anyhow::Error> {
    if !exists(staging).await {
        anyhow::bail!("cannot cache pack: staging directory missing");
    }
    let entry = entry_dir(paths, &pack_cache_key(version, java_type));
    if entry_valid(&entry, url).await {
        return Ok(());
    }
    let temp = entry.with_extension("tmp");
    remove_if_exists(&temp).await?;
    copy_tree(staging, &temp.join("pack")).await?;
    let timestamp = now_secs();
    write_json(
        &temp.join("meta.json"),
        &serde_json::to_value(&CacheMeta {
            pack_version: version.to_string(),
            java_type: java_type.to_string(),
            download_url: url.to_string(),
            cached_at_secs: timestamp,
            last_used_at_secs: timestamp,
        })?,
    )
    .await?;
    remove_if_exists(&entry).await?;
    if let Some(parent) = entry.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(&temp, &entry).await?;
    Ok(())
}

pub async fn copy_cached_pack_to_staging(
    cache_pack: &Path,
    staging: &Path,
) -> Result<(), anyhow::Error> {
    remove_if_exists(staging).await?;
    copy_tree(cache_pack, staging).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-cache-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn stores_hits_and_evicts() {
        let paths = temp_paths("cycle");
        let staging = paths.base().join("staging");
        tokio::fs::create_dir_all(staging.join("mods"))
            .await
            .unwrap();
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();

        assert_eq!(
            lookup_pack_cache(&paths, "2.8.0", "java17+", "https://x/pack.zip")
                .await
                .unwrap(),
            None
        );
        store_pack_cache(&paths, "2.8.0", "java17+", "https://x/pack.zip", &staging)
            .await
            .unwrap();
        let hit = lookup_pack_cache(&paths, "2.8.0", "java17+", "https://x/pack.zip")
            .await
            .unwrap()
            .unwrap();
        assert!(hit.join("mmc-pack.json").exists());

        // A changed URL invalidates the entry.
        assert_eq!(
            lookup_pack_cache(&paths, "2.8.0", "java17+", "https://x/other.zip")
                .await
                .unwrap(),
            None
        );

        // Expired entries are evicted.
        store_pack_cache(&paths, "2.8.0", "java17+", "https://x/other.zip", &staging)
            .await
            .unwrap();
        let entry = entry_dir(&paths, &pack_cache_key("2.8.0", "java17+"));
        let mut meta: serde_json::Value = serde_json::from_str(
            &tokio::fs::read_to_string(entry.join("meta.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        meta["cached_at_secs"] = serde_json::json!(0);
        tokio::fs::write(
            entry.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .await
        .unwrap();
        evict_expired_pack_cache(&paths).await.unwrap();
        assert!(!entry.exists());
    }

    #[test]
    fn cache_keys_are_filename_safe() {
        assert_eq!(
            pack_cache_key("2.9.0-beta-1", "java17+"),
            "2.9.0-beta-1__java17+"
        );
        assert_eq!(pack_cache_key("a/b", "c d"), "a_b__c_d");
    }
}
