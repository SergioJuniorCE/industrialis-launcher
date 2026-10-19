//! Guarded custom-mod management on the original persistent overlay.
use crate::{
    pack_files::{self, ModEntry},
    paths::LauncherPaths,
};
use std::path::Path;
pub async fn list(paths: &LauncherPaths, id: &str) -> anyhow::Result<Vec<ModEntry>> {
    let _lease = crate::lifecycle::admit(paths)?;
    let instance = paths.instance_dir(id)?;
    if instance.join("persistent-minecraft/mods").exists() {
        crate::minecraft_files::scoped(paths, id, "persistent-minecraft", "mods", false)?;
    }
    pack_files::list_custom_mods(&instance).await
}
pub async fn add(paths: &LauncherPaths, id: &str, source: &Path) -> anyhow::Result<ModEntry> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let filename = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Mod filename is invalid"))?
        .to_string();
    let lower = filename.to_lowercase();
    anyhow::ensure!(
        lower.ends_with(".jar") || lower.ends_with(".zip"),
        "Only .jar and .zip mods are supported"
    );
    crate::minecraft_files::sanitize_rel_path(&filename)?;
    let paths = paths.clone();
    let id = id.to_string();
    let source = source.to_path_buf();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let _lease = lease;
            let file = std::fs::File::open(source)?;
            anyhow::ensure!(file.metadata()?.is_file(), "Mod file not found");
            const LIMIT: u64 = 512 * 1024 * 1024;
            anyhow::ensure!(
                file.metadata()?.len() <= LIMIT,
                "Mod exceeds the 512 MB import limit"
            );
            let mut raw = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut raw)?;
            anyhow::ensure!(
                raw.len() as u64 <= LIMIT,
                "Mod exceeds the 512 MB import limit"
            );
            let rel = format!("mods/{filename}");
            let persistent =
                crate::minecraft_files::scoped(&paths, &id, "persistent-minecraft", &rel, true)?;
            let active = crate::minecraft_files::scoped(&paths, &id, ".minecraft", &rel, true)?;
            let old = match std::fs::read(&persistent) {
                Ok(raw) => Some(raw),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            crate::fs_utils::write_atomic(&persistent, &raw)?;
            if let Err(e) = crate::fs_utils::write_atomic(&active, &raw) {
                match old {
                    Some(old) => crate::fs_utils::write_atomic(&persistent, &old)?,
                    None => std::fs::remove_file(persistent)?,
                };
                return Err(e);
            }
            Ok(ModEntry {
                identity: pack_files::mod_identity_from_filename(&filename),
                filename,
                size_bytes: raw.len() as u64,
            })
        })
        .await?
    })
    .await
}
pub async fn remove(paths: &LauncherPaths, id: &str, identity: &str) -> anyhow::Result<()> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    anyhow::ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing mods"
    );
    let normalized = identity.trim().to_lowercase();
    let entries = list(paths, id).await?;
    anyhow::ensure!(
        entries.iter().any(|entry| entry.identity == normalized),
        "Custom mod not found"
    );
    let instance = paths.instance_dir(id)?;
    let mut targets = Vec::new();
    for root in ["persistent-minecraft", ".minecraft"] {
        let directory = instance.join(root).join("mods");
        if !directory.exists() {
            continue;
        }
        crate::minecraft_files::scoped(paths, id, root, "mods", false)?;
        let mut files = tokio::fs::read_dir(&directory).await?;
        while let Some(file) = files.next_entry().await? {
            let name = file.file_name().to_string_lossy().to_string();
            if pack_files::mod_identity_from_filename(&name) == normalized {
                let path = crate::minecraft_files::scoped(
                    paths,
                    id,
                    root,
                    &format!("mods/{name}"),
                    false,
                )?;
                let meta = tokio::fs::symlink_metadata(&path).await?;
                anyhow::ensure!(
                    meta.is_file() && !meta.permissions().readonly(),
                    "A custom mod cannot be removed; its files have been kept"
                );
                targets.push(path);
                if root == "persistent-minecraft" {
                    let receipt_rel = format!(".industrialis-content/mods/{name}.json");
                    match crate::minecraft_files::scoped(paths, id, root, &receipt_rel, false) {
                        Ok(receipt) => {
                            let meta = tokio::fs::symlink_metadata(&receipt).await?;
                            anyhow::ensure!(
                                meta.is_file() && !meta.permissions().readonly(),
                                "The mod receipt is read-only; its files have been kept"
                            );
                            targets.push(receipt);
                        }
                        Err(e)
                            if e.downcast_ref::<std::io::Error>()
                                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let mut staged: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
            for target in targets {
                let pending = target
                    .with_file_name(format!(".industrialis-mod-{}.tmp", uuid::Uuid::new_v4()));
                if let Err(error) = std::fs::rename(&target, &pending) {
                    for (original, pending) in staged.into_iter().rev() {
                        std::fs::rename(pending, original)?;
                    }
                    return Err(error.into());
                }
                staged.push((target, pending));
            }
            // Once all names have been removed, disposal can continue to completion
            // even if the UI task is cancelled. An interrupted disposal leaves a
            // .tmp file, never a loadable partially removed jar.
            for (_, pending) in staged {
                std::fs::remove_file(pending)?;
            }
            Ok(())
        })
        .await?
    })
    .await
}

pub async fn open_folder(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    let probe = crate::minecraft_files::scoped(
        paths,
        id,
        "persistent-minecraft",
        "mods/.folder-probe",
        true,
    )?;
    let dir = probe.parent().unwrap().to_path_buf();
    tokio::task::spawn_blocking(move || open::that(dir)).await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn management_preserves_pack_mods_and_failed_removal_keeps_both_copies() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-mods-{}", uuid::Uuid::new_v4())),
        );
        let instance = paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(instance.join(".minecraft/mods")).unwrap();
        let source = paths.base().join("CustomMod-1.0.jar");
        std::fs::write(&source, b"custom").unwrap();
        std::fs::write(instance.join(".minecraft/mods/pack-only.jar"), b"pack").unwrap();
        let entry = add(&paths, "pack", &source).await.unwrap();
        assert_eq!(
            list(&paths, "pack").await.unwrap()[0].identity,
            entry.identity
        );
        let active = instance.join(".minecraft/mods/CustomMod-1.0.jar");
        let persistent = instance.join("persistent-minecraft/mods/CustomMod-1.0.jar");
        let mut permissions = std::fs::metadata(&active).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&active, permissions).unwrap();
        assert!(remove(&paths, "pack", &entry.identity).await.is_err());
        assert_eq!(std::fs::read(&active).unwrap(), b"custom");
        assert_eq!(std::fs::read(&persistent).unwrap(), b"custom");
        let mut permissions = std::fs::metadata(&active).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&active, permissions).unwrap();
        remove(&paths, "pack", &entry.identity).await.unwrap();
        assert!(list(&paths, "pack").await.unwrap().is_empty());
        assert!(!active.exists());
        assert!(!persistent.exists());
        assert_eq!(
            std::fs::read(instance.join(".minecraft/mods/pack-only.jar")).unwrap(),
            b"pack"
        );
        assert!(remove(&paths, "pack", "pack-only").await.is_err());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
