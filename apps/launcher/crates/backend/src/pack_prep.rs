//! Launch-time pack preparation: nested-pack flattening, config seeding,
//! and persistent-overlay restores. Ports the corresponding helpers from
//! `pack.ts` / `minecraft-files.ts` (full install/update flows arrive later).

use std::path::{Path, PathBuf};

use crate::fs_utils::{copy_tree, exists};

pub fn persistent_custom_mods_dir(instance: &Path) -> PathBuf {
    instance.join("persistent-minecraft").join("mods")
}

pub fn persistent_minecraft_dir(instance: &Path) -> PathBuf {
    instance.join("persistent-minecraft")
}

pub fn minecraft_game_dir(instance: &Path) -> PathBuf {
    instance.join(".minecraft")
}

const OVERLAY_EXCLUDED_PREFIXES: [&str; 5] =
    ["saves/", "assets/", "logs/", "crash-reports/", "mods/"];

fn is_overlay_excluded(rel: &str) -> bool {
    let normalized = rel.replace('\\', "/").to_lowercase();
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    OVERLAY_EXCLUDED_PREFIXES.iter().any(|prefix| {
        let bare = prefix.trim_end_matches('/');
        normalized == bare || normalized.starts_with(prefix)
    })
}

/// Move `source` into `destination_root/<basename>`, merging directories and
/// falling back to copy when rename crosses devices. Mirrors `moveInto`.
async fn move_into(destination_root: &Path, source: &Path) -> Result<(), anyhow::Error> {
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid source path"))?;
    let destination = destination_root.join(file_name);
    if !exists(source).await {
        return Ok(());
    }
    if exists(&destination).await {
        let (src_meta, dst_meta) = (
            tokio::fs::symlink_metadata(source).await?,
            tokio::fs::symlink_metadata(&destination).await?,
        );
        if src_meta.is_dir() && dst_meta.is_dir() {
            let mut entries = tokio::fs::read_dir(source).await?;
            while let Some(entry) = entries.next_entry().await? {
                Box::pin(move_into(&destination, &entry.path())).await?;
            }
            tokio::fs::remove_dir_all(source).await.ok();
            return Ok(());
        }
        if dst_meta.is_dir() {
            tokio::fs::remove_dir_all(&destination).await?;
        } else {
            tokio::fs::remove_file(&destination).await?;
        }
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::rename(source, &destination).await.is_err() {
        copy_tree(source, &destination).await?;
        if tokio::fs::symlink_metadata(source)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            tokio::fs::remove_dir_all(source).await?;
        } else {
            tokio::fs::remove_file(source).await?;
        }
    }
    Ok(())
}

/// Mirrors `flattenNestedPack`: hoist the first (sorted) nested directory
/// containing `mmc-pack.json` when the instance root lacks one.
pub async fn flatten_nested_pack(instance: &Path) -> Result<(), anyhow::Error> {
    if exists(&instance.join("mmc-pack.json")).await {
        return Ok(());
    }
    let mut candidates = Vec::new();
    let mut entries = match tokio::fs::read_dir(instance).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if tokio::fs::symlink_metadata(&path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
            && exists(&path.join("mmc-pack.json")).await
        {
            candidates.push(path);
        }
    }
    candidates.sort();
    let Some(source) = candidates.into_iter().next() else {
        return Ok(());
    };
    let mut entries = tokio::fs::read_dir(&source).await?;
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        names.push(entry.file_name());
    }
    for name in names {
        move_into(instance, &source.join(name)).await?;
    }
    tokio::fs::remove_dir_all(&source).await.ok();
    if !exists(&instance.join("mmc-pack.json")).await {
        anyhow::bail!("failed to flatten instance pack in {}", instance.display());
    }
    Ok(())
}

/// Copy `instance/config` → `.minecraft/config` without overwriting unless
/// asked. Mirrors `seedPackConfigs`.
pub async fn seed_pack_configs(instance: &Path, overwrite: bool) -> Result<(), anyhow::Error> {
    let source = instance.join("config");
    if !exists(&source).await {
        return Ok(());
    }
    let destination = minecraft_game_dir(instance).join("config");
    visit_copy(&source, &destination, overwrite).await
}

async fn visit_copy(from: &Path, to: &Path, overwrite: bool) -> Result<(), anyhow::Error> {
    // Iterative to avoid async-recursion boxing.
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        tokio::fs::create_dir_all(&to).await?;
        let mut entries = tokio::fs::read_dir(&from).await?;
        while let Some(entry) = entries.next_entry().await? {
            let source_path = entry.path();
            let dest_path = to.join(entry.file_name());
            if tokio::fs::symlink_metadata(&source_path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                stack.push((source_path, dest_path));
            } else if overwrite || !exists(&dest_path).await {
                tokio::fs::copy(&source_path, &dest_path).await?;
            }
        }
    }
    Ok(())
}

