//! Port of the JSON/size helpers in `fs-utils.ts`.
//!
//! Atomic JSON replacement, tolerant reads for cache/legacy callers, and
//! directory sizing/copy. Profile reads use `profile_store` to report corruption.

use anyhow::Context;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Mirrors `exists`.
pub async fn exists(target: &Path) -> bool {
    tokio::fs::try_exists(target).await.unwrap_or(false)
}

/// Mirrors `readJson`: missing or invalid files yield `None`, never an error.
pub async fn read_json_value(target: &Path) -> Option<serde_json::Value> {
    let raw = tokio::fs::read_to_string(target).await.ok()?;
    serde_json::from_str(&raw).ok()
}

/// Pretty-print and atomically replace the document on the same filesystem.
/// The old file survives any failure before replacement. Blocking work continues
/// to completion even if the initiating async task is dropped.
pub async fn write_json(target: &Path, value: &serde_json::Value) -> Result<(), anyhow::Error> {
    let mut raw = serde_json::to_string_pretty(value)?;
    raw.push('\n');
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || write_atomic(&target, raw.as_bytes())).await?
}

struct PendingFile(PathBuf);
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(crate) fn write_atomic(target: &Path, raw: &[u8]) -> anyhow::Result<()> {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let pending = PendingFile(parent.join(format!(".industrialis-{}.tmp", uuid::Uuid::new_v4())));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Account files contain credentials. New files must not be world-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&pending.0)?;
    if let Ok(metadata) = std::fs::metadata(target) {
        // A read-only destination must remain protected on Windows too.
        anyhow::ensure!(
            !metadata.permissions().readonly(),
            "{} is read-only",
            target.display()
        );
        #[cfg(unix)]
        file.set_permissions(metadata.permissions())?;
    }
    file.write_all(raw)?;
    file.sync_all()?;
    drop(file);
    // No delete-then-rename fallback: readers always see a complete document.
    replace_file(&pending.0, target).with_context(|| {
        format!(
            "Could not replace {}. The original file has been kept.",
            target.display()
        )
    })?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    // std::fs normalizes long Windows paths internally; this direct Win32 call
    // needs the same extended-length absolute paths. The destination need not
    // exist yet, so canonicalize its existing parent and append the filename.
    let source = source.canonicalize()?;
    let destination = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?
        .join(destination.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Atomic destination has no filename",
            )
        })?);
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH, same-directory source.
    for attempt in 0..20 {
        let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) };
        if result != 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        // Readers and antivirus scanners may briefly hold the old document.
        // Retry replacement, never remove or truncate the destination.
        if attempt == 19 || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    unreachable!()
}

/// Mirrors `removeIfExists` (`rm -rf` semantics: missing targets are fine).
pub async fn remove_if_exists(target: &Path) -> Result<(), anyhow::Error> {
    let metadata = match tokio::fs::symlink_metadata(target).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.is_dir() && !metadata.is_symlink() {
        tokio::fs::remove_dir_all(target).await?;
    } else {
        tokio::fs::remove_file(target).await?;
    }
    Ok(())
}

/// Copy progress counts bytes in regular files and never follows symlinks.
pub async fn copy_tree_with_progress(
    source: &Path,
    destination: &Path,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> Result<(), anyhow::Error> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
        let entries = walkdir::WalkDir::new(&source)
            .follow_links(false)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let total = entries
            .iter()
            .filter(|e| e.file_type().is_file())
            .try_fold(0u64, |sum, entry| entry.metadata().map(|m| sum + m.len()))?;
        let mut copied = 0;
        progress(0, total);
        for entry in entries {
            let to = destination.join(entry.path().strip_prefix(&source)?);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&to)?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                copied += std::fs::copy(entry.path(), to)?;
                progress(copied, total);
            }
        }
        Ok(())
    })
    .await?
}

/// Recursive copy (files + dirs, symlinks skipped). Mirrors `copyTree`
/// without progress reporting; Wolverine-sized trees run on a worker thread.
pub async fn copy_tree(source: &Path, destination: &Path) -> Result<(), anyhow::Error> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || copy_tree_blocking(&source, &destination)).await?
}

