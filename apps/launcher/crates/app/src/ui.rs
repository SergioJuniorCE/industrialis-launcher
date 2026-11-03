//! Minimal 1:1 primitive kit: shadcn-style button / badge / progress / card /
//! rows, all colored from [`ActiveTheme`]. Views attach click handlers with
//! `cx.listener`; these helpers only build styled elements.

use std::time::Duration;

use gpui::{
    canvas, div, point, prelude::*, px, relative, Animation, AnimationExt, App, Bounds, Div,
    ElementId, FontWeight, Hsla, IntoElement, PathBuilder, Pixels, RenderOnce, Rgba, SpringAnimation,
    SpringConfig, Svg, Transformation, Window,
};

use crate::theme::{readable_color, ActiveTheme};

/// Modal overlay: dimmed backdrop + centered card. Views attach Escape/close
/// handlers; closing is idempotent so bubbled duplicates are harmless.
pub fn dialog_shell(theme: &ActiveTheme, title: &str, body: Div, footer: Div) -> Div {
    dialog_shell_with_width(theme, title, 460.0, body, footer)
}

/// Same modal shell with an explicit card width for content-heavy dialogs
/// (for example the Prism instance selection list).
pub fn dialog_shell_with_width(
    theme: &ActiveTheme,
    title: &str,
    width: f32,
    body: Div,
    footer: Div,
) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x00000099))
        .child(
            div()
                .w(px(width))
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
    /// A borderless row inside a bordered menu surface.
    MenuItem,
    /// A quiet section row: transparent until it is hovered.
    Row,
}

#[derive(IntoElement)]
pub struct Icon {
    name: &'static str,
    size: f32,
    rotation: Option<f32>,
}

pub fn icon(name: &'static str, size: f32) -> Icon {
    Icon {
        name,
        size,
        rotation: None,
    }
}

impl Icon {
    fn with_rotation(mut self, radians: f32) -> Self {
        self.rotation = Some(radians);
        self
    }

    fn into_svg(self, inherited_color: Hsla) -> Svg {
        let svg = gpui::svg()
            .path(format!("icons/{}.svg", self.name))
            .size(px(self.size))
            .flex_shrink_0()
            // Svg::paint requires its own color in the pinned framework. Resolve the
            // parent's text style during layout, including button/hover colors.
            .text_color(inherited_color);
        match self.rotation {
            Some(rotation) => {
                svg.with_transformation(Transformation::rotate(gpui::radians(rotation)))
            }
            None => svg,
        }
    }
}

/// Rotates a loader glyph while an operation is active. The id must stay
/// stable while the operation runs and be unique for each visible spinner.
pub fn spinner(
    id: impl Into<ElementId>,
    name: &'static str,
    size: f32,
) -> impl IntoElement {
    icon(name, size).with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |icon, phase| icon.with_rotation(phase * std::f32::consts::TAU),
    )
}

#[derive(IntoElement)]
struct ProgressRing {
    progress: f32,
    track: Rgba,
    fill: Rgba,
}

impl ProgressRing {
    fn with_progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }
}

impl RenderOnce for ProgressRing {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let progress = self.progress;
        let track = self.track;
        let fill = self.fill;
        canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                paint_progress_ring(bounds, progress, track, fill, window);
            },
        )
        .absolute()
        .inset_0()
    }
}

fn paint_progress_ring(
    bounds: Bounds<Pixels>,
    progress: f32,
    track: Rgba,
    fill: Rgba,
    window: &mut Window,
) {
    const STROKE: f32 = 2.5;
    let size = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
    if size <= STROKE {
        return;
    }
    let center = bounds.center();
    let radius = (size - STROKE) / 2.0;
    let top = point(center.x, center.y - px(radius));
    let bottom = point(center.x, center.y + px(radius));
    let radii = point(px(radius), px(radius));

    let mut track_path = PathBuilder::stroke(px(STROKE));
    track_path.move_to(top);
    track_path.arc_to(radii, px(0.0), false, true, bottom);
    track_path.arc_to(radii, px(0.0), false, true, top);
    if let Ok(path) = track_path.build() {
        window.paint_path(path, track);
    }

    if progress <= 0.0 {
        return;
    }
    let mut progress_path = PathBuilder::stroke(px(STROKE));
    progress_path.move_to(top);
    if progress >= 1.0 {
        progress_path.arc_to(radii, px(0.0), false, true, bottom);
        progress_path.arc_to(radii, px(0.0), false, true, top);
    } else {
        let angle = -std::f32::consts::FRAC_PI_2 + progress * std::f32::consts::TAU;
        let end = point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        );
        progress_path.arc_to(radii, px(0.0), progress > 0.5, true, end);
    }
    if let Ok(path) = progress_path.build() {
        window.paint_path(path, fill);
    }
}

