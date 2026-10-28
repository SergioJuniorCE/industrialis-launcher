//! Port of `apps/launcher/src/lib/launcher-settings.ts`.

use serde::{Deserialize, Serialize};

use crate::launcher_window::{
    LauncherWindowSettings, DEFAULT_LAUNCHER_WINDOW_HEIGHT, DEFAULT_LAUNCHER_WINDOW_WIDTH,
};

pub const DEFAULT_THEME_PRESET_ID: &str = "industrialis";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMaterial {
    #[default]
    Opaque,
    Frosted,
    Mica,
}

fn default_window_tint_opacity() -> u8 {
    60
}

impl ThemeOverrides {
    /// Set (non-empty) overrides as pairs, for the launcher theme bridge.
    pub fn as_pairs(&self) -> Vec<(String, String)> {
        [
            ("background", &self.background),
            ("foreground", &self.foreground),
            ("primary", &self.primary),
            ("card", &self.card),
            ("border", &self.border),
            ("muted", &self.muted),
            ("muted_foreground", &self.muted_foreground),
            ("accent", &self.accent),
            ("accent_foreground", &self.accent_foreground),
            ("card_foreground", &self.card_foreground),
            ("popover", &self.popover),
            ("popover_foreground", &self.popover_foreground),
            ("primary_foreground", &self.primary_foreground),
            ("secondary", &self.secondary),
            ("secondary_foreground", &self.secondary_foreground),
            ("destructive", &self.destructive),
            ("destructive_foreground", &self.destructive_foreground),
            ("input", &self.input),
            ("ring", &self.ring),
            ("radius", &self.radius),
        ]
        .iter()
        .filter_map(|(k, v)| {
            v.as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| (k.to_string(), s.to_string()))
        })
        .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeOverrides {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popover: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popover_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LauncherSettingsData {
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
    pub theme_mode: ThemeMode,
    pub theme_preset: String,
    #[serde(default)]
    pub window_material: WindowMaterial,
    #[serde(default = "default_window_tint_opacity")]
    pub window_tint_opacity: u8,
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
    /// One-time first-run Prism import prompt was answered (either choice).
    #[serde(default)]
    pub prism_import_prompted: bool,
    #[serde(flatten)]
    pub window: LauncherWindowSettings,
}

impl<'de> Deserialize<'de> for LauncherSettingsData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if !value.is_object() {
            return Err(serde::de::Error::custom(
                "launcher settings must be an object",
            ));
        }
        // Two flattened fields (window + extensions) would make serde copy
        // window keys into the extension map and serialize duplicate keys.
        Ok(parse_launcher_settings_json(&value.to_string()))
    }
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
            extra_fields: Default::default(),
            theme_mode: ThemeMode::Dark,
            theme_preset: DEFAULT_THEME_PRESET_ID.to_string(),
            window_material: WindowMaterial::Opaque,
            window_tint_opacity: default_window_tint_opacity(),
            theme_overrides: ThemeOverrides::default(),
            custom_theme_presets: Vec::new(),
            default_account_id: None,
            default_java_path: None,
            active_account_id: None,
            instance_grid_columns: default_grid_columns(),
            backup_retention_limit: default_backup_retention(),
            prism_import_prompted: false,
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

/// Tolerant loader mirroring `loadLauncherSettings`: unknown/missing JSON
/// overlays defaults field-wise, and window dimensions pass through
/// `normalize_launcher_window_settings` so invalid saved values fall back.
pub fn parse_launcher_settings_json(raw: &str) -> LauncherSettingsData {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return LauncherSettingsData::default(),
    };
    let obj = match value.as_object() {
        Some(o) => o,
        None => return LauncherSettingsData::default(),
    };
    let mut settings = LauncherSettingsData::default();
    let get_str = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(str::to_string);
    if let Some(material) = obj.get("window_material") {
        settings.window_material = serde_json::from_value(material.clone()).unwrap_or_default();
    }
    if let Some(opacity) = obj
        .get("window_tint_opacity")
        .and_then(|v| v.as_u64())
        .filter(|n| (20..=90).contains(n))
    {
        settings.window_tint_opacity = opacity as u8;
    }
    if let Some(mode) = obj.get("theme_mode").and_then(|v| v.as_str()) {
        settings.theme_mode = match mode {
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        };
    }
    if let Some(preset) = get_str("theme_preset") {
        if !preset.is_empty() {
            settings.theme_preset = preset;
        }
    }
    if let Some(overrides) = obj.get("theme_overrides").and_then(|v| v.as_object()) {
        let mut merged = ThemeOverrides::default();
        for (key, target) in [
            ("background", &mut merged.background),
            ("foreground", &mut merged.foreground),
            ("primary", &mut merged.primary),
            ("card", &mut merged.card),
            ("border", &mut merged.border),
            ("muted", &mut merged.muted),
            ("muted_foreground", &mut merged.muted_foreground),
            ("accent", &mut merged.accent),
            ("accent_foreground", &mut merged.accent_foreground),
            ("card_foreground", &mut merged.card_foreground),
            ("popover", &mut merged.popover),
            ("popover_foreground", &mut merged.popover_foreground),
            ("primary_foreground", &mut merged.primary_foreground),
            ("secondary", &mut merged.secondary),
            ("secondary_foreground", &mut merged.secondary_foreground),
            ("destructive", &mut merged.destructive),
            ("destructive_foreground", &mut merged.destructive_foreground),
            ("input", &mut merged.input),
            ("ring", &mut merged.ring),
            ("radius", &mut merged.radius),
        ] {
            if let Some(s) = overrides.get(key).and_then(|v| v.as_str()) {
                *target = Some(s.to_string());
            }
        }
        merged.extra_fields = overrides
            .iter()
            .filter(|(key, _)| !crate::theme::THEME_OVERRIDE_KEYS.contains(&key.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        settings.theme_overrides = merged;
    }
    if let Some(presets) = obj.get("custom_theme_presets").and_then(|v| v.as_array()) {
        settings.custom_theme_presets = presets.clone();
    }
    settings.default_account_id = get_str("default_account_id");
    settings.active_account_id = get_str("active_account_id");
    settings.default_java_path = get_str("default_java_path");
    if let Some(cols) = obj
        .get("instance_grid_columns")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
    {
        settings.instance_grid_columns = cols;
    }
    if let Some(limit) = obj
        .get("backup_retention_limit")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
    {
        settings.backup_retention_limit = limit;
    }
    settings.prism_import_prompted = obj
        .get("prism_import_prompted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let partial_window = crate::launcher_window::PartialLauncherWindowSettings {
        launch_maximized: obj.get("launch_maximized").and_then(|v| v.as_bool()),
        window_width: obj.get("window_width").and_then(|v| v.as_i64()),
        window_height: obj.get("window_height").and_then(|v| v.as_i64()),
    };
    settings.window = crate::launcher_window::normalize_launcher_window_settings(partial_window);
    let known = serde_json::to_value(LauncherSettingsData::default()).unwrap();
    settings.extra_fields = obj
        .iter()
        .filter(|(key, _)| known.get(*key).is_none())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    settings
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

    #[test]
    fn preserves_unknown_fields_and_all_theme_tokens_without_duplicate_window_keys() {
        let mut overrides = serde_json::Map::new();
        for key in crate::theme::THEME_OVERRIDE_KEYS {
            overrides.insert(
                key.into(),
                serde_json::json!(if key == "radius" { "1rem" } else { "#123456" }),
            );
        }
        overrides.insert("future".into(), serde_json::json!({"nested":true}));
        let raw = serde_json::json!({"theme_overrides":overrides,"future_preferences":[1,2],"window_width":1234});
        let settings = parse_launcher_settings_json(&raw.to_string());
        assert_eq!(settings.theme_overrides.as_pairs().len(), 20);
        let saved = serde_json::to_string(&settings).unwrap();
        assert_eq!(saved.matches("\"window_width\"").count(), 1);
        let restored: LauncherSettingsData = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(
            serde_json::to_value(restored).unwrap()["future_preferences"],
            raw["future_preferences"]
        );
    }

    #[test]
    fn parses_partial_window_settings_like_typescript_loader() {
        let parsed = parse_launcher_settings_json(
            r#"{"launch_maximized":true,"window_width":1440,"window_height":900}"#,
        );
        assert!(parsed.window.launch_maximized);
        assert_eq!(parsed.window.window_width, 1440);
        assert_eq!(parsed.window.window_height, 900);
        // Other keys keep defaults.
        assert_eq!(parsed.theme_mode, ThemeMode::Dark);
    }

    #[test]
    fn falls_back_to_defaults_for_invalid_saved_dimensions() {
        let parsed = parse_launcher_settings_json(r#"{"window_width":1,"window_height":"large"}"#);
        let defaults = LauncherSettingsData::default();
        assert_eq!(parsed.window.window_width, defaults.window.window_width);
        assert_eq!(parsed.window.window_height, defaults.window.window_height);
    }

    #[test]
    fn invalid_json_yields_defaults() {
        assert_eq!(
            parse_launcher_settings_json("nope"),
            LauncherSettingsData::default()
        );
    }

    #[test]
    fn prism_import_prompt_marker_round_trips_without_extra_field_duplication() {
        let parsed = parse_launcher_settings_json(r#"{"prism_import_prompted":true}"#);
        assert!(parsed.prism_import_prompted);
        assert!(!parsed.extra_fields.contains_key("prism_import_prompted"));
        let raw = serde_json::to_string(&parsed).unwrap();
        assert!(parse_launcher_settings_json(&raw).prism_import_prompted);
        assert!(!parse_launcher_settings_json("{}").prism_import_prompted);
    }

    #[test]
    fn backdrop_preferences_survive_tolerant_loading() {
        for material in [
            WindowMaterial::Opaque,
            WindowMaterial::Frosted,
            WindowMaterial::Mica,
        ] {
            let settings = LauncherSettingsData {
                window_material: material,
                window_tint_opacity: 40,
                ..Default::default()
            };
            let raw = serde_json::to_string(&settings).unwrap();
            assert_eq!(parse_launcher_settings_json(&raw), settings);
        }
    }

    #[test]
    fn legacy_or_invalid_backdrop_preferences_are_opaque() {
        for raw in [
            "{}",
            r#"{"window_material":"unknown","window_tint_opacity":101}"#,
            r#"{"window_material":null,"window_tint_opacity":"low"}"#,
        ] {
            let settings = parse_launcher_settings_json(raw);
            assert_eq!(settings.window_material, WindowMaterial::Opaque);
            assert_eq!(settings.window_tint_opacity, 60);
        }
    }
}
