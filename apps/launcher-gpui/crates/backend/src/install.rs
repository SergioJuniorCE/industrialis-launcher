//! Instance install / update / reinstall orchestration. Ports
//! `downloadInstall`, `previewUpdate`, `updateInstance`, `reinstallInstance`,
//! and `refreshSizes` from `electron/backend/instance-lifecycle.ts`.
//!
//! Mutating operations reserve instance paths and check the running pid file.
//! Reinstalls acquire the target pack before touching installed player data.

use std::collections::{HashMap, HashSet};

use industrialis_core::instance_settings::InstanceSettings;

use crate::fs_utils::{dir_size, remove_if_exists};
use crate::groups::set_instance_group;
use crate::icons::install_default_instance_icon;
use crate::instances::{known_instance_ids, load_instances};
use crate::launch::is_already_running;
use crate::migration::{
    backup_player_data, restore_player_data, wipe_instance_for_reinstall, PRESERVE_DIR_NAME,
};
use crate::pack_files::{
    build_update_preview, download_and_extract_to_staging, install_staging_contents,
    remove_custom_mods_except, ProgressCtx, UpdateModPreview,
};
use crate::pack_prep::{
    apply_persistent_custom_mods, apply_persistent_minecraft, flatten_nested_pack,
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
    // Staging lives outside the instance so wiping cannot remove it.
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
    staging: &std::path::Path,
) -> anyhow::Result<()> {
    let instance = paths.instance_dir(id)?;
    let preserve = instance.join(PRESERVE_DIR_NAME);
    // Validate before creating a backup or wiping the installed instance.
    let pack: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(staging.join("mmc-pack.json")).await?)?;
    if !pack.is_object() {
        anyhow::bail!("Target pack manifest is invalid");
    }
    if tokio::fs::try_exists(&preserve).await? {
        anyhow::bail!(
            "Recovery data from a previous reinstall exists. Restore it before retrying: {}",
            preserve.display()
        );
    }
    backup_player_data(&instance, &preserve).await?;
    if let Some(keep) = keep_mods {
        let removed =
            remove_custom_mods_except(&persistent_custom_mods_dir(&preserve), keep).await?;
        if removed > 0 {
            ctx.progress("updating", 0.05, |p| {
                p.log_line = Some(format!(
                    "Removed {removed} custom mod(s) not selected to keep"
                ))
            });
        }
    }
    let stage = if operation == "update-pack" {
        "updating"
    } else {
        "reinstalling"
    };
    ctx.progress(stage, 0.05, |p| {
        p.log_line = Some("Backing up saves, JourneyMap, and player settings".to_string())
    });
    wipe_instance_for_reinstall(&instance, &preserve).await?;
    tokio::fs::create_dir_all(&instance).await?;
    ctx.progress(stage, 0.75, |p| {
        p.log_line = Some("Installing fresh pack files".to_string())
    });
    install_staging_contents(&staging, &instance).await?;
    remove_if_exists(&staging).await?;
    flatten_nested_pack(&instance).await?;
    prepare_instance_configs(&instance, true).await?;
    restore_player_data(&instance, &preserve).await?;
    let mut settings = load_instance_settings(paths, id).await?;
    settings.pack_version = pack_version.to_string();
    settings.pack_java_type = java_type.to_string();
    apply_persistent_custom_mods(&instance).await?;
    apply_persistent_minecraft(&instance).await?;
    save_and_refresh_size(paths, id, &mut settings).await?;
    remove_if_exists(&preserve).await?;
    let done_line = if operation == "update-pack" {
        "Update complete"
    } else {
        "Clean reinstall complete"
    };
    ctx.progress("done", 1.0, |p| p.log_line = Some(done_line.to_string()));
    Ok(())
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
}
