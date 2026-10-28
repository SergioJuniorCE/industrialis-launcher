//! UI bridge over `industrialis-core` theme tokens.
//!
//! Every color in the UI comes from here — no hardcoded hex in views.

use gpui::{px, Pixels, Rgba};
use industrialis_core::launcher_settings::WindowMaterial;
use industrialis_core::theme::{
    merge_overrides_into_tokens, resolve_preset_or_default, tokens_for_preset, ThemeTokens,
};

#[derive(Debug, Clone)]
pub struct ActiveTheme {
    pub tokens: ThemeTokens,
    pub editor_theme: Option<serde_json::Value>,
    pub window_material: WindowMaterial,
    pub window_tint_opacity: u8,
    #[allow(dead_code)]
    pub dark: bool,
    #[allow(dead_code)]
    pub preset_id: String,
}

impl ActiveTheme {
    pub fn from_preferences(
        settings: &industrialis_core::launcher_settings::LauncherSettingsData,
    ) -> Self {
        let dark = settings.theme_mode == industrialis_core::launcher_settings::ThemeMode::Dark;
        let mut theme = Self::from_settings(
            if dark { "dark" } else { "light" },
            &settings.theme_preset,
            &settings.theme_overrides.as_pairs(),
        );
        if let Some(preset) = settings
            .custom_theme_presets
            .iter()
            .filter(|raw| raw["id"].as_str() == Some(&settings.theme_preset))
            .find_map(|raw| {
                serde_json::from_value::<industrialis_core::theme::ThemePreset>(raw.clone()).ok()
            })
        {
            let tokens = tokens_for_preset(&preset, dark);
            if industrialis_core::theme::is_valid_theme_tokens(&tokens) {
                theme.tokens =
                    merge_overrides_into_tokens(tokens, &settings.theme_overrides.as_pairs());
                theme.preset_id = preset.id;
            }
        }
        if let Some(raw) = settings
            .custom_theme_presets
            .iter()
            .find(|raw| raw["id"].as_str() == Some(&settings.theme_preset))
        {
            let mode_matches = raw["openvsx"]["mode"]
                .as_str()
                .is_none_or(|mode| (mode == "dark") == dark);
            if mode_matches {
                theme.editor_theme = raw.get("editorTheme").filter(|v| v.is_object()).cloned();
            }
        }
        theme
    }
    pub fn from_settings(
        theme_mode: &str,
        preset_id: &str,
        overrides: &[(String, String)],
    ) -> Self {
        let dark = theme_mode != "light";
        let preset = resolve_preset_or_default(preset_id);
        let tokens = merge_overrides_into_tokens(tokens_for_preset(&preset, dark), overrides);
        Self {
            tokens,
            editor_theme: None,
            window_material: WindowMaterial::Opaque,
            window_tint_opacity: 60,
            dark,
            preset_id: preset.id.clone(),
        }
    }

    pub fn code_background(&self) -> Rgba {
        if self.editor_theme.is_some() {
            self.background()
        } else {
            editor_background(self.dark)
        }
    }
    pub fn code_active_background(&self) -> Rgba {
        self.editor_theme
            .as_ref()
            .and_then(|raw| raw["colors"]["editor.lineHighlightBackground"].as_str())
            .and_then(|color| {
                industrialis_core::vscode_theme::opaque_color(color, &self.tokens.background)
            })
            .map(|color| parse_hex(&color))
            .unwrap_or_else(|| {
                if self.editor_theme.is_some() {
                    self.code_background()
                } else {
                    editor_active_background(self.dark)
                }
            })
    }

    fn hex(&self, value: &str) -> Rgba {
        parse_hex(value)
    }

    pub fn background(&self) -> Rgba {
        self.hex(&self.tokens.background)
    }

    /// Tint the window only; controls, text, icons and popovers stay opaque.
    pub fn window_background(&self) -> Rgba {
        self.window_surface(self.background())
    }

    pub fn window_surface(&self, mut color: Rgba) -> Rgba {
        if self.window_material != WindowMaterial::Opaque {
            color.a *= f32::from(self.window_tint_opacity) / 100.0;
        }
        color
    }

