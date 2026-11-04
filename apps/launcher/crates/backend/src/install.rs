//! Instance install / update / reinstall orchestration. Ports
//! `downloadInstall`, `previewUpdate`, `updateInstance`, `reinstallInstance`,
//! and `refreshSizes` from `electron/backend/instance-lifecycle.ts`.
//!
//! Mutating operations reserve instance paths and check the running pid file.
//! Reinstalls acquire the target pack before touching installed player data.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use industrialis_core::instance_settings::InstanceSettings;

use crate::fs_utils::{dir_size, exists, remove_if_exists, write_json};
use crate::groups::set_instance_group;
use crate::icons::install_default_instance_icon;
use crate::instances::{known_instance_ids, load_instances};
use crate::launch::is_already_running;
use crate::migration::{
    restore_modified_configs, restore_player_data_from_instance, PRESERVE_DIR_NAME,
};
use crate::mod_toggles::{apply_disabled_mods, disabled_identities};
use crate::pack_cache::find_cached_pack;
use crate::pack_files::{
    build_update_preview, download_and_extract_to_staging, install_staging_contents,
    remove_custom_mods_except, ProgressCtx, UpdateModPreview,
};
use crate::pack_prep::{
    apply_persistent_custom_mods_excluding, apply_persistent_minecraft, flatten_nested_pack,
    persistent_custom_mods_dir, prepare_instance_configs,
};
use crate::paths::{validate_instance_id, LauncherPaths};
use crate::settings::{load_instance_settings, save_instance_settings};

/// Mirrors `saveAndRefreshSize`.
pub async fn save_and_refresh_size(
    paths: &LauncherPaths,
    id: &str,
    settings: &mut InstanceSettings,
) -> Result<(), anyhow::Error> {
    let instance = paths.instance_dir(id)?;
    settings.cached_size_bytes = dir_size(&instance).await;
    save_instance_settings(paths, id, settings).await
}

/// Mirrors `refreshSizes`.
pub async fn refresh_sizes(
    paths: &LauncherPaths,
    ids: Option<&[String]>,
) -> Result<HashMap<String, u64>, anyhow::Error> {
    let targets: Vec<String> = match ids {
        Some(list) => list
            .iter()
            .map(|id| crate::paths::sanitize_name(id.trim()))
            .collect(),
        None => known_instance_ids(paths).await?.into_iter().collect(),
    };
    let mut sizes = HashMap::new();
    for id in targets {
        let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
        if is_already_running(paths, &id).await? {
            anyhow::bail!("Cannot refresh size while instance is running");
        }
        let bytes = refresh_size_cache(paths, &id).await?;
        sizes.insert(id, bytes);
    }
    Ok(sizes)
}

/// Caller coordinates the instance operation/lifetime. Game-exit handlers
/// already own that lifetime and must not acquire a second instance lease.
pub(crate) async fn refresh_size_cache(paths: &LauncherPaths, id: &str) -> anyhow::Result<u64> {
    let bytes = dir_size(&paths.instance_dir(id)?).await;
    let latest_paths = paths.clone();
    let target = id.to_owned();
    // Read the latest settings inside the transaction; a directory walk must
    // never overwrite preferences saved while the measurement was running.
    crate::profile_store::mutate(async move {
        let mut latest = load_instance_settings(&latest_paths, &target).await?;
        latest.cached_size_bytes = bytes;
        crate::settings::save_instance_settings_unlocked(&latest_paths, &target, &latest).await
    })
    .await?;
    Ok(bytes)
}

fn progress_ctx(emit: crate::pack_files::EmitProgress, operation: &str, id: &str) -> ProgressCtx {
    ProgressCtx {
        emit,
        operation: operation.to_string(),
        id: Some(id.to_string()),
    }
}

const MAINTENANCE_ROLLBACK_DIR: &str = "maintenance-rollback";

fn rollback_root(paths: &LauncherPaths) -> PathBuf {
    paths.base().join(MAINTENANCE_ROLLBACK_DIR)
}

fn rollback_dir(paths: &LauncherPaths, id: &str) -> PathBuf {
    rollback_root(paths).join(id)
}

fn rollback_ready_path(paths: &LauncherPaths, id: &str) -> PathBuf {
    rollback_root(paths).join(format!("{id}.ready"))
}

async fn rename_with_retry(source: &Path, destination: &Path) -> Result<(), anyhow::Error> {
    let mut last_error = None;
    for attempt in 0..20 {
        match tokio::fs::rename(source, destination).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < 19 {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| std::io::Error::other("rename did not run"))
        .into())
}

/// Restore the previous instance if an update was interrupted before its
/// completion marker was written. Safe to call on every startup.
pub async fn recover_interrupted_update(
    paths: &LauncherPaths,
    id: &str,
) -> Result<bool, anyhow::Error> {
    let rollback = rollback_dir(paths, id);
    let ready = rollback_ready_path(paths, id);
    if !exists(&rollback).await {
        remove_if_exists(&ready).await?;
        return Ok(false);
    }
    let instance = paths.instance_dir(id)?;
    if exists(&ready).await && exists(&instance).await {
        // The new instance was fully committed; only cleanup was interrupted.
        remove_if_exists(&rollback).await?;
        remove_if_exists(&ready).await?;
        return Ok(true);
    }
    // Anything else means the previous instance must come back.
    remove_if_exists(&instance).await?;
    if let Some(parent) = instance.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    rename_with_retry(&rollback, &instance).await.with_context(|| {
        format!(
            "Could not restore the previous instance from {}",
            rollback.display()
        )
    })?;
    let _ = remove_if_exists(&ready).await;
    Ok(true)
}

