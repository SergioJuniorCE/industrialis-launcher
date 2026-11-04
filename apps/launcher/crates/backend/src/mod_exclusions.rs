//! Per-instance persistent mod removal.
//!
//! A removed mod's files move into a launcher-owned quarantine outside the
//! instance tree, and its identity is recorded in `instance.json`. Pack
//! installs, updates, reinstalls, and launches then keep that identity out of
//! `.minecraft/mods`, so a pack update cannot bring the mod back.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use industrialis_core::instance_settings::{RemovedMod, RemovedModFile};

use crate::fs_utils::{copy_tree, exists, remove_if_exists};
use crate::pack_files::{list_mods, mod_identity_from_filename};
use crate::paths::{sanitize_name, LauncherPaths};
use crate::settings::{load_instance_settings, save_instance_settings_unlocked};

const GAME_ROOT: &str = ".minecraft";
const OVERLAY_ROOT: &str = "persistent-minecraft";
const MODS_DIR: &str = "mods";
const QUARANTINE_DIR: &str = "removed-mods";
const QUARANTINE_ENTRY: &str = "entry.json";

pub fn quarantine_root(paths: &LauncherPaths) -> PathBuf {
    paths.base().join(QUARANTINE_DIR)
}

pub fn quarantine_dir(paths: &LauncherPaths, id: &str) -> PathBuf {
    quarantine_root(paths).join(id)
}

pub fn quarantine_mod_dir(paths: &LauncherPaths, id: &str, identity: &str) -> PathBuf {
    quarantine_dir(paths, id).join(sanitize_name(identity))
}

pub fn excluded_identities(removed: &[RemovedMod]) -> HashSet<String> {
    removed.iter().map(|entry| entry.identity.clone()).collect()
}

fn normalize_identity(identity: &str) -> anyhow::Result<String> {
    let normalized = identity.trim().to_lowercase();
    anyhow::ensure!(!normalized.is_empty(), "mod identity is required");
    let safe = sanitize_name(&normalized);
    anyhow::ensure!(
        !safe.is_empty() && safe != "." && safe != "..",
        "invalid mod identity"
    );
    Ok(normalized)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn move_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(source, destination).is_err() {
        // The data dir and instance can live on different volumes (portable
        // instances, symlinked profiles), where rename is not atomic.
        std::fs::copy(source, destination)?;
        std::fs::remove_file(source)?;
    }
    Ok(())
}

fn rollback_moves(moved: Vec<(PathBuf, PathBuf)>) {
    for (source, destination) in moved.into_iter().rev() {
        let _ = move_file(&destination, &source);
    }
}

/// Remove every file matching `identity` from the active mods dir and the
/// custom overlay, then record the exclusion. The settings write and the file
/// moves share one profile transaction.
pub async fn remove_mod(
    paths: &LauncherPaths,
    id: &str,
    identity: &str,
) -> anyhow::Result<RemovedMod> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let normalized = normalize_identity(identity)?;
    let instance = paths.instance_dir(id)?;
    let settings = load_instance_settings(paths, id).await?;
    anyhow::ensure!(
        !settings
            .removed_mods
            .iter()
            .any(|entry| entry.identity == normalized),
        "This mod is already removed"
    );
    let mut files = Vec::new();
    for root in [GAME_ROOT, OVERLAY_ROOT] {
        let dir = instance.join(root).join(MODS_DIR);
        for entry in list_mods(&dir).await? {
            if entry.identity != normalized {
                continue;
            }
            let path = dir.join(&entry.filename);
            let meta = tokio::fs::symlink_metadata(&path).await?;
            anyhow::ensure!(
                meta.is_file() && !meta.permissions().readonly(),
                "A mod file cannot be removed; its files have been kept"
            );
            files.push(RemovedModFile {
                root: root.to_string(),
                filename: entry.filename,
            });
        }
    }
    anyhow::ensure!(!files.is_empty(), "Mod not found");
    let record = RemovedMod {
        identity: normalized.clone(),
        filename: files[0].filename.clone(),
        removed_at: unix_now(),
        files: files.clone(),
    };
    let quarantine = quarantine_mod_dir(paths, id, &normalized);
    let paths = paths.clone();
    let id = id.to_string();
    let record_to_save = record.clone();
    let marker_record = record.clone();
    crate::profile_store::mutate(async move {
        let move_paths = paths.clone();
        let move_id = id.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let _lease = lease;
            std::fs::create_dir_all(&quarantine)?;
            // Journal the intent before moving bytes so an interrupted removal
            // is completed on the next startup instead of being reverted by a
            // pack update.
            std::fs::write(
                quarantine.join(QUARANTINE_ENTRY),
                serde_json::to_vec_pretty(&marker_record)?,
            )?;
            let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
            for file in &files {
                let source = move_paths
                    .instance_dir(&move_id)?
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                let destination = quarantine
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                if let Err(error) = move_file(&source, &destination) {
                    rollback_moves(moved);
                    let _ = std::fs::remove_dir_all(&quarantine);
                    return Err(error);
                }
                moved.push((source, destination));
            }
            Ok(())
        })
        .await??;
        let mut settings = load_instance_settings(&paths, &id).await?;
        settings.removed_mods.push(record_to_save);
        save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await?;
    Ok(record)
}

