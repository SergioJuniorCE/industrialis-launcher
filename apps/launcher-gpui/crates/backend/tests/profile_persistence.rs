//! Electron-shaped profiles, copied before exercising the GPUI services.
//! These fixtures contain synthetic credentials only and never touch user data.

use industrialis_backend::{accounts, fs_utils, groups, instances, paths::LauncherPaths, settings};
use industrialis_core::launcher_settings::{LauncherSettingsData, ThemeMode};
use serde_json::{json, Value};
use std::{collections::HashSet, path::PathBuf};

struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("industrialis-profile-{}", uuid::Uuid::new_v4())))
    }
    fn paths(&self) -> LauncherPaths {
        LauncherPaths::new(self.0.clone())
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn read(path: PathBuf) -> Value {
    serde_json::from_slice(&tokio::fs::read(path).await.unwrap()).unwrap()
}

#[tokio::test]
async fn existing_settings_change_only_edited_fields_and_noops_keep_original_bytes() {
    let profile = Profile::new();
    let paths = profile.paths();
    tokio::fs::create_dir_all(paths.instance_dir("pack").unwrap())
        .await
        .unwrap();
    let launcher = r##"{ "theme_mode":"dark", "window_width":1, "default_java_path":17,
        "theme_overrides":{"primary":"#123456","secondary":false,"future":7}, "extension":[3] }
"##;
    tokio::fs::write(paths.launcher_settings_path(), launcher)
        .await
        .unwrap();
    let mut prefs = settings::load_launcher_settings(&paths).await.unwrap();
    settings::save_launcher_preferences(&paths, &prefs)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read_to_string(paths.launcher_settings_path())
            .await
            .unwrap(),
        launcher
    );
    prefs.theme_overrides.primary = Some("#abcdef".into());
    prefs.backup_retention_limit = 27;
    settings::save_launcher_preferences(&paths, &prefs)
        .await
        .unwrap();
    let mut expected: Value = serde_json::from_str(launcher).unwrap();
    expected["theme_overrides"]["primary"] = json!("#abcdef");
    expected["backup_retention_limit"] = json!(27);
    assert_eq!(read(paths.launcher_settings_path()).await, expected);
    prefs.theme_overrides.primary = None;
    settings::save_launcher_preferences(&paths, &prefs)
        .await
        .unwrap();
    expected["theme_overrides"]
        .as_object_mut()
        .unwrap()
        .remove("primary");
    assert_eq!(read(paths.launcher_settings_path()).await, expected);

    let instance = r#"{ "name":"Pack", "max_ram_mb":"legacy", "cached_size_bytes":123, "extension":{"x":1} }
"#;
    let path = paths.settings_path("pack").unwrap();
    tokio::fs::write(&path, instance).await.unwrap();
    tokio::fs::write(
        paths.instance_dir("pack").unwrap().join("mmc-pack.json"),
        "{}",
    )
    .await
    .unwrap();
    let loaded = settings::load_instance_settings(&paths, "pack")
        .await
        .unwrap();
    settings::save_instance_settings(&paths, "pack", &loaded)
        .await
        .unwrap();
    assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), instance);
    settings::patch_instance_settings(
        &paths,
        "pack",
        json!({"name":"Renamed"}).as_object().unwrap(),
    )
    .await
    .unwrap();
    let mut expected: Value = serde_json::from_str(instance).unwrap();
    expected["name"] = json!("Renamed");
    assert_eq!(read(path).await, expected);
}

#[tokio::test]
async fn account_edits_preserve_untouched_legacy_rows_nulls_and_order_in_place() {
    let profile = Profile::new();
    let paths = profile.paths();
    tokio::fs::create_dir_all(paths.base()).await.unwrap();
    let original = r#"[
      {"id":"old","username":"Alex","uuid":"uuid","access_token":"fake","refresh_token":"fake-refresh","expires_at":123,"extension":9},
      {"format_version":4,"id":"future"},
      {"format_version":3,"account_type":"offline","id":"current","msa_token":null,"minecraft_profile":{"id":"uuid2","name":"Steve","skins":null}}
    ]
"#;
    tokio::fs::write(paths.accounts_path(), original)
        .await
        .unwrap();
    let loaded = accounts::load_accounts(&paths).await.unwrap();
    accounts::save_accounts(&paths, &loaded).await.unwrap();
    assert_eq!(
        tokio::fs::read_to_string(paths.accounts_path())
            .await
            .unwrap(),
        original
    );
    let added = accounts::create_offline_account(&paths, "NewPlayer")
        .await
        .unwrap();
    let before: Value = serde_json::from_str(original).unwrap();
    let saved = read(paths.accounts_path()).await;
    assert_eq!(&saved.as_array().unwrap()[..3], before.as_array().unwrap());
    accounts::remove_account(&paths, &added.id).await.unwrap();
    assert_eq!(read(paths.accounts_path()).await, before);
    accounts::remove_account(&paths, "current").await.unwrap();
    let mut expected = before;
    expected.as_array_mut().unwrap().pop();
    assert_eq!(read(paths.accounts_path()).await, expected);
}

