//! Background operation history, matching the Electron Processes tab.

use std::collections::HashMap;

const MAX_LOG_LINES: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Install,
    Update,
    Delete,
    Reinstall,
    Copy,
    Import,
}

impl Operation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "install" => Some(Self::Install),
            "update-pack" | "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "reinstall" => Some(Self::Reinstall),
            "copy" => Some(Self::Copy),
            "import" => Some(Self::Import),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update-pack",
            Self::Delete => "delete",
            Self::Reinstall => "reinstall",
            Self::Copy => "copy",
            Self::Import => "import",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Install => "Installing",
            Self::Update => "Updating pack",
            Self::Delete => "Deleting",
            Self::Reinstall => "Clean reinstall",
            Self::Copy => "Copying instance",
            Self::Import => "Importing instance",
        }
    }
    fn initial_stage(self) -> &'static str {
        match self {
            Self::Install => "downloading",
            Self::Update => "updating",
            Self::Delete => "deleting",
            Self::Reinstall => "reinstalling",
            Self::Copy => "copying",
            Self::Import => "importing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Process {
    pub id: String,
    pub name: String,
    pub operation: Operation,
    pub stage: String,
    pub pct: f64,
    pub speed_mbps: Option<f64>,
    pub downloaded_mb: Option<f64>,
    pub total_mb: Option<f64>,
    pub logs: Vec<String>,
    pub status: Status,
    generation: u64,
}

