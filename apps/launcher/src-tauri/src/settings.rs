use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeTokens {
    pub background: String,
    pub foreground: String,
    pub card: String,
    pub card_foreground: String,
    pub popover: String,
    pub popover_foreground: String,
    pub primary: String,
    pub primary_foreground: String,
    pub secondary: String,
    pub secondary_foreground: String,
    pub muted: String,
    pub muted_foreground: String,
    pub accent: String,
    pub accent_foreground: String,
    pub destructive: String,
    pub destructive_foreground: String,
    pub border: String,
    pub input: String,
    pub ring: String,
    pub radius: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default = "default_background_effect")]
    pub background_effect: String,
    pub dark: ThemeTokens,
    pub light: ThemeTokens,
}

fn default_background_effect() -> String {
    "none".to_string()
}

fn default_theme_preset() -> String {
    "industrialis".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default = "default_theme_preset")]
    pub theme_preset: String,
    #[serde(default)]
    pub theme_overrides: ThemeOverrides,
    #[serde(default)]
    pub custom_theme_presets: Vec<CustomThemePreset>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "active_account_id"
    )]
    pub default_account_id: Option<String>,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            theme_preset: default_theme_preset(),
            theme_overrides: ThemeOverrides::default(),
            custom_theme_presets: Vec::new(),
            default_account_id: None,
        }
    }
}

fn validate_theme_override_value(value: &str) -> Result<(), String> {
    if value.len() > 32 {
        return Err("theme override value too long".into());
    }
    Ok(())
}

fn validate_theme_tokens(tokens: &ThemeTokens) -> Result<(), String> {
    let fields = [
        &tokens.background,
        &tokens.foreground,
        &tokens.card,
        &tokens.card_foreground,
        &tokens.popover,
        &tokens.popover_foreground,
        &tokens.primary,
        &tokens.primary_foreground,
        &tokens.secondary,
        &tokens.secondary_foreground,
        &tokens.muted,
        &tokens.muted_foreground,
        &tokens.accent,
        &tokens.accent_foreground,
        &tokens.destructive,
        &tokens.destructive_foreground,
        &tokens.border,
        &tokens.input,
        &tokens.ring,
        &tokens.radius,
    ];
    for value in fields {
        validate_theme_override_value(value)?;
    }
    Ok(())
}

pub fn validate_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    if settings.theme_preset.is_empty() || settings.theme_preset.len() > 64 {
        return Err("invalid theme preset id".into());
    }
    for value in [
        settings.theme_overrides.background.as_deref(),
        settings.theme_overrides.foreground.as_deref(),
        settings.theme_overrides.primary.as_deref(),
        settings.theme_overrides.card.as_deref(),
        settings.theme_overrides.border.as_deref(),
        settings.theme_overrides.muted.as_deref(),
        settings.theme_overrides.muted_foreground.as_deref(),
        settings.theme_overrides.accent.as_deref(),
        settings.theme_overrides.accent_foreground.as_deref(),
        settings.theme_overrides.radius.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_theme_override_value(value)?;
    }

    for preset in &settings.custom_theme_presets {
        if !preset.id.starts_with("custom-") || preset.id.len() > 64 {
            return Err("invalid custom theme preset id".into());
        }
        if preset.name.is_empty() || preset.name.len() > 64 {
            return Err("invalid custom theme preset name".into());
        }
        if preset.description.len() > 256 {
            return Err("custom theme preset description too long".into());
        }
        if preset.background_effect != "grid" && preset.background_effect != "none" {
            return Err("invalid custom theme background effect".into());
        }
        validate_theme_tokens(&preset.dark)?;
        validate_theme_tokens(&preset.light)?;
    }

    if !settings.theme_preset.starts_with("custom-") {
        return Ok(());
    }

    if settings
        .custom_theme_presets
        .iter()
        .any(|p| p.id == settings.theme_preset)
    {
        Ok(())
    } else {
        Err("active custom theme preset not found".into())
    }
}

pub fn launcher_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("launcher-settings.json")
}

pub fn load_launcher_settings(data_dir: &Path) -> LauncherSettings {
    fs::read_to_string(launcher_settings_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_launcher_settings(data_dir: &Path, settings: &LauncherSettings) -> Result<(), String> {
    validate_launcher_settings(settings)?;
    let path = launcher_settings_path(data_dir);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_legacy_launcher_settings() {
        let json = r#"{ "microsoft_client_id": "abc" }"#;
        let settings: LauncherSettings =
            serde_json::from_str(json).expect("legacy launcher settings should parse");
        assert!(matches!(settings.theme_mode, ThemeMode::Dark));
        assert_eq!(settings.theme_preset, "industrialis");
        assert!(settings.custom_theme_presets.is_empty());
    }

    #[test]
    fn deserialize_theme_mode_light_and_dark() {
        let dark: LauncherSettings =
            serde_json::from_str(r#"{ "theme_mode": "dark" }"#).expect("dark theme_mode");
        assert!(matches!(dark.theme_mode, ThemeMode::Dark));

        let light: LauncherSettings =
            serde_json::from_str(r#"{ "theme_mode": "light" }"#).expect("light theme_mode");
        assert!(matches!(light.theme_mode, ThemeMode::Light));
    }

    #[test]
    fn deserialize_legacy_active_account_id_as_default() {
        let json = r#"{ "active_account_id": "offline-steve" }"#;
        let settings: LauncherSettings =
            serde_json::from_str(json).expect("legacy active account id should parse");
        assert_eq!(
            settings.default_account_id.as_deref(),
            Some("offline-steve")
        );
    }

    #[test]
    fn reject_missing_active_custom_preset() {
        let settings = LauncherSettings {
            theme_preset: "custom-deadbeef".to_string(),
            ..LauncherSettings::default()
        };
        assert!(validate_launcher_settings(&settings).is_err());
    }

    #[test]
    fn reject_oversized_override() {
        let settings = LauncherSettings {
            theme_overrides: ThemeOverrides {
                background: Some("x".repeat(33)),
                ..ThemeOverrides::default()
            },
            ..LauncherSettings::default()
        };
        assert!(validate_launcher_settings(&settings).is_err());
    }
}