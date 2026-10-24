//! Self-update presentation and async state. Backend owns release/download I/O.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Context, Div, FocusHandle, FontWeight, KeyDownEvent, MouseButton, Window,
};
use industrialis_backend::release_updater::{
    self, AvailableUpdate, DownloadProgress, Platform, ReleaseNote, UpdateCheck,
};

use super::{Dialog, LauncherRoot};
use crate::state::format_bytes;
use crate::ui::{action_button, btn, icon, progress_bar, ButtonVariant};

#[derive(Debug, Clone)]
enum State {
    Disabled,
    Idle,
    Checking,
    Current,
    Available,
    Downloading(DownloadProgress),
    Installing,
    Failed(String),
}

pub(crate) struct UpdaterUi {
    enabled: bool,
    state: State,
    available: Option<AvailableUpdate>,
    installation: Option<PathBuf>,
    focus: Option<FocusHandle>,
    changelog: Option<Vec<ReleaseNote>>,
    changelog_loading: bool,
    changelog_error: Option<String>,
    changelog_focus: Option<FocusHandle>,
}

impl UpdaterUi {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: if enabled {
                State::Idle
            } else {
                State::Disabled
            },
            available: None,
            installation: None,
            focus: None,
            changelog: None,
            changelog_loading: false,
            changelog_error: None,
            changelog_focus: None,
        }
    }

    pub(crate) fn in_progress(&self) -> bool {
        matches!(self.state, State::Downloading(_) | State::Installing)
    }

    pub(super) fn notice_visible(&self) -> bool {
        self.available.is_some() || matches!(self.state, State::Failed(_))
    }

    fn status(&self) -> String {
        match &self.state {
            State::Disabled => "Launcher updates are disabled in development builds.".into(),
            State::Idle => "Launcher update channel".into(),
            State::Checking => "Checking for launcher updates…".into(),
            State::Current => "You're using the latest published version.".into(),
            State::Available => format!(
                "Launcher v{} is available.",
                self.available.as_ref().unwrap().version
            ),
            State::Downloading(p) => format!(
                "Downloading update: {} / {}",
                format_bytes(p.downloaded),
                format_bytes(p.total)
            ),
            State::Installing => "Starting installer. The launcher will restart…".into(),
            State::Failed(error) => format!("Launcher update failed: {error}"),
        }
    }

    fn automatic(&self) -> bool {
        self.installation.is_some()
            && self
                .available
                .as_ref()
                .is_some_and(|u| u.has_windows_installer())
    }
}