/// Put a quarantined mod back into its original mods dirs and drop the record.
pub async fn restore_mod(paths: &LauncherPaths, id: &str, identity: &str) -> anyhow::Result<()> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let normalized = normalize_identity(identity)?;
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let mut settings = load_instance_settings(&paths, &id).await?;
        let index = settings
            .removed_mods
            .iter()
            .position(|entry| entry.identity == normalized)
            .ok_or_else(|| anyhow::anyhow!("Removed mod not found"))?;
        let record = settings.removed_mods[index].clone();
        let quarantine = quarantine_mod_dir(&paths, &id, &record.identity);
        anyhow::ensure!(
            exists(&quarantine).await,
            "The removed mod's backup is missing; it cannot be restored"
        );
        let instance = paths.instance_dir(&id)?;
        let move_quarantine = quarantine.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let _lease = lease;
            let mut restored: Vec<(PathBuf, PathBuf)> = Vec::new();
            for file in &record.files {
                let source = move_quarantine
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                let destination = instance
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                anyhow::ensure!(
                    source.is_file(),
                    "The removed mod's backup is incomplete; its files have been kept"
                );
                if let Err(error) = move_file(&source, &destination) {
                    rollback_moves(restored);
                    return Err(error);
                }
                restored.push((source, destination));
            }
            Ok(())
        })
        .await??;
        settings.removed_mods.remove(index);
        save_instance_settings_unlocked(&paths, &id, &settings).await?;
        let _ = remove_if_exists(&quarantine).await;
        Ok(())
    })
    .await
}

/// Finish a removal that was interrupted between journaling the intent and
/// saving the settings record. Missing quarantined files are moved out of the
/// instance first, then the record is added to the settings.
pub async fn recover_interrupted_removal(paths: &LauncherPaths, id: &str) -> anyhow::Result<bool> {
    let root = quarantine_dir(paths, id);
    if !exists(&root).await {
        return Ok(false);
    }
    let mut records = Vec::new();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        if let Ok(raw) = tokio::fs::read(entry.path().join(QUARANTINE_ENTRY)).await {
            if let Ok(record) = serde_json::from_slice::<RemovedMod>(&raw) {
                records.push(record);
            }
        }
    }
    if records.is_empty() {
        return Ok(false);
    }
    let instance = paths.instance_dir(id)?;
    let complete_records = records.clone();
    let complete_root = root.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        for record in &complete_records {
            let quarantine = complete_root.join(sanitize_name(&record.identity));
            for file in &record.files {
                let quarantined = quarantine
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                let original = instance
                    .join(&file.root)
                    .join(MODS_DIR)
                    .join(&file.filename);
                if !quarantined.exists() && original.exists() {
                    move_file(&original, &quarantined)?;
                }
            }
        }
        Ok(())
    })
    .await??;
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let mut settings = load_instance_settings(&paths, &id).await?;
        let mut changed = false;
        for record in records {
            if !settings
                .removed_mods
                .iter()
                .any(|entry| entry.identity == record.identity)
            {
                settings.removed_mods.push(record);
                changed = true;
            }
        }
        if changed {
            save_instance_settings_unlocked(&paths, &id, &settings).await?;
        }
        Ok(changed)
    })
    .await
}

