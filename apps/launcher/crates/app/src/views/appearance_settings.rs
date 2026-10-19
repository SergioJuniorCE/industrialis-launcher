//! Appearance settings: visual choices first, grouped window controls below.
use super::LauncherRoot;
use crate::{
    theme::{parse_hex, ActiveTheme},
    ui::{btn, button_base, card, icon, mix, ButtonVariant},
};
use gpui::{div, prelude::*, px, relative, Context, Div, FontWeight};
use industrialis_core::{
    launcher_settings::{ThemeMode, WindowMaterial},
    theme::ThemePreset,
};

fn heading(text: &str, theme: &ActiveTheme) -> Div {
    div()
        .text_xs()
        .text_color(theme.muted_foreground())
        .font_weight(FontWeight::MEDIUM)
        .child(text.to_owned())
}

fn setting_row(
    theme: &ActiveTheme,
    title: &str,
    description: &str,
    control: impl gpui::IntoElement,
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .p_3()
        .child(
            div()
                .flex_1()
                .min_w(px(160.))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground())
                        .child(description.to_owned()),
                ),
        )
        .child(control)
}

/// Native elements, rather than image assets, keep previews accurate for every
/// built-in, imported, or edited palette.
fn launcher_preview(theme: &ActiveTheme) -> Div {
    div()
        .h(px(104.))
        .w_full()
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border())
        .bg(theme.background())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(16.))
                .bg(theme.card())
                .border_b_1()
                .border_color(theme.border())
                .flex()
                .items_center()
                .px_2()
                .gap_1()
                .child(
                    div()
                        .w(px(28.))
                        .h(px(4.))
                        .rounded_full()
                        .bg(theme.muted_foreground()),
                )
                .child(div().w(px(14.)).h(px(6.)).rounded_sm().bg(theme.primary())),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .w(relative(0.25))
                        .p_2()
                        .border_r_1()
                        .border_color(theme.border())
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(
                            [0, 1, 2].map(|_| div().h(px(5.)).rounded_full().bg(theme.muted())),
                        ),
                )
                .child(
                    div()
                        .p_2()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(div().flex().gap_1().children([0, 1].map(|_| {
                            div()
                                .w(px(23.))
                                .h(px(25.))
                                .rounded_sm()
                                .border_1()
                                .border_color(theme.border())
                                .bg(theme.card())
                                .child(div().size(px(8.)).m_1().rounded_sm().bg(theme.primary()))
                        })))
                        .child(div().h(px(7.)).rounded_sm().bg(theme.primary())),
                ),
        )
}

pub(super) fn theme_swatches(preset: &ThemePreset, selected: bool, theme: &ActiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap_3()
        .py_2()
        .children(
            [(&preset.light, "sun"), (&preset.dark, "moon")]
                .into_iter()
                .map(|(tokens, glyph)| {
                    let accent = parse_hex(&tokens.primary);
                    let surface = parse_hex(&tokens.background);
                    div()
                        .relative()
                        .size(px(48.))
                        .rounded_full()
                        .p(px(3.))
                        .border_1()
                        .border_color(if selected {
                            theme.primary()
                        } else {
                            theme.border()
                        })
                        .child(div().size_full().rounded_full().bg(gpui::linear_gradient(
                            135.,
                            gpui::linear_color_stop(
                                mix(accent, parse_hex(&tokens.foreground), 0.45),
                                0.,
                            ),
                            gpui::linear_color_stop(surface, 1.),
                        )))
                        .child(
                            div()
                                .absolute()
                                .right(px(-2.))
                                .bottom(px(-2.))
                                .size(px(16.))
                                .rounded_full()
                                .bg(theme.card())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(theme.muted_foreground())
                                .child(icon(glyph, 10.)),
                        )
                }),
        )
}

