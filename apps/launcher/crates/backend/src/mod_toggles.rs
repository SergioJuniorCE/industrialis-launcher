//! Per-instance persistent mod enable/disable.
//!
//! Disabling renames a mod file to `<name>.disabled` in the active mods dir and
//! the custom overlay. The stable filename identity is recorded in
//! `instance.json`, and every install/update/reinstall/launch re-disables
//! matching files, so a new pack version stays disabled.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use industrialis_core::instance_settings::DisabledMod;
use serde::{Deserialize, Serialize};

use crate::fs_utils::exists;
use crate::pack_files::mod_identity_from_filename;
use crate::paths::LauncherPaths;
use crate::settings::{load_instance_settings, save_instance_settings_unlocked};

const GAME_ROOT: &str = ".minecraft";
const OVERLAY_ROOT: &str = "persistent-minecraft";
const MODS_DIR: &str = "mods";
const DISABLED_SUFFIX: &str = ".disabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMod {
    pub filename: String,
    pub identity: String,
    pub size_bytes: u64,
    pub enabled: bool,
}

pub fn disabled_identities(disabled: &[DisabledMod]) -> HashSet<String> {
    disabled.iter().map(|entry| entry.identity.clone()).collect()
}

/// The enabled filename behind a `.disabled` file.
pub fn enabled_filename(name: &str) -> Option<&str> {
    let lower = name.to_lowercase();
    if lower.ends_with(".jar.disabled") || lower.ends_with(".zip.disabled") {
        Some(&name[..name.len() - DISABLED_SUFFIX.len()])
    } else {
        None
    }
}

pub fn is_enabled_filename(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jar") || lower.ends_with(".zip")
}

pub fn identity_for_filename(name: &str) -> String {
    mod_identity_from_filename(enabled_filename(name).unwrap_or(name))
}

fn normalize_identity(identity: &str) -> anyhow::Result<String> {
    let normalized = identity.trim().to_lowercase();
    anyhow::ensure!(!normalized.is_empty(), "mod identity is required");
    Ok(normalized)
}

fn mods_dirs(instance: &Path) -> [PathBuf; 2] {
    [
        instance.join(GAME_ROOT).join(MODS_DIR),
        instance.join(OVERLAY_ROOT).join(MODS_DIR),
    ]
}

pub async fn list_installed(dir: &Path) -> anyhow::Result<Vec<InstalledMod>> {
    if !exists(dir).await {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut mods = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let enabled = is_enabled_filename(&name);
        let disabled = enabled_filename(&name).is_some();
        if !enabled && !disabled {
            continue;
        }
        let meta = tokio::fs::symlink_metadata(entry.path()).await?;
        if !meta.is_file() {
            continue;
        }
        mods.push(InstalledMod {
            filename: name.clone(),
            identity: identity_for_filename(&name),
            size_bytes: meta.len(),
            enabled,
        });
    }
    mods.sort_by(|a, b| {
        a.identity
            .cmp(&b.identity)
            .then_with(|| b.enabled.cmp(&a.enabled))
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(mods)
}

/// Disable every enabled file matching `identity` in both mod roots and record
/// the identity. A disabled file left behind by a crash is recorded too.
pub async fn disable_mod(paths: &LauncherPaths, id: &str, identity: &str) -> anyhow::Result<()> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let normalized = normalize_identity(identity)?;
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let _lease = lease;
        let mut settings = load_instance_settings(&paths, &id).await?;
        anyhow::ensure!(
            !settings
                .disabled_mods
                .iter()
                .any(|entry| entry.identity == normalized),
            "This mod is already disabled"
        );
        let instance = paths.instance_dir(&id)?;
        let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut filename = None;
        for dir in mods_dirs(&instance) {
            for entry in list_installed(&dir).await? {
                if entry.identity != normalized {
                    continue;
                }
                if entry.enabled {
                    filename.get_or_insert_with(|| entry.filename.clone());
                    moves.push((
                        dir.join(&entry.filename),
                        dir.join(format!("{}{}", entry.filename, DISABLED_SUFFIX)),
                    ));
                } else if filename.is_none() {
                    filename = enabled_filename(&entry.filename).map(str::to_string);
                }
            }
        }
        let Some(filename) = filename else {
            anyhow::bail!("Mod not found");
        };
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut renamed = Vec::new();
            for (source, destination) in &moves {
                if let Err(error) = std::fs::rename(source, destination) {
                    for (source, destination) in renamed.into_iter().rev() {
                        let _ = std::fs::rename(destination, source);
                    }
                    return Err(error.into());
                }
                renamed.push((source.clone(), destination.clone()));
            }
            Ok(())
        })
        .await??;
        settings
            .disabled_mods
            .push(DisabledMod { identity: normalized, filename });
        save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await
}

