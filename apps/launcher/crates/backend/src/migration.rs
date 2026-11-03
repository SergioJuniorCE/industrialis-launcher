//! Port of `electron/backend/migration.ts`: player-data preserve/restore/wipe
//! used by clean reinstalls and pack updates.

use std::path::{Path, PathBuf};

use crate::fs_utils::{copy_tree, exists, remove_if_exists};

pub const PRESERVE_DIR_NAME: &str = ".reinstall-preserve";

const PRESERVE_DIRS: [&str; 11] = [
    "backups",
    "ESM",
    "journeymap",
    "resourcepacks",
    "saves",
    "schematics",
    "screenshots",
    "shaderpacks",
    "TCNodeTracker",
    "visualprospecting",
    "serverutilities",
];

const PRESERVE_FILES: [&str; 8] = [
    "shaders.properties",
    "BotaniaVars.dat",
    "localconfig.cfg",
    "options.txt",
    "optionsnf.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "servers.dat",
];

const PRESERVE_CONFIGS: [&str; 6] = [
    "vendingmachine/favourites",
    "GregTech/Pollution.cfg",
    "txloader/load/minecraft/sounds/music/menu",
    "gtnhintergalactic.cfg",
    "lwjgl3ify.cfg",
    "tectech.cfg",
];

async fn copy_if_exists(source: &Path, destination: &Path) -> Result<(), anyhow::Error> {
    if !exists(source).await {
        return Ok(());
    }
    remove_if_exists(destination).await?;
    copy_tree(source, destination).await
}

/// Mirrors `backupPlayerData`.
pub async fn backup_player_data(instance: &Path, preserve: &Path) -> Result<(), anyhow::Error> {
    remove_if_exists(preserve).await?;
    tokio::fs::create_dir_all(preserve).await?;
    copy_if_exists(
        &instance.join("instance.json"),
        &preserve.join("instance.json"),
    )
    .await?;
    copy_if_exists(
        &instance.join("persistent-minecraft"),
        &preserve.join("persistent-minecraft"),
    )
    .await?;
    let icons = preserve.join("launcher-icons");
    tokio::fs::create_dir_all(&icons).await?;
    if let Ok(mut entries) = tokio::fs::read_dir(instance).await {
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("instance-icon") {
                copy_if_exists(&entry.path(), &icons.join(&name)).await?;
            }
        }
    }
    let minecraft = instance.join(".minecraft");
    if !exists(&minecraft).await {
        return Ok(());
    }
    let preserve_minecraft = preserve.join("minecraft");
    for name in PRESERVE_DIRS {
        copy_if_exists(&minecraft.join(name), &preserve_minecraft.join(name)).await?;
    }
    for name in PRESERVE_FILES {
        copy_if_exists(&minecraft.join(name), &preserve_minecraft.join(name)).await?;
    }
    for rel in PRESERVE_CONFIGS {
        copy_if_exists(
            &minecraft.join("config").join(rel),
            &preserve_minecraft.join("config").join(rel),
        )
        .await?;
    }
    Ok(())
}

/// Mirrors `restorePlayerData`.
pub async fn restore_player_data(instance: &Path, preserve: &Path) -> Result<(), anyhow::Error> {
    copy_if_exists(
        &preserve.join("instance.json"),
        &instance.join("instance.json"),
    )
    .await?;
    copy_if_exists(
        &preserve.join("persistent-minecraft"),
        &instance.join("persistent-minecraft"),
    )
    .await?;
    let icons = preserve.join("launcher-icons");
    if let Ok(mut entries) = tokio::fs::read_dir(&icons).await {
        while let Some(entry) = entries.next_entry().await? {
            copy_if_exists(&entry.path(), &instance.join(entry.file_name())).await?;
        }
    }
    let minecraft = instance.join(".minecraft");
    tokio::fs::create_dir_all(&minecraft).await?;
    let preserve_minecraft = preserve.join("minecraft");
    if !exists(&preserve_minecraft).await {
        return Ok(());
    }
    for name in PRESERVE_DIRS {
        copy_if_exists(&preserve_minecraft.join(name), &minecraft.join(name)).await?;
    }
    for name in PRESERVE_FILES {
        copy_if_exists(&preserve_minecraft.join(name), &minecraft.join(name)).await?;
    }
    for rel in PRESERVE_CONFIGS {
        copy_if_exists(
            &preserve_minecraft.join("config").join(rel),
            &minecraft.join("config").join(rel),
        )
        .await?;
    }
    Ok(())
}

