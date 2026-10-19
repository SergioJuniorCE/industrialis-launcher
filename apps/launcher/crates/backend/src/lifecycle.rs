//! Admission and graceful drain for one profile. Running games detach on shutdown;
//! admitted filesystem work finishes before the profile ownership guard is released.
use crate::paths::LauncherPaths;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Default)]
struct State {
    closing: bool,
    active: usize,
}
#[derive(Default)]
pub struct Lifecycle {
    state: Mutex<State>,
    changed: tokio::sync::Notify,
}
pub struct WorkLease(Arc<Lifecycle>);

pub fn for_profile(paths: &LauncherPaths) -> Arc<Lifecycle> {
    static PROFILES: OnceLock<Mutex<HashMap<PathBuf, Arc<Lifecycle>>>> = OnceLock::new();
    let key = std::path::absolute(paths.base()).unwrap_or_else(|_| paths.base().into());
    #[cfg(windows)]
    let key = PathBuf::from(key.to_string_lossy().to_lowercase());
    PROFILES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(key)
        .or_default()
        .clone()
}
pub fn admit(paths: &LauncherPaths) -> anyhow::Result<WorkLease> {
    let lifecycle = for_profile(paths);
    {
        let mut state = lifecycle.state.lock().unwrap();
        if state.closing {
            anyhow::bail!("The launcher is closing; no new operations can start.");
        }
        state.active += 1;
    }
    Ok(WorkLease(lifecycle))
}
impl Drop for WorkLease {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().active -= 1;
        self.0.changed.notify_waiters();
    }
}
impl Lifecycle {
    pub fn is_closing(&self) -> bool {
        self.state.lock().unwrap().closing
    }
    pub async fn closing(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_closing() {
                return;
            }
            notified.await;
        }
    }
    pub async fn shutdown(&self, paths: &LauncherPaths) -> anyhow::Result<()> {
        self.state.lock().unwrap().closing = true;
        self.changed.notify_waiters();
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.lock().unwrap().active == 0 {
                break;
            }
            notified.await;
        }
        crate::launch::flush_capture_sessions(paths).await?;
        crate::minecraft_files::flush_external_watches(paths).await?;
        crate::profile_store::flush().await;
        crate::console_log::flush_profile(paths).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shutdown_rejects_new_work_and_waits_for_admitted_work() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-drain-{}", uuid::Uuid::new_v4())),
        );
        let lease = admit(&paths).unwrap();
        let lifecycle = for_profile(&paths);
        let task = tokio::spawn({
            let paths = paths.clone();
            let lifecycle = lifecycle.clone();
            async move { lifecycle.shutdown(&paths).await }
        });
        lifecycle.closing().await;
        assert!(admit(&paths).is_err());
        assert!(!task.is_finished());
        drop(lease);
        task.await.unwrap().unwrap();
        assert!(admit(&paths).is_err());
    }
}
