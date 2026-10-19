//! Port of `apps/launcher/src/lib/launcher-window.ts`.

use serde::{Deserialize, Serialize};

pub const LAUNCHER_WINDOW_MIN_WIDTH: i64 = 800;
pub const LAUNCHER_WINDOW_MIN_HEIGHT: i64 = 600;
pub const LAUNCHER_WINDOW_MAX_WIDTH: i64 = 7680;
pub const LAUNCHER_WINDOW_MAX_HEIGHT: i64 = 4320;
pub const DEFAULT_LAUNCHER_WINDOW_WIDTH: i64 = 1100;
pub const DEFAULT_LAUNCHER_WINDOW_HEIGHT: i64 = 750;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherWindowSettings {
    pub launch_maximized: bool,
    pub window_width: i64,
    pub window_height: i64,
}

impl Default for LauncherWindowSettings {
    fn default() -> Self {
        Self {
            launch_maximized: false,
            window_width: DEFAULT_LAUNCHER_WINDOW_WIDTH,
            window_height: DEFAULT_LAUNCHER_WINDOW_HEIGHT,
        }
    }
}

/// Draft validation result shown while the user is typing a dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionValidation {
    pub value: Option<i64>,
    pub error: Option<String>,
}

fn normalize_dimension(value: Option<i64>, fallback: i64, minimum: i64, maximum: i64) -> i64 {
    match value {
        Some(v) if v >= minimum && v <= maximum => v,
        _ => fallback,
    }
}

/// Mirrors `validateLauncherWindowDimension`: validates the raw text without
/// mutating it, so the field stays editable mid-typing.
pub fn validate_launcher_window_dimension(raw: &str, label: &str, minimum: i64, maximum: i64) -> DimensionValidation {
    let trimmed = raw.trim();
    let digits_only = !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit());
    if !digits_only {
        return DimensionValidation {
            value: None,
            error: Some(format!("{label} must be a whole number.")),
        };
    }
    let value: i64 = match trimmed.parse() {
        Ok(v) => v,
        // Huge digit strings overflow i64; TS reports "not a whole number"
        // via Number.isSafeInteger, so do the same.
        Err(_) => {
            return DimensionValidation {
                value: None,
                error: Some(format!("{label} must be a whole number.")),
            };
        }
    };
    if value < minimum {
        return DimensionValidation {
            value: None,
            error: Some(format!("{label} must be at least {minimum} pixels.")),
        };
    }
    if value > maximum {
        return DimensionValidation {
            value: None,
            error: Some(format!("{label} must be at most {maximum} pixels.")),
        };
    }
    DimensionValidation { value: Some(value), error: None }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PartialLauncherWindowSettings {
    pub launch_maximized: Option<bool>,
    pub window_width: Option<i64>,
    pub window_height: Option<i64>,
}

pub fn normalize_launcher_window_settings(settings: PartialLauncherWindowSettings) -> LauncherWindowSettings {
    LauncherWindowSettings {
        launch_maximized: settings.launch_maximized.unwrap_or(false),
        window_width: normalize_dimension(
            settings.window_width,
            DEFAULT_LAUNCHER_WINDOW_WIDTH,
            LAUNCHER_WINDOW_MIN_WIDTH,
            LAUNCHER_WINDOW_MAX_WIDTH,
        ),
        window_height: normalize_dimension(
            settings.window_height,
            DEFAULT_LAUNCHER_WINDOW_HEIGHT,
            LAUNCHER_WINDOW_MIN_HEIGHT,
            LAUNCHER_WINDOW_MAX_HEIGHT,
        ),
    }
}

pub fn is_valid_launcher_window_settings(settings: &LauncherWindowSettings) -> bool {
    // `launch_maximized` is a plain bool in Rust, so it is always valid;
    // only the dimensions need range checks (mirrors the TS NaN-fallback path).
    is_in_range(settings.window_width, LAUNCHER_WINDOW_MIN_WIDTH, LAUNCHER_WINDOW_MAX_WIDTH)
        && is_in_range(settings.window_height, LAUNCHER_WINDOW_MIN_HEIGHT, LAUNCHER_WINDOW_MAX_HEIGHT)
}

fn is_in_range(value: i64, minimum: i64, maximum: i64) -> bool {
    value >= minimum && value <= maximum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_missing_or_invalid_values_to_safe_defaults() {
        let settings = normalize_launcher_window_settings(PartialLauncherWindowSettings {
            launch_maximized: None,
            window_width: Some(799),
            window_height: Some(99999),
        });
        assert_eq!(settings, LauncherWindowSettings::default());
    }

    #[test]
    fn preserves_valid_size_and_maximize_preference() {
        let settings = normalize_launcher_window_settings(PartialLauncherWindowSettings {
            launch_maximized: Some(true),
            window_width: Some(1920),
            window_height: Some(1080),
        });
        assert_eq!(
            settings,
            LauncherWindowSettings {
                launch_maximized: true,
                window_width: 1920,
                window_height: 1080,
            }
        );
        assert!(is_valid_launcher_window_settings(&settings));
    }

    #[test]
    fn rejects_dimensions_below_native_minimum() {
        assert!(!is_valid_launcher_window_settings(&LauncherWindowSettings {
            launch_maximized: false,
            window_width: LAUNCHER_WINDOW_MIN_WIDTH - 1,
            window_height: LAUNCHER_WINDOW_MIN_HEIGHT,
        }));
    }

    #[test]
    fn validates_draft_without_changing_it() {
        assert_eq!(
            validate_launcher_window_dimension("1", "Width", LAUNCHER_WINDOW_MIN_WIDTH, 7680),
            DimensionValidation {
                value: None,
                error: Some("Width must be at least 800 pixels.".to_string()),
            }
        );
        assert_eq!(
            validate_launcher_window_dimension("1280", "Width", LAUNCHER_WINDOW_MIN_WIDTH, 7680),
            DimensionValidation { value: Some(1280), error: None }
        );
    }
}
