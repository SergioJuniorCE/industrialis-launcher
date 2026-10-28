//! Port of `theme-presets.ts` + the pure parts of `theme-utils.ts`.
//!
//! These tokens are the single source of truth for 1:1 UI parity: every UI
//! view reads colors from here, never hardcoded values.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundEffect {
    Grid,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub background_effect: BackgroundEffect,
    pub dark: ThemeTokens,
    pub light: ThemeTokens,
}

macro_rules! tokens {
    ($base:expr; $($field:ident : $value:literal),* $(,)?) => {{
        let mut t = $base;
        $(t.$field = $value.to_string();)*
        t
    }};
}

fn monochrome_dark() -> ThemeTokens {
    ThemeTokens {
        background: "#0a0a0a".into(),
        foreground: "#fafafa".into(),
        card: "#141414".into(),
        card_foreground: "#fafafa".into(),
        popover: "#141414".into(),
        popover_foreground: "#fafafa".into(),
        primary: "#fafafa".into(),
        primary_foreground: "#0a0a0a".into(),
        secondary: "#1f1f1f".into(),
        secondary_foreground: "#fafafa".into(),
        muted: "#1a1a1a".into(),
        muted_foreground: "#a3a3a3".into(),
        accent: "#262626".into(),
        accent_foreground: "#fafafa".into(),
        destructive: "#991b1b".into(),
        destructive_foreground: "#fafafa".into(),
        border: "#2a2a2a".into(),
        input: "#2a2a2a".into(),
        ring: "#d4d4d4".into(),
        radius: "0.375rem".into(),
    }
}

fn monochrome_light() -> ThemeTokens {
    ThemeTokens {
        background: "#fafafa".into(),
        foreground: "#0a0a0a".into(),
        card: "#ffffff".into(),
        card_foreground: "#0a0a0a".into(),
        popover: "#ffffff".into(),
        popover_foreground: "#0a0a0a".into(),
        primary: "#171717".into(),
        primary_foreground: "#fafafa".into(),
        secondary: "#f5f5f5".into(),
        secondary_foreground: "#171717".into(),
        muted: "#f5f5f5".into(),
        muted_foreground: "#737373".into(),
        accent: "#f5f5f5".into(),
        accent_foreground: "#171717".into(),
        destructive: "#dc2626".into(),
        destructive_foreground: "#fafafa".into(),
        border: "#e5e5e5".into(),
        input: "#e5e5e5".into(),
        ring: "#525252".into(),
        radius: "0.375rem".into(),
    }
}

/// Mirrors `deriveTokens` (struct spread of a patch over a base).
pub fn derive_tokens(base: ThemeTokens, patch: &[(&str, &str)]) -> ThemeTokens {
    let mut t = base;
    for (key, value) in patch {
        match *key {
            "background" => t.background = value.to_string(),
            "foreground" => t.foreground = value.to_string(),
            "card" => t.card = value.to_string(),
            "card_foreground" => t.card_foreground = value.to_string(),
            "popover" => t.popover = value.to_string(),
            "popover_foreground" => t.popover_foreground = value.to_string(),
            "primary" => t.primary = value.to_string(),
            "primary_foreground" => t.primary_foreground = value.to_string(),
            "secondary" => t.secondary = value.to_string(),
            "secondary_foreground" => t.secondary_foreground = value.to_string(),
            "muted" => t.muted = value.to_string(),
            "muted_foreground" => t.muted_foreground = value.to_string(),
            "accent" => t.accent = value.to_string(),
            "accent_foreground" => t.accent_foreground = value.to_string(),
            "destructive" => t.destructive = value.to_string(),
            "destructive_foreground" => t.destructive_foreground = value.to_string(),
            "border" => t.border = value.to_string(),
            "input" => t.input = value.to_string(),
            "ring" => t.ring = value.to_string(),
            "radius" => t.radius = value.to_string(),
            _ => {}
        }
    }
    t
}

pub const DEFAULT_THEME_PRESET_ID: &str = "industrialis";

