//! Instance discovery + loading (mirrors `knownInstanceIds` / `loadInstances`
//! in `electron/backend/index.ts`).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use industrialis_core::instance_settings::InstanceSettings;

use crate::fs_utils::exists;
use crate::groups::get_instance_group;
use crate::icons::resolve_icon_path;
use crate::paths::LauncherPaths;
use crate::settings::load_instance_settings;

/// Mirrors the TS `InstanceInfo` (icon resolution arrives with Phase 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: String,
    pub installed: bool,
    pub size_bytes: u64,
    pub settings: InstanceSettings,
    pub group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
}

async fn is_instance_dir(dir: &std::path::Path) -> bool {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return false;
    }
    if exists(&dir.join("mmc-pack.json")).await {
        return true;
    }
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return false,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if tokio::fs::symlink_metadata(&path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
            && exists(&path.join("mmc-pack.json")).await
        {
            return true;
        }
    }
    false
}

/// A directory counts as an instance when it (or a nested child, for legacy
/// layouts) contains `mmc-pack.json` — mirrors `knownInstanceIds`.
pub async fn known_instance_ids(paths: &LauncherPaths) -> Result<HashSet<String>, anyhow::Error> {
    let root = paths.instances_dir();
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(e) => return Err(e.into()),
    };
    let mut ids = HashSet::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        // Import staging directories are hidden partial copies; a cancelled or
        // crashed import may leave one behind and it must not become an instance.
        if name.starts_with(".import-") {
            continue;
        }
        if !tokio::fs::symlink_metadata(&path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        if is_instance_dir(&path).await {
            ids.insert(name);
        }
    }
    Ok(ids)
}

/// Mirrors `loadInstances`: settings load with `pack_version` backfill to the
/// directory id, sorted by id, with group + icon resolution.
pub async fn load_instances(paths: &LauncherPaths) -> Result<Vec<InstanceInfo>, anyhow::Error> {
    Ok(load_instances_with_issues(paths).await?.0)
}

/// Discovery is read-only. A damaged instance stays on disk and does not hide
/// healthy instances; the UI receives the recovery issue alongside the list.
pub async fn load_instances_with_issues(
    paths: &LauncherPaths,
) -> anyhow::Result<(Vec<InstanceInfo>, Vec<String>)> {
    let known = known_instance_ids(paths).await?;
    let mut ids: Vec<String> = known.iter().cloned().collect();
    ids.sort();
    let mut list = Vec::new();
    let mut issues = Vec::new();
    for id in ids {
        let mut settings = match load_instance_settings(paths, &id).await {
            Ok(settings) => settings,
            Err(e) => {
                issues.push(format!("Could not load {id}: {e:#}"));
                continue;
            }
        };
        // Frozen Prism imports have no pack version by design; only legacy
        // documents with a missing version fall back to the directory id.
        if settings.pack_version.is_empty() && !settings.imported {
            settings.pack_version = id.clone();
        }
        let group = match get_instance_group(paths, &id, &known).await {
            Ok(group) => group,
            Err(e) => {
                issues.push(format!("Could not load groups: {e:#}"));
                String::new()
            }
        };
        let icon_path = match paths.instance_dir(&id) {
            Ok(dir) => resolve_icon_path(&dir, settings.custom_icon.as_deref()).await,
            Err(_) => None,
        };
        list.push(InstanceInfo {
            size_bytes: settings.cached_size_bytes,
            settings,
            group,
            icon_path: icon_path.map(|p| p.to_string_lossy().into_owned()),
            id,
            installed: true,
        });
    }
    Ok((list, issues))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-instances-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn discovers_mmc_instances_and_skips_plain_dirs() {
        let paths = temp_paths("discover");
        let root = paths.instances_dir();
        tokio::fs::create_dir_all(root.join("gtnh")).await.unwrap();
        tokio::fs::write(root.join("gtnh").join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("nested").join("inner"))
            .await
            .unwrap();
        tokio::fs::write(
            root.join("nested").join("inner").join("mmc-pack.json"),
            "{}",
        )
        .await
        .unwrap();
        tokio::fs::create_dir_all(root.join("empty")).await.unwrap();
        tokio::fs::write(root.join("stray.txt"), "x").await.unwrap();

        let mut ids: Vec<String> = known_instance_ids(&paths)
            .await
            .unwrap()
            .into_iter()
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["gtnh", "nested"]);
    }

    #[tokio::test]
    async fn backfills_pack_version_from_directory_id() {
        let paths = temp_paths("backfill");
        let root = paths.instances_dir();
        tokio::fs::create_dir_all(root.join("gtnh")).await.unwrap();
        tokio::fs::write(root.join("gtnh").join("mmc-pack.json"), "{}")
            .await
            .unwrap();

        let list = load_instances(&paths).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "gtnh");
        assert_eq!(list[0].settings.pack_version, "gtnh");
        assert!(list[0].installed);
    }

    #[tokio::test]
    async fn empty_root_yields_no_instances() {
        let paths = temp_paths("empty");
        assert!(known_instance_ids(&paths).await.unwrap().is_empty());
        assert!(load_instances(&paths).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn frozen_prism_imports_keep_an_empty_pack_version() {
        let paths = temp_paths("frozen-import");
        let root = paths.instances_dir();
        tokio::fs::create_dir_all(root.join("imported")).await.unwrap();
        tokio::fs::write(root.join("imported").join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(
            paths.settings_path("imported").unwrap(),
            r#"{"name":"Imported","imported":true}"#,
        )
        .await
        .unwrap();

        let list = load_instances(&paths).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].settings.imported);
        assert_eq!(list[0].settings.pack_version, "");
    }

    #[tokio::test]
    async fn import_staging_directories_are_not_instances() {
        let paths = temp_paths("import-staging");
        let staging = paths.instances_dir().join(".import-gtnh-uuid");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();

        assert!(load_instances(&paths).await.unwrap().is_empty());
    }
}