#[tokio::test]
async fn copied_electron_profile_keeps_extensions_credentials_groups_and_opaque_files() {
    let source = Profile::new();
    let copy = Profile::new();
    let old = source.paths();
    fs_utils::write_json(&old.launcher_settings_path(), &json!({
        "theme_mode":"dark", "theme_preset":"custom-personal",
        "custom_theme_presets":[{"id":"custom-personal","dark":{"future-token":"keep"}}],
        "theme_overrides":{"primary":"#123456","secondary":"#654321", "ring":"#102030", "future-token":{"value":7}},
        "future_preferences":{"nested":[1,2,3]}, "active_account_id":"legacy"
    })).await.unwrap();
    let legacy = json!({"id":"legacy", "username":"Alex", "uuid":"legacy-uuid", "access_token":"synthetic-game-token", "refresh_token":"synthetic-refresh", "expires_at":123, "legacy_extension":{"keep":true}});
    let current = json!({"format_version":3,"account_type":"offline","id":"current", "account_extension":[4], "minecraft_profile":{"id":"current-uuid","name":"Steve", "capes":[{"url":"keep"}], "skins":[{"id":"skin","state":"ACTIVE","url":"https://example.test/skin","variant":"CLASSIC","skin_extension":9}]}});
    let damaged = json!({"format_version":3,"id":"damaged","msa_token":"unsupported"});
    let future =
        json!({"format_version":4,"account_type":"msa","id":"future","future_token":"keep"});
    fs_utils::write_json(
        &old.accounts_path(),
        &json!([legacy, current, damaged, future, 17]),
    )
    .await
    .unwrap();
    let dir = old.instance_dir("pack").unwrap();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("mmc-pack.json"), "{}")
        .await
        .unwrap();
    fs_utils::write_json(&old.settings_path("pack").unwrap(), &json!({"name":"Pack", "pack_version":"2.9.0", "total_play_seconds":321, "custom_icon":"custom.png", "cached_size_bytes":987, "future_instance":{"data":5}})).await.unwrap();
    fs_utils::write_json(&old.groups_path(), &json!({"formatVersion":1,"root_extension":true,"groups":{"Main":{"hidden":true,"instances":["pack"],"color":"keep"},"Empty":{"hidden":false,"instances":[],"note":"keep"}},"ungrouped":{"name":"Other","hidden":false,"instances":[],"extension":7}})).await.unwrap();
    let opaque = [
        "icons/custom.png",
        "instances/pack/console.log",
        "instances/pack/persistent-minecraft/config/user.cfg",
        "instances/pack/custom-mods/user.jar",
        "pack-cache/archive.zip",
        "backup-state.json",
        "google-drive-state.json",
        "instances/pack/.minecraft/backups/world.zip",
    ];
    for file in opaque {
        let path = source.0.join(file);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, format!("opaque contents: {file}"))
            .await
            .unwrap();
    }
    fs_utils::write_json(
        &old.running_game_pids_path(),
        &json!({"future":{"pid":"unsupported","keep":true}}),
    )
    .await
    .unwrap();
    let original_pids = tokio::fs::read(old.running_game_pids_path()).await.unwrap();
    fs_utils::copy_tree(&source.0, &copy.0).await.unwrap();
    let paths = copy.paths();
    let before = tokio::fs::read(old.accounts_path()).await.unwrap();
    let (loaded, skipped) = accounts::load_accounts_with_issues(&paths).await.unwrap();
    assert_eq!((loaded.len(), skipped), (2, 3));
    assert_eq!(
        loaded
            .iter()
            .find(|a| a.id == "legacy")
            .unwrap()
            .msa_token
            .as_ref()
            .unwrap()
            .refresh_token,
        "synthetic-refresh"
    );
    let mut prefs = settings::load_launcher_settings(&paths).await.unwrap();
    assert_eq!(prefs.theme_overrides.secondary.as_deref(), Some("#654321"));
    prefs.backup_retention_limit = 27;
    settings::save_launcher_preferences(&paths, &prefs)
        .await
        .unwrap();
    accounts::set_default_account(&paths, Some("current"))
        .await
        .unwrap();
    let mut refreshed = loaded.iter().find(|a| a.id == "current").unwrap().clone();
    refreshed.extra_fields.clear();
    refreshed
        .minecraft_profile
        .as_mut()
        .unwrap()
        .extra_fields
        .clear();
    refreshed
        .minecraft_profile
        .as_mut()
        .unwrap()
        .skins
        .as_mut()
        .unwrap()[0]
        .extra_fields
        .clear();
    accounts::upsert_account(&paths, refreshed).await.unwrap();
    accounts::create_offline_account(&paths, "NewPlayer")
        .await
        .unwrap();
    let known = HashSet::from(["pack".to_string()]);
    groups::set_instance_group(&paths, "pack", "New", &known)
        .await
        .unwrap();
    settings::patch_instance_settings(
        &paths,
        "pack",
        json!({"name":"Renamed"}).as_object().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        tokio::fs::read(paths.running_game_pids_path())
            .await
            .unwrap(),
        original_pids
    );
    let prefs = read(paths.launcher_settings_path()).await;
    assert_eq!(prefs["future_preferences"], json!({"nested":[1,2,3]}));
    assert_eq!(prefs["theme_overrides"]["future-token"], json!({"value":7}));
    assert_eq!(prefs["theme_overrides"]["ring"], "#102030");
    assert_eq!(
        prefs["custom_theme_presets"][0]["dark"]["future-token"],
        "keep"
    );
    assert_eq!(prefs["default_account_id"], "current");
    let rows = read(paths.accounts_path()).await;
    let rows = rows.as_array().unwrap();
    assert!(rows.contains(&damaged) && rows.contains(&future) && rows.contains(&json!(17)));
    let current = rows.iter().find(|r| r["id"] == "current").unwrap();
    assert_eq!(current["account_extension"], json!([4]));
    assert_eq!(
        current["minecraft_profile"]["capes"],
        json!([{"url":"keep"}])
    );
    assert_eq!(
        current["minecraft_profile"]["skins"][0]["skin_extension"],
        9
    );
    let legacy = rows.iter().find(|r| r["id"] == "legacy").unwrap();
    assert_eq!(legacy["legacy_extension"], json!({"keep":true}));
    assert_eq!(legacy["access_token"], "synthetic-game-token");
    assert!(
        legacy.get("format_version").is_none(),
        "untouched legacy rows keep their original shape"
    );
    let group = read(paths.groups_path()).await;
    assert_eq!(group["root_extension"], true);
    assert_eq!(group["groups"]["Main"]["color"], "keep");
    assert_eq!(group["groups"]["Empty"]["note"], "keep");
    assert_eq!(group["ungrouped"]["extension"], 7);
    let instance = read(paths.settings_path("pack").unwrap()).await;
    assert_eq!(instance["future_instance"], json!({"data":5}));
    assert_eq!(instance["total_play_seconds"], 321);
    assert_eq!(instance["custom_icon"], "custom.png");
    for file in opaque {
        assert_eq!(
            tokio::fs::read(copy.0.join(file)).await.unwrap(),
            tokio::fs::read(source.0.join(file)).await.unwrap()
        );
    }
    assert_eq!(
        tokio::fs::read(old.accounts_path()).await.unwrap(),
        before,
        "source profile was not modified"
    );
}