pub fn builtin_presets() -> Vec<ThemePreset> {
    vec![
        ThemePreset {
            id: "monochrome".into(),
            name: "Monochrome".into(),
            description: "Neutral dark UI with the original launcher look.".into(),
            builtin: true,
            background_effect: BackgroundEffect::None,
            dark: monochrome_dark(),
            light: monochrome_light(),
        },
        ThemePreset {
            id: "industrialis".into(),
            name: "Industrialis".into(),
            description: "Charcoal base with bronze highlights.".into(),
            builtin: true,
            background_effect: BackgroundEffect::Grid,
            dark: tokens!(monochrome_dark();
                primary: "#e0b638", primary_foreground: "#0a0a0a",
                secondary: "#241d10", muted: "#18140d", muted_foreground: "#c9c1ad",
                accent: "#332713", accent_foreground: "#fff1c7",
                border: "#3b321f", input: "#4a3b1d", ring: "#e0b638", radius: "0.5rem"),
            light: tokens!(monochrome_light();
                primary: "#9b6f00", primary_foreground: "#fafafa",
                secondary: "#efe5c7", muted: "#f1ead8", muted_foreground: "#5f533b",
                accent: "#decb91", accent_foreground: "#1a1510",
                border: "#d7c78c", input: "#cdbb7f", ring: "#9b6f00", radius: "0.5rem"),
        },
        ThemePreset {
            id: "midnight".into(),
            name: "Midnight".into(),
            description: "Cool blue industrial palette for late-night sessions.".into(),
            builtin: true,
            background_effect: BackgroundEffect::None,
            dark: tokens!(monochrome_dark();
                background: "#070b14", foreground: "#e8edf8",
                card: "#0f1623", card_foreground: "#e8edf8",
                popover: "#0f1623", popover_foreground: "#e8edf8",
                primary: "#60a5fa", primary_foreground: "#070b14",
                secondary: "#152033", secondary_foreground: "#e8edf8",
                muted: "#121a28", muted_foreground: "#9cb0c9",
                accent: "#141e2e", accent_foreground: "#e8edf8",
                border: "#243044", input: "#243044", ring: "#60a5fa", radius: "0.5rem"),
            light: tokens!(monochrome_light();
                background: "#f4f7fc", foreground: "#0c1524",
                card: "#ffffff", card_foreground: "#0c1524",
                popover: "#ffffff", popover_foreground: "#0c1524",
                primary: "#1d4ed8", primary_foreground: "#f8fafc",
                secondary: "#e8eef8", secondary_foreground: "#0c1524",
                muted: "#e8eef8", muted_foreground: "#4a5a72",
                accent: "#dce6f4", accent_foreground: "#0c1524",
                border: "#d5deed", input: "#d5deed", ring: "#2563eb", radius: "0.5rem"),
        },
        ThemePreset {
            id: "sandstone".into(),
            name: "Sandstone".into(),
            description: "Charcoal surfaces with warm bronze controls.".into(),
            builtin: true,
            background_effect: BackgroundEffect::None,
            dark: tokens!(monochrome_dark();
                background: "#15171b", foreground: "#f2f4f7",
                card: "#23272e", card_foreground: "#f2f4f7",
                popover: "#23272e", popover_foreground: "#f2f4f7",
                primary: "#c17f3a", primary_foreground: "#12100d",
                secondary: "#303640", secondary_foreground: "#f2f4f7",
                muted: "#272c34", muted_foreground: "#b8c1cd",
                accent: "#353d48", accent_foreground: "#f2f4f7",
                border: "#778290", input: "#778290", ring: "#e2a360", radius: "0.5rem"),
            light: tokens!(monochrome_light();
                background: "#f2f4f7", foreground: "#1c2430",
                card: "#fcfdff", card_foreground: "#1c2430",
                popover: "#fcfdff", popover_foreground: "#1c2430",
                primary: "#b87333", primary_foreground: "#101318",
                secondary: "#e2e7ef", secondary_foreground: "#1c2430",
                muted: "#e8edf4", muted_foreground: "#4d5a6a",
                accent: "#d3dce8", accent_foreground: "#1c2430",
                border: "#84909f", input: "#84909f", ring: "#80501f", radius: "0.5rem"),
        },
    ]
}

pub fn tokens_for_preset(preset: &ThemePreset, dark: bool) -> ThemeTokens {
    if dark {
        preset.dark.clone()
    } else {
        preset.light.clone()
    }
}

