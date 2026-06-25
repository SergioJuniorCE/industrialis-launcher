use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const OVERLAY_EXCLUDED_PREFIXES: &[&str] = &["saves/", "assets/", "logs/", "crash-reports/", "mods/"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftDirEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub has_persistent_override: bool,
    pub editable: bool,
}

pub fn persistent_minecraft_dir(inst_dir: &Path) -> PathBuf {
    inst_dir.join("persistent-minecraft")
}

pub fn minecraft_game_dir(inst_dir: &Path) -> PathBuf {
    inst_dir.join(".minecraft")
}

pub fn sanitize_minecraft_rel_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Ok(PathBuf::new());
    }
    let mut out = PathBuf::new();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => return Err("invalid path".into()),
            Component::RootDir | Component::Prefix(_) => return Err("invalid path".into()),
            Component::CurDir => {}
        }
    }
    Ok(out)
}

pub fn is_overlay_excluded(rel: &str) -> bool {
    let norm = rel.replace('\\', "/").to_lowercase();
    let norm = norm.strip_prefix("./").unwrap_or(&norm);
    OVERLAY_EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| norm == prefix.trim_end_matches('/') || norm.starts_with(prefix))
}

pub fn is_path_editable(rel: &str) -> bool {
    if is_overlay_excluded(rel) {
        return false;
    }
    let lower = rel.to_lowercase();
    !lower.ends_with(".jar")
        && !lower.ends_with(".zip")
        && !lower.ends_with(".png")
        && !lower.ends_with(".jpg")
}

fn persistent_paths_set(inst_dir: &Path) -> Result<std::collections::HashSet<String>, String> {
    let mut set = std::collections::HashSet::new();
    let overlay = persistent_minecraft_dir(inst_dir);
    if !overlay.is_dir() {
        return Ok(set);
    }
    collect_rel_paths(&overlay, &overlay, &mut set)?;
    Ok(set)
}

fn collect_rel_paths(base: &Path, dir: &Path, out: &mut std::collections::HashSet<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            collect_rel_paths(base, &path, out)?;
        } else {
            out.insert(rel);
        }
    }
    Ok(())
}

pub fn list_minecraft_entries(
    inst_dir: &Path,
    subpath: &str,
) -> Result<Vec<MinecraftDirEntry>, String> {
    let rel = sanitize_minecraft_rel_path(subpath)?;
    let mc = minecraft_game_dir(inst_dir);
    let dir = if rel.as_os_str().is_empty() {
        mc.clone()
    } else {
        mc.join(&rel)
    };
    if !dir.exists() {
        return Ok(vec![]);
    }
    if !dir.is_dir() {
        return Err("not a directory".into());
    }

    let persistent = persistent_paths_set(inst_dir)?;
    let mut entries = vec![];
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_rel = if rel.as_os_str().is_empty() {
            name.clone()
        } else {
            format!("{}/{}", rel.to_string_lossy(), name)
        };
        let norm_rel = entry_rel.replace('\\', "/");
        let is_dir = entry.path().is_dir();
        entries.push(MinecraftDirEntry {
            name,
            rel_path: norm_rel.clone(),
            is_dir,
            has_persistent_override: !is_dir && persistent.contains(&norm_rel),
            editable: !is_dir && is_path_editable(&norm_rel),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn read_minecraft_file(inst_dir: &Path, rel_path: &str) -> Result<String, String> {
    let rel = sanitize_minecraft_rel_path(rel_path)?;
    if is_overlay_excluded(rel_path) {
        return Err("path is not readable".into());
    }
    if !is_path_editable(rel_path) {
        return Err("file is not editable".into());
    }
    let path = minecraft_game_dir(inst_dir).join(&rel);
    if !path.is_file() {
        return Err("file not found".into());
    }
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_READ_BYTES {
        return Err("file too large to edit in launcher".into());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

pub fn write_minecraft_file(
    inst_dir: &Path,
    rel_path: &str,
    content: &str,
    persist: bool,
) -> Result<(), String> {
    let rel = sanitize_minecraft_rel_path(rel_path)?;
    let norm = rel.to_string_lossy().replace('\\', "/");
    if is_overlay_excluded(&norm) {
        return Err("path is not writable".into());
    }
    if !is_path_editable(&norm) {
        return Err("file type is not editable".into());
    }

    let mc_path = minecraft_game_dir(inst_dir).join(&rel);
    if let Some(parent) = mc_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&mc_path, content).map_err(|e| e.to_string())?;

    if persist {
        let overlay_path = persistent_minecraft_dir(inst_dir).join(&rel);
        if let Some(parent) = overlay_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&overlay_path, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn delete_persistent_file(inst_dir: &Path, rel_path: &str) -> Result<(), String> {
    let rel = sanitize_minecraft_rel_path(rel_path)?;
    let overlay_path = persistent_minecraft_dir(inst_dir).join(&rel);
    if overlay_path.is_file() {
        fs::remove_file(&overlay_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn list_persistent_files(inst_dir: &Path) -> Result<Vec<String>, String> {
    let overlay = persistent_minecraft_dir(inst_dir);
    let mut set = std::collections::HashSet::new();
    if overlay.is_dir() {
        collect_rel_paths(&overlay, &overlay, &mut set)?;
    }
    let mut list: Vec<String> = set.into_iter().collect();
    list.sort();
    Ok(list)
}

pub fn apply_persistent_minecraft(inst_dir: &Path) -> Result<(), String> {
    let overlay = persistent_minecraft_dir(inst_dir);
    if !overlay.is_dir() {
        return Ok(());
    }
    let mc = minecraft_game_dir(inst_dir);
    apply_overlay_tree(&overlay, &overlay, &mc)
}

fn apply_overlay_tree(base: &Path, dir: &Path, mc_root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            if rel == "mods" {
                continue;
            }
            if is_overlay_excluded(&format!("{rel}/")) {
                continue;
            }
            apply_overlay_tree(base, &path, mc_root)?;
        } else {
            if is_overlay_excluded(&rel) {
                continue;
            }
            let dest = mc_root.join(&rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&path, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_parent_dir() {
        assert!(sanitize_minecraft_rel_path("../secret").is_err());
    }

    #[test]
    fn overlay_excludes_saves() {
        assert!(is_overlay_excluded("saves/world1/level.dat"));
        assert!(!is_overlay_excluded("config/gadomancy.cfg"));
    }
}