    /// Chrome overlays the root tint, so use a lighter layer here.
    pub fn chrome_background(&self) -> Rgba {
        let mut color = self.card();
        if self.window_material != WindowMaterial::Opaque {
            color.a = 0.24;
        }
        color
    }
    pub fn foreground(&self) -> Rgba {
        self.hex(&self.tokens.foreground)
    }
    pub fn card(&self) -> Rgba {
        self.hex(&self.tokens.card)
    }
    pub fn card_foreground(&self) -> Rgba {
        self.hex(&self.tokens.card_foreground)
    }
    #[allow(dead_code)]
    pub fn popover(&self) -> Rgba {
        self.hex(&self.tokens.popover)
    }
    pub fn primary(&self) -> Rgba {
        self.hex(&self.tokens.primary)
    }
    pub fn primary_foreground(&self) -> Rgba {
        readable_color(
            self.hex(&self.tokens.primary_foreground),
            self.primary(),
            4.5,
        )
    }
    pub fn secondary(&self) -> Rgba {
        self.hex(&self.tokens.secondary)
    }
    pub fn secondary_foreground(&self) -> Rgba {
        self.hex(&self.tokens.secondary_foreground)
    }
    pub fn muted(&self) -> Rgba {
        self.hex(&self.tokens.muted)
    }
    pub fn muted_foreground(&self) -> Rgba {
        self.hex(&self.tokens.muted_foreground)
    }
    pub fn accent(&self) -> Rgba {
        self.hex(&self.tokens.accent)
    }
    pub fn accent_foreground(&self) -> Rgba {
        self.hex(&self.tokens.accent_foreground)
    }
    pub fn destructive(&self) -> Rgba {
        self.hex(&self.tokens.destructive)
    }
    pub fn destructive_foreground(&self) -> Rgba {
        self.hex(&self.tokens.destructive_foreground)
    }
    pub fn border(&self) -> Rgba {
        self.hex(&self.tokens.border)
    }
    pub fn ring(&self) -> Rgba {
        self.hex(&self.tokens.ring)
    }
    #[allow(dead_code)]
    pub fn input(&self) -> Rgba {
        self.hex(&self.tokens.input)
    }

    /// Imported editor themes can supply a fill color as their input token.
    /// Keep actionable outlines visible without changing the saved palette.
    pub fn control_border(&self, surface: Rgba) -> Rgba {
        readable_color(self.input(), surface, 3.0)
    }

    /// `radius` token (`0.5rem` / `12px`) → pixels (1rem = 16px, like the web).
    pub fn radius(&self) -> Pixels {
        parse_radius(&self.tokens.radius)
    }
}

pub fn editor_background(dark: bool) -> Rgba {
    gpui::rgb(if dark { 0x16191f } else { 0xf8fafc })
}

pub fn editor_active_background(dark: bool) -> Rgba {
    gpui::rgb(if dark { 0x23272e } else { 0xe8edf4 })
}