/// Mirrors `prepareInstanceConfigs` (config seed + gadomancy tweak).
pub async fn prepare_instance_configs(
    instance: &Path,
    overwrite: bool,
) -> Result<(), anyhow::Error> {
    seed_pack_configs(instance, overwrite).await?;
    for rel in ["config/gadomancy.cfg", ".minecraft/config/gadomancy.cfg"] {
        let target = instance.join(rel);
        if exists(&target).await {
            let content = tokio::fs::read_to_string(&target).await?;
            tokio::fs::write(
                &target,
                content.replace("B:ancientStoneRecipes=false", "B:ancientStoneRecipes=true"),
            )
            .await?;
        }
    }
    Ok(())
}

/// Copy every file from the persistent-mods overlay into the active mods dir.
/// Returns the restored count. Mirrors `applyPersistentCustomMods`.
pub async fn apply_persistent_custom_mods(instance: &Path) -> Result<usize, anyhow::Error> {
    let persistent = persistent_custom_mods_dir(instance);
    if !exists(&persistent).await {
        return Ok(0);
    }
    let active = minecraft_game_dir(instance).join("mods");
    tokio::fs::create_dir_all(&active).await?;
    let mut restored = 0;
    let mut entries = tokio::fs::read_dir(&persistent).await?;
    while let Some(entry) = entries.next_entry().await? {
        if tokio::fs::symlink_metadata(entry.path())
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            tokio::fs::copy(entry.path(), active.join(entry.file_name())).await?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// Copy the persistent-minecraft overlay into `.minecraft`, skipping `mods`
/// and the excluded prefixes. Mirrors `applyPersistentMinecraft`.
pub async fn apply_persistent_minecraft(instance: &Path) -> Result<(), anyhow::Error> {
    let overlay = persistent_minecraft_dir(instance);
    if !exists(&overlay).await {
        return Ok(());
    }
    let target_root = minecraft_game_dir(instance);
    let mut stack = vec![overlay.clone()];
    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let child = entry.path();
            let rel = child
                .strip_prefix(&overlay)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let meta = tokio::fs::symlink_metadata(&child).await?;
            if meta.is_dir() {
                if rel == "mods" || is_overlay_excluded(&format!("{rel}/")) {
                    continue;
                }
                stack.push(child);
            } else if !is_overlay_excluded(&rel) {
                let target = target_root.join(&rel);
                if let Some(parent) = target.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::copy(&child, &target).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_instance(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-prep-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn flattens_nested_pack_layout() {
        let instance = temp_instance("flatten");
        let nested = instance.join("GTNH-2.8.0");
        tokio::fs::create_dir_all(&nested).await.unwrap();
        tokio::fs::write(nested.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(nested.join("patches.json"), "{}")
            .await
            .unwrap();
        flatten_nested_pack(&instance).await.unwrap();
        assert!(instance.join("mmc-pack.json").exists());
        assert!(!nested.exists());
    }

    #[tokio::test]
    async fn seeds_configs_without_overwriting() {
        let instance = temp_instance("seed");
        tokio::fs::create_dir_all(instance.join("config"))
            .await
            .unwrap();
        tokio::fs::write(instance.join("config").join("a.cfg"), "pack")
            .await
            .unwrap();
        let active = instance.join(".minecraft").join("config");
        tokio::fs::create_dir_all(&active).await.unwrap();
        tokio::fs::write(active.join("a.cfg"), "user")
            .await
            .unwrap();
        prepare_instance_configs(&instance, false).await.unwrap();
        assert_eq!(
            tokio::fs::read_to_string(active.join("a.cfg"))
                .await
                .unwrap(),
            "user"
        );
        prepare_instance_configs(&instance, true).await.unwrap();
        assert_eq!(
            tokio::fs::read_to_string(active.join("a.cfg"))
                .await
                .unwrap(),
            "pack"
        );
    }

    #[tokio::test]
    async fn restores_persistent_overlays_selectively() {
        let instance = temp_instance("overlay");
        let persistent = instance.join("persistent-minecraft");
        tokio::fs::create_dir_all(persistent.join("mods"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(persistent.join("config"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(persistent.join("saves"))
            .await
            .unwrap();
        tokio::fs::write(persistent.join("mods").join("custom.jar"), "mod")
            .await
            .unwrap();
        tokio::fs::write(persistent.join("config").join("c.cfg"), "cfg")
            .await
            .unwrap();
        tokio::fs::write(persistent.join("saves").join("world.dat"), "save")
            .await
            .unwrap();
        assert_eq!(apply_persistent_custom_mods(&instance).await.unwrap(), 1);
        apply_persistent_minecraft(&instance).await.unwrap();
        let game = instance.join(".minecraft");
        assert!(game.join("mods").join("custom.jar").exists());
        assert!(game.join("config").join("c.cfg").exists());
        // `saves/` is overlay-excluded; `mods/` is restored by the mods pass only.
        assert!(!game.join("saves").exists());
    }
}