/// Recover every interrupted update before instance state is loaded.
pub async fn recover_interrupted_maintenance(
    paths: &LauncherPaths,
) -> Result<Vec<String>, anyhow::Error> {
    crate::mod_toggles::recover_interrupted_disables_all(paths).await?;
    let root = rollback_root(paths);
    if !exists(&root).await {
        return Ok(Vec::new());
    }
    let mut recovered = Vec::new();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix(".ready") {
            if !exists(&rollback_dir(paths, id)).await {
                let _ = remove_if_exists(&entry.path()).await;
            }
            continue;
        }
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let Ok(id) = validate_instance_id(&name) else {
            continue;
        };
        if id != name {
            continue;
        }
        if recover_interrupted_update(paths, &id).await? {
            recovered.push(id);
        }
    }
    Ok(recovered)
}

struct InstanceRollback {
    instance: PathBuf,
    rollback: PathBuf,
    ready: PathBuf,
    committed: bool,
}

impl InstanceRollback {
    async fn begin(paths: &LauncherPaths, id: &str) -> Result<Self, anyhow::Error> {
        let instance = paths.instance_dir(id)?;
        anyhow::ensure!(exists(&instance).await, "instance not found");
        let root = rollback_root(paths);
        tokio::fs::create_dir_all(&root).await?;
        let rollback = root.join(id);
        let ready = rollback_ready_path(paths, id);
        anyhow::ensure!(
            !exists(&rollback).await,
            "Recovery data from a previous update exists: {}. Restart the launcher to recover it.",
            rollback.display()
        );
        remove_if_exists(&ready).await?;
        rename_with_retry(&instance, &rollback)
            .await
            .context("Could not move the current instance into the update rollback area; close any apps using it and retry")?;
        if let Err(error) = tokio::fs::create_dir_all(&instance).await {
            let _ = rename_with_retry(&rollback, &instance).await;
            return Err(error.into());
        }
        Ok(Self {
            instance,
            rollback,
            ready,
            committed: false,
        })
    }

    fn previous(&self) -> &Path {
        &self.rollback
    }

    async fn commit(mut self) -> Result<(), anyhow::Error> {
        write_json(&self.ready, &serde_json::json!({"format": 1})).await?;
        self.committed = true;
        // The marker makes cleanup resumable; a locked rollback directory is
        // not a reason to fail a completed update.
        if remove_if_exists(&self.rollback).await.is_ok() {
            let _ = remove_if_exists(&self.ready).await;
        }
        Ok(())
    }

    async fn restore(mut self) -> Result<(), anyhow::Error> {
        self.committed = true;
        remove_if_exists(&self.instance).await?;
        rename_with_retry(&self.rollback, &self.instance)
            .await
            .context("Could not restore the previous instance")?;
        let _ = remove_if_exists(&self.ready).await;
        Ok(())
    }
}

impl Drop for InstanceRollback {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.instance);
            let _ = std::fs::rename(&self.rollback, &self.instance);
            let _ = std::fs::remove_file(&self.ready);
        }
    }
}

/// Mirrors `downloadInstall`.
pub async fn download_install(
    paths: &LauncherPaths,
    raw_id: &str,
    pack_version: &str,
    java_type: &str,
    name: Option<&str>,
    group: Option<&str>,
    emit: crate::pack_files::EmitProgress,
) -> Result<(), anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    let instance = paths.instance_dir(&id)?;
    let known = known_instance_ids(paths).await?;
    if known.contains(&id) {
        anyhow::bail!("an instance with that id already exists");
    }
    tokio::fs::create_dir_all(paths.instances_dir()).await?;
    tokio::fs::create_dir(&instance).await?;
    let ctx = progress_ctx(emit, "install", &id);
    let result = async {
        let staging = download_and_extract_to_staging(
            paths,
            &ctx,
            pack_version,
            java_type,
            &instance,
            "install",
        )
        .await?;
        install_staging_contents(&staging, &instance).await?;
        remove_if_exists(&staging).await?;
        flatten_nested_pack(&instance).await?;
        prepare_instance_configs(&instance, true).await?;
        let custom_icon = install_default_instance_icon(&instance).await?;
        let mut settings = InstanceSettings::default();
        let trimmed_name = name.unwrap_or_default().trim();
        settings.name = if trimmed_name.is_empty() {
            format!("GTNH {pack_version}")
        } else {
            trimmed_name.to_string()
        };
        settings.pack_version = pack_version.to_string();
        settings.pack_java_type = java_type.to_string();
        settings.custom_icon = Some(custom_icon);
        save_and_refresh_size(paths, &id, &mut settings).await?;
        if let Some(group) = group.filter(|g| !g.is_empty()) {
            set_instance_group(paths, &id, group, &known_instance_ids(paths).await?).await?;
        }
        ctx.progress("done", 1.0, |_| {});
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        remove_if_exists(&instance).await?;
    }
    result
}

