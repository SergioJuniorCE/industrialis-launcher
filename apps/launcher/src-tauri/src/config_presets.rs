use std::fs;
use std::path::{Path, PathBuf};

use crate::minecraft_files::persistent_minecraft_dir;

const ADITYA_BUNDLED_FILES: &[&str] = &[
    "config/salisarcana.cfg",
    "config/salisarcana/addons.cfg",
    "config/salisarcana/bugfixes.cfg",
    "config/salisarcana/commands.cfg",
    "config/salisarcana/enhancements.cfg",
    "config/salisarcana/mod_integrations.cfg",
    "config/salisarcana/thaumcraft_configuration.cfg",
    "config/Betterloadingscreen/betterloadingscreen.cfg",
    "config/RandomThings.cfg",
];

fn bundled_config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config")
}

fn bundled_config_path(rel: &str) -> PathBuf {
    bundled_config_dir().join(rel.strip_prefix("config/").unwrap_or(rel))
}

fn game_config_path(inst_dir: &Path, rel: &str) -> PathBuf {
    inst_dir.join(".minecraft").join(rel)
}

fn persistent_config_path(inst_dir: &Path, rel: &str) -> PathBuf {
    persistent_minecraft_dir(inst_dir).join(rel)
}

fn copy_bundled_to_game(inst_dir: &Path, rel: &str) -> Result<(), String> {
    let src = bundled_config_path(rel);
    if !src.is_file() {
        return Err(format!("bundled preset file missing: {}", src.display()));
    }
    let dest = game_config_path(inst_dir, rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&src, &dest).map_err(|e| {
        format!(
            "failed to copy preset {} -> {}: {e}",
            src.display(),
            dest.display()
        )
    })?;
    Ok(())
}

fn copy_game_to_persistent(inst_dir: &Path, rel: &str) -> Result<(), String> {
    let src = game_config_path(inst_dir, rel);
    if !src.is_file() {
        return Ok(());
    }
    let dest = persistent_config_path(inst_dir, rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&src, &dest).map_err(|e| {
        format!(
            "failed to persist preset {} -> {}: {e}",
            src.display(),
            dest.display()
        )
    })?;
    Ok(())
}

fn replace_in_config_file(path: &Path, from: &str, to: &str) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read config {}: {e}", path.display()))?;
    if !content.contains(from) {
        return Ok(());
    }
    let updated = content.replace(from, to);
    fs::write(path, updated).map_err(|e| format!("failed to write config {}: {e}", path.display()))
}

fn apply_gadomancy_patch(inst_dir: &Path) -> Result<(), String> {
    const GADOMANCY_FALSE: &str = "B:ancientStoneRecipes=false";
    const GADOMANCY_TRUE: &str = "B:ancientStoneRecipes=true";
    replace_in_config_file(
        &game_config_path(inst_dir, "config/gadomancy.cfg"),
        GADOMANCY_FALSE,
        GADOMANCY_TRUE,
    )?;
    Ok(())
}

fn remove_persistent_file(inst_dir: &Path, rel: &str) -> Result<(), String> {
    let path = persistent_config_path(inst_dir, rel);
    if path.is_file() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn apply_aditya_preset(inst_dir: &Path, enabled: bool) -> Result<(), String> {
    if enabled {
        for rel in ADITYA_BUNDLED_FILES {
            copy_bundled_to_game(inst_dir, rel)?;
        }
        apply_gadomancy_patch(inst_dir)?;
        for rel in ADITYA_BUNDLED_FILES {
            copy_game_to_persistent(inst_dir, rel)?;
        }
        if game_config_path(inst_dir, "config/gadomancy.cfg").is_file() {
            apply_gadomancy_patch(inst_dir)?;
            copy_game_to_persistent(inst_dir, "config/gadomancy.cfg")?;
        }
        return Ok(());
    }

    for rel in ADITYA_BUNDLED_FILES {
        remove_persistent_file(inst_dir, rel)?;
    }
    remove_persistent_file(inst_dir, "config/gadomancy.cfg")?;
    Ok(())
}

pub fn aditya_preset_file_paths() -> Vec<String> {
    let mut paths: Vec<String> = ADITYA_BUNDLED_FILES
        .iter()
        .map(|p| (*p).to_string())
        .collect();
    paths.push("config/gadomancy.cfg".to_string());
    paths
}

pub fn is_aditya_preset_active(inst_dir: &Path) -> bool {
    let overlay = persistent_minecraft_dir(inst_dir);
    aditya_preset_file_paths()
        .iter()
        .any(|rel| overlay.join(rel).is_file())
}

#[tauri::command]
pub fn apply_config_preset(id: String, instance_id: String, enabled: bool) -> Result<(), String> {
    let inst_dir = crate::instance_dir(&crate::sanitize_name(instance_id.trim()));
    match id.as_str() {
        "aditya" => apply_aditya_preset(&inst_dir, enabled),
        other => Err(format!("unknown config preset: {other}")),
    }
}

#[tauri::command]
pub fn get_config_preset_status(id: String, instance_id: String) -> Result<bool, String> {
    let inst_dir = crate::instance_dir(&crate::sanitize_name(instance_id.trim()));
    match id.as_str() {
        "aditya" => Ok(is_aditya_preset_active(&inst_dir)),
        other => Err(format!("unknown config preset: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_instance_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("industrialis-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn apply_aditya_preset_copies_bundled_configs() {
        let inst = temp_instance_dir("aditya-apply");
        fs::create_dir_all(inst.join(".minecraft/config")).unwrap();

        apply_aditya_preset(&inst, true).expect("apply should succeed");

        assert!(inst.join(".minecraft/config/salisarcana.cfg").is_file());
        assert!(inst
            .join("persistent-minecraft/config/salisarcana.cfg")
            .is_file());
        assert!(is_aditya_preset_active(&inst));

        let _ = fs::remove_dir_all(&inst);
    }

    #[test]
    fn disable_aditya_preset_removes_persistent_overrides() {
        let inst = temp_instance_dir("aditya-disable");
        fs::create_dir_all(inst.join(".minecraft/config")).unwrap();

        apply_aditya_preset(&inst, true).expect("apply should succeed");
        apply_aditya_preset(&inst, false).expect("disable should succeed");

        assert!(!is_aditya_preset_active(&inst));

        let _ = fs::remove_dir_all(&inst);
    }

    #[test]
    fn gadomancy_patch_enables_ancient_stone_recipes() {
        let inst = temp_instance_dir("gadomancy-patch");
        let gad = inst.join(".minecraft/config/gadomancy.cfg");
        fs::create_dir_all(gad.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&gad).unwrap();
        writeln!(f, "B:ancientStoneRecipes=false").unwrap();

        apply_gadomancy_patch(&inst).unwrap();
        let content = fs::read_to_string(gad).unwrap();
        assert!(content.contains("B:ancientStoneRecipes=true"));

        let _ = fs::remove_dir_all(&inst);
    }
}