fn copy_tree_blocking(source: &Path, destination: &Path) -> Result<(), anyhow::Error> {
    let mut stack = vec![(source.to_path_buf(), destination.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        let meta = std::fs::symlink_metadata(&from)?;
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            std::fs::create_dir_all(&to)?;
            for entry in std::fs::read_dir(&from)? {
                let entry = entry?;
                stack.push((entry.path(), to.join(entry.file_name())));
            }
        } else if meta.is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Mirrors `dirSize`: sums file sizes recursively, skipping vanished entries.
/// Blocking walk runs on a worker thread so the runtime stays responsive.
pub async fn dir_size(target: &Path) -> u64 {
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(&target)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        total
    })
    .await
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-fs-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn missing_or_invalid_json_yields_none() {
        let dir = temp_dir("missing");
        assert_eq!(read_json_value(&dir.join("nope.json")).await, None);
        tokio::fs::write(dir.join("bad.json"), "{oops")
            .await
            .unwrap();
        assert_eq!(read_json_value(&dir.join("bad.json")).await, None);
    }

    #[cfg(windows)]
    #[test]
    fn atomically_creates_and_replaces_files_beyond_windows_max_path() {
        let root = std::env::temp_dir().join(format!("gpui-long-path-{}", uuid::Uuid::new_v4()));
        let mut parent = root.clone();
        while parent.as_os_str().len() < 280 {
            parent = parent.join("long-component-directory");
        }
        let target = parent.join("settings.json");
        let result = (|| -> anyhow::Result<()> {
            write_atomic(&target, b"before")?;
            write_atomic(&target, b"after")?;
            assert_eq!(std::fs::read(&target)?, b"after");
            assert_eq!(std::fs::read_dir(&parent)?.count(), 1);
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result.unwrap();
    }

    #[tokio::test]
    async fn round_trips_pretty_json() {
        let dir = temp_dir("roundtrip");
        let target = dir.join("sub").join("data.json");
        write_json(&target, &json!({"a": 1})).await.unwrap();
        assert_eq!(read_json_value(&target).await, Some(json!({"a": 1})));
        let raw = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(raw.ends_with('\n'));
    }

    #[tokio::test]
    async fn replacement_failure_keeps_original_and_allows_retry_without_temp_files() {
        let dir = temp_dir("replacement-failure");
        let target = dir.join("document.json");
        write_json(&target, &json!({"old": true})).await.unwrap();
        let original = tokio::fs::read(&target).await.unwrap();
        let original_permissions = std::fs::metadata(&target).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();
        let result = write_json(&target, &json!({"new": true})).await;
        std::fs::set_permissions(&target, original_permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        write_json(&target, &json!({"new": true})).await.unwrap();
        assert_eq!(read_json_value(&target).await, Some(json!({"new": true})));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn readers_never_see_a_missing_or_partially_written_document() {
        let dir = temp_dir("concurrent-readers");
        let target = dir.join("document.json");
        write_json(&target, &json!({"revision":0,"payload":"start"}))
            .await
            .unwrap();
        let write_target = target.clone();
        let writer = tokio::spawn(async move {
            for revision in 1..25 {
                write_json(
                    &write_target,
                    &json!({"revision":revision,"payload":"x".repeat(128*1024)}),
                )
                .await
                .unwrap();
            }
        });
        let mut reads = 0;
        while !writer.is_finished() {
            let raw = tokio::fs::read(&target).await.unwrap();
            let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            assert!(value["revision"].is_number());
            assert!(value["payload"].is_string());
            reads += 1;
        }
        writer.await.unwrap();
        assert!(reads > 0);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_sharing_violation_keeps_the_original_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp_dir("sharing-violation");
        let target = dir.join("document.json");
        write_json(&target, &json!({"old":true})).await.unwrap();
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .unwrap();
        assert!(write_json(&target, &json!({"new":true})).await.is_err());
        assert_eq!(read_json_value(&target).await, Some(json!({"old":true})));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        drop(handle);
        write_json(&target, &json!({"new":true})).await.unwrap();
    }

    #[tokio::test]
    async fn sizes_directories() {
        let dir = temp_dir("size");
        tokio::fs::create_dir_all(dir.join("sub")).await.unwrap();
        tokio::fs::write(dir.join("a.bin"), vec![0u8; 100])
            .await
            .unwrap();
        tokio::fs::write(dir.join("sub").join("b.bin"), vec![0u8; 50])
            .await
            .unwrap();
        assert_eq!(dir_size(&dir).await, 150);
        assert_eq!(dir_size(&dir.join("missing")).await, 0);
    }
}
