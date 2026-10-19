//! Port of `apps/launcher/electron/backend/paths.ts`.
//!
//! The data dir matches Electron's `app.getPath("appData")` layout:
//! `%APPDATA%` on Windows, `~/Library/Application Support` on macOS,
//! `$XDG_CONFIG_HOME`/`~/.config` on Linux — plus `industrialis-launcher`.
//!
//! `INDUSTRIALIS_DATA_DIR` overrides the base dir (tests, CLI, portable mode).
//!
//! I/O code takes an explicit [`LauncherPaths`] so tests stay hermetic; the
//! free functions below resolve the process-global location for app/CLI use.

use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = "industrialis-launcher";

/// Explicit base dir for every launcher path. Prefer this over the free
/// functions in code that must be testable.
#[derive(Debug, Clone)]
pub struct LauncherPaths {
    base: PathBuf,
}

impl LauncherPaths {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// Process-global location (`INDUSTRIALIS_DATA_DIR` or the OS app-data dir).
    pub fn system() -> Self {
        Self::new(system_data_dir())
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.base.join("instances")
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.base.join("icons")
    }

    pub fn instance_dir(&self, id: &str) -> Result<PathBuf, anyhow::Error> {
        Ok(self.instances_dir().join(validate_instance_id(id)?))
    }

    pub fn settings_path(&self, id: &str) -> Result<PathBuf, anyhow::Error> {
        Ok(self.instance_dir(id)?.join("instance.json"))
    }

    pub fn accounts_path(&self) -> PathBuf {
        self.base.join("accounts.json")
    }

    pub fn launcher_settings_path(&self) -> PathBuf {
        self.base.join("launcher-settings.json")
    }

    pub fn running_game_pids_path(&self) -> PathBuf {
        self.base.join("running-game-pids.json")
    }

    pub fn groups_path(&self) -> PathBuf {
        self.instances_dir().join("instgroups.json")
    }

    pub fn console_log_path(&self, id: &str) -> Result<PathBuf, anyhow::Error> {
        Ok(self.instance_dir(id)?.join("console.log"))
    }

    pub fn instance_backups_dir(&self, id: &str) -> Result<PathBuf, anyhow::Error> {
        Ok(self.instance_dir(id)?.join(".minecraft").join("backups"))
    }

    pub fn google_drive_state_path(&self) -> PathBuf {
        self.base.join("google-drive-state.json")
    }

    pub fn backup_state_path(&self) -> PathBuf {
        self.base.join("backup-state.json")
    }

    pub fn pack_cache_root(&self) -> PathBuf {
        self.base.join("pack-cache")
    }
}

pub fn system_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("INDUSTRIALIS_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    default_data_dir()
}
pub(crate) fn default_data_dir() -> PathBuf {
    match directories::BaseDirs::new().map(|b| b.config_dir().to_path_buf()) {
        Some(dir) => dir.join(APP_DIR_NAME),
        None => PathBuf::from(APP_DIR_NAME),
    }
}

// Free-function wrappers over the process-global location.

pub fn data_dir() -> PathBuf {
    system_data_dir()
}

pub fn instances_dir() -> PathBuf {
    LauncherPaths::system().instances_dir()
}

pub fn icons_dir() -> PathBuf {
    LauncherPaths::system().icons_dir()
}

pub fn instance_dir(id: &str) -> Result<PathBuf, anyhow::Error> {
    LauncherPaths::system().instance_dir(id)
}

pub fn settings_path(id: &str) -> Result<PathBuf, anyhow::Error> {
    LauncherPaths::system().settings_path(id)
}

pub fn accounts_path() -> PathBuf {
    LauncherPaths::system().accounts_path()
}

pub fn launcher_settings_path() -> PathBuf {
    LauncherPaths::system().launcher_settings_path()
}

pub fn running_game_pids_path() -> PathBuf {
    LauncherPaths::system().running_game_pids_path()
}

pub fn groups_path() -> PathBuf {
    LauncherPaths::system().groups_path()
}

pub fn console_log_path(id: &str) -> Result<PathBuf, anyhow::Error> {
    LauncherPaths::system().console_log_path(id)
}

pub fn instance_backups_dir(id: &str) -> Result<PathBuf, anyhow::Error> {
    LauncherPaths::system().instance_backups_dir(id)
}

pub fn google_drive_state_path() -> PathBuf {
    LauncherPaths::system().google_drive_state_path()
}

pub fn backup_state_path() -> PathBuf {
    LauncherPaths::system().backup_state_path()
}

pub fn pack_cache_root() -> PathBuf {
    LauncherPaths::system().pack_cache_root()
}

fn is_forbidden_char(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
    )
}

/// Mirrors `sanitizeName` (TS tests whitespace via `\s`; Rust covers the
/// ASCII whitespace that can appear in file names).
pub fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|c| if is_forbidden_char(c) { '_' } else { c })
        .collect()
}

/// Mirrors `validateInstanceId`.
pub fn validate_instance_id(raw: &str) -> Result<String, anyhow::Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("instance id cannot be empty");
    }
    let id = sanitize_name(trimmed);
    if id.is_empty() {
        anyhow::bail!("instance id cannot be empty");
    }
    if id == "." || id == ".." {
        anyhow::bail!("invalid instance id: {raw}");
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(
            sanitize_name("a/b\\c:d*e?f\"g<h>i|j k"),
            "a_b_c_d_e_f_g_h_i_j_k"
        );
    }

    #[test]
    fn validates_instance_ids() {
        assert!(validate_instance_id("  ").is_err());
        assert!(validate_instance_id(".").is_err());
        assert!(validate_instance_id("..").is_err());
        assert_eq!(validate_instance_id("  gtnh  ").unwrap(), "gtnh");
        assert_eq!(validate_instance_id("my/pack").unwrap(), "my_pack");
    }

    #[test]
    fn resolves_explicit_base_dir() {
        let paths = LauncherPaths::new(PathBuf::from("/tmp/industrialis-test-x"));
        assert_eq!(
            paths.launcher_settings_path(),
            PathBuf::from("/tmp/industrialis-test-x/launcher-settings.json")
        );
        assert_eq!(
            paths.settings_path("my/pack").unwrap(),
            PathBuf::from("/tmp/industrialis-test-x/instances/my_pack/instance.json")
        );
    }
}