/// Restore player data from an intact previous instance kept as the update
/// rollback copy. Unlike [`restore_player_data`], the source has the live
/// instance layout instead of the `.reinstall-preserve` layout.
pub async fn restore_player_data_from_instance(
    instance: &Path,
    previous: &Path,
) -> Result<(), anyhow::Error> {
    copy_if_exists(
        &previous.join("instance.json"),
        &instance.join("instance.json"),
    )
    .await?;
    copy_if_exists(
        &previous.join("persistent-minecraft"),
        &instance.join("persistent-minecraft"),
    )
    .await?;
    if let Ok(mut entries) = tokio::fs::read_dir(previous).await {
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("instance-icon") {
                copy_if_exists(&entry.path(), &instance.join(&name)).await?;
            }
        }
    }
    let previous_minecraft = previous.join(".minecraft");
    if !exists(&previous_minecraft).await {
        return Ok(());
    }
    let minecraft = instance.join(".minecraft");
    for name in PRESERVE_DIRS {
        copy_if_exists(&previous_minecraft.join(name), &minecraft.join(name)).await?;
    }
    for name in PRESERVE_FILES {
        copy_if_exists(&previous_minecraft.join(name), &minecraft.join(name)).await?;
    }
    for rel in PRESERVE_CONFIGS {
        copy_if_exists(
            &previous_minecraft.join("config").join(rel),
            &minecraft.join("config").join(rel),
        )
        .await?;
    }
    Ok(())
}

/// Merge the previous instance's config and script files into a freshly
/// installed pack. Files that still match the old pack are left to the new
/// pack; only user-modified or user-created files are copied over. When the
/// previous pack is unavailable, every user file is kept.
pub async fn restore_modified_configs(
    instance: &Path,
    previous: &Path,
    previous_pack: Option<&Path>,
) -> Result<usize, anyhow::Error> {
    let instance = instance.to_path_buf();
    let previous = previous.to_path_buf();
    let previous_pack = previous_pack.map(Path::to_path_buf);
    tokio::task::spawn_blocking(move || {
        let mut restored = 0;
        for rel in ["config", "scripts"] {
            restored += merge_user_tree(&instance, &previous, previous_pack.as_deref(), rel)?;
        }
        Ok(restored)
    })
    .await?
}

fn merge_user_tree(
    instance: &Path,
    previous: &Path,
    previous_pack: Option<&Path>,
    rel: &str,
) -> Result<usize, anyhow::Error> {
    let source = previous.join(".minecraft").join(rel);
    if !source.is_dir() {
        return Ok(0);
    }
    let baseline = previous_pack.and_then(|pack| {
        let direct = pack.join(".minecraft").join(rel);
        if direct.is_dir() {
            Some(direct)
        } else {
            let root = pack.join(rel);
            root.is_dir().then_some(root)
        }
    });
    let mut restored = 0;
    for entry in walkdir::WalkDir::new(&source).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(&source)?;
        let user = std::fs::read(entry.path())?;
        let unchanged = baseline.as_ref().is_some_and(|base| {
            std::fs::read(base.join(relative)).is_ok_and(|pack| pack == user)
        });
        if unchanged {
            continue;
        }
        let destination = instance.join(".minecraft").join(rel).join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(entry.path(), &destination)?;
        restored += 1;
    }
    Ok(restored)
}

