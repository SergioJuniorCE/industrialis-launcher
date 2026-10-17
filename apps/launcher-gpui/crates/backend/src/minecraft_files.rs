//! Minecraft file browsing and the existing persistent-minecraft overlay.
//! All paths are instance-relative; saves and binary assets stay read-only.
use crate::{fs_utils::write_atomic, paths::LauncherPaths};
use anyhow::{ensure, Context};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct MinecraftEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub has_persistent_override: bool,
    pub editable: bool,
    pub too_large_to_edit: bool,
}
pub fn sanitize_rel_path(input: &str) -> anyhow::Result<String> {
    let input = input.trim().replace('\\', "/");
    ensure!(
        !input.starts_with('/') && !input.contains('\0'),
        "Invalid path"
    );
    let mut parts = Vec::new();
    for part in input.split('/') {
        ensure!(
            part != ".."
                && !part.contains(':')
                && !part.ends_with(' ')
                && (part == "." || !part.ends_with('.')),
            "Invalid path"
        );
        if !part.is_empty() && part != "." {
            parts.push(part);
        }
    }
    Ok(parts.join("/"))
}
pub fn is_overlay_excluded(rel: &str) -> bool {
    let rel = rel.replace('\\', "/").to_lowercase();
    ["saves", "assets", "logs", "crash-reports", "mods"]
        .iter()
        .any(|prefix| rel == *prefix || rel.starts_with(&format!("{prefix}/")))
}
pub fn is_path_editable(rel: &str) -> bool {
    !is_overlay_excluded(rel)
        && !["jar", "zip", "png", "jpg"].contains(
            &Path::new(rel)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_lowercase()
                .as_str(),
        )
}
fn editable(rel: &str) -> anyhow::Result<String> {
    let rel = sanitize_rel_path(rel)?;
    ensure!(
        !rel.is_empty() && is_path_editable(&rel),
        "File type is not editable"
    );
    Ok(rel)
}
/// Check every existing component, including the instance and root. A missing
/// leaf is allowed for writing, but links/reparse points cannot redirect it.
pub(crate) fn scoped(
    paths: &LauncherPaths,
    id: &str,
    root: &str,
    rel: &str,
    create: bool,
) -> anyhow::Result<PathBuf> {
    let instance = paths.instance_dir(id)?;
    ensure!(
        std::fs::symlink_metadata(&instance)?.is_dir(),
        "Instance not found"
    );
    ensure!(
        !std::fs::symlink_metadata(&instance)?.is_symlink(),
        "Linked instance directories are not supported by the file editor"
    );
    let mut current = instance;
    let parts: Vec<_> = std::iter::once(root)
        .chain(rel.split('/').filter(|s| !s.is_empty()))
        .collect();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                ensure!(
                    !meta.is_symlink(),
                    "Linked paths cannot be edited: {}",
                    current.display()
                );
                if index + 1 < parts.len() {
                    ensure!(meta.is_dir(), "Parent is not a directory");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if create && index + 1 < parts.len() {
                    std::fs::create_dir(&current)?;
                } else if !create {
                    return Err(e.into());
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(current)
}
fn read_bounded(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "File not found");
    ensure!(
        file.metadata()?.len() <= limit,
        "File too large to edit in launcher"
    );
    let mut raw = Vec::new();
    file.take(limit + 1).read_to_end(&mut raw)?;
    ensure!(
        raw.len() as u64 <= limit,
        "File too large to edit in launcher"
    );
    Ok(raw)
}
pub async fn list_entries(
    paths: &LauncherPaths,
    id: &str,
    subpath: &str,
) -> anyhow::Result<Vec<MinecraftEntry>> {
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    let rel = sanitize_rel_path(subpath)?;
    tokio::task::spawn_blocking(move || {
        let _lease = lease;
        let dir = match scoped(&paths, &id, ".minecraft", &rel, false) {
            Ok(dir) => dir,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(Vec::new())
            }
            Err(e) => return Err(e),
        };
        ensure!(dir.is_dir(), "Not a directory");
        let mut result = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let meta = std::fs::symlink_metadata(entry.path())?;
            let linked = entry.file_type()?.is_symlink();
            let is_dir = meta.is_dir() && !linked;
            let editable = !is_dir && !linked && is_path_editable(&child_rel);
            let persistent = scoped(&paths, &id, "persistent-minecraft", &child_rel, false)
                .ok()
                .is_some_and(|p| p.is_file());
            result.push(MinecraftEntry {
                name,
                rel_path: child_rel,
                is_dir,
                editable,
                has_persistent_override: persistent,
                too_large_to_edit: editable && meta.len() > MAX_READ_BYTES,
            });
        }
        result.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(result)
    })
    .await?
}
pub async fn read_file(paths: &LauncherPaths, id: &str, rel: &str) -> anyhow::Result<String> {
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    let rel = editable(rel)?;
    tokio::task::spawn_blocking(move || {
        let _lease = lease;
        String::from_utf8(read_bounded(
            &scoped(&paths, &id, ".minecraft", &rel, false)?,
            MAX_READ_BYTES,
        )?)
        .context("File is not UTF-8 text")
    })
    .await?
}
fn write_pair(
    paths: &LauncherPaths,
    id: &str,
    rel: &str,
    raw: &[u8],
    persist: bool,
    expected: Option<&str>,
) -> anyhow::Result<()> {
    let game = scoped(paths, id, ".minecraft", rel, true)?;
    if let Some(expected) = expected {
        ensure!(read_bounded(&game, MAX_READ_BYTES)? == expected.as_bytes(), "The file changed outside this editor. Reload it before saving; your draft has been kept.");
    }
    if persist {
        let overlay = scoped(paths, id, "persistent-minecraft", rel, true)?;
        let old = match std::fs::read(&overlay) {
            Ok(raw) => Some(raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        write_atomic(&overlay, raw)?;
        if let Err(error) = write_atomic(&game, raw) {
            let rollback = match old {
                Some(old) => write_atomic(&overlay, &old),
                None => std::fs::remove_file(&overlay).map_err(Into::into),
            };
            rollback.context(format!(
                "Could not save the game file ({error:#}) or restore the persistent copy"
            ))?;
            return Err(error);
        }
    } else {
        write_atomic(&game, raw)?;
    }
    Ok(())
}
pub async fn write_file(
    paths: &LauncherPaths,
    id: &str,
    rel: &str,
    content: String,
    persist: bool,
    expected: Option<String>,
) -> anyhow::Result<()> {
    ensure!(
        content.len() as u64 <= MAX_READ_BYTES,
        "File too large to edit in launcher"
    );
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    let paths = paths.clone();
    let id = id.to_owned();
    let rel = editable(rel)?;
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            write_pair(
                &paths,
                &id,
                &rel,
                content.as_bytes(),
                persist,
                expected.as_deref(),
            )
        })
        .await?
    })
    .await
}
pub async fn list_persistent_files(paths: &LauncherPaths, id: &str) -> anyhow::Result<Vec<String>> {
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        let _lease = lease;
        let root = match scoped(&paths, &id, "persistent-minecraft", "", false) {
            Ok(root) => root,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(Vec::new())
            }
            Err(e) => return Err(e),
        };
        let mut result = Vec::new();
        for entry in walkdir::WalkDir::new(&root).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_file() {
                result.push(
                    entry
                        .path()
                        .strip_prefix(&root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        result.sort();
        Ok(result)
    })
    .await?
}
pub async fn delete_persistent_file(
    paths: &LauncherPaths,
    id: &str,
    rel: &str,
) -> anyhow::Result<()> {
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    let rel = editable(rel)?;
    stop_external_watch(paths, id, &rel).await;
    let paths = paths.clone();
    let id = id.to_owned();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            match scoped(&paths, &id, "persistent-minecraft", &rel, false) {
                Ok(path) => {
                    ensure!(path.is_file(), "Override is not a file");
                    std::fs::remove_file(path)?;
                    Ok(())
                }
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    Ok(())
                }
                Err(e) => Err(e),
            }
        })
        .await?
    })
    .await
}
fn external_path(paths: &LauncherPaths, id: &str, rel: &str) -> anyhow::Result<PathBuf> {
    let rel = editable(rel)?;
    let ext = Path::new(&rel)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    ensure!(
        ![
            "exe", "com", "bat", "cmd", "msi", "msp", "scr", "ps1", "sh", "app", "lnk", "url",
            "desktop", "dll", "so", "dylib"
        ]
        .contains(&ext.as_str()),
        "Executable files cannot be opened from the file editor"
    );
    let target = scoped(paths, id, ".minecraft", &rel, false)?;
    ensure!(target.is_file(), "File not found");
    Ok(target)
}
struct Watch {
    paths: LauncherPaths,
    id: String,
    rel: String,
    stop: tokio::sync::Notify,
    stopped: std::sync::atomic::AtomicBool,
    done: tokio::sync::Notify,
    finished: std::sync::atomic::AtomicBool,
    error: Mutex<Option<String>>,
}
fn watches() -> &'static Mutex<HashMap<PathBuf, Arc<Watch>>> {
    static WATCHES: OnceLock<Mutex<HashMap<PathBuf, Arc<Watch>>>> = OnceLock::new();
    WATCHES.get_or_init(Default::default)
}
fn key(paths: &LauncherPaths, id: &str, rel: &str) -> anyhow::Result<PathBuf> {
    let path = std::path::absolute(paths.instance_dir(id)?.join(".minecraft").join(rel))?;
    #[cfg(windows)]
    let path = PathBuf::from(path.to_string_lossy().to_lowercase());
    Ok(path)
}
async fn mirror(watch: &Watch) -> anyhow::Result<()> {
    let paths = watch.paths.clone();
    let id = watch.id.clone();
    let rel = watch.rel.clone();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let source = external_path(&paths, &id, &rel)?;
            let raw = match read_bounded(&source, 32 * 1024 * 1024) {
                Ok(raw) => raw,
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    return Ok(())
                }
                Err(e) => return Err(e),
            };
            let target = scoped(&paths, &id, "persistent-minecraft", &rel, true)?;
            if std::fs::read(&target).ok().as_deref() != Some(raw.as_slice()) {
                write_atomic(&target, &raw)?;
            }
            Ok(())
        })
        .await?
    })
    .await
}
pub async fn watch_external_file(paths: &LauncherPaths, id: &str, rel: &str) -> anyhow::Result<()> {
    use std::sync::atomic::Ordering;
    let lease = crate::lifecycle::admit(paths)?;
    let rel = editable(rel)?;
    let source = external_path(paths, id, &rel)?;
    let initial =
        tokio::task::spawn_blocking(move || read_bounded(&source, 32 * 1024 * 1024)).await??;
    let initial = Some(initial);
    let key = key(paths, id, &rel)?;
    let watch = Arc::new(Watch {
        paths: paths.clone(),
        id: id.into(),
        rel,
        stop: Default::default(),
        stopped: false.into(),
        done: Default::default(),
        finished: false.into(),
        error: Default::default(),
    });
    {
        let mut map = watches().lock().unwrap();
        if map.contains_key(&key) {
            return Ok(());
        }
        map.insert(key, watch.clone());
    }
    tokio::spawn(async move {
        let _lease = lease;
        let lifecycle = crate::lifecycle::for_profile(&watch.paths);
        // Poll the pathname, rather than an inode, so atomic saves by an OS editor
        // are observed on all platforms. The last bytes are mirrored on close.
        let mut previous = initial;
        loop {
            tokio::select! {
                _ = watch.stop.notified() => break,
                _ = lifecycle.closing() => { if let Err(e) = mirror(&watch).await { *watch.error.lock().unwrap() = Some(format!("{e:#}")); } break; },
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {},
            }
            if watch.stopped.load(Ordering::Acquire) {
                break;
            }
            let reader = watch.clone();
            let current = tokio::task::spawn_blocking(move || {
                external_path(&reader.paths, &reader.id, &reader.rel)
                    .ok()
                    .and_then(|p| read_bounded(&p, 32 * 1024 * 1024).ok())
            })
            .await
            .ok()
            .flatten();
            if current.is_some() && current != previous {
                match mirror(&watch).await {
                    Ok(()) => {
                        previous = current;
                        *watch.error.lock().unwrap() = None;
                    }
                    Err(e) => *watch.error.lock().unwrap() = Some(format!("{e:#}")),
                }
            }
        }
        watch.finished.store(true, Ordering::Release);
        watch.done.notify_waiters();
    });
    Ok(())
}
pub async fn open_external_editor(
    paths: &LauncherPaths,
    id: &str,
    rel: &str,
) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    let path = external_path(paths, id, rel)?;
    watch_external_file(paths, id, rel).await?;
    if let Err(e) = tokio::task::spawn_blocking(move || open::that(path)).await? {
        stop_external_watch(paths, id, rel).await;
        return Err(e.into());
    }
    Ok(())
}
pub fn watch_error(paths: &LauncherPaths, id: &str, rel: &str) -> Option<String> {
    watches()
        .lock()
        .unwrap()
        .get(&key(paths, id, rel).ok()?)
        .and_then(|watch| watch.error.lock().unwrap().clone())
}
pub async fn stop_external_watch(paths: &LauncherPaths, id: &str, rel: &str) {
    use std::sync::atomic::Ordering;
    let Ok(key) = key(paths, id, rel) else {
        return;
    };
    let watch = watches().lock().unwrap().remove(&key);
    if let Some(watch) = watch {
        watch.stopped.store(true, Ordering::Release);
        watch.stop.notify_one();
        loop {
            let notified = watch.done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if watch.finished.load(Ordering::Acquire) {
                break;
            }
            notified.await;
        }
    }
}
pub async fn flush_external_watches(paths: &LauncherPaths) -> anyhow::Result<()> {
    let pending: Vec<_> = watches()
        .lock()
        .unwrap()
        .values()
        .filter(|w| w.paths.base() == paths.base())
        .cloned()
        .collect();
    for watch in pending {
        if watch.error.lock().unwrap().is_some() {
            mirror(&watch).await?;
            *watch.error.lock().unwrap() = None;
        }
    }
    Ok(())
}
/// Freeze external editors before a pack is replaced/copied/deleted. Their last
/// save belongs to the old pack, never to the replacement's default config.
pub async fn stop_instance_watches(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let pending: Vec<_> = watches()
        .lock()
        .unwrap()
        .values()
        .filter(|w| w.paths.base() == paths.base() && w.id == id)
        .cloned()
        .collect();
    for watch in pending {
        mirror(&watch).await?;
        stop_external_watch(paths, id, &watch.rel).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> LauncherPaths {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-files-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(
            paths
                .instance_dir("pack")
                .unwrap()
                .join(".minecraft/config"),
        )
        .unwrap();
        paths
    }
    #[tokio::test]
    async fn maintenance_reservation_blocks_editor_save_and_override_removal() {
        let paths = fixture();
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config/a.cfg");
        std::fs::write(&game, "original").unwrap();
        write_file(&paths, "pack", "config/a.cfg", "saved".into(), true, None)
            .await
            .unwrap();
        let lease = crate::instance_guard::reserve_instances(&paths, &["pack"]).unwrap();
        assert!(write_file(
            &paths,
            "pack",
            "config/a.cfg",
            "racing edit".into(),
            true,
            None
        )
        .await
        .is_err());
        assert!(delete_persistent_file(&paths, "pack", "config/a.cfg")
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(&game).unwrap(), "saved");
        assert_eq!(
            std::fs::read_to_string(
                paths
                    .instance_dir("pack")
                    .unwrap()
                    .join("persistent-minecraft/config/a.cfg")
            )
            .unwrap(),
            "saved"
        );
        drop(lease);
        delete_persistent_file(&paths, "pack", "config/a.cfg")
            .await
            .unwrap();
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn browse_edit_preserve_remove_and_conflict_use_existing_files() {
        let paths = fixture();
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config/a.cfg");
        std::fs::write(&game, "original").unwrap();
        let list = list_entries(&paths, "pack", "config").await.unwrap();
        assert!(list[0].editable);
        assert!(!list[0].has_persistent_override);
        assert_eq!(
            read_file(&paths, "pack", "config/a.cfg").await.unwrap(),
            "original"
        );
        write_file(
            &paths,
            "pack",
            "config/a.cfg",
            "edited".into(),
            true,
            Some("original".into()),
        )
        .await
        .unwrap();
        let overlay = paths
            .instance_dir("pack")
            .unwrap()
            .join("persistent-minecraft/config/a.cfg");
        assert_eq!(std::fs::read_to_string(&overlay).unwrap(), "edited");
        assert_eq!(
            list_persistent_files(&paths, "pack").await.unwrap(),
            vec!["config/a.cfg"]
        );
        std::fs::write(&game, "external").unwrap();
        assert!(write_file(
            &paths,
            "pack",
            "config/a.cfg",
            "draft".into(),
            true,
            Some("edited".into())
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_to_string(&game).unwrap(), "external");
        assert_eq!(std::fs::read_to_string(&overlay).unwrap(), "edited");
        let mut permissions = std::fs::metadata(&game).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&game, permissions).unwrap();
        assert!(write_file(
            &paths,
            "pack",
            "config/a.cfg",
            "draft".into(),
            true,
            Some("external".into())
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_to_string(&overlay).unwrap(), "edited");
        let mut permissions = std::fs::metadata(&game).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&game, permissions).unwrap();
        delete_persistent_file(&paths, "pack", "config/a.cfg")
            .await
            .unwrap();
        assert!(!overlay.exists());
        assert!(game.exists());
        for rel in [
            "../a",
            "C:/a",
            "/a",
            "config/a:stream",
            "saves/world/data",
            "mods/a.jar",
            "a.png",
        ] {
            assert!(write_file(&paths, "pack", rel, "bad".into(), true, None)
                .await
                .is_err());
        }
        std::fs::write(game.with_file_name("run.cmd"), "echo harmless").unwrap();
        assert!(external_path(&paths, "pack", "config/run.cmd").is_err());
        std::fs::write(
            game.with_file_name("large.cfg"),
            vec![b'x'; MAX_READ_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_file(&paths, "pack", "config/large.cfg").await.is_err());
        assert!(list_entries(&paths, "pack", "config")
            .await
            .unwrap()
            .iter()
            .any(|entry| entry.too_large_to_edit));
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn atomic_external_saves_mirror_and_shutdown_drains_final_edit() {
        let paths = fixture();
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config/a.cfg");
        std::fs::write(&game, "original").unwrap();
        watch_external_file(&paths, "pack", "config/a.cfg")
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        write_atomic(&game, b"atomic save").unwrap();
        let overlay = paths
            .instance_dir("pack")
            .unwrap()
            .join("persistent-minecraft/config/a.cfg");
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if std::fs::read_to_string(&overlay).ok().as_deref() == Some("atomic save") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        write_atomic(&game, b"last save before close").unwrap();
        crate::lifecycle::for_profile(&paths)
            .shutdown(&paths)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&overlay).unwrap(),
            "last save before close"
        );
        stop_external_watch(&paths, "pack", "config/a.cfg").await;
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn removing_override_stops_mirroring_without_recreating_it() {
        let paths = fixture();
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config/a.cfg");
        std::fs::write(&game, "original").unwrap();
        watch_external_file(&paths, "pack", "config/a.cfg")
            .await
            .unwrap();
        write_file(&paths, "pack", "config/a.cfg", "edited".into(), true, None)
            .await
            .unwrap();
        delete_persistent_file(&paths, "pack", "config/a.cfg")
            .await
            .unwrap();
        write_atomic(&game, b"later").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        assert!(list_persistent_files(&paths, "pack")
            .await
            .unwrap()
            .is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
