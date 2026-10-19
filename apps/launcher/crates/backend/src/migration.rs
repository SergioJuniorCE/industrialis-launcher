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
}