impl LauncherRoot {
    pub(super) fn appearance_view(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        let scheme = div().flex().gap_3().children(
            [(ThemeMode::Light, "Light"), (ThemeMode::Dark, "Dark")]
                .into_iter()
                .map(|(mode, label)| {
                    let selected = mode == self.data.settings.theme_mode;
                    let mut preferences = self.data.settings.clone();
                    preferences.theme_mode = mode;
                    let preview = ActiveTheme::from_preferences(&preferences);
                    button_base("")
                        .id(format!("appearance-mode:{label}"))
                        .debug_selector(move || format!("appearance-mode:{label}").into())
                        .aria_label(format!("{label} color scheme"))
                        .aria_selected(selected)
                        .h_auto()
                        .flex_1()
                        .min_w_0()
                        .p_2()
                        .rounded(t.radius())
                        .border_1()
                        .border_color(if selected { t.primary() } else { t.border() })
                        .bg(t.card())
                        .hover(|style| style.border_color(t.primary()))
                        .flex_col()
                        .items_stretch()
                        .gap_2()
                        .child(launcher_preview(&preview))
                        .child(
                            div()
                                .flex()
                                .justify_center()
                                .items_center()
                                .gap_2()
                                .text_xs()
                                .child(label)
                                .when(selected, |row| row.child(icon("check", 12.))),
                        )
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                            v.set_theme(mode, v.data.settings.theme_preset.clone(), cx)
                        }))
                }),
        );

        let theme_header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .child(heading("Themes", &t))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        btn(&t, "Create theme", ButtonVariant::Outline)
                            .h(px(26.))
                            .text_xs()
                            .debug_selector(|| "theme-create".into())
                            .on_click(cx.listener(|v, _, _, cx| v.create_theme_editor(cx))),
                    )
                    .child(self.vsx_theme_button(cx)),
            );

        let material_controls = div().flex().flex_wrap().gap_1().children(
            [
                (WindowMaterial::Opaque, "Opaque"),
                (WindowMaterial::Frosted, "Frosted glass"),
                (WindowMaterial::Mica, "Mica"),
            ]
            .into_iter()
            .map(|(material, label)| {
                btn(
                    &t,
                    label,
                    if self.data.settings.window_material == material {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                )
                .h(px(28.))
                .text_xs()
                .when(self.data.settings.window_material == material, |button| {
                    button.border_1().border_color(t.primary())
                })
                .debug_selector(move || format!("appearance-material:{label}").into())
                .on_click(cx.listener(move |v, _, _, cx| v.set_window_material(material, cx)))
            }),
        );
        let mut interface = card(&t).flex().flex_col().child(setting_row(
            &t,
            "Window material",
            "Choose a solid surface or a Windows desktop backdrop.",
            material_controls,
        ));
        if self.data.settings.window_material != WindowMaterial::Opaque {
            let opacity = self.data.settings.window_tint_opacity;
            let controls =
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child(format!("{opacity}%")),
                    )
                    .children([(-10i16, "Less"), (10, "More")].into_iter().map(
                        |(delta, label)| {
                            let enabled = if delta < 0 {
                                opacity > 20
                            } else {
                                opacity < 90
                            };
                            btn(&t, label, ButtonVariant::Outline)
                                .h(px(26.))
                                .text_xs()
                                .tab_stop(enabled)
                                .aria_label(format!("{label} glass opacity"))
                                .when(!enabled, |b| b.opacity(0.4).cursor_default())
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    if enabled {
                                        v.data.settings.window_tint_opacity =
                                            (i16::from(opacity) + delta).clamp(20, 90) as u8;
                                        v.persist_launcher_preferences(cx);
                                        cx.notify();
                                    }
                                }))
                        },
                    ));
            interface = interface.child(div().border_t_1().border_color(t.border()).child(
                setting_row(
                    &t,
                    "Glass opacity",
                    "Higher values make the launcher surface more solid.",
                    controls,
                ),
            ));
        }
        interface = interface.child(
            div()
                .px_3()
                .pb_3()
                .text_xs()
                .text_color(t.muted_foreground())
                .child(self.backdrop.fallback.clone().unwrap_or_else(|| {
                    "Frosted glass and Mica require Windows 11 22H2 or later.".into()
                })),
        );

        let maximized = self.data.settings.window.launch_maximized;
        let dimensions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.field("settings-width", "1100", cx).w(px(78.)))
            .child(div().text_xs().text_color(t.muted_foreground()).child("×"))
            .child(self.field("settings-height", "750", cx).w(px(78.)))
            .child(
                btn(&t, "Save window size", ButtonVariant::Outline)
                    .h(px(28.))
                    .text_xs()
                    .on_click(cx.listener(|v, _, _, cx| v.save_launcher_settings_form(cx))),
            );
        let window_settings = card(&t)
            .flex()
            .flex_col()
            .child(setting_row(
                &t,
                "Start maximized",
                "Open the launcher using the full screen area.",
                crate::ui::checkbox(&t, "", maximized)
                    .aria_label("Start launcher maximized")
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.data.settings.window.launch_maximized =
                            !v.data.settings.window.launch_maximized;
                        v.save_launcher_settings_form(cx);
                    })),
            ))
            .child(
                div()
                    .border_t_1()
                    .border_color(t.border())
                    .child(setting_row(
                        &t,
                        "Window size",
                        "Default width and height in pixels when restored.",
                        dimensions,
                    )),
            );

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .py_2()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Appearance"),
            )
            .child(heading("Color scheme", &t))
            .child(scheme)
            .child(div().mt_3().child(theme_header))
            .child(self.theme_gallery(cx))
            .child(self.openvsx_theme_browser(cx))
            .child(self.custom_theme_controls(cx))
            .child(div().mt_3().child(heading("Interface", &t)))
            .child(interface)
            .child(div().mt_3().child(heading("Launcher window", &t)))
            .child(window_settings)
    }
}