/// Mirrors `previewUpdate` (cleans the `.update-preview` dir afterwards).
pub async fn preview_update(
    paths: &LauncherPaths,
    raw_id: &str,
    pack_version: &str,
    java_type: Option<&str>,
    emit: crate::pack_files::EmitProgress,
) -> Result<UpdateModPreview, anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    recover_interrupted_update(paths, &id).await?;
    crate::mod_toggles::recover_interrupted_disables(paths, &id).await?;
    if is_already_running(paths, &id).await? {
        anyhow::bail!("cannot analyze an update while instance is running");
    }
    let known = known_instance_ids(paths).await?;
    if !known.contains(&id) {
        anyhow::bail!("instance not found");
    }
    let instance = paths.instance_dir(&id)?;
    let settings = load_instance_settings(paths, &id).await?;
    let java_type = java_type.unwrap_or(&settings.pack_java_type).to_string();
    let ctx = progress_ctx(emit, "preview", &id);
    ctx.progress("preview", 0.0, |p| {
        p.log_line = Some(format!(
            "Preparing mod analysis: {} → {pack_version}",
            if settings.pack_version.is_empty() {
                id.clone()
            } else {
                settings.pack_version.clone()
            }
        ));
    });
    let preview_dir = instance.join(".update-preview");
    remove_if_exists(&preview_dir).await?;
    let result = build_update_preview(paths, &ctx, &instance, pack_version, &java_type).await;
    remove_if_exists(&preview_dir).await?;
    result
}

/// Mirrors `updateInstance`.
pub async fn update_instance(
    paths: &LauncherPaths,
    raw_id: &str,
    pack_version: &str,
    java_type: Option<&str>,
    keep_mod_identities: &[String],
    emit: crate::pack_files::EmitProgress,
) -> Result<(), anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    recover_interrupted_update(paths, &id).await?;
    crate::mod_toggles::recover_interrupted_disables(paths, &id).await?;
    if is_already_running(paths, &id).await? {
        anyhow::bail!("cannot update while instance is running");
    }
    crate::minecraft_files::stop_instance_watches(paths, &id).await?;
    let settings = load_instance_settings(paths, &id).await?;
    let java_type = java_type.unwrap_or(&settings.pack_java_type).to_string();
    let keep: HashSet<String> = keep_mod_identities.iter().cloned().collect();
    reinstall_core(
        paths,
        &id,
        pack_version,
        &java_type,
        Some(&keep),
        "update-pack",
        emit,
    )
    .await
}

/// Mirrors `reinstallInstance`.
pub async fn reinstall_instance(
    paths: &LauncherPaths,
    raw_id: &str,
    pack_version: &str,
    java_type: Option<&str>,
    emit: crate::pack_files::EmitProgress,
) -> Result<(), anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    recover_interrupted_update(paths, &id).await?;
    crate::mod_toggles::recover_interrupted_disables(paths, &id).await?;
    if is_already_running(paths, &id).await? {
        anyhow::bail!("cannot reinstall while instance is running");
    }
    crate::minecraft_files::stop_instance_watches(paths, &id).await?;
    let settings = load_instance_settings(paths, &id).await?;
    let java_type = java_type.unwrap_or(&settings.pack_java_type).to_string();
    reinstall_core(
        paths,
        &id,
        pack_version,
        &java_type,
        None,
        "reinstall",
        emit,
    )
    .await
}

async fn reinstall_core(
    paths: &LauncherPaths,
    id: &str,
    pack_version: &str,
    java_type: &str,
    keep_mods: Option<&HashSet<String>>,
    operation: &str,
    emit: crate::pack_files::EmitProgress,
) -> Result<(), anyhow::Error> {
    let known = known_instance_ids(paths).await?;
    if !known.contains(id) {
        anyhow::bail!("instance not found");
    }
    let instance = paths.instance_dir(id)?;
    let preserve = instance.join(PRESERVE_DIR_NAME);
    if tokio::fs::try_exists(&preserve).await? {
        anyhow::bail!(
            "Recovery data from a previous reinstall exists: {}",
            preserve.display()
        );
    }
    let ctx = progress_ctx(emit, operation, id);
    // Acquire a complete target pack before modifying the installed instance.
    // Staging lives outside the instance so the rollback swap cannot remove it.
    let staging_parent = paths.base().join("maintenance-staging").join(id);
    tokio::fs::create_dir_all(&staging_parent).await?;
    let staging = download_and_extract_to_staging(
        paths,
        &ctx,
        pack_version,
        java_type,
        &staging_parent,
        operation,
    )
    .await?;
    let result = apply_reinstall_staging(
        paths,
        id,
        pack_version,
        java_type,
        keep_mods,
        operation,
        &ctx,
        &staging,
    )
    .await;
    if result.is_ok() {
        remove_if_exists(&staging_parent).await?;
    }
    result
}