#[tokio::test]
async fn stale_preference_snapshots_cannot_restore_removed_or_old_default_accounts() {
    let profile = Profile::new();
    let paths = profile.paths();
    let a = accounts::create_offline_account(&paths, "Alex")
        .await
        .unwrap();
    let b = accounts::create_offline_account(&paths, "Steve")
        .await
        .unwrap();
    accounts::set_default_account(&paths, Some(&a.id))
        .await
        .unwrap();
    let stale = settings::load_launcher_settings(&paths).await.unwrap();
    for _ in 0..12 {
        accounts::set_default_account(&paths, Some(&a.id))
            .await
            .unwrap();
        let mut edit = stale.clone();
        edit.theme_mode = ThemeMode::Light;
        edit.backup_retention_limit = 31;
        let (prefs, account) = tokio::join!(
            settings::save_launcher_preferences(&paths, &edit),
            accounts::set_default_account(&paths, Some(&b.id))
        );
        prefs.unwrap();
        account.unwrap();
        let saved = settings::load_launcher_settings(&paths).await.unwrap();
        assert_eq!(saved.default_account_id.as_deref(), Some(b.id.as_str()));
        assert_eq!(saved.backup_retention_limit, 31);
    }
    let (prefs, removed) = tokio::join!(
        settings::save_launcher_preferences(&paths, &stale),
        accounts::remove_account(&paths, &b.id)
    );
    prefs.unwrap();
    removed.unwrap();
    let saved = settings::load_launcher_settings(&paths).await.unwrap();
    assert!(saved.default_account_id.is_none() && saved.active_account_id.is_none());
    let (one, two) = tokio::join!(
        accounts::create_offline_account(&paths, "Same"),
        accounts::create_offline_account(&paths, "Same")
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(accounts::load_accounts(&paths).await.unwrap().len(), 2);
}

#[tokio::test]
async fn corrupt_documents_are_never_replaced_by_defaults() {
    let profile = Profile::new();
    let paths = profile.paths();
    tokio::fs::create_dir_all(paths.instances_dir())
        .await
        .unwrap();
    for raw in ["{broken", "null", "[]"] {
        tokio::fs::write(paths.launcher_settings_path(), raw)
            .await
            .unwrap();
        assert!(settings::load_launcher_settings(&paths).await.is_err());
        assert!(
            settings::save_launcher_preferences(&paths, &LauncherSettingsData::default())
                .await
                .is_err()
        );
        assert_eq!(
            tokio::fs::read_to_string(paths.launcher_settings_path())
                .await
                .unwrap(),
            raw
        );
    }
    tokio::fs::write(paths.accounts_path(), "{broken")
        .await
        .unwrap();
    assert!(accounts::load_accounts(&paths).await.is_err());
    assert!(accounts::create_offline_account(&paths, "Alex")
        .await
        .is_err());
    assert_eq!(
        tokio::fs::read_to_string(paths.accounts_path())
            .await
            .unwrap(),
        "{broken"
    );
    let dir = paths.instance_dir("bad").unwrap();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("mmc-pack.json"), "{}")
        .await
        .unwrap();
    tokio::fs::write(paths.settings_path("bad").unwrap(), "{broken")
        .await
        .unwrap();
    let good = paths.instance_dir("good").unwrap();
    tokio::fs::create_dir_all(&good).await.unwrap();
    tokio::fs::write(good.join("mmc-pack.json"), "{}")
        .await
        .unwrap();
    let (instances, issues) = instances::load_instances_with_issues(&paths).await.unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].id, "good");
    assert_eq!(issues.len(), 1);
    assert!(
        !paths.settings_path("good").unwrap().exists(),
        "discovery must not rewrite a legacy profile"
    );
    assert!(
        settings::save_instance_settings(&paths, "bad", &Default::default())
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read_to_string(paths.settings_path("bad").unwrap())
            .await
            .unwrap(),
        "{broken"
    );
    tokio::fs::write(paths.groups_path(), "{broken")
        .await
        .unwrap();
    assert!(groups::set_instance_group(
        &paths,
        "good",
        "Main",
        &HashSet::from(["good".to_string()])
    )
    .await
    .is_err());
    assert_eq!(
        tokio::fs::read_to_string(paths.groups_path())
            .await
            .unwrap(),
        "{broken"
    );
}

