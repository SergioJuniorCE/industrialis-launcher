//! Port of `console-log-writer.ts`: JSON-lines console logs with compaction.
//!
//! TS serializes appends per instance through a promise chain; here a single
//! async mutex provides the same ordering guarantee with less machinery.

use crate::paths::LauncherPaths;
use industrialis_core::launch_log::{LaunchLogLine, LogStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub(crate) enum LogWrite {
    Line(String, String),
    Flush(tokio::sync::oneshot::Sender<Result<(), String>>),
}
type Queue = Mutex<tokio::sync::mpsc::UnboundedSender<LogWrite>>;

fn normalized_path(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.into());
    #[cfg(windows)]
    let path = PathBuf::from(path.to_string_lossy().to_lowercase());
    path
}

fn active_queues() -> &'static Mutex<std::collections::HashMap<PathBuf, Weak<Queue>>> {
    static QUEUES: OnceLock<Mutex<std::collections::HashMap<PathBuf, Weak<Queue>>>> =
        OnceLock::new();
    QUEUES.get_or_init(Default::default)
}
pub(crate) fn register_queue(path: &Path, queue: &Arc<Queue>) {
    let mut queues = active_queues().lock().unwrap();
    queues.retain(|_, q| q.strong_count() > 0);
    queues.insert(normalized_path(path), Arc::downgrade(queue));
}
async fn flush_pending(path: &Path) -> anyhow::Result<()> {
    let path = normalized_path(path);
    let queue = active_queues()
        .lock()
        .unwrap()
        .get(&path)
        .and_then(Weak::upgrade);
    if let Some(queue) = queue {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        queue.lock().unwrap().send(LogWrite::Flush(sender))?;
        receiver.await?.map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

pub(crate) async fn flush_profile(paths: &LauncherPaths) -> anyhow::Result<()> {
    let root = normalized_path(paths.base());
    let targets: Vec<_> = active_queues()
        .lock()
        .unwrap()
        .keys()
        .filter(|path| path.starts_with(&root))
        .cloned()
        .collect();
    for path in targets {
        flush_pending(&path).await?;
    }
    Ok(())
}

fn file_lock(path: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<
        Mutex<std::collections::HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>>,
    > = OnceLock::new();
    let path = normalized_path(path);
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap();
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = locks
        .get(&path)
        .and_then(Weak::upgrade)
        .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())));
    locks.insert(path, Arc::downgrade(&lock));
    lock
}

/// Read the bounded persisted file, skipping malformed/partial JSON lines.
/// Full copy retains the whole persisted window; display reads retain 5,000 lines.
pub async fn read_console_log(
    paths: &LauncherPaths,
    id: &str,
    full: bool,
) -> anyhow::Result<Vec<LaunchLogLine>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let path = paths.console_log_path(id)?;
    flush_pending(&path).await?;
    let lock = file_lock(&path);
    let _guard = lock.lock().await;
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let size = file.metadata().await?.len();
    let start = size.saturating_sub(MAX_PERSISTED_CONSOLE_LOG_BYTES);
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut bytes = vec![];
    file.take(MAX_PERSISTED_CONSOLE_LOG_BYTES)
        .read_to_end(&mut bytes)
        .await?;
    let mut lines = Vec::new();
    for (index, raw) in bytes.split(|b| *b == b'\n').enumerate() {
        if index == 0 && start > 0 {
            continue;
        }
        if let Ok(entry) = serde_json::from_slice::<LaunchLogLine>(raw) {
            lines.push(entry);
        } else if let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) {
            if let Some(line) = value.get("line").and_then(|v| v.as_str()) {
                let stream = match value.get("stream").and_then(|v| v.as_str()) {
                    Some("stderr") => LogStream::Stderr,
                    Some("system") => LogStream::System,
                    _ => LogStream::Stdout,
                };
                lines.push(LaunchLogLine {
                    stream,
                    line: line.into(),
                });
            }
        }
    }
    if !full && lines.len() > industrialis_core::log_buffer::MAX_RETAINED_LOG_LINES {
        lines.drain(..lines.len() - industrialis_core::log_buffer::MAX_RETAINED_LOG_LINES);
    }
    Ok(lines)
}

/// Clear only when no launch, copy, or maintenance operation owns the instance.
pub async fn clear_console_log(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let _lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    if crate::launch::is_already_running(paths, id).await? {
        anyhow::bail!("Cannot clear console while instance is running");
    }
    let path = paths.console_log_path(id)?;
    let lock = file_lock(&path);
    let _guard = lock.lock().await;
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub const MAX_PERSISTED_CONSOLE_LOG_BYTES: u64 = 4 * 1024 * 1024;
const COMPACTION_TARGET_BYTES: u64 = 3 * 1024 * 1024;

#[derive(Clone)]
pub struct ConsoleLogWriter {
    path_for_id: Arc<dyn Fn(&str) -> PathBuf + Send + Sync>,
}

impl ConsoleLogWriter {
    pub fn new(path_for_id: impl Fn(&str) -> PathBuf + Send + Sync + 'static) -> Self {
        Self {
            path_for_id: Arc::new(path_for_id),
        }
    }

    /// Append one `{stream, line}` entry. Oversized single entries are dropped,
    /// mirroring the TS writer.
    pub async fn append(&self, id: &str, stream: &str, line: &str) -> Result<(), anyhow::Error> {
        let serialized = format!(
            "{}\n",
            serde_json::json!({ "stream": stream, "line": line })
        );
        if serialized.len() as u64 > MAX_PERSISTED_CONSOLE_LOG_BYTES {
            return Ok(());
        }
        let path = (self.path_for_id)(id);
        let lock = file_lock(&path);
        let _guard = lock.lock().await;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        append_bytes(&path, serialized.as_bytes()).await?;
        compact_log_file(&path).await?;
        Ok(())
    }

    pub async fn compact(&self, id: &str) -> Result<(), anyhow::Error> {
        let path = (self.path_for_id)(id);
        let lock = file_lock(&path);
        let _guard = lock.lock().await;
        compact_log_file(&path).await
    }
}

async fn append_bytes(path: &PathBuf, bytes: &[u8]) -> Result<(), anyhow::Error> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    // Tokio can finish write_all before its blocking filesystem write. Keep
    // the file lock until that write finishes, including before a read barrier.
    file.flush().await?;
    Ok(())
}