async fn apply_reinstall_staging(
    paths: &LauncherPaths,
    id: &str,
    pack_version: &str,
    java_type: &str,
    keep_mods: Option<&HashSet<String>>,
    operation: &str,
    ctx: &ProgressCtx,
    staging: &Path,
) -> anyhow::Result<()> {
    let instance = paths.instance_dir(id)?;
    let preserve = instance.join(PRESERVE_DIR_NAME);
    // Validate before touching the installed instance.
    let pack: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(staging.join("mmc-pack.json")).await?)?;
    if !pack.is_object() {
        anyhow::bail!("Target pack manifest is invalid");
    }
    if exists(&preserve).await {
        anyhow::bail!(
            "Recovery data from a previous reinstall exists. Restore it before retrying: {}",
            preserve.display()
        );
    }
    recover_interrupted_update(paths, id).await?;
    let previous_settings = load_instance_settings(paths, id).await?;
    let previous_pack =
        if operation == "update-pack" && !previous_settings.pack_version.is_empty() {
            find_cached_pack(paths, &previous_settings.pack_version, java_type).await
        } else {
            None
        };
    let disabled = disabled_identities(&previous_settings.disabled_mods);
    let stage = if operation == "update-pack" {
        "updating"
    } else {
        "reinstalling"
    };
    // The old instance becomes the rollback copy. Nothing is deleted until the
    // new instance has been installed, restored, and saved.
    ctx.progress(stage, 0.02, |p| {
        p.log_line = Some("Preparing a rollback copy of the previous instance…".to_string())
    });
    let rollback = InstanceRollback::begin(paths, id).await?;
    ctx.progress(stage, 0.05, |p| {
        p.log_line = Some("Rollback copy ready; starting the pack swap".to_string())
    });
    let result = async {
        ctx.progress(stage, 0.10, |p| {
            p.log_line = Some("Copying fresh pack files into the instance…".to_string())
        });
        install_staging_contents(staging, &instance).await?;
        // Staging cleanup is best effort; a locked file must not undo a
        // successfully installed instance.
        let _ = remove_if_exists(staging).await;
        ctx.progress(stage, 0.55, |p| {
            p.log_line = Some("Fresh pack files installed".to_string())
        });
        flatten_nested_pack(&instance).await?;
        prepare_instance_configs(&instance, true).await?;
        ctx.progress(stage, 0.62, |p| {
            p.log_line = Some("Pack layout normalized and configs seeded".to_string())
        });
        ctx.progress(stage, 0.68, |p| {
            p.log_line = Some("Restoring saves, options, and launcher data…".to_string())
        });
        restore_player_data_from_instance(&instance, rollback.previous()).await?;
        ctx.progress(stage, 0.74, |p| {
            p.log_line = Some("Player data restored".to_string())
        });
        if operation == "update-pack" {
            ctx.progress(stage, 0.78, |p| {
                p.log_line = Some("Merging modified configs and scripts…".to_string())
            });
            let restored =
                restore_modified_configs(&instance, rollback.previous(), previous_pack.as_deref())
                    .await?;
            ctx.progress(stage, 0.82, |p| {
                p.log_line = Some(if previous_pack.is_some() {
                    format!("Restored {restored} modified config/script file(s)")
                } else {
                    format!(
                        "Restored {restored} config/script file(s); the previous pack was not cached"
                    )
                })
            });
        }
        if let Some(keep) = keep_mods {
            ctx.progress(stage, 0.86, |p| {
                p.log_line = Some("Applying custom mod selections…".to_string())
            });
            let removed =
                remove_custom_mods_except(&persistent_custom_mods_dir(&instance), keep).await?;
            ctx.progress(stage, 0.88, |p| {
                p.log_line = Some(if removed > 0 {
                    format!("Removed {removed} custom mod(s) not selected to keep")
                } else {
                    "All custom mods kept".to_string()
                })
            });
        }
        ctx.progress(stage, 0.90, |p| {
            p.log_line = Some("Applying persistent launcher overlays…".to_string())
        });
        apply_persistent_custom_mods_excluding(&instance, &disabled).await?;
        apply_persistent_minecraft(&instance).await?;
        ctx.progress(stage, 0.92, |p| {
            p.log_line = Some("Applying disabled-mod state…".to_string())
        });
        let disabled_count =
            apply_disabled_mods(&instance, &previous_settings.disabled_mods).await?;
        ctx.progress(stage, 0.94, |p| {
            p.log_line = Some(if disabled_count > 0 {
                format!("Kept {disabled_count} mod(s) disabled")
            } else {
                "Launcher overlays applied".to_string()
            })
        });
        ctx.progress(stage, 0.97, |p| {
            p.log_line = Some("Saving instance settings and refreshing size…".to_string())
        });
        let mut settings = load_instance_settings(paths, id).await?;
        settings.pack_version = pack_version.to_string();
        settings.pack_java_type = java_type.to_string();
        save_and_refresh_size(paths, id, &mut settings).await?;
        ctx.progress(stage, 0.99, |p| {
            p.log_line = Some("Settings saved and size refreshed".to_string())
        });
        Ok::<(), anyhow::Error>(())
    }
    .await;
    match result {
        Ok(()) => {
            rollback.commit().await?;
            let done_line = if operation == "update-pack" {
                "Update complete"
            } else {
                "Clean reinstall complete"
            };
            ctx.progress("done", 1.0, |p| p.log_line = Some(done_line.to_string()));
            Ok(())
        }
        Err(error) => match rollback.restore().await {
            Ok(()) => Err(error),
            Err(restore_error) => Err(error.context(format!(
                "The previous instance could not be restored automatically: {restore_error:#}"
            ))),
        },
    }
}

