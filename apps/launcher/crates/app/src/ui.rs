//! Minimal 1:1 primitive kit: shadcn-style button / badge / progress / card /
//! rows, all colored from [`ActiveTheme`]. Views attach click handlers with
//! `cx.listener`; these helpers only build styled elements.

use gpui::{
    div, prelude::*, px, relative, App, Div, FontWeight, Hsla, IntoElement, RenderOnce, Rgba, Svg,
    Window,
};

use crate::theme::{readable_color, ActiveTheme};

/// Modal overlay: dimmed backdrop + centered card. Views attach Escape/close
/// handlers; closing is idempotent so bubbled duplicates are harmless.
pub fn dialog_shell(theme: &ActiveTheme, title: &str, body: Div, footer: Div) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x00000099))
        .child(
            div()
                .w(px(460.0))
                .rounded(theme.radius())
                .bg(theme.popover())
                .border_1()
                .border_color(theme.border())
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_base()
                        .text_color(theme.foreground())
                        .child(title.to_string()),
                )
                .child(body)
                .child(div().flex().flex_row().justify_end().gap_2().child(footer)),
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
    Outline,
}

#[derive(IntoElement)]
pub struct Icon {
    name: &'static str,
    size: f32,
}

pub fn icon(name: &'static str, size: f32) -> Icon {
    Icon { name, size }
}