/// Rename disabled files back to their loadable names and remove the record.
pub async fn enable_mod(paths: &LauncherPaths, id: &str, identity: &str) -> anyhow::Result<()> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let normalized = normalize_identity(identity)?;
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let _lease = lease;
        let mut settings = load_instance_settings(&paths, &id).await?;
        let index = settings
            .disabled_mods
            .iter()
            .position(|entry| entry.identity == normalized)
            .ok_or_else(|| anyhow::anyhow!("Disabled mod not found"))?;
        let instance = paths.instance_dir(&id)?;
        let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
        for dir in mods_dirs(&instance) {
            for entry in list_installed(&dir).await? {
                if entry.identity == normalized && !entry.enabled {
                    let base = enabled_filename(&entry.filename)
                        .unwrap_or(&entry.filename)
                        .to_string();
                    moves.push((dir.join(&entry.filename), dir.join(base)));
                }
            }
        }
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut renamed: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (source, destination) in &moves {
                // An enabled copy may already exist after a manual import; the
                // mod is enabled, so the disabled duplicate can go.
                if destination.exists() {
                    if let Err(error) = std::fs::remove_file(source) {
                        for (source, destination) in renamed.into_iter().rev() {
                            let _ = std::fs::rename(destination, source);
                        }
                        return Err(error.into());
                    }
                    continue;
                }
                if let Err(error) = std::fs::rename(source, destination) {
                    for (source, destination) in renamed.into_iter().rev() {
                        let _ = std::fs::rename(destination, source);
                    }
                    return Err(error.into());
                }
                renamed.push((source.clone(), destination.clone()));
            }
            Ok(())
        })
        .await??;
        settings.disabled_mods.remove(index);
        save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await
}

pub async fn set_enabled(
    paths: &LauncherPaths,
    id: &str,
    identity: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    if enabled {
        enable_mod(paths, id, identity).await
    } else {
        disable_mod(paths, id, identity).await
    }
}