#[tokio::test]
async fn concurrent_group_changes_do_not_drop_other_assignments() {
    let profile = Profile::new();
    let paths = profile.paths();
    let known = HashSet::from(["a".to_string(), "b".to_string()]);
    let (a, b) = tokio::join!(
        groups::set_instance_group(&paths, "a", "First", &known),
        groups::set_instance_group(&paths, "b", "Second", &known)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        groups::get_instance_group(&paths, "a", &known)
            .await
            .unwrap(),
        "First"
    );
    assert_eq!(
        groups::get_instance_group(&paths, "b", &known)
            .await
            .unwrap(),
        "Second"
    );
    let (remove, add) = tokio::join!(
        groups::remove_instance_from_groups(&paths, "a", &known),
        groups::set_instance_group(&paths, "b", "Third", &known)
    );
    remove.unwrap();
    add.unwrap();
    assert_eq!(
        groups::get_instance_group(&paths, "a", &known)
            .await
            .unwrap(),
        ""
    );
    assert_eq!(
        groups::get_instance_group(&paths, "b", &known)
            .await
            .unwrap(),
        "Third"
    );
}

#[tokio::test]
async fn failed_default_write_does_not_delete_account_credentials() {
    let profile = Profile::new();
    let paths = profile.paths();
    let account = accounts::create_offline_account(&paths, "Alex")
        .await
        .unwrap();
    accounts::set_default_account(&paths, Some(&account.id))
        .await
        .unwrap();
    let target = paths.launcher_settings_path();
    let permissions = std::fs::metadata(&target).unwrap().permissions();
    let mut read_only = permissions.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&target, read_only).unwrap();
    let result = accounts::remove_account(&paths, &account.id).await;
    std::fs::set_permissions(&target, permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(
        accounts::load_accounts(&paths).await.unwrap()[0].id,
        account.id
    );
    assert_eq!(
        settings::load_launcher_settings(&paths)
            .await
            .unwrap()
            .default_account_id,
        Some(account.id.clone())
    );
    accounts::remove_account(&paths, &account.id).await.unwrap();
    assert!(accounts::load_accounts(&paths).await.unwrap().is_empty());
}