impl Icon {
    fn into_svg(self, inherited_color: Hsla) -> Svg {
        gpui::svg()
            .path(format!("icons/{}.svg", self.name))
            .size(px(self.size))
            .flex_shrink_0()
            // Svg::paint requires its own color in GPUI 0.3.8. Resolve the
            // parent's text style during layout, including button/hover colors.
            .text_color(inherited_color)
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        self.into_svg(window.text_style().color)
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn icon_supplies_the_color_required_by_gpui_svg_paint() {
        // GPUI 0.3.8 Svg::paint skips drawing when its own text color is None,
        // even when an ancestor supplies a text color.
        for color in [
            gpui::rgb(0xfafafa),
            gpui::rgb(0x0a0a0a),
            gpui::rgb(0xfbbf24),
        ] {
            let mut glyph = icon("plus", 14.0).into_svg(color.into());
            assert_eq!(
                glyph.style().text.color,
                Some(color.into()),
                "GPUI will skip painting this icon or use the wrong theme color"
            );
        }
    }
}

#[track_caller]
pub fn icon_btn(
    theme: &ActiveTheme,
    name: &'static str,
    variant: ButtonVariant,
) -> gpui::Stateful<Div> {
    let mut control = btn(theme, "", variant);
    control.interactivity().element_id = Some(
        format!(
            "icon:{name}:{}:{}",
            std::panic::Location::caller().file(),
            std::panic::Location::caller().line()
        )
        .into(),
    );
    control
        .aria_label(name.replace('-', " "))
        .w(px(32.0))
        .px_0()
        .gap_0()
        .child(icon(name, 16.0))
}

pub fn tint(mut color: Rgba, opacity: f32) -> Rgba {
    color.a *= opacity;
    color
}

pub fn mix(a: Rgba, b: Rgba, weight: f32) -> Rgba {
    let other = 1.0 - weight;
    Rgba {
        r: a.r * weight + b.r * other,
        g: a.g * weight + b.g * other,
        b: a.b * weight + b.b * other,
        a: a.a * weight + b.a * other,
    }
}

#[track_caller]
pub fn checkbox(theme: &ActiveTheme, label: &str, checked: bool) -> gpui::Stateful<Div> {
    let location = std::panic::Location::caller();
    div()
        .id(format!(
            "checkbox:{}:{}:{label}",
            location.file(),
            location.line()
        ))
        .role(gpui::accesskit::Role::CheckBox)
        .aria_label(label.to_string())
        .aria_toggled(if checked {
            gpui::accesskit::Toggled::True
        } else {
            gpui::accesskit::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .focus_visible(|style| style.border_2().border_color(theme.primary()))
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .cursor_pointer()
        .child(
            div()
                .size(px(16.0))
                .flex_shrink_0()
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.primary())
                .bg(if checked {
                    theme.primary()
                } else {
                    gpui::rgba(0x00000000)
                })
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.primary_foreground())
                .when(checked, |el| el.child(icon("check", 12.0))),
        )
        .when(!label.is_empty(), |el| el.child(label.to_string()))
}

/// Match the accepted navigation colors: selected background, muted inactive.
#[track_caller]
pub fn tab_btn(theme: &ActiveTheme, label: &str, selected: bool) -> gpui::Stateful<Div> {
    button_base(label)
        .role(gpui::accesskit::Role::Tab)
        .aria_selected(selected)
        .h(px(28.0))
        .px(px(10.0))
        .text_xs()
        .bg(if selected {
            theme.background()
        } else {
            theme.muted()
        })
        .text_color(if selected {
            theme.foreground()
        } else {
            theme.muted_foreground()
        })
        .hover(|style| {
            if selected {
                style.bg(theme.background())
            } else {
                style
                    .bg(theme.accent())
                    .text_color(theme.accent_foreground())
            }
        })
}

#[track_caller]
pub fn btn(theme: &ActiveTheme, label: &str, variant: ButtonVariant) -> gpui::Stateful<Div> {
    let (bg, fg, hover) = match variant {
        ButtonVariant::Primary => (
            theme.primary(),
            theme.primary_foreground(),
            tint(theme.primary(), 0.95),
        ),
        ButtonVariant::Secondary => (
            theme.secondary(),
            theme.secondary_foreground(),
            tint(theme.primary(), 0.18),
        ),
        ButtonVariant::Ghost => (
            gpui::rgba(0x00000000),
            theme.foreground(),
            tint(theme.primary(), 0.14),
        ),
        ButtonVariant::Danger => (
            theme.destructive(),
            theme.destructive_foreground(),
            tint(theme.destructive(), 0.9),
        ),
        ButtonVariant::Outline => (
            theme.card(),
            readable_color(theme.card_foreground(), theme.card(), 4.5),
            theme.accent(),
        ),
    };
    button_base(label)
        .bg(bg)
        .text_color(fg)
        .when(variant == ButtonVariant::Outline, |el| {
            el.border_1().border_color(theme.control_border(bg))
        })
        .hover(move |style| {
            let style = style.bg(hover);
            if variant == ButtonVariant::Outline {
                style
                    .border_color(theme.control_border(hover))
                    .text_color(readable_color(theme.accent_foreground(), hover, 4.5))
            } else {
                style
            }
        })
}

// GPUI permits one hover refinement per element, so tabs and buttons each
// attach their own hover style to this shared, non-interactive base.
#[track_caller]
pub(crate) fn button_base(label: &str) -> gpui::Stateful<Div> {
    let location = std::panic::Location::caller();
    div()
        .id(format!(
            "control:{}:{}:{label}",
            location.file(),
            location.line()
        ))
        .role(gpui::accesskit::Role::Button)
        .when(!label.is_empty(), |control| {
            control.aria_label(label.to_string())
        })
        .focusable()
        .tab_stop(true)
        .focus_visible(|style| style.border_2().border_color(gpui::rgb(0x60a5fa)))
        .px_3()
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded(px(6.0))
        .cursor_pointer()
        .font_weight(FontWeight::MEDIUM)
        .text_sm()
        .when(!label.is_empty(), |element| {
            element.child(label.to_string())
        })
}

/// A full-width settings action with a persistent, matching click boundary.
#[track_caller]
pub fn action_button(theme: &ActiveTheme, label: &str, glyph: &'static str) -> gpui::Stateful<Div> {
    btn(theme, "", ButtonVariant::Outline)
        .aria_label(label.to_string())
        .h(px(40.0))
        .w_full()
        .justify_start()
        .gap_2()
        .child(icon(glyph, 16.0))
        .child(label.to_string())
        .child(div().flex_1())
        .child(icon("chevron-right", 14.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Normal,
    Danger,
}

pub fn badge(theme: &ActiveTheme, label: &str, kind: BadgeKind) -> Div {
    let (bg, fg) = match kind {
        BadgeKind::Normal => (theme.secondary(), theme.secondary_foreground()),
        BadgeKind::Danger => (theme.destructive(), theme.destructive_foreground()),
    };
    div()
        .px(px(10.0))
        .py(px(2.0))
        .border_1()
        .border_color(gpui::rgba(0x00000000))
        .rounded(px(6.0))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(bg)
        .text_color(fg)
        .text_xs()
        .child(label.to_string())
}

pub fn progress_bar(theme: &ActiveTheme, pct: f64) -> Div {
    let pct = pct.clamp(0.0, 1.0) as f32;
    div()
        .w_full()
        .h_2()
        .rounded_full()
        .bg(theme.secondary())
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(theme.primary())
                .w(relative(pct)),
        )
}

/// A persistent boundary for grouped content, including rows with several actions.
pub fn card(theme: &ActiveTheme) -> Div {
    div()
        .rounded(theme.radius())
        .bg(theme.card())
        .text_color(theme.card_foreground())
        .border_1()
        .border_color(theme.border())
}

/// Selectable records stay identifiable before hover. Menus and navigation
/// continue to use ghost buttons; data rows use this outlined surface.
#[track_caller]
pub fn list_item(theme: &ActiveTheme, label: &str, selected: bool) -> gpui::Stateful<Div> {
    let surface = if selected {
        theme.secondary()
    } else {
        theme.card()
    };
    button_base(label)
        .flex_shrink_0()
        .bg(surface)
        .text_color(readable_color(
            if selected {
                theme.secondary_foreground()
            } else {
                theme.card_foreground()
            },
            surface,
            4.5,
        ))
        .border_1()
        .border_color(if selected {
            readable_color(theme.primary(), surface, 3.0)
        } else {
            theme.control_border(surface)
        })
        .rounded(theme.radius())
        .hover(|style| {
            style
                .bg(theme.accent())
                .text_color(readable_color(
                    theme.accent_foreground(),
                    theme.accent(),
                    4.5,
                ))
                .border_color(if selected {
                    readable_color(theme.primary(), theme.accent(), 3.0)
                } else {
                    theme.control_border(theme.accent())
                })
        })
}

pub fn section_title(theme: &ActiveTheme, text: &str) -> Div {
    div()
        .text_sm()
        .text_color(theme.muted_foreground())
        .child(text.to_string())
}

pub fn empty_state(theme: &ActiveTheme, text: &str) -> Div {
    div()
        .p_8()
        .text_sm()
        .text_color(theme.muted_foreground())
        .child(text.to_string())
}
