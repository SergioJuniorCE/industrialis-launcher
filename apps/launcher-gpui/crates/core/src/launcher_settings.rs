//! Port of `apps/launcher/src/lib/launcher-settings.ts`.

use serde::{Deserialize, Serialize};

use crate::launcher_window::{LauncherWindowSettings, DEFAULT_LAUNCHER_WINDOW_HEIGHT, DEFAULT_LAUNCHER_WINDOW_WIDTH};

pub const DEFAULT_THEME_PRESET_ID: &str = "industrialis";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherSettingsData {
    pub theme_mode: ThemeMode,
    pub theme_preset: String,
    #[serde(default)]
    pub theme_overrides: ThemeOverrides,
    #[serde(default)]
    pub custom_theme_presets: Vec<serde_json::Value>,
    #[serde(default)]
    pub default_account_id: Option<String>,
    #[serde(default)]
    pub default_java_path: Option<String>,
    /// @deprecated Renamed to `default_account_id`.
    #[serde(default)]
    pub active_account_id: Option<String>,
    /// @deprecated Legacy setting retained for compatibility.
    #[serde(default = "default_grid_columns")]
    pub instance_grid_columns: u32,
    #[serde(default = "default_backup_retention")]
    pub backup_retention_limit: u32,
    #[serde(flatten)]
    pub window: LauncherWindowSettings,
}

fn default_grid_columns() -> u32 {
    4
}

fn default_backup_retention() -> u32 {
    10
}

impl Default for LauncherSettingsData {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            theme_preset: DEFAULT_THEME_PRESET_ID.to_string(),
            theme_overrides: ThemeOverrides::default(),
            custom_theme_presets: Vec::new(),
            default_account_id: None,
            default_java_path: None,
            active_account_id: None,
            instance_grid_columns: 4,
            backup_retention_limit: 10,
            window: LauncherWindowSettings {
                launch_maximized: false,
                window_width: DEFAULT_LAUNCHER_WINDOW_WIDTH,
                window_height: DEFAULT_LAUNCHER_WINDOW_HEIGHT,
            },
        }
    }
}

/// Mirrors `resolveDefaultAccountId` (includes the deprecated fallback).
pub fn resolve_default_account_id(settings: &LauncherSettingsData) -> Option<&str> {
    settings
        .default_account_id
        .as_deref()
        .or(settings.active_account_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_typescript() {
        let settings = LauncherSettingsData::default();
        assert_eq!(settings.theme_mode, ThemeMode::Dark);
        assert_eq!(settings.theme_preset, "industrialis");
        assert_eq!(settings.backup_retention_limit, 10);
        assert_eq!(resolve_default_account_id(&settings), None);
    }

    #[test]
    fn prefers_default_account_over_deprecated_active_account() {
        let mut settings = LauncherSettingsData::default();
        settings.active_account_id = Some("legacy".to_string());
        assert_eq!(resolve_default_account_id(&settings), Some("legacy"));
        settings.default_account_id = Some("current".to_string());
        assert_eq!(resolve_default_account_id(&settings), Some("current"));
    }

    #[test]
    fn round_trips_json_shape() {
        let settings = LauncherSettingsData::default();
        let raw = serde_json::to_string(&settings).unwrap();
        let parsed: LauncherSettingsData = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, settings);
    }
}
