//! Industrialis launcher — GPUI shell (Phase 0).
//!
//! Opens the main window and renders the primary nav skeleton
//! (Instances / Processes / Accounts / Settings). Real views, backend
//! wiring, and `gpui-component` widgets arrive in later phases.

use gpui::{
    Context, IntoElement, MouseButton, MouseDownEvent, Render, SharedString, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size, App, Bounds,
};
use industrialis_core::launcher_settings::LauncherSettingsData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Instances,
    Processes,
    Accounts,
    Settings,
}

impl Tab {
    fn all() -> [Tab; 4] {
        [Tab::Instances, Tab::Processes, Tab::Accounts, Tab::Settings]
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Instances => "Instances",
            Tab::Processes => "Processes",
            Tab::Accounts => "Accounts",
            Tab::Settings => "Settings",
        }
    }
}

struct LauncherRoot {
    active_tab: Tab,
    status: SharedString,
}

impl LauncherRoot {
    fn nav_button(&self, tab: Tab, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.active_tab == tab;
        let label = tab.label();
        // This snapshot exposes handlers imperatively on `Div`
        // (no chained `on_click`), so attach the listener in place.
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .cursor_pointer()
            .bg(if selected { rgb(0x2a2a2a) } else { rgb(0x141414) })
            .text_color(if selected { rgb(0xfafafa) } else { rgb(0xa3a3a3) })
            .hover(|style| style.bg(rgb(0x262626)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, _event: &MouseDownEvent, _window: &mut Window, cx| {
                    this.active_tab = tab;
                    this.status = format!("Opened {label}").into();
                    cx.notify();
                }),
            )
    }

    fn content(&self) -> impl IntoElement {
        let (heading, body) = match self.active_tab {
            Tab::Instances => ("Instances", "Instance grid (Phase 3) — core settings/logic already in industrialis-core."),
            Tab::Processes => ("Processes", "Background downloads, installs, and running game processes (Phase 4)."),
            Tab::Accounts => ("Accounts", "Offline + Microsoft device-code accounts (Phase 4)."),
            Tab::Settings => ("Settings", "Launcher settings, theme, Java defaults (Phase 5)."),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_6()
            .child(div().text_xl().text_color(rgb(0xfafafa)).child(heading))
            .child(div().text_sm().text_color(rgb(0xa3a3a3)).child(body))
            .child(
                div()
                    .mt_4()
                    .text_xs()
                    .text_color(rgb(0x737373))
                    .child(self.status.clone()),
            )
    }
}

impl Render for LauncherRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(0x0a0a0a))
            .text_color(rgb(0xfafafa))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .w(px(200.0))
                    .bg(rgb(0x141414))
                    .border_r_1()
                    .border_color(rgb(0x2a2a2a))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(rgb(0x737373))
                            .child("Industrialis"),
                    )
                    .children(Tab::all().iter().map(|tab| self.nav_button(*tab, cx))),
            )
            .child(div().flex_1().child(self.content()))
    }
}

fn main() {
    let settings = LauncherSettingsData::default();
    let width = settings.window.window_width as f32;
    let height = settings.window.window_height as f32;

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| LauncherRoot {
                    active_tab: Tab::Instances,
                    status: "GPUI shell — backend wiring lands in Phase 2.".into(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
