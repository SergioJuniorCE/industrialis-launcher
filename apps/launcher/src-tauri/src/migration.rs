//! GTNH player-data preservation for clean reinstalls.
//! Paths follow https://wiki.gtnewhorizons.com/wiki/Installing_and_Migrating (Method 1).

use crate::pack::copy_file_create_parent;
use std::fs;
use std::path::{Path, PathBuf};

const MC_DIR_NAMES: &[&str] = &[
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

const MC_FILE_NAMES: &[&str] = &[
    "shaders.properties",
    "BotaniaVars.dat",
    "localconfig.cfg",
    "options.txt",
    "optionsnf.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "servers.dat",
];

/// Config paths under `.minecraft/config/` (partial — not the whole config tree).
const MC_CONFIG_REL_PATHS: &[&str] = &[
    "vendingmachine/favourites",
    "GregTech/Pollution.cfg",
    "txloader/load/minecraft/sounds/music/menu",
    "gtnhintergalactic.cfg",
    "lwjgl3ify.cfg",
    "tectech.cfg",
];

pub fn preserve_dir_name() -> &'static str {
    ".reinstall-preserve"
}

fn minecraft_game_dir(inst_dir: &Path) -> PathBuf {
    inst_dir.join(".minecraft")
}

fn preserve_minecraft_root(preserve_dir: &Path) -> PathBuf {
    preserve_dir.join("minecraft")
}

fn copy_tree_preserve(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if src.is_file() {
        return copy_file_create_parent(src, dest);
    }
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        copy_tree_preserve(&entry.path(), &dest.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_path_if_exists(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(dest).map_err(|e| e.to_string())?;
        }
    }
    if src.is_dir() {
        copy_tree_preserve(src, dest)
    } else {
        copy_file_create_parent(src, dest)
    }
}

fn backup_launcher_icons(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    let icons_dir = preserve_dir.join("launcher-icons");
    fs::create_dir_all(&icons_dir).map_err(|e| e.to_string())?;
    if !inst_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(inst_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("instance-icon") {
            copy_path_if_exists(&entry.path(), &icons_dir.join(&name))?;
        }
    }
    Ok(())
}

fn restore_launcher_icons(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    let icons_dir = preserve_dir.join("launcher-icons");
    if !icons_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&icons_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        copy_path_if_exists(&entry.path(), &inst_dir.join(entry.file_name()))?;
    }
    Ok(())
}

/// Backs up wiki-listed player data plus launcher overlays before a clean reinstall.
pub fn backup_player_data(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    if preserve_dir.exists() {
        fs::remove_dir_all(preserve_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(preserve_dir).map_err(|e| e.to_string())?;

    let instance_json = inst_dir.join("instance.json");
    copy_path_if_exists(&instance_json, &preserve_dir.join("instance.json"))?;

    let persistent = inst_dir.join("persistent-minecraft");
    copy_path_if_exists(&persistent, &preserve_dir.join("persistent-minecraft"))?;

    backup_launcher_icons(inst_dir, preserve_dir)?;

    let mc = minecraft_game_dir(inst_dir);
    let preserve_mc = preserve_minecraft_root(preserve_dir);
    if !mc.is_dir() {
        return Ok(());
    }

    for name in MC_DIR_NAMES {
        copy_path_if_exists(&mc.join(name), &preserve_mc.join(name))?;
    }

    for name in MC_FILE_NAMES {
        copy_path_if_exists(&mc.join(name), &preserve_mc.join(name))?;
    }

    for rel in MC_CONFIG_REL_PATHS {
        copy_path_if_exists(&mc.join("config").join(rel), &preserve_mc.join("config").join(rel))?;
    }

    Ok(())
}

/// Restores backed-up player data into a freshly installed instance.
pub fn restore_player_data(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    let settings_src = preserve_dir.join("instance.json");
    if settings_src.is_file() {
        copy_path_if_exists(&settings_src, &inst_dir.join("instance.json"))?;
    }

    copy_path_if_exists(
        &preserve_dir.join("persistent-minecraft"),
        &inst_dir.join("persistent-minecraft"),
    )?;

    restore_launcher_icons(inst_dir, preserve_dir)?;

    let mc = minecraft_game_dir(inst_dir);
    fs::create_dir_all(&mc).map_err(|e| e.to_string())?;
    let preserve_mc = preserve_minecraft_root(preserve_dir);
    if !preserve_mc.exists() {
        return Ok(());
    }

    for name in MC_DIR_NAMES {
        copy_path_if_exists(&preserve_mc.join(name), &mc.join(name))?;
    }

    for name in MC_FILE_NAMES {
        copy_path_if_exists(&preserve_mc.join(name), &mc.join(name))?;
    }

    for rel in MC_CONFIG_REL_PATHS {
        copy_path_if_exists(
            &preserve_mc.join("config").join(rel),
            &mc.join("config").join(rel),
        )?;
    }

    Ok(())
}

/// Removes all instance contents except the preservation directory.
pub fn wipe_instance_for_reinstall(inst_dir: &Path, preserve_dir: &Path) -> Result<(), String> {
    if !inst_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(inst_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path == preserve_dir {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn backup_and_restore_round_trip() {
        let temp = std::env::temp_dir().join(format!(
            "il-migration-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let inst = temp.join("instance");
        let preserve = inst.join(preserve_dir_name());
        fs::create_dir_all(inst.join(".minecraft/saves/world1")).unwrap();
        fs::create_dir_all(inst.join(".minecraft/config")).unwrap();
        fs::write(inst.join(".minecraft/options.txt"), "fov:70.0").unwrap();
        fs::write(
            inst.join(".minecraft/config/lwjgl3ify.cfg"),
            "borderless=true",
        )
        .unwrap();
        fs::write(inst.join("instance.json"), r#"{"name":"test"}"#).unwrap();
        fs::write(inst.join("instance-icon.png"), b"icon").unwrap();

        backup_player_data(&inst, &preserve).unwrap();

        wipe_instance_for_reinstall(&inst, &preserve).unwrap();
        assert!(!inst.join(".minecraft").exists());
        assert!(preserve.join("minecraft").join("saves").join("world1").is_dir());

        fs::create_dir_all(inst.join(".minecraft")).unwrap();
        restore_player_data(&inst, &preserve).unwrap();

        assert!(inst.join(".minecraft/saves/world1").is_dir());
        assert_eq!(
            fs::read_to_string(inst.join(".minecraft/options.txt")).unwrap(),
            "fov:70.0"
        );
        assert!(inst.join("instance-icon.png").is_file());

        fs::remove_dir_all(&temp).ok();
    }
}