/// Rename every enabled file whose identity is disabled. Runs after pack
/// applies and on every launch, so new pack versions stay disabled.
pub async fn apply_disabled_mods(
    instance: &Path,
    disabled: &[DisabledMod],
) -> anyhow::Result<usize> {
    if disabled.is_empty() {
        return Ok(0);
    }
    let identities = disabled_identities(disabled);
    let mut count = 0;
    for dir in mods_dirs(instance) {
        for entry in list_installed(&dir).await? {
            if entry.enabled && identities.contains(&entry.identity) {
                tokio::fs::rename(
                    dir.join(&entry.filename),
                    dir.join(format!("{}{}", entry.filename, DISABLED_SUFFIX)),
                )
                .await?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Record `.disabled` files that have no settings entry yet. This heals a
/// crash between the rename and the settings write.
pub async fn recover_interrupted_disables(
    paths: &LauncherPaths,
    id: &str,
) -> anyhow::Result<bool> {
    let instance = paths.instance_dir(id)?;
    let mut found = Vec::new();
    for dir in mods_dirs(&instance) {
        for entry in list_installed(&dir).await? {
            if !entry.enabled {
                found.push(DisabledMod {
                    identity: entry.identity,
                    filename: enabled_filename(&entry.filename)
                        .unwrap_or(&entry.filename)
                        .to_string(),
                });
            }
        }
    }
    if found.is_empty() {
        return Ok(false);
    }
    let paths = paths.clone();
    let id = id.to_string();
    crate::profile_store::mutate(async move {
        let mut settings = load_instance_settings(&paths, &id).await?;
        let mut changed = false;
        for record in found {
            if !settings
                .disabled_mods
                .iter()
                .any(|entry| entry.identity == record.identity)
            {
                settings.disabled_mods.push(record);
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

pub async fn recover_interrupted_disables_all(
    paths: &LauncherPaths,
) -> anyhow::Result<Vec<String>> {
    let mut recovered = Vec::new();
    for id in crate::instances::known_instance_ids(paths).await? {
        if recover_interrupted_disables(paths, &id).await? {
            recovered.push(id);
        }
    }
    Ok(recovered)
}

/// Whether a file is covered by a disabled identity (used by the overlay pass).
pub fn is_disabled(filename: &str, disabled: &HashSet<String>) -> bool {
    disabled.contains(&identity_for_filename(filename))
}

#[cfg(test)]
mod tests {
    use super::*;
    use industrialis_core::instance_settings::InstanceSettings;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-toggles-test-{name}-{}",
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
    async fn disable_and_enable_rename_both_roots() {
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
        disable_mod(&paths, "pack", "DefaultServerList")
            .await
            .unwrap();
        let instance = paths.instance_dir("pack").unwrap();
        for root in [".minecraft", "persistent-minecraft"] {
            assert!(instance
                .join(root)
                .join("mods/DefaultServerList-1.0.jar.disabled")
                .exists());
            assert!(!instance
                .join(root)
                .join("mods/DefaultServerList-1.0.jar")
                .exists());
        }
        assert!(instance.join(".minecraft/mods/keep-me.jar").exists());
        let settings = load_instance_settings(&paths, "pack").await.unwrap();
        assert_eq!(
            settings.disabled_mods,
            vec![DisabledMod {
                identity: "defaultserverlist".into(),
                filename: "DefaultServerList-1.0.jar".into(),
            }]
        );

        enable_mod(&paths, "pack", "defaultserverlist")
            .await
            .unwrap();
        for root in [".minecraft", "persistent-minecraft"] {
            assert!(instance
                .join(root)
                .join("mods/DefaultServerList-1.0.jar")
                .exists());
            assert!(!instance
                .join(root)
                .join("mods/DefaultServerList-1.0.jar.disabled")
                .exists());
        }
        assert!(load_instance_settings(&paths, "pack")
            .await
            .unwrap()
            .disabled_mods
            .is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn enforcement_disables_new_versions_and_manual_drops() {
        let paths = temp_paths("enforce");
        seed(&paths, "pack", &[(".minecraft", "keep-me.jar")]).await;
        let instance = paths.instance_dir("pack").unwrap();
        tokio::fs::write(
            instance.join(".minecraft/mods/DefaultServerList-2.0.jar"),
            "new",
        )
        .await
        .unwrap();
        let disabled = vec![DisabledMod {
            identity: "defaultserverlist".into(),
            filename: "DefaultServerList-1.0.jar".into(),
        }];
        assert_eq!(
            apply_disabled_mods(&instance, &disabled).await.unwrap(),
            1
        );
        assert!(instance
            .join(".minecraft/mods/DefaultServerList-2.0.jar.disabled")
            .exists());
        assert!(!instance
            .join(".minecraft/mods/DefaultServerList-2.0.jar")
            .exists());
        assert!(instance.join(".minecraft/mods/keep-me.jar").exists());
        assert_eq!(apply_disabled_mods(&instance, &[]).await.unwrap(), 0);
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn recovery_records_disabled_files_without_a_settings_entry() {
        let paths = temp_paths("recovery");
        seed(
            &paths,
            "pack",
            &[(".minecraft", "DefaultServerList-1.0.jar.disabled")],
        )
        .await;
        assert!(recover_interrupted_disables(&paths, "pack")
            .await
            .unwrap());
        assert_eq!(
            load_instance_settings(&paths, "pack")
                .await
                .unwrap()
                .disabled_mods,
            vec![DisabledMod {
                identity: "defaultserverlist".into(),
                filename: "DefaultServerList-1.0.jar".into(),
            }]
        );
        assert!(!recover_interrupted_disables(&paths, "pack")
            .await
            .unwrap());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn installed_list_includes_enabled_and_disabled_files() {
        let paths = temp_paths("list");
        seed(
            &paths,
            "pack",
            &[
                (".minecraft", "DefaultServerList-1.0.jar.disabled"),
                (".minecraft", "keep-me.jar"),
                (".minecraft", "notes.txt"),
            ],
        )
        .await;
        let dir = paths.instance_dir("pack").unwrap().join(".minecraft/mods");
        let mods = list_installed(&dir).await.unwrap();
        assert_eq!(mods.len(), 2);
        assert!(!mods
            .iter()
            .find(|entry| entry.identity == "defaultserverlist")
            .unwrap()
            .enabled);
        assert!(mods.iter().any(|entry| entry.identity == "keep-me" && entry.enabled));
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
