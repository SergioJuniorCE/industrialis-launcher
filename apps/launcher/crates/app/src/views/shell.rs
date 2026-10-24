//! LauncherToolbar / LauncherFooter dimensions and responsive controls.
use super::{LauncherRoot, Tab};
use crate::ui::{badge, btn, icon, icon_btn, tint, BadgeKind, ButtonVariant};
use gpui::{
    div, img, prelude::*, px, Context, Div, FontWeight, MouseButton, Window,
    WindowControlArea,
};
use industrialis_core::launcher_settings::ThemeMode;

impl LauncherRoot {
    fn nav_button(&self, tab: Tab, compact: bool, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let selected = self.tab == tab;
        let t = &self.theme;
        let glyph = match tab {
            Tab::Instances => "boxes",
            Tab::Processes => "activity",
            Tab::Accounts => "users",
            Tab::Settings => "settings",
        };
        // GPUI keeps hover transition state only for elements with stable IDs.
        let mut button = div()
            .id(("toolbar-nav", tab as usize)).role(gpui::accesskit::Role::Tab).aria_label(tab.label()).aria_selected(selected).focusable().tab_stop(true)
            .debug_selector(move || format!("toolbar-nav-{}", tab.label().to_ascii_lowercase()))
            .occlude()
            .relative()
            .rounded(px(6.0))
            .h(px(24.0))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .text_xs()
            .line_height(px(16.0))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .text_color(if selected {
                t.foreground()
            } else {
                t.muted_foreground()
            })
            .hover(|s| s.bg(tint(t.primary(), 0.14)).text_color(t.foreground()))
            .child(icon(glyph, 16.0));
        if !compact {
            button = button.child(tab.label());
        }
        if selected {
            button = button.child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .right(px(8.0))
                    .bottom(px(-6.0))
                    .h(px(2.0))
                    .bg(t.primary()),
            );
        }
        let count = self.processes_ui.history.running_count();
        if tab == Tab::Processes && count > 0 {
            button = button.child(
                badge(t, &count.to_string(), BadgeKind::Normal)
                    .h(px(16.0))
                    .px_1()
                    .py_0()
                    .text_size(px(10.0)),
            );
        }
        button.on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                v.tab = tab;
                v.processes_ui.dropdown = false;
                cx.notify();
            }),
        )
    }

    pub(super) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let width = f32::from(window.viewport_size().width);
        let compact = width <= 960.0;
        let dark = matches!(self.data.settings.theme_mode, ThemeMode::Dark);
        let mut brand = div()
            .flex()
            .items_center()
            .gap_2()
            .pr(px(6.0))
            .child(img("brand.png").size(px(20.0)).rounded_md());
        if width > 860.0 {
            brand = brand.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Industrialis"),
            );
        }
        let mut add = btn(t, "", ButtonVariant::Primary)
            .aria_label("Add instance")
            .id("toolbar-add")
            .debug_selector(|| "toolbar-add".into())
            .h(px(28.0))
            .px(px(10.0))
            .text_xs()
            .gap_2()
            .child(icon("plus", 14.0));
        if !compact {
            add = add.child("Add");
        }
        let mut process = btn(t, "", ButtonVariant::Ghost)
            .aria_label("Background processes")
            .id("toolbar-processes")
            .debug_selector(|| "toolbar-processes".into())
            .h(px(28.0))
            .px(px(10.0))
            .text_xs()
            .gap(px(6.0))
            .child(icon("loader-circle", 16.0));
        if !compact {
            process = process.child("Processes");
        }
        let count = self.processes_ui.history.running_count();
        if count > 0 {
            process = process.child(
                badge(t, &count.to_string(), BadgeKind::Normal)
                    .h(px(20.0))
                    .px(px(6.0))
                    .py_0(),
            );
        }
        div()
            .h(px(44.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px_3()
            .gap(px(if compact { 4.0 } else { 6.0 }))
            .bg(t.chrome_background())
            .border_b_1()
            .border_color(tint(t.border(), 0.82))
            // Windows maps this to HTCAPTION. start_window_move() is a no-op
            // in the pinned Windows adapter; Linux still needs the mouse hook.
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .child(brand)
            .child(add.occlude().on_click(cx.listener(|v, e: &gpui::ClickEvent, _, cx| {
                    cx.stop_propagation();
                    v.tab = Tab::Instances;
                    v.workspace_ui.menu = None;
                    v.add_menu = Some(e.position());
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .bg(tint(t.border(), 0.8))
                    .mx_1(),
            )
            .child(
                div()
                    .flex()
                    .h(px(32.0))
                    .items_center()
                    .gap(px(2.0))
                    // Exclude interactive controls from the native drag region.
                    .occlude()
                    .children(
                        [Tab::Instances, Tab::Processes, Tab::Accounts]
                            .into_iter()
                            .map(|tab| self.nav_button(tab, compact, cx)),
                    ),
            )
            .child(div().flex_1())
            .when(self.launcher_update.notice_visible(), |el| {
                el.child(
                    btn(
                        t,
                        if compact { "Update" } else { "Launcher update" },
                        ButtonVariant::Outline,
                    )
                    .id("toolbar-update")
                    .debug_selector(|| "toolbar-update".into())
                    .h(px(28.0))
                    .text_xs()
                    .occlude()
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            v.open_launcher_update(window, cx);
                        }),
                    ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.account_switcher(width <= 860.0, cx))
                    .child(process.on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            if v.processes_ui.history.is_empty() {
                                v.tab = Tab::Processes;
                            } else {
                                v.processes_ui.dropdown = !v.processes_ui.dropdown;
                            }
                            cx.notify();
                        }),
                    ))
                    .child(
                        icon_btn(
                            t,
                            "settings",
                            if self.tab == Tab::Settings {
                                ButtonVariant::Secondary
                            } else {
                                ButtonVariant::Ghost
                            },
                        )
                        .id("toolbar-settings")
                        .aria_label("Launcher preferences")
                        .debug_selector(|| "toolbar-settings".into())
                        .size(px(28.0))
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.tab = Tab::Settings;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        icon_btn(t, if dark { "sun" } else { "moon" }, ButtonVariant::Ghost)
                            .id("toolbar-theme")
                            .aria_label(if dark { "Switch to light theme" } else { "Switch to dark theme" })
                            .debug_selector(|| "toolbar-theme".into())
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    v.set_theme(
                                        if dark {
                                            ThemeMode::Light
                                        } else {
                                            ThemeMode::Dark
                                        },
                                        v.data.settings.theme_preset.clone(),
                                        cx,
                                    );
                                }),
                            ),
                    ),
            )
            .when(cfg!(target_os = "windows"), |el| {
                el.child(
                    div()
                        .flex()
                        .h_full()
                        .ml_1()
                        .mr(px(-12.0))
                        .occlude()
                        .children(
                            [
                                ("minus", 0),
                                (
                                    if window.is_maximized() {
                                        "panels-top-left"
                                    } else {
                                        "square"
                                    },
                                    1,
                                ),
                                ("x", 2),
                            ]
                            .into_iter()
                            .map(|(glyph, action)| {
                                crate::ui::button_base("")
                                    .id(("toolbar-window-control", action as usize))
                                    .aria_label(match action {
                                        0 => "Minimize window",
                                        1 if window.is_maximized() => "Restore window",
                                        1 => "Maximize window",
                                        _ => "Close launcher",
                                    })
                                    .debug_selector(move || {
                                        match action {
                                            0 => "toolbar-minimize",
                                            1 => "toolbar-maximize",
                                            _ => "toolbar-close",
                                        }
                                        .into()
                                    })
                                    .w(px(46.0))
                                    .h_full()
                                    .p_0()
                                    .rounded_none()
                                    // Let Windows retain the native caption-button
                                    // fallback when a GPUI click is not dispatched.
                                    // Keep on_click for keyboard/accessibility activation.
                                    .when(action == 0, |button| {
                                        button.window_control_area(WindowControlArea::Min)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |s| {
                                        if action == 2 {
                                            s.bg(gpui::rgb(0xc42b1c))
                                                .text_color(gpui::rgb(0xffffff))
                                        } else {
                                            s.bg(tint(t.foreground(), 0.1))
                                        }
                                    })
                                    .child(icon(glyph, 14.0))
                                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, window, cx| {
                                            cx.stop_propagation();
                                            match action {
                                                0 => window.minimize_window(),
                                                1 => window.zoom_window(),
                                                _ => {
                                                    v.request_close(cx);
                                                }
                                            }
                                        }),
                                    )
                            }),
                        ),
                )
            })
    }

    /// Dropdown for the toolbar Add button: install a new pack or import from
    /// Prism. Rendered as a deferred overlay like the library context menu.
    pub(super) fn toolbar_add_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Deferred> {
        let position = self.add_menu?;
        if self.dialog.is_some() || self.closing.is_some() {
            return None;
        }
        let t = &self.theme;
        let install = btn(t, "", ButtonVariant::Ghost)
            .id("add-menu-install")
            .debug_selector(|| "add-menu-install".into())
            .aria_label("Install new GTNH pack")
            .role(gpui::accesskit::Role::MenuItem)
            .w_full()
            .justify_start()
            .text_xs()
            .px_2()
            .gap_2()
            .child(icon("boxes", 14.0))
            .child("Install new GTNH pack…")
            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                v.add_menu = None;
                v.tab = Tab::Instances;
                v.open_new_instance(cx);
            }));
        let import = btn(t, "", ButtonVariant::Ghost)
            .id("add-menu-import")
            .debug_selector(|| "add-menu-import".into())
            .aria_label("Import from Prism")
            .role(gpui::accesskit::Role::MenuItem)
            .w_full()
            .justify_start()
            .text_xs()
            .px_2()
            .gap_2()
            .child(icon("archive", 14.0))
            .child("Import from Prism…")
            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                v.add_menu = None;
                v.open_prism_import(None, false, cx);
            }));
        let menu = div()
            .id("toolbar-add-menu")
            .role(gpui::accesskit::Role::Menu)
            .absolute()
            .left(px((f32::from(position.x) - 12.0)
                .min(f32::from(window.viewport_size().width) - 232.)
                .max(0.)))
            .top(px((f32::from(position.y) + 20.0)
                .min(f32::from(window.viewport_size().height) - 88.)
                .max(44.)))
            .w(px(228.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(t.border())
            .bg(t.popover())
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|v, event: &gpui::MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left {
                    v.add_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(install)
            .child(import);
        Some(gpui::deferred(div().absolute().inset_0().child(menu)).with_priority(21))
    }

    pub(super) fn footer(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let count = self.data.instances.len();
        let mut footer = div()
            .h(px(24.0))
            .flex_shrink_0()
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(tint(t.border(), 0.8))
            .bg(tint(t.chrome_background(), 0.8))
            .text_size(px(11.0))
            .text_color(t.muted_foreground())
            .child(format!(
                "{count} instance{}",
                if count == 1 { "" } else { "s" }
            ));
        if let Some(info) = self.data.selected_instance(self.selected_id.as_deref()) {
            footer = footer.child(
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .child(super::workspace::display_name(info)),
            );
        }
        if !self.data.launching.is_empty() {
            footer = footer.child(format!(
                "Launching {}…",
                self.data
                    .launching
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let running: Vec<_> = self
            .data
            .running
            .iter()
            .filter(|(_, (_, alive))| *alive)
            .map(|(id, _)| id.as_str())
            .collect();
        if !running.is_empty() {
            footer = footer.child(format!("Running {}", running.join(", ")));
        }
        let background = self.processes_ui.history.running_count();
        if background > 0 {
            footer = footer.child(format!(
                "{background} background process{}",
                if background == 1 { "" } else { "es" }
            ));
        }
        if self.data.status != "Ready" {
            footer = footer.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(self.data.status.clone()),
            );
        } else {
            footer = footer.child(div().flex_1());
        }
        if let Some(update) = self.footer_update(cx) {
            footer = footer.child(update);
        }
        footer
    }
}

#[cfg(test)]
mod hover_tests {
    use super::*;
    use gpui::{point, size, TestAppContext, VisualTestContext};
    use industrialis_backend::paths::LauncherPaths;
    use industrialis_core::launcher_settings::LauncherSettingsData;
    use std::sync::Arc;

    #[gpui::test]
    fn toolbar_buttons_repaint_on_hover_and_restore_on_exit(cx: &mut TestAppContext) {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        for width in [800.0, 1100.0] {
            for mode in [ThemeMode::Dark, ThemeMode::Light] {
                for preset in ["monochrome", "industrialis"] {
                    check_toolbar_hover(cx, rt.clone(), width, mode, preset);
                }
            }
        }
    }

    fn check_toolbar_hover(
        cx: &mut TestAppContext,
        rt: Arc<tokio::runtime::Runtime>,
        width: f32,
        mode: ThemeMode,
        preset: &str,
    ) {
        let data = crate::state::AppData {
            paths: LauncherPaths::new(std::env::temp_dir().join("industrialis-hover-readonly")),
            rt,
            instances: Vec::new(),
            accounts: Vec::new(),
            settings: LauncherSettingsData {
                theme_mode: mode,
                theme_preset: preset.into(),
                ..Default::default()
            },
            default_account_id: None,
            running: Default::default(),
            logs: Default::default(),
            owned_running: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(width), px(750.0)), |_, _| LauncherRoot::new(data));
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        // Establish mouse modality before taking the baseline. The initial
        // modality change itself invalidates the view and could mask the bug.
        visual.simulate_mouse_move(point(px(600.0), px(300.0)), None, Default::default());
        let mut selectors = vec![
            "toolbar-add",
            "toolbar-nav-instances",
            "toolbar-nav-processes",
            "toolbar-nav-accounts",
            "toolbar-account",
            "toolbar-processes",
            "toolbar-settings",
            "toolbar-theme",
        ];
        if cfg!(target_os = "windows") {
            selectors.extend(["toolbar-minimize", "toolbar-maximize", "toolbar-close"]);
        }
        for selector in selectors {
            let bounds = visual
                .debug_bounds(selector)
                .expect("toolbar control must be laid out");
            let baseline = visual.update(|window, _| window.painted_quads());
            let scale = visual.update(|window, _| window.scale_factor());
            visual.simulate_mouse_move(bounds.center(), None, Default::default());
            let hovered = visual.update(|window, _| window.painted_quads());
            let colors = |quads: Vec<gpui::Quad>| -> Vec<gpui::Background> {
                quads
                    .into_iter()
                    .filter(|q| {
                        let left = q.bounds.origin.x.as_f32() / scale;
                        let top = q.bounds.origin.y.as_f32() / scale;
                        left >= f32::from(bounds.origin.x)
                            && left < f32::from(bounds.right())
                            && top >= f32::from(bounds.origin.y)
                            && top < f32::from(bounds.bottom())
                    })
                    .map(|q| q.background)
                    .collect()
            };
            assert_ne!(
                colors(baseline.clone()),
                colors(hovered),
                "{selector} must change its painted color on hover ({width}, {mode:?}, {preset})"
            );
            visual.simulate_mouse_move(point(px(600.0), px(300.0)), None, Default::default());
            let exited = visual.update(|window, _| window.painted_quads());
            assert_eq!(
                colors(exited),
                colors(baseline),
                "{selector} must restore its color on mouse exit ({width}, {mode:?}, {preset})"
            );
        }
    }
}
