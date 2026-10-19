//! File-backed game output survives launcher exit. Tailers drain a bounded file
//! snapshot on detach; persisted offsets allow a later launcher to resume.
use crate::process_manager::LogEmitter;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub(crate) struct Capture {
    pub directory: PathBuf,
    stopped: Arc<AtomicBool>,
    offsets: [Arc<AtomicU64>; 2],
    workers: Vec<tokio::task::JoinHandle<anyhow::Result<()>>>,
}
impl Capture {
    pub async fn start(
        directory: &Path,
        offsets: [u64; 2],
        emit: LogEmitter,
    ) -> anyhow::Result<Self> {
        let stopped = Arc::new(AtomicBool::new(false));
        let positions = [
            Arc::new(AtomicU64::new(offsets[0])),
            Arc::new(AtomicU64::new(offsets[1])),
        ];
        let mut workers = Vec::new();
        // Open both before spawning either worker, so failure leaks no tailer.
        let stdout = tokio::fs::File::open(directory.join("stdout.log")).await?;
        let stderr = tokio::fs::File::open(directory.join("stderr.log")).await?;
        for (index, (stream, mut file)) in [("stdout", stdout), ("stderr", stderr)]
            .into_iter()
            .enumerate()
        {
            let stop = stopped.clone();
            let position = positions[index].clone();
            let emit = emit.clone();
            workers.push(tokio::spawn(async move {
                let offset = offsets[index].min(file.metadata().await?.len());
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                position.store(offset, Ordering::Release);
                let mut pending = Vec::new();
                let mut buffer = vec![0; 64 * 1024];
                let mut finish_at = None;
                loop {
                    if stop.load(Ordering::Acquire) && finish_at.is_none() {
                        finish_at = Some(file.metadata().await?.len());
                    }
                    let remaining =
                        finish_at.map(|end| end.saturating_sub(position.load(Ordering::Acquire)));
                    if remaining == Some(0) {
                        break;
                    }
                    let limit =
                        remaining.map_or(buffer.len(), |n| n.min(buffer.len() as u64) as usize);
                    let n = file.read(&mut buffer[..limit]).await?;
                    if n == 0 {
                        if finish_at.is_some() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    position.fetch_add(n as u64, Ordering::AcqRel);
                    pending.extend_from_slice(&buffer[..n]);
                    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                        let mut line = pending.drain(..=end).collect::<Vec<_>>();
                        while matches!(line.last(), Some(b'\n' | b'\r')) {
                            line.pop();
                        }
                        if !line.is_empty() {
                            emit(stream, &String::from_utf8_lossy(&line));
                        }
                    }
                    if pending.len() > 1024 * 1024 {
                        emit(stream, &String::from_utf8_lossy(&pending));
                        pending.clear();
                    }
                }
                if !pending.is_empty() {
                    emit(stream, &String::from_utf8_lossy(&pending));
                }
                Ok(())
            }));
        }
        Ok(Self {
            directory: directory.into(),
            stopped,
            offsets: positions,
            workers,
        })
    }
    pub async fn drain(&mut self) -> anyhow::Result<[u64; 2]> {
        self.stopped.store(true, Ordering::Release);
        let mut failure = None;
        for worker in self.workers.drain(..) {
            if let Err(error) = worker.await.unwrap_or_else(|error| Err(error.into())) {
                failure = Some(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok([
            self.offsets[0].load(Ordering::Acquire),
            self.offsets[1].load(Ordering::Acquire),
        ])
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        for worker in &self.workers {
            worker.abort();
        }
    }
}

pub(crate) fn session_directory(
    paths: &crate::paths::LauncherPaths,
    id: &str,
    capture_id: &str,
) -> anyhow::Result<PathBuf> {
    let uuid = uuid::Uuid::parse_str(capture_id)?;
    if uuid.to_string() != capture_id {
        anyhow::bail!("Invalid game capture identity");
    }
    let instance = paths.instance_dir(id)?;
    let capture = instance.join(".industrialis-capture").join(capture_id);
    for part in [
        instance,
        capture.parent().unwrap().into(),
        capture.clone(),
        capture.join("stdout.log"),
        capture.join("stderr.log"),
        capture.join("ready.json"),
        capture.join("exit.json"),
    ] {
        match std::fs::symlink_metadata(&part) {
            Ok(meta) => anyhow::ensure!(
                !meta.file_type().is_symlink(),
                "Game capture must not use symbolic links"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(capture)
}