/// Recover every interrupted removal in the profile. Safe to call on startup.
pub async fn recover_interrupted_removals(paths: &LauncherPaths) -> anyhow::Result<Vec<String>> {
    let root = quarantine_root(paths);
    if !exists(&root).await {
        return Ok(Vec::new());
    }
    let mut recovered = Vec::new();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(id) = crate::paths::validate_instance_id(&name) else {
            continue;
        };
        if id != name {
            continue;
        }
        if recover_interrupted_removal(paths, &id).await? {
            recovered.push(id);
        }
    }
    Ok(recovered)
}

/// Delete excluded identities from the active mods dir. Runs after every pack
/// apply and on every launch, so a pack update or a manual drop cannot
/// resurrect a removed mod.
pub async fn apply_removed_mods(instance: &Path, removed: &[RemovedMod]) -> anyhow::Result<usize> {
    if removed.is_empty() {
        return Ok(0);
    }
    let excluded = excluded_identities(removed);
    let dir = instance.join(GAME_ROOT).join(MODS_DIR);
    let mut count = 0;
    for entry in list_mods(&dir).await? {
        if excluded.contains(&entry.identity) {
            tokio::fs::remove_file(dir.join(&entry.filename)).await?;
            count += 1;
        }
    }
    Ok(count)
}

/// Whether a file in the custom overlay is covered by an exclusion.
pub fn is_excluded(filename: &str, excluded: &HashSet<String>) -> bool {
    excluded.contains(&mod_identity_from_filename(filename))
}

pub async fn copy_quarantine(
    paths: &LauncherPaths,
    source_id: &str,
    new_id: &str,
) -> anyhow::Result<()> {
    let source = quarantine_dir(paths, source_id);
    if !exists(&source).await {
        return Ok(());
    }
    copy_tree(&source, &quarantine_dir(paths, new_id)).await
}