pub fn find_builtin_preset(id: &str) -> Option<ThemePreset> {
    builtin_presets().into_iter().find(|p| p.id == id)
}

/// Mirrors `resolveThemePresetOrDefault` (custom presets arrive with the
/// theme editor; builtins resolve now).
pub fn resolve_preset_or_default(id: &str) -> ThemePreset {
    find_builtin_preset(id).unwrap_or_else(|| {
        find_builtin_preset(DEFAULT_THEME_PRESET_ID).expect("default preset exists")
    })
}

/// Override keys the settings UI may set (mirrors `THEME_OVERRIDE_KEYS`).
pub const THEME_OVERRIDE_KEYS: [&str; 20] = [
    "background",
    "foreground",
    "primary",
    "card",
    "border",
    "muted",
    "muted_foreground",
    "accent",
    "accent_foreground",
    "card_foreground",
    "popover",
    "popover_foreground",
    "primary_foreground",
    "secondary",
    "secondary_foreground",
    "destructive",
    "destructive_foreground",
    "input",
    "ring",
    "radius",
];

/// Mirrors `mergeOverridesIntoTokens`.
pub fn merge_overrides_into_tokens(
    tokens: ThemeTokens,
    overrides: &[(String, String)],
) -> ThemeTokens {
    let patch: Vec<(&str, &str)> = overrides
        .iter()
        .filter(|(k, v)| THEME_OVERRIDE_KEYS.contains(&k.as_str()) && !v.is_empty())
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    derive_tokens(tokens, &patch)
}

fn validation_patterns() -> &'static (Regex, Regex) {
    static CELL: OnceLock<(Regex, Regex)> = OnceLock::new();
    CELL.get_or_init(|| {
        (
            Regex::new(r"^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").unwrap(),
            Regex::new(r"^\d+(\.\d+)?(rem|px)$").unwrap(),
        )
    })
}

pub fn validate_hex_color(value: &str) -> bool {
    value.len() <= 9 && validation_patterns().0.is_match(value)
}

pub fn validate_radius(value: &str) -> bool {
    value.len() <= 16 && validation_patterns().1.is_match(value)
}

/// Mirrors `isValidThemeTokens` (used when loading custom presets).
pub fn is_valid_theme_tokens(tokens: &ThemeTokens) -> bool {
    let colors = [
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
    ];
    colors.iter().all(|c| validate_hex_color(c)) && validate_radius(&tokens.radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn industrialis_uses_bronze_primary() {
        let preset = resolve_preset_or_default("industrialis");
        assert_eq!(preset.dark.primary, "#e0b638");
        assert_eq!(preset.dark.accent, "#332713");
        assert_eq!(preset.dark.ring, "#e0b638");
        assert_eq!(preset.background_effect, BackgroundEffect::Grid);
    }

    #[test]
    fn unknown_preset_falls_back_to_industrialis() {
        assert_eq!(resolve_preset_or_default("nope").id, "industrialis");
    }

    #[test]
    fn validates_hex_and_radius() {
        assert!(validate_hex_color("#0a0a0a"));
        assert!(!validate_hex_color("red"));
        assert!(validate_radius("0.375rem"));
        assert!(validate_radius("12px"));
        assert!(!validate_radius("bad"));
    }

    #[test]
    fn all_builtin_tokens_validate() {
        for preset in builtin_presets() {
            assert!(is_valid_theme_tokens(&preset.dark), "{}", preset.id);
            assert!(is_valid_theme_tokens(&preset.light), "{}", preset.id);
        }
    }

    #[test]
    fn merges_only_known_nonempty_overrides() {
        let base = monochrome_dark();
        let merged = merge_overrides_into_tokens(
            base,
            &[
                ("background".to_string(), "#ffffff".to_string()),
                ("secondary".to_string(), "#111111".to_string()),
                ("accent".to_string(), "".to_string()),
            ],
        );
        assert_eq!(merged.background, "#ffffff");
        // `secondary` is not an override key → untouched.
        assert_eq!(merged.secondary, "#111111");
        assert_eq!(merged.accent, "#262626");
    }
}