impl LauncherRoot {
    pub(crate) fn check_launcher_update(&mut self, cx: &mut Context<Self>) {
        if !self.launcher_update.enabled
            || self.launcher_update.in_progress()
            || matches!(self.launcher_update.state, State::Checking)
        {
            return;
        }
        self.launcher_update.state = State::Checking;
        cx.notify();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async {
                    let installation = std::env::current_exe().ok().and_then(|exe| {
                        release_updater::installation_directory(&exe, Platform::current())
                    });
                    (
                        release_updater::check_for_update(
                            env!("CARGO_PKG_VERSION"),
                            Platform::current(),
                        )
                        .await,
                        installation,
                    )
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok((Ok(check), installation)) => {
                        v.launcher_update.installation = installation;
                        match check {
                            UpdateCheck::Current => {
                                v.launcher_update.state = State::Current;
                                v.launcher_update.available = None;
                            }
                            UpdateCheck::Available(update) => {
                                v.launcher_update.available = Some(update);
                                v.launcher_update.state = State::Available;
                            }
                        }
                    }
                    Ok((Err(error), _)) => {
                        v.launcher_update.state = State::Failed(format!("{error:#}"))
                    }
                    Err(error) => v.launcher_update.state = State::Failed(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_launcher_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            return;
        }
        self.dialog = Some(Dialog::LauncherUpdate);
        self.focused_field = None;
        self.processes_ui.dropdown = false;
        let focus = self
            .launcher_update
            .focus
            .get_or_insert_with(|| cx.focus_handle());
        window.focus(focus, cx);
        cx.notify();
    }

    pub(super) fn footer_update(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.launcher_update.enabled {
            return None;
        }
        let t = &self.theme;
        let (label, glyph) = match &self.launcher_update.state {
            State::Disabled => return None,
            State::Idle => ("Check for updates".to_string(), "refresh-cw"),
            State::Checking => ("Checking for updates…".to_string(), "loader-circle"),
            State::Current => ("Launcher up to date".to_string(), "check"),
            State::Available => (
                format!(
                    "Update available · v{}",
                    self.launcher_update
                        .available
                        .as_ref()
                        .map(|update| update.version.as_str())
                        .unwrap_or("")
                ),
                "arrow-up-circle",
            ),
            State::Downloading(progress) => (
                if progress.total == 0 {
                    "Downloading update…".to_string()
                } else {
                    format!(
                        "Downloading update… {}%",
                        progress.downloaded.saturating_mul(100) / progress.total
                    )
                },
                "loader-circle",
            ),
            State::Installing => ("Installing update…".to_string(), "loader-circle"),
            State::Failed(_) => ("Update check failed".to_string(), "info"),
        };
        let color = match &self.launcher_update.state {
            State::Failed(_) => t.destructive(),
            State::Available | State::Downloading(_) | State::Installing => t.primary(),
            _ => t.muted_foreground(),
        };
        Some(
            btn(t, "", ButtonVariant::Ghost)
                .id("footer-launcher-update")
                .aria_label(label.clone())
                .debug_selector(|| "footer-launcher-update".into())
                .h(px(20.0))
                .px_1()
                .gap(px(4.0))
                .text_size(px(11.0))
                .text_color(color)
                .flex_shrink_0()
                .child(icon(glyph, 12.0))
                .child(label)
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, window, cx| {
                    v.open_launcher_update(window, cx)
                }))
                .into_any_element(),
        )
    }

    pub(super) fn open_changelog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        self.dialog = Some(Dialog::Changelog);
        self.focused_field = None;
        self.processes_ui.dropdown = false;
        let focus = self
            .launcher_update
            .changelog_focus
            .get_or_insert_with(|| cx.focus_handle());
        window.focus(focus, cx);
        cx.notify();
        if self.launcher_update.changelog.is_none() && !self.launcher_update.changelog_loading {
            self.load_changelog(cx);
        }
    }

    fn load_changelog(&mut self, cx: &mut Context<Self>) {
        if self.launcher_update.changelog_loading {
            return;
        }
        self.launcher_update.changelog_loading = true;
        self.launcher_update.changelog_error = None;
        cx.notify();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(release_updater::recent_releases(20)).await;
            let _ = view.update(cx, |v, cx| {
                v.launcher_update.changelog_loading = false;
                match result {
                    Ok(Ok(releases)) => {
                        v.launcher_update.changelog = Some(releases);
                        v.launcher_update.changelog_error = None;
                    }
                    Ok(Err(error)) => {
                        v.launcher_update.changelog_error = Some(format!("{error:#}"));
                    }
                    Err(error) => {
                        v.launcher_update.changelog_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn update_blocker(&self) -> Option<&'static str> {
        if self.data.has_active_games() {
            Some("Stop running games before updating the launcher.")
        } else if !self.busy.is_empty() || self.processes_ui.history.running_count() > 0 {
            Some("Wait for instance operations to finish before updating.")
        } else if self.accounts_ui.is_busy() {
            Some("Finish the account operation before updating.")
        } else if self.settings_saving
            || self.settings_save_pending.is_some()
            || self.instance_ui.has_unsaved_changes()
        {
            Some("Finish saving or correct invalid settings before updating.")
        } else {
            None
        }
    }

    fn install_launcher_update(&mut self, cx: &mut Context<Self>) {
        if !self.launcher_update.enabled
            || self.launcher_update.in_progress()
            || matches!(self.launcher_update.state, State::Checking)
        {
            return;
        }
        let Some(update) = self.launcher_update.available.clone() else {
            self.check_launcher_update(cx);
            return;
        };
        if !self.launcher_update.automatic() {
            cx.open_url(&update.release_url);
            return;
        }
        if let Some(reason) = self.update_blocker() {
            self.launcher_update.state = State::Failed(reason.into());
            cx.notify();
            return;
        }
        let directory = self.launcher_update.installation.clone().unwrap();
        self.launcher_update.state = State::Downloading(DownloadProgress::default());
        cx.notify();
        let progress = Arc::new(Mutex::new(DownloadProgress::default()));
        let emit = progress.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let download = rt.spawn(async move {
                release_updater::download_installer(
                    &update,
                    Arc::new(move |p| *emit.lock().unwrap() = p),
                )
                .await
            });
            while !download.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let p = *progress.lock().unwrap();
                if view
                    .update(cx, |v, cx| {
                        v.launcher_update.state = State::Downloading(p);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let installer = match download.await {
                Ok(Ok(installer)) => installer,
                error => {
                    let message = match error {
                        Ok(Err(e)) => format!("{e:#}"),
                        Err(e) => e.to_string(),
                        _ => unreachable!(),
                    };
                    let _ = view.update(cx, |v, cx| {
                        v.launcher_update.state = State::Failed(message);
                        cx.notify();
                    });
                    return;
                }
            };
            // Recheck after awaits. The modal blocks new input, but outstanding
            // callbacks may still change state. Never discard pending writes.
            let proceed = view
                .update(cx, |v, cx| {
                    if let Some(reason) = v.update_blocker() {
                        v.launcher_update.state = State::Failed(reason.into());
                        cx.notify();
                        false
                    } else {
                        v.launcher_update.state = State::Installing;
                        cx.notify();
                        true
                    }
                })
                .unwrap_or(false);
            if !proceed {
                return;
            }
            let result = rt
                .spawn(async move { installer.hand_off(&directory).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok(Ok(())) => v.begin_close(cx),
                    Ok(Err(error)) => v.launcher_update.state = State::Failed(format!("{error:#}")),
                    Err(error) => v.launcher_update.state = State::Failed(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn launcher_update_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut controls = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.launcher_update.status());
        let mut actions = div().flex().gap_2();
        if self.launcher_update.enabled {
            actions = actions.child(
                btn(&self.theme, "Check for updates", ButtonVariant::Outline)
                    .id("launcher-update-check")
                    .debug_selector(|| "launcher-update-check".into())
                    .on_click(
                        cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.check_launcher_update(cx)),
                    ),
            );
        }
        actions = actions.child(
            btn(&self.theme, "Changelog", ButtonVariant::Outline)
                .id("launcher-changelog")
                .debug_selector(|| "about-changelog".into())
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, window, cx| {
                    v.open_changelog(window, cx)
                })),
        );
        controls = controls.child(actions);
        if self.launcher_update.notice_visible() {
            controls = controls.child(
                btn(&self.theme, "Launcher update", ButtonVariant::Primary)
                    .id("launcher-update-open")
                    .debug_selector(|| "launcher-update-open".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, window, cx| {
                        v.open_launcher_update(window, cx)
                    })),
            );
        }
        controls.child(
            action_button(&self.theme, "Launcher releases", "external-link")
                .id("launcher-update-releases")
                .debug_selector(|| "about-releases".into())
                .on_click(|_, _, cx| cx.open_url(release_updater::RELEASE_PAGE)),
        )
    }

    pub(super) fn launcher_update_dialog(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(format!("Installed version: {}", env!("CARGO_PKG_VERSION")))
            .child(
                div()
                    .text_color(if matches!(self.launcher_update.state, State::Failed(_)) {
                        self.theme.destructive()
                    } else {
                        self.theme.foreground()
                    })
                    .child(self.launcher_update.status()),
            );
        if let Some(update) = &self.launcher_update.available {
            if !update.notes.is_empty() {
                body = body.child(
                    div()
                        .id("launcher-update-notes")
                        .max_h(px(170.0))
                        .overflow_y_scroll()
                        .text_sm()
                        .child(update.notes.clone()),
                );
            }
            body = body.child(div().text_sm().text_color(self.theme.muted_foreground()).child(if self.launcher_update.automatic() {
                "The launcher will close, install the verified update, and restart. Your instances and settings stay in the data directory."
            } else { "This build uses manual updates. Download the matching launcher artifact from the release page." }));
        }
        if let State::Downloading(p) = self.launcher_update.state {
            body = body.child(progress_bar(
                &self.theme,
                if p.total == 0 {
                    0.0
                } else {
                    p.downloaded as f64 / p.total as f64
                },
            ));
        }
        if let Some(reason) = self
            .update_blocker()
            .filter(|_| self.launcher_update.automatic())
        {
            body = body.child(div().text_sm().child(reason));
        }
        let mut footer = div().flex().gap_2();
        if !self.launcher_update.in_progress() {
            footer = footer.child(
                btn(&self.theme, "Close", ButtonVariant::Ghost)
                    .id("launcher-update-close")
                    .debug_selector(|| "launcher-update-close".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx))),
            );
            let can_install = self.launcher_update.available.is_some()
                && !matches!(self.launcher_update.state, State::Checking);
            let label = if can_install {
                if self.launcher_update.automatic() {
                    "Download and restart"
                } else {
                    "Open release page"
                }
            } else {
                "Retry check"
            };
            footer = footer.child(
                btn(&self.theme, "Changelog", ButtonVariant::Ghost)
                    .id("launcher-update-changelog")
                    .debug_selector(|| "launcher-update-changelog".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, window, cx| {
                        v.open_changelog(window, cx)
                    })),
            );
            footer = footer
                .child(
                    btn(&self.theme, "Release page", ButtonVariant::Ghost)
                        .id("launcher-update-manual")
                        .on_click(|_, _, cx| cx.open_url(release_updater::RELEASE_PAGE)),
                )
                .child(
                    btn(&self.theme, label, ButtonVariant::Primary)
                        .id("launcher-update-confirm")
                        .debug_selector(|| "launcher-update-confirm".into())
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                            if can_install {
                                v.install_launcher_update(cx);
                            } else {
                                v.check_launcher_update(cx);
                            }
                        })),
                );
        }
        let focus = self.launcher_update.focus.as_ref();
        div().absolute().inset_0().child(
            crate::ui::dialog_shell(&self.theme, "Launcher update", body, footer)
                .id("launcher-update-dialog")
                .debug_selector(|| "launcher-update-dialog".into())
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(focus, |el, focus| el.track_focus(focus))
                .on_key_down(cx.listener(|v, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        v.close_dialog(cx);
                    }
                    cx.stop_propagation();
                })),
        )
    }
    pub(super) fn changelog_dialog(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(t.muted_foreground())
                    .child(format!("Installed version: {}", env!("CARGO_PKG_VERSION"))),
            );
        if self.launcher_update.changelog_loading {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("loader-circle", 16.0))
                    .child("Loading changelog…"),
            );
        } else if let Some(error) = &self.launcher_update.changelog_error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(t.destructive())
                    .child(format!("Could not load the changelog: {error}")),
            );
        } else if let Some(releases) = &self.launcher_update.changelog {
            if releases.is_empty() {
                body = body.child(div().text_sm().child("No published releases were found."));
            } else {
                body = body.child(
                    div()
                        .id("changelog-list")
                        .debug_selector(|| "changelog-list".into())
                        .flex()
                        .flex_col()
                        .gap_3()
                        .max_h(px(420.0))
                        .overflow_y_scroll()
                        .children(releases.iter().map(|release| {
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(format!("v{}", release.version)),
                                        )
                                        .child(
                                            btn(t, "Release", ButtonVariant::Ghost)
                                                .id(format!(
                                                    "changelog-release:{}",
                                                    release.version
                                                ))
                                                .text_xs()
                                                .px_1()
                                                .h(px(20.0))
                                                .on_click({
                                                    let url = release.release_url.clone();
                                                    move |_, _, cx| cx.open_url(&url)
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .line_height(gpui::relative(1.35))
                                        .child(if release.notes.is_empty() {
                                            "No release notes provided.".to_string()
                                        } else {
                                            release.notes.clone()
                                        }),
                                )
                        })),
                );
            }
        }
        let footer = div()
            .flex()
            .gap_2()
            .child(
                btn(t, "Close", ButtonVariant::Ghost)
                    .id("changelog-close")
                    .debug_selector(|| "changelog-close".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx))),
            )
            .child(
                btn(t, "Reload", ButtonVariant::Ghost)
                    .id("changelog-reload")
                    .debug_selector(|| "changelog-reload".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                        v.load_changelog(cx)
                    })),
            )
            .child(
                btn(t, "Release page", ButtonVariant::Ghost)
                    .id("changelog-release-page")
                    .debug_selector(|| "changelog-release-page".into())
                    .on_click(|_, _, cx| cx.open_url(release_updater::RELEASE_PAGE)),
            );
        let focus = self.launcher_update.changelog_focus.as_ref();
        div().absolute().inset_0().child(
            crate::ui::dialog_shell_with_width(&self.theme, "Changelog", 640.0, body, footer)
                .id("changelog-dialog")
                .debug_selector(|| "changelog-dialog".into())
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(focus, |el, focus| el.track_focus(focus))
                .on_key_down(cx.listener(|v, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        v.close_dialog(cx);
                    }
                    cx.stop_propagation();
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppData;
    use gpui::{size, TestAppContext, VisualTestContext};
    use industrialis_backend::paths::LauncherPaths;
    use industrialis_core::launcher_settings::{LauncherSettingsData, ThemeMode};

    fn data(rt: Arc<tokio::runtime::Runtime>, theme_mode: ThemeMode) -> AppData {
        AppData {
            paths: LauncherPaths::new(std::env::temp_dir().join("industrialis-update-ui-readonly")),
            rt,
            instances: vec![],
            accounts: vec![],
            settings: LauncherSettingsData {
                theme_mode,
                ..Default::default()
            },
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        }
    }

    #[gpui::test]
    fn update_modal_blocks_background_input_and_cannot_close_during_download(
        cx: &mut TestAppContext,
    ) {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        for width in [800.0, 1100.0] {
            for mode in [ThemeMode::Dark, ThemeMode::Light] {
                let handle = cx.open_window(size(px(width), px(750.0)), |_, _| {
                    let mut v = LauncherRoot::new(data(rt.clone(), mode));
                    v.launcher_update = UpdaterUi::new(false); // Tests never call GitHub.
                    v.launcher_update.state = State::Failed("Offline".into());
                    v
                });
                let root = handle.root(cx).unwrap();
                cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
                    .unwrap();
                let mut visual = VisualTestContext::from_window(handle.into(), cx);
                let update = visual
                    .debug_bounds("toolbar-update")
                    .expect("Failed check must remain discoverable");
                let accounts = visual.debug_bounds("toolbar-nav-accounts").unwrap();
                visual.simulate_click(update.center(), Default::default());
                assert!(visual.debug_bounds("launcher-update-dialog").is_some());
                visual.simulate_click(accounts.center(), Default::default());
                visual.update(|_, cx| assert_eq!(root.read(cx).tab, super::super::Tab::Instances));
                visual.update(|window, cx| {
                    root.update(cx, |v, cx| {
                        v.launcher_update.state = State::Downloading(DownloadProgress {
                            downloaded: 50,
                            total: 100,
                        });
                        cx.notify();
                    });
                    window.draw(cx).clear(cx);
                });
                assert!(visual.debug_bounds("launcher-update-confirm").is_none());
                visual.simulate_keystrokes("escape");
                visual.update(|_, cx| {
                    assert!(matches!(root.read(cx).dialog, Some(Dialog::LauncherUpdate)))
                });
                visual.update(|window, cx| {
                    root.update(cx, |v, cx| {
                        v.launcher_update.state = State::Failed("Retry".into());
                        cx.notify();
                    });
                    window.draw(cx).clear(cx);
                });
                let close = visual.debug_bounds("launcher-update-close").unwrap();
                visual.simulate_click(close.center(), Default::default());
                visual.update(|_, cx| assert!(root.read(cx).dialog.is_none()));
            }
        }
    }

    #[test]
    fn development_and_busy_guards_protect_games_operations_and_pending_preferences() {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let mut root = LauncherRoot::new(data(rt, ThemeMode::Dark));
        root.launcher_update = UpdaterUi::new(false);
        assert!(matches!(root.launcher_update.state, State::Disabled));
        assert!(root.update_blocker().is_none());
        root.data.launching.insert("game".into());
        assert!(root.update_blocker().unwrap().contains("games"));
        root.data.launching.clear();
        root.busy.insert("maintenance:game".into());
        assert!(root.update_blocker().unwrap().contains("operations"));
        root.busy.clear();
        root.settings_saving = true;
        assert!(root.update_blocker().unwrap().contains("saving"));
        root.settings_saving = false;
        root.settings_save_pending = Some(root.data.settings.clone());
        assert!(root.update_blocker().is_some());
    }

    #[gpui::test]
    fn footer_update_status_opens_changelog_and_shows_cached_releases(cx: &mut TestAppContext) {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let handle = cx.open_window(size(px(1100.0), px(750.0)), |_, _| {
            let mut v = LauncherRoot::new(data(rt.clone(), ThemeMode::Dark));
            v.launcher_update = UpdaterUi::new(true);
            v.launcher_update.state = State::Current;
            v.launcher_update.changelog = Some(vec![ReleaseNote {
                version: "0.4.2".into(),
                notes: "Release notes".into(),
                release_url: "https://example.test/launcher-v0.4.2".into(),
            }]);
            v
        });
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        let footer = visual
            .debug_bounds("footer-launcher-update")
            .expect("footer update status must be laid out");
        visual.simulate_click(footer.center(), Default::default());
        visual.update(|_, cx| {
            assert!(matches!(
                root.read(cx).dialog,
                Some(Dialog::LauncherUpdate)
            ))
        });
        visual.update(|window, cx| {
            root.update(cx, |v, cx| {
                v.dialog = None;
                v.open_changelog(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        assert!(visual.debug_bounds("changelog-dialog").is_some());
        assert!(visual.debug_bounds("changelog-list").is_some());
        visual.simulate_keystrokes("escape");
        visual.update(|_, cx| assert!(root.read(cx).dialog.is_none()));
    }
}
