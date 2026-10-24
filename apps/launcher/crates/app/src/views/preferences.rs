//! SettingsTab's sidebar and ported Java, appearance and retention controls.
use super::LauncherRoot;
use crate::{
    input::FieldState,
    ui::{action_button, btn, card, icon, tint, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, FontWeight, PathPromptOptions};
use industrialis_backend::java::JavaInfo;
#[cfg(test)]
use industrialis_core::launcher_settings::ThemeMode;

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Section {
    #[default]
    Java,
    Appearance,
    Backups,
    About,
}

#[derive(Default)]
pub(super) struct PreferencesUi {
    pub(super) section: Section,
    pub(super) installations: Option<Vec<JavaInfo>>,
    pub(super) scanning: bool,
    error: Option<String>,
}

impl LauncherRoot {
    pub(super) fn scan_launcher_java(&mut self, cx: &mut Context<Self>) {
        if self.preferences_ui.scanning {
            return;
        }
        self.preferences_ui.scanning = true;
        self.preferences_ui.error = None;
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(industrialis_backend::java::detect_java()).await;
            let _ = view.update(cx, |v, cx| {
                v.preferences_ui.scanning = false;
                match result {
                    Ok(java) => v.preferences_ui.installations = Some(java),
                    Err(e) => {
                        v.preferences_ui.installations = Some(Vec::new());
                        v.preferences_ui.error = Some(format!("Java scan failed: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn browse_launcher_java(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Java executable".into()),
        });
        cx.spawn(async move |view, cx| {
            let result = picker.await;
            let _ = view.update(cx, |v, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            v.settings_java = FieldState::new(&path.to_string_lossy());
                            v.save_launcher_settings_form(cx);
                        }
                    }
                    Ok(Err(e)) => v.preferences_ui.error = Some(format!("Browse failed: {e}")),
                    Err(e) => v.preferences_ui.error = Some(format!("Browse failed: {e}")),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn settings_view(&mut self, cx: &mut Context<Self>) -> Div {
        if self.preferences_ui.installations.is_none() && !self.preferences_ui.scanning {
            self.scan_launcher_java(cx);
        }
        let t = self.theme.clone();
        let section = self.preferences_ui.section;
        let sidebar = div()
            .id("preferences-sidebar")
            .w(px(192.0))
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .flex_shrink_0()
            .pt(px(20.0))
            .pb_4()
            .pr_4()
            .border_r_1()
            .border_color(tint(t.border(), 0.7))
            .child(
                div()
                    .px_2()
                    .mb_4()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child("Launcher preferences"),
                    ),
            )
            .child(
                div().flex().flex_col().gap_1().children(
                    [
                        (Section::Java, "Java", "terminal"),
                        (Section::Appearance, "Appearance", "sliders-horizontal"),
                        (Section::Backups, "Backups", "copy"),
                        (Section::About, "About", "info"),
                    ]
                    .into_iter()
                    .map(|(value, label, glyph)| {
                        btn(&t, "", ButtonVariant::Ghost)
                            .id(format!("launcher-preference:{label}"))
                            .aria_label(label)
                            .h(px(36.0))
                            .w_full()
                            .justify_start()
                            .px_3()
                            .rounded_none()
                            .border_l_2()
                            .border_color(if value == section {
                                t.primary()
                            } else {
                                gpui::rgba(0x00000000)
                            })
                            .bg(if value == section {
                                t.secondary()
                            } else {
                                gpui::rgba(0x00000000)
                            })
                            .text_color(if value == section {
                                t.foreground()
                            } else {
                                t.muted_foreground()
                            })
                            .child(icon(glyph, 16.0))
                            .child(label)
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.preferences_ui.section = value;
                                cx.notify();
                            }))
                    }),
                ),
            );
        let mut content = div().flex_1().min_w_0().flex().flex_col().gap_3();
        match section {
            Section::Java => {
                let actions = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        btn(&t, "Auto-detect", ButtonVariant::Outline)
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.settings_java = FieldState::empty();
                                v.save_launcher_settings_form(cx);
                            })),
                    )
                    .child(
                        btn(&t, "Browse...", ButtonVariant::Outline)
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.browse_launcher_java(cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        btn(&t, "Save", ButtonVariant::Outline)
                            .h(px(28.0))
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.save_launcher_settings_form(cx)
                            })),
                    );
                let default_java = card(&t)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Default Java executable"))
                    .child(div().text_xs().text_color(t.muted_foreground()).child(
                        "Used by instances without a Java override. Select an installed runtime below or choose an executable.",
                    ))
                    .child(self.field("settings-java", "Auto-detect (JAVA_HOME / PATH)", cx))
                    .child(actions);
                content = content
                    .child(
                        div()
                            .py(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Java Detection"),
                    )
                    .child(default_java)
                    .child(self.java_chooser(None, true, cx));
            }
            Section::Appearance => {
                content = content.child(self.appearance_view(cx));
            }
            Section::Backups => {
                content = content
                    .child(
                        div()
                            .py(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Backups"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child("Backup retention"),
                    )
                    .child(self.field("settings-retention", "10", cx).w(px(120.0)))
                    .child(
                        btn(&t, "Save retention", ButtonVariant::Outline)
                            .h(px(28.0))
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.save_launcher_settings_form(cx)
                            })),
                    )
                    .child(self.backup_preferences(cx));
            }
            Section::About => {
                content = content
                    .child(
                        div()
                            .py(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("About"),
                    )
                    .child(
                        card(&t)
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                                format!("Industrialis Launcher v{}", env!("CARGO_PKG_VERSION")),
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(t.muted_foreground())
                                    .child("GT New Horizons modpack manager."),
                            )
                            .child(div().text_xs().text_color(t.muted_foreground()).child(
                                format!(
                                    "{} / {}",
                                    std::env::consts::OS,
                                    std::env::consts::ARCH
                                ),
                            )),
                    )
                    .child(
                        card(&t)
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .mb_1()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Launcher actions"),
                            )
                            .child(
                                action_button(&t, "Open launcher data folder", "folder-open")
                                    .debug_selector(|| "about-data-folder".into())
                                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                        cx.reveal_path(v.data.paths.base())
                                    })),
                            )
                            .child(
                                action_button(&t, "View on GitHub", "external-link")
                                    .debug_selector(|| "about-github".into())
                                    .on_click(|_, _, cx| {
                                        cx.open_url(
                                    "https://github.com/SergioJuniorCE/industrialis-launcher",
                                )
                                    }),
                            ),
                    )
                    .child(
                        card(&t)
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Launcher updates"),
                            )
                            .child(self.launcher_update_controls(cx)),
                    );
            }
        }
        if let Some(error) = &self.settings_save_error {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.destructive())
                            .child(error.clone()),
                    )
                    .child(
                        btn(&t, "Retry save", ButtonVariant::Outline)
                            .id("retry-preferences-save")
                            .debug_selector(|| "retry-preferences-save".into())
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.start_preferences_save(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        btn(&t, "Discard unsaved changes", ButtonVariant::Ghost)
                            .id("discard-preferences-save")
                            .debug_selector(|| "discard-preferences-save".into())
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.discard_failed_preferences(cx);
                            })),
                    ),
            );
        }
        if let Some(error) = &self.preferences_ui.error {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t.destructive())
                    .child(error.clone()),
            );
        }
        div().flex_1().min_h_0().flex().justify_start().child(
            div()
                .id("launcher-preferences")
                .w_full()
                .h_full()
                .max_w(px(1024.0))
                .min_h_0()
                .px_4()
                .flex()
                .items_stretch()
                .gap_6()
                .child(sidebar)
                .child(
                    div()
                        .id("launcher-preferences-content")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pt(px(20.0))
                        .pb_4()
                        .child(content),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppData;
    use gpui::{size, TestAppContext, VisualTestContext};
    use industrialis_backend::{paths::LauncherPaths, settings};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    #[gpui::test]
    async fn failed_save_keeps_latest_edits_blocks_close_and_retries_from_the_ui(
        cx: &mut TestAppContext,
    ) {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        cx.executor().allow_parking();
        let dir = std::env::temp_dir().join(format!(
            "industrialis-preference-ui-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = LauncherPaths::new(dir.clone());
        let saved = industrialis_core::launcher_settings::LauncherSettingsData::default();
        rt.block_on(settings::save_launcher_settings(&paths, &saved))
            .unwrap();
        let target = paths.launcher_settings_path();
        let original = std::fs::read(&target).unwrap();
        let original_permissions = std::fs::metadata(&target).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();
        let handle = cx.open_window(
            size(px(1100.0), px(750.0)),
            |window, cx: &mut Context<LauncherRoot>| {
                let mut root = LauncherRoot::new(AppData {
                    paths,
                    rt: rt.clone(),
                    instances: vec![],
                    accounts: vec![],
                    settings: saved,
                    default_account_id: None,
                    running: Default::default(),
                    owned_running: Default::default(),
                    logs: Default::default(),
                    launching: Default::default(),
                    status: "Ready".into(),
                });
                root.tab = super::super::Tab::Settings;
                root.preferences_ui.section = Section::About;
                root.preferences_ui.installations = Some(Vec::new());
                let weak = cx.entity().downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    weak.upgrade()
                        .is_none_or(|root| root.update(cx, |root, cx| root.close_allowed(cx)))
                });
                root
            },
        );
        let root = handle.root(cx).unwrap();
        root.update(cx, |v, cx| {
            v.set_theme(ThemeMode::Light, "midnight".into(), cx);
            v.set_theme(ThemeMode::Light, "sandstone".into(), cx);
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while root.update(cx, |v, _| v.settings_saving) {
            cx.run_until_parked();
            assert!(Instant::now() < deadline, "save task did not complete");
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        root.update(cx, |v, _| {
            assert!(v.settings_save_error.is_some());
            assert_eq!(
                v.settings_save_pending.as_ref().unwrap().theme_preset,
                "sandstone"
            );
        });
        assert_eq!(std::fs::read(&target).unwrap(), original);
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        assert!(!visual.simulate_close());
        std::fs::set_permissions(&target, original_permissions).unwrap();
        let retry = visual.debug_bounds("retry-preferences-save").unwrap();
        visual.simulate_click(retry.center(), Default::default());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            visual.run_until_parked();
            let saving = visual.update(|_, cx| root.read(cx).settings_saving);
            if !saving {
                break;
            }
            assert!(Instant::now() < deadline, "retry task did not complete");
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        visual.update(|_, cx| {
            let v = root.read(cx);
            assert!(v.settings_save_pending.is_none() && v.settings_save_error.is_none());
        });
        let loaded = rt
            .block_on(settings::load_launcher_settings(&LauncherPaths::new(
                dir.clone(),
            )))
            .unwrap();
        assert_eq!(loaded.theme_mode, ThemeMode::Light);
        assert_eq!(loaded.theme_preset, "sandstone");
        // An explicit discard must allow closing without rewriting the file.
        let current_bytes = std::fs::read(&target).unwrap();
        let permissions = std::fs::metadata(&target).unwrap().permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&target, read_only).unwrap();
        visual.update(|_, cx| {
            root.update(cx, |v, cx| {
                v.set_theme(ThemeMode::Dark, "monochrome".into(), cx)
            })
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            visual.run_until_parked();
            if !visual.update(|_, cx| root.read(cx).settings_saving) {
                break;
            }
            assert!(Instant::now() < deadline, "save task did not complete");
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let discard = visual.debug_bounds("discard-preferences-save").unwrap();
        visual.simulate_click(discard.center(), Default::default());
        visual.update(|_, cx| {
            let v = root.read(cx);
            assert!(v.settings_save_pending.is_none() && v.settings_save_error.is_none());
            assert_eq!(v.data.settings.theme_preset, "sandstone");
        });
        assert_eq!(std::fs::read(&target).unwrap(), current_bytes);
        std::fs::set_permissions(&target, permissions).unwrap();
        assert!(visual.simulate_close());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