impl Process {
    fn append(&mut self, line: String) {
        self.logs.push(line);
        if self.logs.len() > MAX_LOG_LINES {
            self.logs.drain(..self.logs.len() - MAX_LOG_LINES);
        }
    }
    pub fn progress_text(&self) -> String {
        let mut parts = vec![format!("{:.0}%", self.pct * 100.0)];
        if matches!(self.stage.as_str(), "downloading" | "copying" | "importing") {
            if let Some(downloaded) = self.downloaded_mb {
                parts.push(self.total_mb.map_or_else(
                    || format!("{downloaded:.1} MB"),
                    |total| format!("{downloaded:.1} / {total:.1} MB"),
                ));
            }
            if let Some(speed) = self.speed_mbps.filter(|s| s.is_finite() && *s > 0.0) {
                parts.push(format!("{speed:.1} MB/s"));
            }
        }
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessToken {
    pub key: String,
    generation: u64,
}

#[derive(Default)]
pub struct Processes {
    entries: HashMap<String, Process>,
    generation: u64,
}

impl Processes {
    pub fn start(&mut self, operation: Operation, id: &str, name: &str) -> ProcessToken {
        self.generation += 1;
        let key = format!("{}:{id}", operation.key());
        self.entries.insert(
            key.clone(),
            Process {
                id: id.into(),
                name: name.into(),
                operation,
                stage: operation.initial_stage().into(),
                pct: 0.0,
                speed_mbps: None,
                downloaded_mb: None,
                total_mb: None,
                logs: vec![format!("{} {name}", operation.label())],
                status: Status::Running,
                generation: self.generation,
            },
        );
        ProcessToken {
            key,
            generation: self.generation,
        }
    }
    fn current(&mut self, token: &ProcessToken) -> Option<&mut Process> {
        self.entries
            .get_mut(&token.key)
            .filter(|p| p.generation == token.generation)
    }
    pub fn progress(
        &mut self,
        token: &ProcessToken,
        stage: &str,
        pct: f64,
        line: Option<String>,
        transfer: (Option<f64>, Option<f64>, Option<f64>),
    ) {
        let Some(p) = self.current(token).filter(|p| p.status == Status::Running) else {
            return;
        };
        p.stage = stage.into();
        p.pct = if pct.is_finite() {
            pct.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if matches!(stage, "downloading" | "copying" | "importing") {
            (p.speed_mbps, p.downloaded_mb, p.total_mb) = transfer;
        } else {
            (p.speed_mbps, p.downloaded_mb, p.total_mb) = (None, None, None);
        }
        if let Some(line) = line {
            p.append(line);
        }
        // Completion comes from the worker result, after all finalization.
    }
    /// Returns true only when this run changes from running to a final state.
    pub fn finish(&mut self, token: &ProcessToken, error: Option<&str>) -> bool {
        let Some(p) = self.current(token).filter(|p| p.status == Status::Running) else {
            return false;
        };
        if error.is_some_and(|e|{
            e.starts_with("Deletion cancelled;") || e.starts_with("Import cancelled;")
        }){p.status=Status::Cancelled;p.stage="cancelled".into();p.append(error.unwrap().into());
        } else if let Some(error) = error {
            p.status = Status::Failed;
            p.stage = "failed".into();
            p.append(format!("Error: {error}"));
        } else {
            p.status = Status::Done;
            p.stage = "done".into();
            p.pct = 1.0;
            p.append("Complete".into());
        }
        true
    }
    pub fn dismiss(&mut self, key: &str) -> bool {
        if self
            .entries
            .get(key)
            .is_some_and(|p| p.status != Status::Running)
        {
            self.entries.remove(key);
            true
        } else {
            false
        }
    }
    pub fn get(&self, key: &str) -> Option<&Process> {
        self.entries.get(key)
    }
    pub fn running_count(&self) -> usize {
        self.entries
            .values()
            .filter(|p| p.status == Status::Running)
            .count()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn sorted(&self) -> Vec<(&str, &Process)> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .map(|(key, p)| (key.as_str(), p))
            .collect();
        entries.sort_by(|(_, a), (_, b)| {
            (a.status != Status::Running)
                .cmp(&(b.status != Status::Running))
                .then_with(|| b.generation.cmp(&a.generation))
        });
        entries
    }
}

pub fn stage_label(stage: &str) -> &str {
    match stage {
        "downloading" => "Downloading",
        "cached" => "Using cache",
        "extracting" => "Extracting",
        "updating" => "Updating pack",
        "deleting" => "Deleting",
        "reinstalling" => "Reinstalling",
        "copying" => "Copying",
        "importing" => "Importing",
        "finalizing" => "Finalizing",
        "done" => "Complete",
        "failed" => "Failed",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_completion_and_errors_but_rejects_late_and_stale_progress() {
        let mut history = Processes::default();
        let first = history.start(Operation::Copy, "copy", "Copy");
        history.progress(
            &first,
            "copying",
            0.5,
            Some("half".into()),
            (None, Some(5.0), Some(10.0)),
        );
        assert_eq!(
            history.get(&first.key).unwrap().progress_text(),
            "50% · 5.0 / 10.0 MB"
        );
        assert!(!history.dismiss(&first.key));
        history.finish(&first, Some("disk full"));
        history.progress(&first, "copying", 1.0, None, (None, None, None));
        assert_eq!(history.get(&first.key).unwrap().status, Status::Failed);
        let retry = history.start(Operation::Copy, "copy", "Retry");
        history.finish(&first, None);
        assert_eq!(history.get(&retry.key).unwrap().status, Status::Running);
        history.finish(&retry, None);
        assert_eq!(history.get(&retry.key).unwrap().pct, 1.0);
        assert!(history.dismiss(&retry.key));
        history.progress(&retry, "copying", 0.1, None, (None, None, None));
        assert!(history.get(&retry.key).is_none());
    }
    #[test]
    fn prioritizes_running_then_newest_and_bounds_logs() {
        let mut history = Processes::default();
        let first = history.start(Operation::Install, "first", "First");
        let second = history.start(Operation::Delete, "second", "Second");
        history.finish(&second, None);
        assert_eq!(history.sorted()[0].0, first.key);
        assert_eq!(history.running_count(), 1);
        for n in 0..MAX_LOG_LINES + 3 {
            history.progress(
                &first,
                "extracting",
                f64::NAN,
                Some(n.to_string()),
                (None, None, None),
            );
        }
        assert_eq!(history.get(&first.key).unwrap().logs.len(), MAX_LOG_LINES);
        assert_eq!(history.get(&first.key).unwrap().pct, 0.0);
        history.finish(&first, None);
        assert_eq!(history.sorted()[0].0, second.key);
        assert_eq!(history.running_count(), 0);
        assert_eq!(Operation::parse("preview"), None);
    }
    #[test]
    fn import_operations_track_transfer_and_cancellation() {
        assert_eq!(Operation::parse("import"), Some(Operation::Import));
        assert_eq!(Operation::Import.key(), "import");
        assert_eq!(Operation::Import.label(), "Importing instance");
        let mut history = Processes::default();
        let token = history.start(Operation::Import, "gtnh", "GTNH 2.8.0");
        assert_eq!(history.get(&token.key).unwrap().stage, "importing");
        history.progress(
            &token,
            "importing",
            0.25,
            None,
            (Some(12.5), Some(2.5), Some(10.0)),
        );
        assert_eq!(
            history.get(&token.key).unwrap().progress_text(),
            "25% · 2.5 / 10.0 MB · 12.5 MB/s"
        );
        assert!(history.finish(&token, Some("Import cancelled; the partial copy was removed.")));
        assert_eq!(history.get(&token.key).unwrap().status, Status::Cancelled);
    }
}
