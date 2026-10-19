//! Map VS Code workbench colors onto the launcher's native token vocabulary.
use crate::theme::{derive_tokens, resolve_preset_or_default, ThemeTokens};
use serde_json::Value;

fn channels(raw: &str) -> Option<[u8; 4]> {
    let hex = raw.strip_prefix('#')?;
    let expanded = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => hex.to_string(),
        _ => return None,
    };
    if !expanded.is_ascii() || !expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&expanded[0..2], 16).ok()?,
        u8::from_str_radix(&expanded[2..4], 16).ok()?,
        u8::from_str_radix(&expanded[4..6], 16).ok()?,
        if expanded.len() == 8 {
            u8::from_str_radix(&expanded[6..8], 16).ok()?
        } else {
            255
        },
    ])
}
pub fn opaque_color(raw: &str, background: &str) -> Option<String> {
    let source = channels(raw)?;
    let base = channels(background)?;
    let blend = |i: usize| {
        ((u32::from(source[i]) * u32::from(source[3])
            + u32::from(base[i]) * (255 - u32::from(source[3]))
            + 127)
            / 255) as u8
    };
    Some(format!("#{:02x}{:02x}{:02x}", blend(0), blend(1), blend(2)))
}
pub fn dark_mode(ui_theme: &str) -> bool {
    !matches!(ui_theme, "vs" | "hc-light" | "light")
}

pub fn convert(document: &Value, dark: bool) -> ThemeTokens {
    let base = resolve_preset_or_default("monochrome");
    let mut tokens = if dark { base.dark } else { base.light };
    let colors = &document["colors"];
    let get = |names: &[&str], fallback: &str, background: &str| -> String {
        names
            .iter()
            .find_map(|name| {
                colors[*name]
                    .as_str()
                    .and_then(|raw| opaque_color(raw, background))
            })
            .unwrap_or_else(|| fallback.to_owned())
    };
    tokens.background = get(
        &["editor.background"],
        &tokens.background,
        &tokens.background,
    );
    tokens.foreground = get(
        &["editor.foreground", "foreground"],
        &tokens.foreground,
        &tokens.background,
    );
    tokens.card = get(
        &["sideBar.background", "panel.background"],
        &tokens.card,
        &tokens.background,
    );
    tokens.card_foreground = get(
        &["sideBar.foreground", "foreground", "editor.foreground"],
        &tokens.foreground,
        &tokens.card,
    );
    tokens.popover = get(
        &["editorWidget.background", "menu.background"],
        &tokens.card,
        &tokens.background,
    );
    tokens.popover_foreground = get(
        &["editorWidget.foreground", "menu.foreground"],
        &tokens.foreground,
        &tokens.popover,
    );
    tokens.primary = get(
        &[
            "button.background",
            "focusBorder",
            "activityBarBadge.background",
        ],
        &tokens.primary,
        &tokens.background,
    );
    tokens.primary_foreground = get(
        &["button.foreground"],
        &tokens.primary_foreground,
        &tokens.primary,
    );
    tokens.secondary = get(
        &["button.secondaryBackground", "input.background"],
        &tokens.secondary,
        &tokens.background,
    );
    tokens.secondary_foreground = get(
        &["button.secondaryForeground", "input.foreground"],
        &tokens.foreground,
        &tokens.secondary,
    );
    tokens.muted = tokens.card.clone();
    tokens.muted_foreground = get(
        &["descriptionForeground", "disabledForeground"],
        &tokens.muted_foreground,
        &tokens.muted,
    );
    tokens.accent = get(
        &["list.activeSelectionBackground", "list.hoverBackground"],
        &tokens.accent,
        &tokens.background,
    );
    tokens.accent_foreground = get(
        &["list.activeSelectionForeground"],
        &tokens.foreground,
        &tokens.accent,
    );
    tokens.destructive = get(
        &["errorForeground"],
        &tokens.destructive,
        &tokens.background,
    );
    tokens.destructive_foreground = contrast_text(&tokens.destructive);
    tokens.border = get(
        &["panel.border", "contrastBorder", "sideBar.border"],
        &tokens.border,
        &tokens.background,
    );
    tokens.input = get(
        &["input.border", "input.background"],
        &tokens.border,
        &tokens.card,
    );
    tokens.ring = get(&["focusBorder"], &tokens.primary, &tokens.background);
    // Missing button text is derived rather than copying a mismatched base palette.
    if colors["button.foreground"]
        .as_str()
        .and_then(|s| channels(s))
        .is_none()
    {
        tokens.primary_foreground = contrast_text(&tokens.primary);
    }
    derive_tokens(tokens, &[])
}
fn contrast_text(background: &str) -> String {
    let c = channels(background).unwrap_or([0, 0, 0, 255]);
    let luminance: f64 = [0.2126, 0.7152, 0.0722]
        .iter()
        .enumerate()
        .map(|(i, weight)| {
            let v = f64::from(c[i]) / 255.;
            weight
                * if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
        })
        .sum();
    if luminance > 0.179 {
        "#000000"
    } else {
        "#ffffff"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_colors_and_composites_alpha_without_invalidating_native_tokens() {
        let doc = serde_json::json!({"colors":{"editor.background":"#282a36","editor.foreground":"#f8f8f2","button.background":"#bd93f9","list.activeSelectionBackground":"#ffffff20"}});
        let t = convert(&doc, true);
        assert_eq!(t.background, "#282a36");
        assert_eq!(t.primary, "#bd93f9");
        assert_eq!(t.primary_foreground, "#000000");
        assert_ne!(t.accent, "#ffffff20");
        assert!(crate::theme::is_valid_theme_tokens(&t));
        assert_eq!(opaque_color("#abc", "#000000").unwrap(), "#aabbcc");
        assert!(opaque_color("#💩", "#000000").is_none());
        assert!(crate::theme::is_valid_theme_tokens(&convert(
            &serde_json::json!({}),
            false
        )));
    }
}