/// Keep the last 3 MB on a line boundary when the file exceeds 4 MB.
async fn compact_log_file(path: &PathBuf) -> Result<(), anyhow::Error> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let size = tokio::fs::metadata(path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if size <= MAX_PERSISTED_CONSOLE_LOG_BYTES {
        return Ok(());
    }
    let bytes_to_read = size.min(COMPACTION_TARGET_BYTES);
    let start = size - bytes_to_read;
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut data = Vec::new();
    file.take(bytes_to_read).read_to_end(&mut data).await?;
    let tail = data.as_slice();
    let retained: &[u8] = if start > 0 {
        match tail.iter().position(|&b| b == b'\n') {
            Some(first_line_end) => &tail[first_line_end + 1..],
            None => &[],
        }
    } else {
        tail
    };
    tokio::fs::write(path, retained).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-console-read-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn loads_full_history_and_display_tail_ignoring_corrupt_entries() {
        let paths = paths("tail");
        let path = paths.console_log_path("game").unwrap();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let mut data = "not json\n{\"stream\":\"other\",\"line\":\"legacy\"}\n".to_string();
        for n in 0..5_010 {
            data.push_str(&format!("{{\"stream\":\"stdout\",\"line\":\"{n}\"}}\n"));
        }
        data.push_str("{\"stream\":\"stderr\",\"line\":");
        tokio::fs::write(&path, data).await.unwrap();
        let full = read_console_log(&paths, "game", true).await.unwrap();
        assert_eq!(full.len(), 5_011);
        assert_eq!(full[0].stream, LogStream::Stdout);
        let tail = read_console_log(&paths, "game", false).await.unwrap();
        assert_eq!(tail.len(), 5_000);
        assert_eq!(tail[0].line, "10");
        assert_eq!(tail.last().unwrap().line, "5009");
        assert!(read_console_log(&paths, "missing", true)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn oversized_legacy_file_is_read_on_a_complete_line_boundary() {
        let paths = paths("large");
        let path = paths.console_log_path("game").unwrap();
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let old = "x".repeat(MAX_PERSISTED_CONSOLE_LOG_BYTES as usize + 100);
        tokio::fs::write(
            &path,
            format!("{old}\n{{\"stream\":\"system\",\"line\":\"recent\"}}\n"),
        )
        .await
        .unwrap();
        let read = read_console_log(&paths, "game", true).await.unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].line, "recent");
    }

    #[tokio::test]
    async fn clear_persists_and_is_blocked_by_other_instance_operations() {
        let paths = paths("clear");
        let path = paths.console_log_path("game").unwrap();
        let writer = ConsoleLogWriter::new({
            let path = path.clone();
            move |_| path.clone()
        });
        writer.append("game", "stdout", "keep").await.unwrap();
        let lease = crate::instance_guard::reserve_instances(&paths, &["game"]).unwrap();
        assert!(clear_console_log(&paths, "game").await.is_err());
        assert_eq!(
            read_console_log(&paths, "game", true).await.unwrap()[0].line,
            "keep"
        );
        drop(lease);
        clear_console_log(&paths, "game").await.unwrap();
        assert!(!path.exists());
        assert!(read_console_log(&paths, "game", true)
            .await
            .unwrap()
            .is_empty());
        clear_console_log(&paths, "game").await.unwrap();
    }

    fn writer(name: &str) -> (ConsoleLogWriter, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-console-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("console.log");
        let writer = ConsoleLogWriter::new({
            let path = path.clone();
            move |_| path.clone()
        });
        (writer, path)
    }

    #[tokio::test]
    async fn appends_json_lines_and_flushes() {
        let (writer, path) = writer("append");
        writer.append("x", "stdout", "hello").await.unwrap();
        writer
            .append("x", "system", "──────── Launch ────────")
            .await
            .unwrap();
        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["stream"], "stdout");
        assert_eq!(first["line"], "hello");
    }

    #[tokio::test]
    async fn compacts_oversized_logs_on_line_boundary() {
        let (writer, path) = writer("compact");
        // Write just over the limit directly, then trigger compaction.
        let big = "x".repeat(1024);
        let mut data = String::new();
        while (data.len() as u64) < MAX_PERSISTED_CONSOLE_LOG_BYTES + 100 {
            data.push_str(&format!("{{\"stream\":\"stdout\",\"line\":\"{big}\"}}\n"));
        }
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, &data).await.unwrap();
        writer.compact("x").await.unwrap();
        let after = tokio::fs::read(&path).await.unwrap();
        assert!(after.len() as u64 <= COMPACTION_TARGET_BYTES + 2048);
        // Starts mid-stream-tolerant: first byte begins a fresh JSON line.
        assert!(after.starts_with(b"{\"stream\""));
    }
}