pub async fn delete_quarantine(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    remove_if_exists(&quarantine_dir(paths, id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use industrialis_core::instance_settings::InstanceSettings;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-exclusions-test-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        LauncherPaths::new(dir)
    }

    async fn seed(paths: &LauncherPaths, id: &str, files: &[(&str, &str)]) {
        let instance = paths.instance_dir(id).unwrap();
        tokio::fs::create_dir_all(instance.join(".minecraft/mods"))
            .await
            .unwrap();
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        for (root, name) in files {
            tokio::fs::create_dir_all(instance.join(root).join("mods"))
                .await
                .unwrap();
            tokio::fs::write(instance.join(root).join("mods").join(name), name)
                .await
                .unwrap();
        }
        let settings = InstanceSettings {
            name: id.to_string(),
            ..InstanceSettings::default()
        };
        crate::settings::save_instance_settings(paths, id, &settings)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn removal_quarantines_both_roots_and_restore_returns_them() {
        let paths = temp_paths("roundtrip");
        seed(
            &paths,
            "pack",
            &[
                (".minecraft", "DefaultServerList-1.0.jar"),
                ("persistent-minecraft", "DefaultServerList-1.0.jar"),
                (".minecraft", "keep-me.jar"),
            ],
        )
        .await;
        let record = remove_mod(&paths, "pack", "DefaultServerList")
            .await
            .unwrap();
        assert_eq!(record.identity, "defaultserverlist");
        assert_eq!(record.files.len(), 2);
        let instance = paths.instance_dir("pack").unwrap();
        assert!(!instance
            .join(".minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert!(!instance
            .join("persistent-minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert!(instance.join(".minecraft/mods/keep-me.jar").exists());
        assert!(quarantine_mod_dir(&paths, "pack", "defaultserverlist")
            .join(".minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        let settings = load_instance_settings(&paths, "pack").await.unwrap();
        assert_eq!(settings.removed_mods, vec![record]);

        restore_mod(&paths, "pack", "defaultserverlist")
            .await
            .unwrap();
        assert!(instance
            .join(".minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert!(instance
            .join("persistent-minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert!(load_instance_settings(&paths, "pack")
            .await
            .unwrap()
            .removed_mods
            .is_empty());
        assert!(!quarantine_mod_dir(&paths, "pack", "defaultserverlist").exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn enforcement_deletes_identity_matches_after_updates_and_manual_drops() {
        let paths = temp_paths("enforce");
        seed(&paths, "pack", &[(".minecraft", "keep-me.jar")]).await;
        let instance = paths.instance_dir("pack").unwrap();
        // Simulate a pack update shipping a newer filename for the same mod.
        tokio::fs::write(
            instance.join(".minecraft/mods/DefaultServerList-2.0.jar"),
            "new",
        )
        .await
        .unwrap();
        let removed = vec![RemovedMod {
            identity: "defaultserverlist".into(),
            filename: "DefaultServerList-1.0.jar".into(),
            removed_at: 1,
            files: Vec::new(),
        }];
        assert_eq!(apply_removed_mods(&instance, &removed).await.unwrap(), 1);
        assert!(!instance
            .join(".minecraft/mods/DefaultServerList-2.0.jar")
            .exists());
        assert!(instance.join(".minecraft/mods/keep-me.jar").exists());
        assert_eq!(apply_removed_mods(&instance, &[]).await.unwrap(), 0);
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn interrupted_removal_is_completed_on_recovery() {
        let paths = temp_paths("recovery");
        seed(
            &paths,
            "pack",
            &[
                (".minecraft", "DefaultServerList-1.0.jar"),
                ("persistent-minecraft", "DefaultServerList-1.0.jar"),
                (".minecraft", "keep-me.jar"),
            ],
        )
        .await;
        let record = RemovedMod {
            identity: "defaultserverlist".into(),
            filename: "DefaultServerList-1.0.jar".into(),
            removed_at: 1,
            files: vec![
                RemovedModFile {
                    root: ".minecraft".into(),
                    filename: "DefaultServerList-1.0.jar".into(),
                },
                RemovedModFile {
                    root: "persistent-minecraft".into(),
                    filename: "DefaultServerList-1.0.jar".into(),
                },
            ],
        };
        let quarantine = quarantine_mod_dir(&paths, "pack", "defaultserverlist");
        std::fs::create_dir_all(quarantine.join(".minecraft/mods")).unwrap();
        std::fs::write(
            quarantine.join(QUARANTINE_ENTRY),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        let instance = paths.instance_dir("pack").unwrap();
        std::fs::rename(
            instance.join(".minecraft/mods/DefaultServerList-1.0.jar"),
            quarantine.join(".minecraft/mods/DefaultServerList-1.0.jar"),
        )
        .unwrap();

        assert!(recover_interrupted_removal(&paths, "pack").await.unwrap());
        assert!(quarantine
            .join("persistent-minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert!(!instance
            .join("persistent-minecraft/mods/DefaultServerList-1.0.jar")
            .exists());
        assert_eq!(
            load_instance_settings(&paths, "pack")
                .await
                .unwrap()
                .removed_mods,
            vec![record]
        );
        assert!(!recover_interrupted_removal(&paths, "pack").await.unwrap());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[allow(clippy::permissions_set_readonly_false)]
    #[tokio::test]
    async fn read_only_files_abort_without_partial_removal() {
        let paths = temp_paths("readonly");
        seed(
            &paths,
            "pack",
            &[
                (".minecraft", "DefaultServerList-1.0.jar"),
                (".minecraft", "keep-me.jar"),
            ],
        )
        .await;
        let target = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/mods/DefaultServerList-1.0.jar");
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();
        assert!(remove_mod(&paths, "pack", "defaultserverlist")
            .await
            .is_err());
        assert!(target.exists());
        assert!(load_instance_settings(&paths, "pack")
            .await
            .unwrap()
            .removed_mods
            .is_empty());
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&target, permissions).unwrap();
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