/// Convenience for UIs/CLIs: current instances with refreshed sizes.
pub async fn list_instances_with_sizes(
    paths: &LauncherPaths,
) -> Result<Vec<crate::instances::InstanceInfo>, anyhow::Error> {
    refresh_sizes(paths, None).await?;
    load_instances(paths).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn size_refresh_preserves_metadata_saved_while_its_write_is_queued() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-size-race-{}", uuid::Uuid::new_v4())),
        );
        let instance = paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(instance.join(".minecraft")).unwrap();
        std::fs::write(instance.join("mmc-pack.json"), "{}").unwrap();
        let settings_path = paths.settings_path("pack").unwrap();
        std::fs::write(&settings_path, r#"{"name":"Before","extension":true}"#).unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let held = tokio::spawn(crate::profile_store::mutate(async move {
            started.send(()).unwrap();
            wait.await.unwrap();
            Ok(())
        }));
        ready.await.unwrap();
        let target = paths.clone();
        let refreshing =
            tokio::spawn(async move { refresh_sizes(&target, Some(&["pack".into()])).await });
        // Keep the writer queued while a legitimate preference save completes.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!refreshing.is_finished());
        std::fs::write(
            &settings_path,
            r#"{"name":"Newest","jvm_args":"-Dlatest=true","extension":true}"#,
        )
        .unwrap();
        release.send(()).unwrap();
        held.await.unwrap().unwrap();
        let sizes = refreshing.await.unwrap().unwrap();
        let saved = load_instance_settings(&paths, "pack").await.unwrap();
        assert_eq!(saved.name, "Newest");
        assert_eq!(saved.jvm_args, "-Dlatest=true");
        assert_eq!(saved.extra_fields["extension"], true);
        assert_eq!(saved.cached_size_bytes, sizes["pack"]);
        assert!(saved.cached_size_bytes > 0);
        let lease = crate::instance_guard::reserve_instances(&paths, &["pack"]).unwrap();
        let before = std::fs::read(&settings_path).unwrap();
        assert!(refresh_sizes(&paths, Some(&["pack".into()])).await.is_err());
        assert_eq!(std::fs::read(&settings_path).unwrap(), before);
        drop(lease);
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    use crate::groups::get_instance_group;
    use crate::icons::{install_default_instance_icon, resolve_icon_path};
    use crate::instances::known_instance_ids;
    use crate::pack_files::install_staging_contents;
    use crate::pack_prep::prepare_instance_configs;
    use std::collections::HashSet;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-install-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn editor_saved_forge_values_survive_updates_and_clean_reinstalls() {
        let paths = temp_paths("editor-overlay");
        let instance = paths.instance_dir("gtnh").unwrap();
        // Deliberately outside migration's specially preserved Pollution.cfg path:
        // this proves the editor overlay, rather than the legacy allowlist, wins.
        let rel = "config/example/Pollution.cfg";
        let game_file = instance.join(".minecraft").join(rel);
        tokio::fs::create_dir_all(game_file.parent().unwrap())
            .await
            .unwrap();
        let original = "# Keep this comment\ngeneral {\n B:enabled=true\n I:amount=80\n}\n";
        tokio::fs::write(&game_file, original).await.unwrap();
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        save_instance_settings(&paths, "gtnh", &InstanceSettings::default())
            .await
            .unwrap();
        // Same lossless value edit and persistent save used by both native editors.
        let changed = industrialis_core::forge_config::update(original, 0, "false").unwrap();
        crate::minecraft_files::write_file(
            &paths,
            "gtnh",
            rel,
            changed.clone(),
            true,
            Some(original.into()),
        )
        .await
        .unwrap();
        for (operation, version) in [("update-pack", "2.9.0"), ("reinstall", "2.9.1")] {
            let staging = paths.base().join(format!("staging-{operation}"));
            let fresh = staging.join(".minecraft").join(rel);
            tokio::fs::create_dir_all(fresh.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(staging.join("mmc-pack.json"), "{}")
                .await
                .unwrap();
            tokio::fs::write(fresh, "general {\n B:enabled=true\n I:amount=120\n}\n")
                .await
                .unwrap();
            let ctx = progress_ctx(std::sync::Arc::new(|_| {}), operation, "gtnh");
            apply_reinstall_staging(
                &paths, "gtnh", version, "java17+", None, operation, &ctx, &staging,
            )
            .await
            .unwrap();
            assert_eq!(
                tokio::fs::read_to_string(&game_file).await.unwrap(),
                changed
            );
            assert_eq!(
                tokio::fs::read_to_string(instance.join("persistent-minecraft").join(rel))
                    .await
                    .unwrap(),
                changed
            );
            assert_eq!(
                load_instance_settings(&paths, "gtnh")
                    .await
                    .unwrap()
                    .pack_version,
                version
            );
        }
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn staged_update_preserves_player_data_settings_and_selected_mods() {
        let paths = temp_paths("update-staged");
        let instance = paths.instance_dir("gtnh").unwrap();
        let staging = paths.base().join("target-pack");
        for path in [
            instance.join(".minecraft/saves/world"),
            instance.join("persistent-minecraft/mods"),
            staging.join(".minecraft/mods"),
        ] {
            tokio::fs::create_dir_all(path).await.unwrap();
        }
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(instance.join(".minecraft/saves/world/level.dat"), "save")
            .await
            .unwrap();
        tokio::fs::write(instance.join(".minecraft/options.txt"), "options")
            .await
            .unwrap();
        tokio::fs::write(
            instance.join("persistent-minecraft/mods/keep-1.0.jar"),
            "keep",
        )
        .await
        .unwrap();
        tokio::fs::write(
            instance.join("persistent-minecraft/mods/drop-1.0.jar"),
            "drop",
        )
        .await
        .unwrap();
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(staging.join(".minecraft/mods/new-2.0.jar"), "new")
            .await
            .unwrap();
        let mut settings = InstanceSettings::default();
        settings.name = "Configured".into();
        settings.override_env = true;
        settings.env_vars.insert("TEST".into(), "value".into());
        settings.total_play_seconds = 100;
        save_instance_settings(&paths, "gtnh", &settings)
            .await
            .unwrap();
        let resource_bytes = crate::content_archive::fixture(&[
            ("pack.mcmeta", br#"{"pack":{"pack_format":1}}"#),
            ("LICENSE.txt", b"third-party credits"),
        ]);
        crate::content::install_bytes(
            &paths,
            "gtnh",
            crate::content::ContentKind::ResourcePack,
            "textures.zip".into(),
            resource_bytes,
            Some(crate::content::SourceRecord {
                project: "fixture".into(),
                release: "fixture-release".into(),
                url: "fixture".into(),
                archive_sha256: "fixture".into(),
                pack_series: None,
            }),
        )
        .await
        .unwrap();
        let resource_installed =
            std::fs::read(instance.join(".minecraft/resourcepacks/textures.zip")).unwrap();
        let ctx = progress_ctx(std::sync::Arc::new(|_| {}), "update-pack", "gtnh");
        apply_reinstall_staging(
            &paths,
            "gtnh",
            "2.9.0",
            "java17+",
            Some(&HashSet::from(["keep".into()])),
            "update-pack",
            &ctx,
            &staging,
        )
        .await
        .unwrap();
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/saves/world/level.dat"))
                .await
                .unwrap(),
            "save"
        );
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/options.txt"))
                .await
                .unwrap(),
            "options"
        );
        assert!(instance.join(".minecraft/mods/new-2.0.jar").exists());
        assert!(instance.join(".minecraft/mods/keep-1.0.jar").exists());
        assert_eq!(
            std::fs::read(instance.join(".minecraft/resourcepacks/textures.zip")).unwrap(),
            resource_installed
        );
        let resources =
            crate::content::list(&paths, "gtnh", crate::content::ContentKind::ResourcePack)
                .await
                .unwrap();
        assert!(resources[0].managed);
        assert_eq!(
            resources[0].source.as_ref().unwrap().release,
            "fixture-release"
        );
        assert!(!instance
            .join("persistent-minecraft/mods/drop-1.0.jar")
            .exists());
        let saved = load_instance_settings(&paths, "gtnh").await.unwrap();
        assert_eq!(saved.pack_version, "2.9.0");
        assert_eq!(saved.name, "Configured");
        assert_eq!(saved.total_play_seconds, 100);
        assert_eq!(saved.env_vars["TEST"], "value");
        assert!(!instance.join(PRESERVE_DIR_NAME).exists());
    }

    #[tokio::test]
    async fn updates_and_reinstalls_keep_disabled_mods_disabled() {
        use industrialis_core::instance_settings::DisabledMod;

        let paths = temp_paths("disabled-mods");
        let instance = paths.instance_dir("gtnh").unwrap();
        tokio::fs::create_dir_all(instance.join(".minecraft/mods"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(instance.join("persistent-minecraft/mods"))
            .await
            .unwrap();
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(
            instance.join(".minecraft/mods/DefaultServerList-1.0.jar"),
            "old",
        )
        .await
        .unwrap();
        tokio::fs::write(
            instance.join("persistent-minecraft/mods/DefaultServerList-1.0.jar"),
            "old custom",
        )
        .await
        .unwrap();
        let disabled = DisabledMod {
            identity: "defaultserverlist".into(),
            filename: "DefaultServerList-1.0.jar".into(),
        };
        let mut settings = InstanceSettings::default();
        settings.pack_version = "2.8.0".into();
        settings.disabled_mods = vec![disabled.clone()];
        save_instance_settings(&paths, "gtnh", &settings)
            .await
            .unwrap();

        for operation in ["update-pack", "reinstall"] {
            let staging = paths.base().join(format!("target-pack-{operation}"));
            tokio::fs::create_dir_all(staging.join(".minecraft/mods"))
                .await
                .unwrap();
            tokio::fs::write(staging.join("mmc-pack.json"), "{}")
                .await
                .unwrap();
            tokio::fs::write(
                staging.join(".minecraft/mods/DefaultServerList-2.0.jar"),
                "new",
            )
            .await
            .unwrap();
            tokio::fs::write(staging.join(".minecraft/mods/keep-me.jar"), "keep")
                .await
                .unwrap();
            let ctx = progress_ctx(std::sync::Arc::new(|_| {}), operation, "gtnh");
            apply_reinstall_staging(
                &paths, "gtnh", "2.9.0", "java17+", None, operation, &ctx, &staging,
            )
            .await
            .unwrap();
            assert!(instance
                .join(".minecraft/mods/DefaultServerList-2.0.jar.disabled")
                .exists());
            assert!(!instance
                .join(".minecraft/mods/DefaultServerList-2.0.jar")
                .exists());
            assert!(instance
                .join("persistent-minecraft/mods/DefaultServerList-1.0.jar.disabled")
                .exists());
            assert!(instance.join(".minecraft/mods/keep-me.jar").exists());
            assert_eq!(
                load_instance_settings(&paths, "gtnh")
                    .await
                    .unwrap()
                    .disabled_mods,
                vec![disabled.clone()]
            );
        }
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn invalid_staging_or_existing_recovery_data_never_wipes_instance() {
        let paths = temp_paths("update-rejected");
        let instance = paths.instance_dir("gtnh").unwrap();
        let staging = paths.base().join("bad-pack");
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(instance.join("sentinel.txt"), "original")
            .await
            .unwrap();
        let ctx = progress_ctx(std::sync::Arc::new(|_| {}), "reinstall", "gtnh");
        assert!(apply_reinstall_staging(
            &paths,
            "gtnh",
            "target",
            "java8",
            None,
            "reinstall",
            &ctx,
            &staging
        )
        .await
        .is_err());
        assert_eq!(
            tokio::fs::read_to_string(instance.join("sentinel.txt"))
                .await
                .unwrap(),
            "original"
        );
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        let preserve = instance.join(PRESERVE_DIR_NAME);
        tokio::fs::create_dir_all(&preserve).await.unwrap();
        tokio::fs::write(preserve.join("recovery.txt"), "previous backup")
            .await
            .unwrap();
        assert!(apply_reinstall_staging(
            &paths,
            "gtnh",
            "target",
            "java8",
            None,
            "reinstall",
            &ctx,
            &staging
        )
        .await
        .is_err());
        assert_eq!(
            tokio::fs::read_to_string(instance.join("sentinel.txt"))
                .await
                .unwrap(),
            "original"
        );
        assert_eq!(
            tokio::fs::read_to_string(preserve.join("recovery.txt"))
                .await
                .unwrap(),
            "previous backup"
        );
    }

    /// Offline tail of `download_install`: staging → instance → configs →
    /// icon → settings → group → sizes (everything after the download).
    #[tokio::test]
    async fn install_file_flow_without_network() {
        let paths = temp_paths("flow");
        let instance = paths.instance_dir("gtnh").unwrap();
        let staging = paths.base().join("staging");
        tokio::fs::create_dir_all(staging.join("config"))
            .await
            .unwrap();
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(staging.join("config").join("pack.cfg"), "pack")
            .await
            .unwrap();

        install_staging_contents(&staging, &instance).await.unwrap();
        flatten_nested_pack(&instance).await.unwrap();
        prepare_instance_configs(&instance, true).await.unwrap();
        let icon = install_default_instance_icon(&instance).await.unwrap();

        let mut settings = InstanceSettings::default();
        settings.name = "GTNH 2.8.0".to_string();
        settings.pack_version = "2.8.0".to_string();
        settings.pack_java_type = "java17+".to_string();
        settings.custom_icon = Some(icon);
        save_and_refresh_size(&paths, "gtnh", &mut settings)
            .await
            .unwrap();
        assert!(settings.cached_size_bytes > 0);

        let known = known_instance_ids(&paths).await.unwrap();
        assert!(known.contains("gtnh"));
        set_instance_group(&paths, "gtnh", "favorites", &known)
            .await
            .unwrap();

        let list = load_instances(&paths).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].settings.name, "GTNH 2.8.0");
        assert_eq!(list[0].group, "favorites");
        assert!(list[0].size_bytes > 0);
        let icon_path = resolve_icon_path(&instance, list[0].settings.custom_icon.as_deref())
            .await
            .unwrap();
        assert!(icon_path.exists());
        assert!(instance
            .join(".minecraft")
            .join("config")
            .join("pack.cfg")
            .exists());

        // Refresh persists what it reports: a reloaded listing agrees with it.
        // (The earlier snapshot legitimately excludes instance.json itself —
        // size is measured before the first save, exactly like the TS
        // `saveAndRefreshSize`.)
        let sizes = refresh_sizes(&paths, None).await.unwrap();
        let relist = load_instances(&paths).await.unwrap();
        assert_eq!(sizes.get("gtnh"), Some(&relist[0].size_bytes));
        assert!(relist[0].size_bytes >= list[0].size_bytes);

        // Unknown ids are rejected like the TS `validateInstanceId` path.
        assert!(paths.instance_dir("").is_err());
        let empty: HashSet<String> = HashSet::new();
        assert_eq!(
            get_instance_group(&paths, "gtnh", &empty).await.unwrap(),
            ""
        );
    }

    #[tokio::test]
    async fn interrupted_update_restores_the_previous_instance() {
        let paths = temp_paths("recover-interrupted");
        let instance = paths.instance_dir("gtnh").unwrap();
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::write(instance.join("sentinel.txt"), "original")
            .await
            .unwrap();
        let rollback_root = paths.base().join("maintenance-rollback");
        tokio::fs::create_dir_all(&rollback_root).await.unwrap();
        tokio::fs::rename(&instance, rollback_root.join("gtnh"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::write(instance.join("partial.txt"), "partial")
            .await
            .unwrap();

        let recovered = recover_interrupted_maintenance(&paths).await.unwrap();
        assert_eq!(recovered, vec!["gtnh".to_string()]);
        assert_eq!(
            tokio::fs::read_to_string(instance.join("sentinel.txt"))
                .await
                .unwrap(),
            "original"
        );
        assert!(!instance.join("partial.txt").exists());
        assert!(!rollback_root.join("gtnh").exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn committed_update_cleanup_preserves_the_new_instance() {
        let paths = temp_paths("recover-committed");
        let instance = paths.instance_dir("gtnh").unwrap();
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::write(instance.join("old.txt"), "old")
            .await
            .unwrap();
        let rollback_root = paths.base().join("maintenance-rollback");
        tokio::fs::create_dir_all(&rollback_root).await.unwrap();
        tokio::fs::rename(&instance, rollback_root.join("gtnh"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::write(instance.join("new.txt"), "new").await.unwrap();
        crate::fs_utils::write_json(
            &rollback_root.join("gtnh.ready"),
            &serde_json::json!({"format": 1}),
        )
        .await
        .unwrap();

        let recovered = recover_interrupted_maintenance(&paths).await.unwrap();
        assert_eq!(recovered, vec!["gtnh".to_string()]);
        assert_eq!(
            tokio::fs::read_to_string(instance.join("new.txt"))
                .await
                .unwrap(),
            "new"
        );
        assert!(!instance.join("old.txt").exists());
        assert!(!rollback_root.join("gtnh").exists());
        assert!(!rollback_root.join("gtnh.ready").exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn update_preserves_modified_configs_and_uses_new_defaults_elsewhere() {
        let paths = temp_paths("update-configs");
        let instance = paths.instance_dir("gtnh").unwrap();
        let old_pack = paths.base().join("old-pack");
        let staging = paths.base().join("new-pack");
        for dir in [
            old_pack.join("config"),
            staging.join("config"),
            instance.join(".minecraft/config"),
        ] {
            tokio::fs::create_dir_all(dir).await.unwrap();
        }
        tokio::fs::write(old_pack.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(old_pack.join("config/edited.cfg"), "old default")
            .await
            .unwrap();
        tokio::fs::write(old_pack.join("config/untouched.cfg"), "old default")
            .await
            .unwrap();
        tokio::fs::write(staging.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(staging.join("config/edited.cfg"), "new default")
            .await
            .unwrap();
        tokio::fs::write(staging.join("config/untouched.cfg"), "new default")
            .await
            .unwrap();
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(instance.join(".minecraft/config/edited.cfg"), "user value")
            .await
            .unwrap();
        tokio::fs::write(
            instance.join(".minecraft/config/untouched.cfg"),
            "old default",
        )
        .await
        .unwrap();
        tokio::fs::write(instance.join(".minecraft/config/created.cfg"), "user created")
            .await
            .unwrap();
        let mut settings = InstanceSettings::default();
        settings.pack_version = "2.8.0".into();
        settings.pack_java_type = "java17+".into();
        save_instance_settings(&paths, "gtnh", &settings)
            .await
            .unwrap();
        crate::pack_cache::store_pack_cache(
            &paths,
            "2.8.0",
            "java17+",
            "https://example.test/old.zip",
            &old_pack,
        )
        .await
        .unwrap();

        let ctx = progress_ctx(std::sync::Arc::new(|_| {}), "update-pack", "gtnh");
        apply_reinstall_staging(
            &paths,
            "gtnh",
            "2.9.0",
            "java17+",
            None,
            "update-pack",
            &ctx,
            &staging,
        )
        .await
        .unwrap();

        let config = instance.join(".minecraft/config");
        assert_eq!(
            tokio::fs::read_to_string(config.join("edited.cfg"))
                .await
                .unwrap(),
            "user value"
        );
        assert_eq!(
            tokio::fs::read_to_string(config.join("untouched.cfg"))
                .await
                .unwrap(),
            "new default"
        );
        assert_eq!(
            tokio::fs::read_to_string(config.join("created.cfg"))
                .await
                .unwrap(),
            "user created"
        );
        assert_eq!(
            load_instance_settings(&paths, "gtnh")
                .await
                .unwrap()
                .pack_version,
            "2.9.0"
        );
        assert!(!paths
            .base()
            .join("maintenance-rollback")
            .join("gtnh")
            .exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
