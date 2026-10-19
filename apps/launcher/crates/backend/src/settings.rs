//! Port of `apps/launcher/electron/backend/settings.ts`.

use industrialis_core::instance_settings::{parse_instance_settings_json, InstanceSettings};
use industrialis_core::launcher_settings::{parse_launcher_settings_json, LauncherSettingsData};
use industrialis_core::launcher_window::is_valid_launcher_window_settings;

use crate::fs_utils::{read_json_value, write_json};
use crate::paths::LauncherPaths;

pub async fn load_instance_settings(
    paths: &LauncherPaths,
    id: &str,
) -> Result<InstanceSettings, anyhow::Error> {
    let path = paths.settings_path(id)?;
    let document = crate::profile_store::read_object(&path).await?;
    Ok(document
        .map(|v| parse_instance_settings_json(&v.to_string()))
        .unwrap_or_default())
}

pub async fn save_instance_settings(
    paths: &LauncherPaths,
    id: &str,
    settings: &InstanceSettings,
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let id = id.to_string();
    let settings = settings.clone();
    crate::profile_store::mutate(async move {
        save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await
}

pub(crate) async fn save_instance_settings_unlocked(
    paths: &LauncherPaths,
    id: &str,
    settings: &InstanceSettings,
) -> anyhow::Result<()> {
    let path = paths.settings_path(id)?;
    let original = crate::profile_store::read_object(&path).await?;
    let mut saved = settings.clone();
    let latest = original
        .as_ref()
        .map(|v| parse_instance_settings_json(&v.to_string()))
        .unwrap_or_default();
    let mut extras = latest.extra_fields.clone();
    extras.extend(saved.extra_fields);
    saved.extra_fields = extras;
    save_document_changes(
        &path,
        original,
        serde_json::to_value(latest)?,
        serde_json::to_value(saved)?,
    )
    .await
}

pub(crate) async fn record_game_time(
    paths: &LauncherPaths,
    id: &str,
    session: &crate::running_pids::GameSession,
    ended_at_ms: u64,
) -> anyhow::Result<()> {
    if !session.record_time {
        return Ok(());
    }
    let paths = paths.clone();
    let id = id.to_string();
    let session = session.clone();
    crate::profile_store::mutate(async move {
        let mut settings = load_instance_settings(&paths, &id).await?;
        let elapsed = ended_at_ms.saturating_sub(session.started_at_ms) / 1000;
        let previous = settings
            .extra_fields
            .get("gpui_playtime_checkpoint")
            .filter(|v| v["capture_id"] == session.capture_id)
            .and_then(|v| v["seconds"].as_u64())
            .unwrap_or(0);
        settings.total_play_seconds = settings
            .total_play_seconds
            .saturating_add(elapsed.saturating_sub(previous));
        settings.extra_fields.insert(
            "gpui_playtime_checkpoint".into(),
            serde_json::json!({"capture_id":session.capture_id,"seconds":elapsed.max(previous)}),
        );
        save_instance_settings_unlocked(&paths, &id, &settings).await
    })
    .await
}

/// Compare interpreted settings, but apply their changes to the original JSON.
/// Reading a default must not insert it into an existing Electron document or
/// normalize an unrelated legacy value. No-op saves preserve the original bytes.
async fn save_document_changes(
    path: &std::path::Path,
    original: Option<serde_json::Value>,
    before: serde_json::Value,
    after: serde_json::Value,
) -> anyhow::Result<()> {
    match original {
        None => write_json(path, &after).await,
        Some(original) => {
            let mut saved = original.clone();
            apply_changes(&mut saved, &before, &after);
            if saved == original {
                return Ok(());
            }
            write_json(path, &saved).await
        }
    }
}

fn apply_changes(
    raw: &mut serde_json::Value,
    before: &serde_json::Value,
    after: &serde_json::Value,
) {
    if before == after {
        return;
    }
    if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
        if !raw.is_object() {
            *raw = serde_json::json!({});
        }
        let target = raw.as_object_mut().unwrap();
        for key in before.keys() {
            if !after.contains_key(key) {
                target.remove(key);
            }
        }
        for (key, value) in after {
            match before.get(key) {
                Some(old) if old == value => {}
                Some(old) => apply_changes(
                    target.entry(key.clone()).or_insert(serde_json::Value::Null),
                    old,
                    value,
                ),
                None => {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *raw = after.clone();
    }
}

/// Apply editor changes to the latest disk document, preserving backend-owned
/// fields such as play time, pack version, icon, and cached size.
pub async fn patch_instance_settings(
    paths: &LauncherPaths,
    id: &str,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<InstanceSettings> {
    let _lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    if crate::launch::is_already_running(paths, id).await? {
        anyhow::bail!("Cannot edit settings while the instance is running");
    }
    if !crate::instances::known_instance_ids(paths)
        .await?
        .contains(id)
    {
        anyhow::bail!("instance not found");
    }
    let paths = paths.clone();
    let id = id.to_string();
    let patch = patch.clone();
    crate::profile_store::mutate(async move {
        let _lease = _lease;
        let mut value = serde_json::to_value(load_instance_settings(&paths, &id).await?)?;
        let object = value.as_object_mut().unwrap();
        let editable = serde_json::to_value(InstanceSettings::default())?;
        for (key, value) in &patch {
            if editable.get(key).is_none()
                || matches!(
                    key.as_str(),
                    "pack_version"
                        | "pack_java_type"
                        | "custom_icon"
                        | "cached_size_bytes"
                        | "total_play_seconds"
                )
            {
                anyhow::bail!("This setting cannot be edited: {key}");
            }
            object.insert(key.clone(), value.clone());
        }
        let settings: InstanceSettings = serde_json::from_value(value)?;
        if settings.override_memory
            && (settings.min_ram_mb == 0 || settings.max_ram_mb < settings.min_ram_mb)
        {
            anyhow::bail!(
                "Maximum memory must be at least the minimum memory, and both must be positive."
            );
        }
        if settings.override_window && (settings.window_width == 0 || settings.window_height == 0) {
            anyhow::bail!("Window dimensions must be positive.");
        }
        if settings
            .backup_retention_override
            .is_some_and(|limit| !(1..=1000).contains(&limit))
        {
            anyhow::bail!("Backup retention limit must be between 1 and 1000.");
        }
        save_instance_settings_unlocked(&paths, &id, &settings).await?;
        Ok(settings)
    })
    .await
}

pub async fn load_launcher_settings(
    paths: &LauncherPaths,
) -> Result<LauncherSettingsData, anyhow::Error> {
    let path = paths.launcher_settings_path();
    let document = crate::profile_store::read_object(&path).await?;
    Ok(document
        .map(|v| parse_launcher_settings_json(&v.to_string()))
        .unwrap_or_default())
}

/// Mirrors `validateLauncherSettings` (runs before every save).
pub fn validate_launcher_settings(settings: &LauncherSettingsData) -> Result<(), anyhow::Error> {
    if !(20..=90).contains(&settings.window_tint_opacity) {
        anyhow::bail!("window tint opacity must be between 20 and 90 percent");
    }
    if settings.theme_preset.is_empty() || settings.theme_preset.len() > 64 {
        anyhow::bail!("invalid theme preset id");
    }
    if !(2..=5).contains(&settings.instance_grid_columns) {
        anyhow::bail!("instance grid columns must be between 2 and 5");
    }
    if !(1..=1000).contains(&settings.backup_retention_limit) {
        anyhow::bail!("backup retention limit must be between 1 and 1000");
    }
    if !is_valid_launcher_window_settings(&settings.window) {
        anyhow::bail!("invalid launcher window settings");
    }
    if let Some(java) = settings.default_java_path.as_deref() {
        if java.trim().is_empty() || java.len() > 4096 {
            anyhow::bail!("invalid default Java path");
        }
    }
    for (_, value) in settings.theme_overrides.as_pairs() {
        if value.len() > 32 {
            anyhow::bail!("theme override value too long");
        }
    }
    if settings.theme_preset.starts_with("custom-")
        && !settings.custom_theme_presets.iter().any(|preset| {
            preset.get("id").and_then(|v| v.as_str()) == Some(settings.theme_preset.as_str())
        })
    {
        anyhow::bail!("active custom theme preset not found");
    }
    Ok(())
}

pub async fn save_launcher_settings(
    paths: &LauncherPaths,
    settings: &LauncherSettingsData,
) -> Result<(), anyhow::Error> {
    validate_launcher_settings(settings)?;
    let paths = paths.clone();
    let settings = settings.clone();
    crate::profile_store::mutate(
        async move { save_launcher_settings_unlocked(&paths, &settings).await },
    )
    .await
}

pub(crate) async fn save_launcher_settings_unlocked(
    paths: &LauncherPaths,
    settings: &LauncherSettingsData,
) -> anyhow::Result<()> {
    validate_launcher_settings(settings)?;
    let path = paths.launcher_settings_path();
    let original = crate::profile_store::read_object(&path).await?;
    let latest = original
        .as_ref()
        .map(|v| parse_launcher_settings_json(&v.to_string()))
        .unwrap_or_default();
    let mut saved = settings.clone();
    let mut extras = latest.extra_fields.clone();
    extras.extend(saved.extra_fields);
    saved.extra_fields = extras;
    let mut overrides = latest.theme_overrides.extra_fields.clone();
    overrides.extend(saved.theme_overrides.extra_fields);
    saved.theme_overrides.extra_fields = overrides;
    save_document_changes(
        &path,
        original,
        serde_json::to_value(latest)?,
        serde_json::to_value(saved)?,
    )
    .await
}

/// UI preferences do not own default-account fields. Merge their snapshot with
/// the latest account defaults within the same transaction as account mutations.
pub async fn save_launcher_preferences(
    paths: &LauncherPaths,
    preferences: &LauncherSettingsData,
) -> anyhow::Result<()> {
    let paths = paths.clone();
    let preferences = preferences.clone();
    crate::profile_store::mutate(async move {
        let latest = load_launcher_settings(&paths).await?;
        let mut saved = preferences;
        saved.default_account_id = latest.default_account_id;
        saved.active_account_id = latest.active_account_id;
        save_launcher_settings_unlocked(&paths, &saved).await
    })
    .await
}

/// Mirrors the TS test helper `readJson` usage for arbitrary state files.
pub async fn load_json_file(path: &std::path::Path) -> Option<serde_json::Value> {
    read_json_value(path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use industrialis_core::launcher_window::DEFAULT_LAUNCHER_WINDOW_WIDTH;

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-settings-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn saves_and_reloads_both_backdrops_and_opaque() {
        use industrialis_core::launcher_settings::WindowMaterial;
        let paths = temp_paths("backdrop");
        for material in [
            WindowMaterial::Frosted,
            WindowMaterial::Mica,
            WindowMaterial::Opaque,
        ] {
            let settings = LauncherSettingsData {
                window_material: material,
                window_tint_opacity: 30,
                ..Default::default()
            };
            save_launcher_settings(&paths, &settings).await.unwrap();
            assert_eq!(load_launcher_settings(&paths).await.unwrap(), settings);
        }
    }

    #[test]
    fn rejects_invalid_tint_strength() {
        for opacity in [0, 19, 91, 255] {
            let settings = LauncherSettingsData {
                window_tint_opacity: opacity,
                ..Default::default()
            };
            assert!(validate_launcher_settings(&settings).is_err());
        }
    }

    #[tokio::test]
    async fn loads_saved_launch_size_and_maximize_preference() {
        let paths = temp_paths("load");
        let target = paths.launcher_settings_path();
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &target,
            r#"{"launch_maximized":true,"window_width":1440,"window_height":900}"#,
        )
        .await
        .unwrap();
        let settings = load_launcher_settings(&paths).await.unwrap();
        assert!(settings.window.launch_maximized);
        assert_eq!(settings.window.window_width, 1440);
        assert_eq!(settings.window.window_height, 900);
    }

    #[tokio::test]
    async fn falls_back_to_defaults_for_invalid_saved_dimensions() {
        let paths = temp_paths("fallback");
        let target = paths.launcher_settings_path();
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&target, r#"{"window_width":1,"window_height":"large"}"#)
            .await
            .unwrap();
        let settings = load_launcher_settings(&paths).await.unwrap();
        let defaults = LauncherSettingsData::default();
        assert_eq!(settings.window.window_width, defaults.window.window_width);
        assert_eq!(settings.window.window_height, defaults.window.window_height);
    }

    #[tokio::test]
    async fn missing_files_yield_defaults() {
        let paths = temp_paths("missing");
        let launcher = load_launcher_settings(&paths).await.unwrap();
        assert_eq!(launcher.window.window_width, DEFAULT_LAUNCHER_WINDOW_WIDTH);
        let instance = load_instance_settings(&paths, "gtnh").await.unwrap();
        assert_eq!(instance.max_ram_mb, 6144);
    }

    #[tokio::test]
    async fn rejects_invalid_dimensions_before_saving() {
        let paths = temp_paths("validate");
        let mut settings = LauncherSettingsData::default();
        settings.window.window_width = 799;
        let err = save_launcher_settings(&paths, &settings).await.unwrap_err();
        assert_eq!(err.to_string(), "invalid launcher window settings");
    }

    #[tokio::test]
    async fn round_trips_instance_settings() {
        let paths = temp_paths("instance");
        let mut settings = InstanceSettings::default();
        settings.name = "GTNH".to_string();
        save_instance_settings(&paths, "gtnh", &settings)
            .await
            .unwrap();
        let loaded = load_instance_settings(&paths, "gtnh").await.unwrap();
        assert_eq!(loaded, settings);
    }

    async fn seed_instance(paths: &LauncherPaths) {
        let dir = paths.instance_dir("gtnh").unwrap();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        let mut settings = InstanceSettings::default();
        settings.pack_version = "2.8.0".into();
        settings.total_play_seconds = 1234;
        settings.cached_size_bytes = 9876;
        save_instance_settings(paths, "gtnh", &settings)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn editor_patches_merge_latest_settings_without_resetting_metadata() {
        let paths = temp_paths("patch-merge");
        seed_instance(&paths).await;
        patch_instance_settings(&paths, "gtnh", serde_json::json!({"name":"Renamed", "override_memory":true, "min_ram_mb":2048, "max_ram_mb":8192}).as_object().unwrap()).await.unwrap();
        let mut backend = load_instance_settings(&paths, "gtnh").await.unwrap();
        backend.total_play_seconds += 50;
        save_instance_settings(&paths, "gtnh", &backend)
            .await
            .unwrap();
        let saved = patch_instance_settings(
            &paths,
            "gtnh",
            serde_json::json!({"override_env":true,"env_vars":{"TEST":"value"}})
                .as_object()
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(saved.name, "Renamed");
        assert_eq!(saved.max_ram_mb, 8192);
        assert_eq!(saved.env_vars["TEST"], "value");
        assert_eq!(saved.total_play_seconds, 1284);
        assert_eq!(saved.pack_version, "2.8.0");
        assert_eq!(saved.cached_size_bytes, 9876);
    }

    #[tokio::test]
    async fn invalid_editor_patches_and_busy_instances_do_not_write() {
        let paths = temp_paths("patch-reject");
        seed_instance(&paths).await;
        let original = tokio::fs::read(paths.settings_path("gtnh").unwrap())
            .await
            .unwrap();
        for patch in [
            serde_json::json!({"override_memory":true,"min_ram_mb":8192,"max_ram_mb":2048}),
            serde_json::json!({"max_ram_mb":"large"}),
            serde_json::json!({"total_play_seconds":0}),
            serde_json::json!({"unknown":true}),
        ] {
            assert!(
                patch_instance_settings(&paths, "gtnh", patch.as_object().unwrap())
                    .await
                    .is_err()
            );
        }
        let lease = crate::instance_guard::reserve_instances(&paths, &["gtnh"]).unwrap();
        let patch = serde_json::json!({"name":"Busy"});
        assert!(
            patch_instance_settings(&paths, "gtnh", patch.as_object().unwrap())
                .await
                .is_err()
        );
        assert_eq!(
            tokio::fs::read(paths.settings_path("gtnh").unwrap())
                .await
                .unwrap(),
            original
        );
        drop(lease);
        assert!(
            patch_instance_settings(&paths, "gtnh", patch.as_object().unwrap())
                .await
                .is_ok()
        );
        assert!(
            patch_instance_settings(&paths, "missing", patch.as_object().unwrap())
                .await
                .is_err()
        );
        assert!(!paths.instance_dir("missing").unwrap().exists());
    }
}