/// Mirrors `wipeInstanceForReinstall`: delete everything except `preserve`.
pub async fn wipe_instance_for_reinstall(
    instance: &Path,
    preserve: &Path,
) -> Result<(), anyhow::Error> {
    let preserve: PathBuf = preserve.to_path_buf();
    let Ok(mut entries) = tokio::fs::read_dir(instance).await else {
        return Ok(());
    };
    while let Some(entry) = entries.next_entry().await? {
        if entry.path() != preserve {
            remove_if_exists(&entry.path()).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_instance(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-migration-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn preserves_and_restores_player_data() {
        let instance = temp_instance("cycle");
        let game = instance.join(".minecraft");
        tokio::fs::create_dir_all(game.join("saves").join("world"))
            .await
            .unwrap();
        tokio::fs::write(game.join("saves").join("world").join("level.dat"), "save")
            .await
            .unwrap();
        tokio::fs::write(game.join("options.txt"), "opts")
            .await
            .unwrap();
        tokio::fs::write(instance.join("instance-icon.png"), "icon")
            .await
            .unwrap();
        tokio::fs::write(instance.join("instance.json"), "{}")
            .await
            .unwrap();

        let preserve = instance.join(PRESERVE_DIR_NAME);
        backup_player_data(&instance, &preserve).await.unwrap();
        assert!(preserve
            .join("minecraft")
            .join("saves")
            .join("world")
            .join("level.dat")
            .exists());
        assert!(preserve
            .join("launcher-icons")
            .join("instance-icon.png")
            .exists());

        wipe_instance_for_reinstall(&instance, &preserve)
            .await
            .unwrap();
        assert!(!game.exists());
        assert!(preserve.exists());

        restore_player_data(&instance, &preserve).await.unwrap();
        assert!(game.join("saves").join("world").join("level.dat").exists());
        assert_eq!(
            tokio::fs::read_to_string(game.join("options.txt"))
                .await
                .unwrap(),
            "opts"
        );
        assert!(instance.join("instance-icon.png").exists());
        assert!(instance.join("instance.json").exists());
    }

    #[tokio::test]
    async fn restores_from_a_previous_instance_layout() {
        let previous = temp_instance("previous-layout");
        let instance = temp_instance("fresh-layout");
        let previous_game = previous.join(".minecraft");
        tokio::fs::create_dir_all(previous_game.join("saves").join("world"))
            .await
            .unwrap();
        tokio::fs::write(
            previous_game.join("saves").join("world").join("level.dat"),
            "save",
        )
        .await
        .unwrap();
        tokio::fs::write(previous_game.join("options.txt"), "opts")
            .await
            .unwrap();
        tokio::fs::write(previous.join("instance-icon.png"), "icon")
            .await
            .unwrap();
        tokio::fs::write(previous.join("instance.json"), "{}")
            .await
            .unwrap();

        restore_player_data_from_instance(&instance, &previous)
            .await
            .unwrap();
        let game = instance.join(".minecraft");
        assert!(game.join("saves").join("world").join("level.dat").exists());
        assert_eq!(
            tokio::fs::read_to_string(game.join("options.txt"))
                .await
                .unwrap(),
            "opts"
        );
        assert!(instance.join("instance-icon.png").exists());
        assert!(instance.join("instance.json").exists());
    }

    #[tokio::test]
    async fn merges_only_user_modified_configs() {
        let root = temp_instance("config-merge");
        let previous = root.join("previous");
        let instance = root.join("instance");
        let pack = root.join("pack");
        tokio::fs::create_dir_all(previous.join(".minecraft/config"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(instance.join(".minecraft/config"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(pack.join("config"))
            .await
            .unwrap();
        for (path, contents) in [
            (previous.join(".minecraft/config/edited.cfg"), "user value"),
            (previous.join(".minecraft/config/untouched.cfg"), "old default"),
            (previous.join(".minecraft/config/created.cfg"), "user created"),
            (instance.join(".minecraft/config/edited.cfg"), "new default"),
            (instance.join(".minecraft/config/untouched.cfg"), "new default"),
            (pack.join("config/edited.cfg"), "old default"),
            (pack.join("config/untouched.cfg"), "old default"),
        ] {
            tokio::fs::write(path, contents).await.unwrap();
        }
        let restored = restore_modified_configs(&instance, &previous, Some(&pack))
            .await
            .unwrap();
        assert_eq!(restored, 2);
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/config/edited.cfg"))
                .await
                .unwrap(),
            "user value"
        );
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/config/untouched.cfg"))
                .await
                .unwrap(),
            "new default"
        );
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/config/created.cfg"))
                .await
                .unwrap(),
            "user created"
        );
    }

    #[tokio::test]
    async fn keeps_every_config_when_the_previous_pack_is_unavailable() {
        let root = temp_instance("config-fallback");
        let previous = root.join("previous");
        let instance = root.join("instance");
        tokio::fs::create_dir_all(previous.join(".minecraft/config"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(instance.join(".minecraft/config"))
            .await
            .unwrap();
        tokio::fs::write(
            previous.join(".minecraft/config/a.cfg"),
            "user value",
        )
        .await
        .unwrap();
        tokio::fs::write(instance.join(".minecraft/config/a.cfg"), "new default")
            .await
            .unwrap();
        let restored = restore_modified_configs(&instance, &previous, None)
            .await
            .unwrap();
        assert_eq!(restored, 1);
        assert_eq!(
            tokio::fs::read_to_string(instance.join(".minecraft/config/a.cfg"))
                .await
                .unwrap(),
            "user value"
        );
    }
}