pub fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
    let luminance = |color: Rgba| {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Preserve the chosen hue while meeting contrast against the given surface.
pub fn readable_color(color: Rgba, background: Rgba, minimum: f32) -> Rgba {
    if contrast_ratio(color, background) >= minimum {
        return color;
    }
    let light = gpui::rgb(0xfefefe);
    let dark = gpui::rgb(0x010203);
    let target = if contrast_ratio(light, background) > contrast_ratio(dark, background) {
        light
    } else {
        dark
    };
    for step in 1..=20 {
        let t = step as f32 / 20.;
        let candidate = Rgba {
            r: color.r + (target.r - color.r) * t,
            g: color.g + (target.g - color.g) * t,
            b: color.b + (target.b - color.b) * t,
            a: 1.,
        };
        if contrast_ratio(candidate, background) >= minimum {
            return candidate;
        }
    }
    target
}

fn hex_digit(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}

pub fn parse_hex(value: &str) -> Rgba {
    // Invalid values render magenta so theme bugs scream during review.
    const INVALID: Rgba = Rgba {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let bytes = value.trim().as_bytes();
    let hex: &[u8] = bytes.strip_prefix(b"#").unwrap_or(bytes);
    if !matches!(hex.len(), 3 | 4 | 6 | 8) || !hex.iter().all(|b| b.is_ascii_hexdigit()) {
        return INVALID;
    }
    let channel = |pair: &[u8]| -> f32 {
        let (hi, lo) = match pair {
            [h, l] => (*h, *l),
            [h] => (*h, *h),
            _ => (0, 0),
        };
        f32::from(hex_digit(hi) * 16 + hex_digit(lo)) / 255.0
    };
    let (r, g, b) = match hex.len() {
        3 | 4 => (
            channel(&hex[0..1]),
            channel(&hex[1..2]),
            channel(&hex[2..3]),
        ),
        _ => (
            channel(&hex[0..2]),
            channel(&hex[2..4]),
            channel(&hex[4..6]),
        ),
    };
    Rgba { r, g, b, a: 1.0 }
}

pub fn parse_radius(value: &str) -> Pixels {
    let value = value.trim();
    if let Some(number) = value
        .strip_suffix("rem")
        .and_then(|n| n.parse::<f32>().ok())
    {
        return px(number * 16.0);
    }
    if let Some(number) = value.strip_suffix("px").and_then(|n| n.parse::<f32>().ok()) {
        return px(number);
    }
    px(6.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_button_text_and_sandstone_surfaces_have_readable_contrast() {
        for mode in ["dark", "light"] {
            for preset in industrialis_core::theme::builtin_presets() {
                let theme = ActiveTheme::from_settings(mode, &preset.id, &[]);
                assert!(
                    contrast_ratio(theme.primary_foreground(), theme.primary()) >= 4.49,
                    "{mode} {} button contrast {}",
                    preset.id,
                    contrast_ratio(theme.primary_foreground(), theme.primary())
                );
            }
            let theme = ActiveTheme::from_settings(mode, "sandstone", &[]);
            assert!(contrast_ratio(theme.foreground(), theme.background()) >= 7.);
            assert!(contrast_ratio(theme.muted_foreground(), theme.card()) >= 4.5);
            assert!(contrast_ratio(theme.border(), theme.card()) >= 3.);
        }
    }

    #[test]
    fn action_borders_stay_visible_with_imported_input_fill_colors() {
        for (dark, background, input, foreground) in [
            (true, "#303446", "#292c3c", "#c6d0f5"),
            (false, "#eff1f5", "#e6e9ef", "#4c4f69"),
        ] {
            let document = serde_json::json!({"colors": {
                "editor.background": background,
                "editor.foreground": foreground,
                "editorWidget.background": background,
                "input.background": input,
                "list.hoverBackground": input,
            }});
            let mut theme =
                ActiveTheme::from_settings(if dark { "dark" } else { "light" }, "monochrome", &[]);
            theme.tokens = industrialis_core::vscode_theme::convert(&document, dark);
            let saved_tokens = theme.tokens.clone();
            for surface in [theme.card(), theme.accent()] {
                assert!(contrast_ratio(theme.control_border(surface), surface) >= 3.0);
            }
            assert_eq!(
                theme.tokens, saved_tokens,
                "Presentation must not rewrite imported colors"
            );
        }
        for preset in industrialis_core::theme::builtin_presets() {
            for mode in ["dark", "light"] {
                let theme = ActiveTheme::from_settings(mode, &preset.id, &[]);
                for surface in [theme.card(), theme.accent()] {
                    assert!(contrast_ratio(theme.control_border(surface), surface) >= 3.0);
                }
            }
        }
    }

    #[test]
    fn parses_hex_forms() {
        let white = parse_hex("#fafafa");
        assert!((white.r - 0.980).abs() < 0.01);
        assert_eq!(
            parse_hex("#000"),
            Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0
            }
        );
        // Invalid values render magenta so theme bugs scream during review.
        assert_eq!(
            parse_hex("nope"),
            Rgba {
                r: 1.0,
                g: 0.0,
                b: 1.0,
                a: 1.0
            }
        );
    }

    #[test]
    fn parses_radius() {
        assert_eq!(parse_radius("0.5rem"), px(8.0));
        assert_eq!(parse_radius("12px"), px(12.0));
    }

    #[test]
    fn industrialis_dark_matches_web() {
        let theme = ActiveTheme::from_settings("dark", "industrialis", &[]);
        assert_eq!(theme.tokens.primary, "#e0b638");
        assert_eq!(theme.tokens.background, "#0a0a0a");
    }

    #[test]
    fn native_materials_tint_surfaces_without_fading_content() {
        for mode in ["dark", "light"] {
            let mut theme = ActiveTheme::from_settings(mode, "industrialis", &[]);
            assert_eq!(theme.window_background(), theme.background());
            assert_eq!(theme.chrome_background(), theme.card());
            for material in [WindowMaterial::Frosted, WindowMaterial::Mica] {
                theme.window_material = material;
                theme.window_tint_opacity = 40;
                assert_eq!(theme.window_background().a, 0.4);
                assert_eq!(theme.window_background().r, theme.background().r);
                assert_eq!(theme.chrome_background().a, 0.24);
                assert_eq!(theme.foreground().a, 1.0);
                assert_eq!(theme.primary().a, 1.0);
                assert_eq!(theme.popover().a, 1.0);
            }
            theme.window_material = WindowMaterial::Opaque;
            assert_eq!(theme.window_background(), theme.background());
        }
    }
}
