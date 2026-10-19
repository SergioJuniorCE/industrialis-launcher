//! UI state: backend snapshots owned by the root view.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use industrialis_backend::accounts::AccountInfo;
use industrialis_backend::instances::InstanceInfo;
use industrialis_backend::paths::LauncherPaths;
use industrialis_core::launcher_settings::LauncherSettingsData;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub stream: String,
    pub line: String,
}

pub struct AppData {
    pub paths: LauncherPaths,
    pub rt: Arc<tokio::runtime::Runtime>,
    pub instances: Vec<InstanceInfo>,
    pub accounts: Vec<AccountInfo>,
    pub settings: LauncherSettingsData,
    pub default_account_id: Option<String>,
    /// instance id → (pid, verified alive).
    pub running: HashMap<String, (u32, bool)>,
    /// Games spawned by this view, protected against older refresh snapshots.
    pub owned_running: HashMap<String, u32>,
    /// Rolling per-instance log buffer for the detail view.
    pub logs: HashMap<String, Vec<LogLine>>,
    pub launching: HashSet<String>,
    pub status: String,
}

impl AppData {
    pub fn selected_instance(&self, id: Option<&str>) -> Option<&InstanceInfo> {
        id.and_then(|want| self.instances.iter().find(|i| i.id == want))
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running
            .get(id)
            .map(|(_, alive)| *alive)
            .unwrap_or(false)
            || self.launching.contains(id)
    }

    pub fn game_started(&mut self, id: &str, pid: u32) {
        self.launching.remove(id);
        self.owned_running.insert(id.into(), pid);
        self.running.insert(id.into(), (pid, true));
    }

    pub fn game_exited(&mut self, id: &str) {
        self.launching.remove(id);
        self.owned_running.remove(id);
        self.running.remove(id);
    }

    pub fn has_active_games(&self) -> bool {
        !self.launching.is_empty() || self.running.values().any(|(_, alive)| *alive)
    }

    pub fn replace_running(&mut self, mut snapshot: HashMap<String, (u32, bool)>) {
        for (id, pid) in &self.owned_running {
            snapshot.insert(id.clone(), (*pid, true));
        }
        self.running = snapshot;
    }

    pub fn push_logs(&mut self, id: &str, lines: Vec<LogLine>) {
        const CAP: usize = industrialis_core::log_buffer::MAX_RETAINED_LOG_LINES;
        let buffer = self.logs.entry(id.to_string()).or_default();
        buffer.extend(lines);
        if buffer.len() > CAP {
            let drop = buffer.len() - CAP;
            buffer.drain(..drop);
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> AppData {
        AppData {
            paths: LauncherPaths::new(std::env::temp_dir().join("industrialis-state-fixture")),
            rt: Arc::new(tokio::runtime::Runtime::new().unwrap()),
            instances: vec![],
            accounts: vec![],
            settings: LauncherSettingsData::default(),
            default_account_id: None,
            running: HashMap::new(),
            owned_running: HashMap::new(),
            logs: HashMap::new(),
            launching: HashSet::new(),
            status: String::new(),
        }
    }
    #[test]
    fn startup_and_exit_do_not_lose_other_games_to_older_snapshots() {
        let mut data = data();
        data.launching.extend(["alpha".into(), "beta".into()]);
        assert!(data.has_active_games());
        data.game_started("alpha", 100);
        assert!(!data.launching.contains("alpha"));
        assert!(data.launching.contains("beta"));
        data.replace_running(HashMap::new());
        assert!(data.is_running("alpha"));
        data.game_started("beta", 200);
        data.game_exited("alpha");
        data.replace_running(HashMap::new());
        assert!(!data.is_running("alpha"));
        assert!(data.is_running("beta"));
        assert!(data.has_active_games());
        data.game_exited("beta");
        data.replace_running(HashMap::from([("stale".into(), (1, false))]));
        assert!(!data.has_active_games());
    }
}