/// A circular progress indicator that springs toward each new target. The id
/// must stay stable for the lifetime of the indicator.
pub fn progress_ring(
    id: impl Into<ElementId>,
    progress: f32,
    track: Rgba,
    fill: Rgba,
) -> impl IntoElement {
    let target = progress.clamp(0.0, 1.0);
    ProgressRing {
        progress: target,
        track,
        fill,
    }
    .with_spring(
        id,
        SpringAnimation::new(SpringConfig::new(170.0, 26.0, 1.0))
            .to(target)
            .from(0.0)
            .with_epsilon(0.002),
        |ring, progress| ring.with_progress(progress),
    )
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
    fn icon_supplies_the_color_required_by_svg_paint() {
        // The pinned Svg::paint skips drawing when its own text color is None,
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
                "The framework will skip painting this icon or use the wrong theme color"
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

/// The button baseline. Filled and outlined variants share the same height,
/// radius, focus ring and 1px token border; Ghost and menu rows stay quiet.
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
        ButtonVariant::MenuItem => (
            gpui::rgba(0x00000000),
            theme.foreground(),
            tint(theme.primary(), 0.14),
        ),
        ButtonVariant::Row => (
            gpui::rgba(0x00000000),
            theme.foreground(),
            theme.secondary(),
        ),
    };
    let bordered = !matches!(
        variant,
        ButtonVariant::Ghost | ButtonVariant::MenuItem | ButtonVariant::Row
    );
    let border = match variant {
        ButtonVariant::Primary => {
            readable_color(theme.primary_foreground(), bg, 3.0)
        }
        ButtonVariant::Secondary => {
            readable_color(theme.secondary_foreground(), bg, 3.0)
        }
        ButtonVariant::Danger => {
            readable_color(theme.destructive_foreground(), bg, 3.0)
        }
        ButtonVariant::Outline => theme.control_border(bg),
        ButtonVariant::Ghost | ButtonVariant::MenuItem | ButtonVariant::Row => {
            gpui::rgba(0x00000000)
        }
    };
    let hover_border = match variant {
        ButtonVariant::Primary => {
            readable_color(theme.primary_foreground(), hover, 3.0)
        }
        ButtonVariant::Secondary => {
            readable_color(theme.secondary_foreground(), hover, 3.0)
        }
        ButtonVariant::Danger => {
            readable_color(theme.destructive_foreground(), hover, 3.0)
        }
        ButtonVariant::Outline => theme.control_border(hover),
        ButtonVariant::Ghost | ButtonVariant::MenuItem | ButtonVariant::Row => {
            gpui::rgba(0x00000000)
        }
    };
    let button = button_base(label).bg(bg).text_color(fg);
    let button = if bordered {
        button.border_1().border_color(border)
    } else {
        button
    };
    button.hover(move |style| {
        let style = style.bg(hover);
        let style = if bordered {
            style.border_color(hover_border)
        } else {
            style
        };
        if variant == ButtonVariant::Outline {
            style.text_color(readable_color(theme.accent_foreground(), hover, 4.5))
        } else {
            style
        }
    })
}

// The framework permits one hover refinement per element, so tabs and buttons each
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
        .border_color(readable_color(fg, bg, 3.0))
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

/// A row inside a shared list surface. The surface owns the outline; the row
/// only carries its selection and hover state, so long lists stay quiet.
#[track_caller]
pub fn select_row(theme: &ActiveTheme, selected: bool) -> gpui::Stateful<Div> {
    button_base("")
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .justify_between()
        .h(px(34.0))
        .px(px(12.0))
        .rounded(px(0.0))
        .bg(if selected {
            tint(theme.primary(), 0.12)
        } else {
            gpui::rgba(0x00000000)
        })
        .text_color(theme.foreground())
        .hover(move |style| {
            style
                .bg(if selected {
                    tint(theme.primary(), 0.16)
                } else {
                    tint(theme.accent(), 0.3)
                })
                .text_color(theme.foreground())
        })
}

/// The single surface around mutually exclusive choices. Use with [`segment`].
pub fn segmented_control(theme: &ActiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(8.0))
        .bg(tint(theme.muted(), 0.45))
        .border_1()
        .border_color(tint(theme.border(), 0.4))
}

/// One choice inside a [`segmented_control`]. No border of its own.
#[track_caller]
pub fn segment(theme: &ActiveTheme, label: &str, selected: bool) -> gpui::Stateful<Div> {
    button_base(label)
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(6.0))
        .text_xs()
        .bg(if selected {
            tint(theme.primary(), 0.14)
        } else {
            gpui::rgba(0x00000000)
        })
        .text_color(if selected {
            theme.foreground()
        } else {
            theme.muted_foreground()
        })
        .hover(move |style| {
            style
                .bg(if selected {
                    tint(theme.primary(), 0.18)
                } else {
                    tint(theme.accent(), 0.3)
                })
                .text_color(theme.foreground())
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
