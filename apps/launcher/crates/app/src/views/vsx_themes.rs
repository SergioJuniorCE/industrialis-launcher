//! Open VSX browser; network/parsing run on the backend pool, settings stay native.
use super::LauncherRoot;
use crate::{
    input::{NativeField, NativeFieldEvent},
    ui::{btn, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div};
use industrialis_backend::openvsx::{self, Extension, ThemeBundle};
use industrialis_core::launcher_settings::ThemeMode;

#[derive(Default)]
pub(super) struct OpenVsxUi {
    open: bool,
    query: String,
    requested: String,
    searched: bool,
    loading: bool,
    generation: u64,
    rows: Vec<Extension>,
    total: usize,
    next_offset: usize,
    installing: Option<String>,
    error: Option<String>,
    imported: Vec<String>,
}
impl LauncherRoot {
    pub(super) fn saved_theme_mode(&self, id: &str) -> ThemeMode {
        self.data
            .settings
            .custom_theme_presets
            .iter()
            .find(|raw| raw["id"].as_str() == Some(id))
            .and_then(|raw| raw["openvsx"]["mode"].as_str())
            .map(|mode| {
                if mode == "light" {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                }
            })
            .unwrap_or(self.data.settings.theme_mode)
    }
    pub(super) fn apply_saved_theme(&mut self, id: String, cx: &mut Context<Self>) {
        let mode = self.saved_theme_mode(&id);
        self.clear_theme_drafts();
        self.set_theme(mode, id, cx);
    }
    fn search_vsx(&mut self, more: bool, cx: &mut Context<Self>) {
        if self.closing.is_some() || self.vsx_ui.loading {
            return;
        }
        self.vsx_ui.generation += 1;
        let generation = self.vsx_ui.generation;
        self.vsx_ui.loading = true;
        self.vsx_ui.error = None;
        let query = if more {
            self.vsx_ui.requested.clone()
        } else {
            self.vsx_ui.query.trim().to_owned()
        };
        let offset = if more {
            self.vsx_ui.next_offset
        } else {
            self.vsx_ui.rows.clear();
            self.vsx_ui.next_offset = 0;
            0
        };
        self.vsx_ui.requested = query.clone();
        self.vsx_ui.searched = true;
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { openvsx::search(&query, offset).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                if v.vsx_ui.generation != generation {
                    return;
                }
                v.vsx_ui.loading = false;
                match result {
                    Ok(Ok(catalog)) => {
                        v.vsx_ui.total = catalog.total_size;
                        v.vsx_ui.next_offset = offset + catalog.extensions.len();
                        if catalog.extensions.is_empty() {
                            v.vsx_ui.total = v.vsx_ui.next_offset;
                        }
                        for item in catalog.extensions {
                            if !v.vsx_ui.rows.iter().any(|old| old.id() == item.id()) {
                                v.vsx_ui.rows.push(item);
                            }
                        }
                    }
                    Ok(Err(e)) => v.vsx_ui.error = Some(format!("{e:#}")),
                    Err(e) => v.vsx_ui.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn install_vsx(&mut self, extension: Extension, cx: &mut Context<Self>) {
        if self.vsx_ui.installing.is_some() || self.closing.is_some() {
            return;
        }
        self.vsx_ui.installing = Some(extension.id());
        self.vsx_ui.error = None;
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { openvsx::download(extension).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.vsx_ui.installing = None;
                match result {
                    Ok(Ok(bundle)) => v.save_vsx_bundle(bundle, cx),
                    Ok(Err(e)) => v.vsx_ui.error = Some(format!("{e:#}")),
                    Err(e) => v.vsx_ui.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn save_vsx_bundle(&mut self, bundle: ThemeBundle, cx: &mut Context<Self>) {
        let mut presets = self.data.settings.custom_theme_presets.clone();
        let mut imported = Vec::new();
        for raw in bundle.themes {
            let Some(id) = raw["id"].as_str().map(str::to_owned) else {
                continue;
            };
            imported.push(id.clone());
            if let Some(old) = presets
                .iter_mut()
                .find(|old| old["id"].as_str() == Some(&id))
            {
                for (key, value) in raw.as_object().unwrap() {
                    if matches!(key.as_str(), "dark" | "light") && old[key].is_object() {
                        old[key]
                            .as_object_mut()
                            .unwrap()
                            .extend(value.as_object().unwrap().clone());
                    } else {
                        old[key] = value.clone();
                    }
                }
            } else {
                presets.push(raw);
            }
        }
        if serde_json::to_vec(&presets).is_ok_and(|raw| raw.len() > 8 * 1024 * 1024) {
            self.vsx_ui.error = Some(
                "Saved themes exceed 8 MB. Remove unused themes before importing more.".into(),
            );
            return;
        }
        self.data.settings.custom_theme_presets = presets;
        self.vsx_ui.imported = imported;
        self.rebuild_theme();
        self.persist_launcher_preferences(cx);
        self.data.status = format!(
            "Imported {} color themes from {}. Choose Apply to use one.",
            self.vsx_ui.imported.len(),
            bundle.extension.id()
        );
    }
    pub(super) fn vsx_theme_button(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        btn(
            &self.theme,
            if self.vsx_ui.open {
                "Close Open VSX"
            } else {
                "Add theme"
            },
            ButtonVariant::Outline,
        )
        .h(px(26.))
        .text_xs()
        .aria_label("Browse Open VSX themes")
        .debug_selector(|| "vsx-browse".into())
        .on_click(cx.listener(|v, _, _, cx| {
            v.vsx_ui.open = !v.vsx_ui.open;
            if v.vsx_ui.open && !v.vsx_ui.searched {
                v.search_vsx(false, cx);
            }
            cx.notify();
        }))
    }
    pub(super) fn openvsx_theme_browser(&mut self, cx: &mut Context<Self>) -> Div {
        let mut panel = div().flex().flex_col().gap_2();
        if !self.vsx_ui.open {
            return panel;
        }
        panel = panel.child(div().text_xs().text_color(self.theme.muted_foreground())
            .child("Install color themes for the launcher and file editor. Each package can contain several variants."));
        let fid = "vsx-query".to_owned();
        if !self.native_fields.contains_key(&fid) {
            let query = self.vsx_ui.query.clone();
            let theme = self.theme.clone();
            let field = cx.new(|cx| {
                NativeField::new(&query, theme, "Search themes, e.g. Dracula", false, cx)
            });
            cx.subscribe(&field, |v, _, event, cx| match event {
                NativeFieldEvent::Change(text) => {
                    v.vsx_ui.query = text.clone();
                    cx.notify();
                }
                NativeFieldEvent::Submit => v.search_vsx(false, cx),
                _ => {}
            })
            .detach();
            self.native_fields.insert(fid.clone(), field);
        }
        let field = self.native_fields[&fid].clone();
        field.update(cx, |field, _| {
            field.value = self.vsx_ui.query.clone();
            field.theme = self.theme.clone();
        });
        let loading = self.vsx_ui.loading;
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(div().flex_1().min_w_0().child(field))
                .child(
                    btn(
                        &self.theme,
                        if loading { "Searching..." } else { "Search" },
                        ButtonVariant::Primary,
                    )
                    .id("vsx-search")
                    .debug_selector(|| "vsx-search".into())
                    .on_click(cx.listener(move |v, _, _, cx| {
                        if !loading {
                            v.search_vsx(false, cx);
                        }
                    })),
                ),
        );
        if let Some(error) = &self.vsx_ui.error {
            panel = panel.child(
                div()
                    .text_color(self.theme.destructive())
                    .child(error.clone()),
            );
        }
        let mut results = div()
            .id("vsx-results")
            .h(px(280.))
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for item in self.vsx_ui.rows.clone() {
            let id = item.id();
            let page = item.page();
            let target = id.clone();
            let selector = format!("vsx-install:{id}");
            let source = self.data.settings.custom_theme_presets.iter().find(|raw| {
                raw["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("openvsx:"))
                    && raw["openvsx"]["namespace"].as_str() == Some(&item.namespace)
                    && raw["openvsx"]["extension"].as_str() == Some(&item.name)
            });
            let installed =
                source.is_some_and(|raw| raw["openvsx"]["version"].as_str() == Some(&item.version));
            let pending = self.vsx_ui.installing.is_some();
            let label = if self.vsx_ui.installing.as_deref() == Some(&id) {
                "Installing..."
            } else if installed {
                "Installed"
            } else if source.is_some() {
                "Update"
            } else {
                "Install"
            };
            let title = item
                .display_name
                .clone()
                .unwrap_or_else(|| item.name.clone());
            results = results.child(
                div()
                    .id(format!("vsx-row:{id}"))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(title)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(self.theme.muted_foreground())
                                    .child(format!("{id} / {}", item.version)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_color(self.theme.muted_foreground())
                                    .child(item.description.clone().unwrap_or_default()),
                            ),
                    )
                    .child(
                        btn(&self.theme, "Details", ButtonVariant::Ghost)
                            .text_xs()
                            .on_click(move |_, _, cx| cx.open_url(&page)),
                    )
                    .child(
                        btn(&self.theme, label, ButtonVariant::Secondary)
                            .id(format!("vsx-install:{target}"))
                            .debug_selector(move || selector.clone().into())
                            .tab_stop(!installed && !pending)
                            .when(installed || pending, |button| button.opacity(0.5))
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if !installed && !pending {
                                    v.install_vsx(item.clone(), cx);
                                }
                            })),
                    ),
            );
        }
        panel = panel.child(results);
        if self.vsx_ui.searched && !loading && self.vsx_ui.rows.is_empty() {
            panel = panel.child("No matching color-theme extensions found.");
        }
        if !loading
            && self.vsx_ui.next_offset < self.vsx_ui.total
            && self.vsx_ui.next_offset <= 2000
        {
            panel = panel.child(
                btn(&self.theme, "Load more", ButtonVariant::Ghost)
                    .id("vsx-more")
                    .on_click(cx.listener(|v, _, _, cx| v.search_vsx(true, cx))),
            );
        }
        for id in self.vsx_ui.imported.clone() {
            if let Some(raw) = self
                .data
                .settings
                .custom_theme_presets
                .iter()
                .find(|raw| raw["id"].as_str() == Some(&id))
            {
                let name = raw["name"].as_str().unwrap_or(&id).to_owned();
                let selector = format!("vsx-apply:{id}");
                panel = panel.child(
                    div()
                        .id(format!("vsx-imported:{id}"))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(name))
                        .child(
                            btn(&self.theme, "Apply", ButtonVariant::Primary)
                                .id(format!("vsx-apply:{id}"))
                                .debug_selector(move || selector.clone().into())
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.apply_saved_theme(id.clone(), cx)
                                })),
                        ),
                );
            }
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::super::{preferences::Section, test_support::Fixture, Tab};
    use super::*;
    use crate::theme::ActiveTheme;
    use gpui::{size, TestAppContext};
    use industrialis_backend::settings;
    use industrialis_core::{launcher_settings::LauncherSettingsData, theme};
    use serde_json::json;
    fn bundle(version: &str) -> ThemeBundle {
        let extension = Extension {
            namespace: "test".into(),
            name: "colors".into(),
            version: version.into(),
            display_name: Some("Test Colors".into()),
            description: None,
        };
        let themes = ["dark","light"].into_iter().map(|mode| {
            let mut raw = serde_json::to_value(theme::resolve_preset_or_default("monochrome")).unwrap();
            raw["id"] = format!("openvsx:test.colors:{mode}").into(); raw["name"] = format!("Test {mode}").into(); raw["builtin"] = false.into();
            raw[mode]["background"] = if mode=="dark" { "#101020" } else { "#fafafa" }.into();
            raw["openvsx"] = json!({"namespace":"test","extension":"colors","version":version,"mode":mode});
            raw["editorTheme"] = json!({"colors":{"editor.background":raw[mode]["background"]},"tokenColors":[{"scope":"comment","settings":{"foreground":"#00ff00"}}]}); raw
        }).collect();
        ThemeBundle { extension, themes }
    }
    #[gpui::test]
    async fn imports_apply_update_and_reload_through_native_controls_preserve_settings(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&[]);
        let mut initial = LauncherSettingsData::default();
        initial
            .extra_fields
            .insert("future_setting".into(), json!({"keep":true}));
        fixture
            .rt
            .block_on(settings::save_launcher_settings(&fixture.paths, &initial))
            .unwrap();
        let (window, root) = fixture.open_sized(cx, size(px(1100.), px(2200.)), |v| {
            v.tab = Tab::Settings;
            v.preferences_ui.section = Section::Appearance;
            v.vsx_ui.open = true;
            v.vsx_ui.searched = true;
        });
        root.update(cx, |v, cx| v.save_vsx_bundle(bundle("1.0.0"), cx));
        fixture
            .until(cx, &root, |v| {
                !v.settings_saving && v.settings_save_pending.is_none()
            })
            .await;
        Fixture::click(cx, window, "vsx-apply:openvsx:test.colors:dark");
        fixture
            .until(cx, &root, |v| {
                !v.settings_saving && v.settings_save_pending.is_none()
            })
            .await;
        root.read_with(cx, |v, _| {
            assert_eq!(v.theme.tokens.background, "#101020");
            assert!(v.theme.editor_theme.is_some());
            assert_eq!(v.data.settings.theme_mode, ThemeMode::Dark);
        });
        root.update(cx, |v, cx| {
            v.data.settings.custom_theme_presets[0]["future_theme"] = json!([1, 2]);
            v.data.settings.custom_theme_presets[0]["dark"]["future_color"] = json!(true);
            v.save_vsx_bundle(bundle("1.1.0"), cx);
        });
        fixture
            .until(cx, &root, |v| {
                !v.settings_saving && v.settings_save_pending.is_none()
            })
            .await;
        // Selecting an installed light variant uses its own mode, including the existing Saved themes control.
        Fixture::click(cx, window, "vsx-browse");
        Fixture::click(cx, window, "theme-select:openvsx:test.colors:light");
        fixture
            .until(cx, &root, |v| {
                !v.settings_saving && v.settings_save_pending.is_none()
            })
            .await;
        let loaded = fixture
            .rt
            .block_on(settings::load_launcher_settings(&fixture.paths))
            .unwrap();
        assert_eq!(loaded.theme_mode, ThemeMode::Light);
        assert_eq!(loaded.theme_preset, "openvsx:test.colors:light");
        assert_eq!(loaded.custom_theme_presets.len(), 2);
        assert_eq!(
            loaded.custom_theme_presets[0]["future_theme"],
            json!([1, 2])
        );
        assert_eq!(loaded.custom_theme_presets[0]["dark"]["future_color"], true);
        assert_eq!(
            loaded.custom_theme_presets[0]["openvsx"]["version"],
            "1.1.0"
        );
        assert_eq!(loaded.extra_fields["future_setting"], json!({"keep":true}));
        let active = ActiveTheme::from_preferences(&loaded);
        assert_eq!(active.tokens.background, "#fafafa");
        assert!(active.editor_theme.is_some());
        assert_eq!(active.code_background(), gpui::rgb(0xfafafa));
        fixture.cleanup();
    }

    // Opt-in public network acceptance. Normal tests stay deterministic and offline.
    #[gpui::test]
    #[ignore = "Uses public Open VSX endpoints and a disposable profile"]
    async fn online_install_and_search_dispatch_from_real_native_controls(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&[]);
        let (window, root) = fixture.open_sized(cx, size(px(1100.), px(2200.)), |v| {
            v.tab = Tab::Settings;
            v.preferences_ui.section = Section::Appearance;
            v.vsx_ui.open = true;
            v.vsx_ui.searched = true;
            v.vsx_ui.query = "dracula".into();
            v.vsx_ui.rows.push(Extension {
                namespace: "dracula-theme".into(),
                name: "theme-dracula".into(),
                version: "2.25.1".into(),
                display_name: Some("Dracula Official".into()),
                description: None,
            });
        });
        async fn settle(
            fixture: &Fixture,
            cx: &mut TestAppContext,
            root: &gpui::Entity<LauncherRoot>,
        ) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            loop {
                cx.run_until_parked();
                if root.read_with(cx, |v, _| {
                    !v.vsx_ui.loading && v.vsx_ui.installing.is_none() && !v.settings_saving
                }) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Online operation did not settle"
                );
                fixture
                    .rt
                    .spawn(async { tokio::time::sleep(std::time::Duration::from_millis(10)).await })
                    .await
                    .unwrap();
            }
            root.read_with(cx, |v, _| {
                assert!(
                    v.vsx_ui.error.is_none(),
                    "Open VSX operation failed: {:?}",
                    v.vsx_ui.error
                )
            });
        }
        Fixture::click(cx, window, "vsx-install:dracula-theme.theme-dracula");
        settle(&fixture, cx, &root).await;
        let id = root.read_with(cx, |v, _| {
            v.vsx_ui
                .imported
                .first()
                .expect("Installed variants")
                .clone()
        });
        let selector: &'static str = Box::leak(format!("vsx-apply:{id}").into_boxed_str());
        Fixture::click(cx, window, selector);
        settle(&fixture, cx, &root).await;
        let loaded = fixture
            .rt
            .block_on(settings::load_launcher_settings(&fixture.paths))
            .unwrap();
        assert_eq!(loaded.theme_preset, id);
        let active = ActiveTheme::from_preferences(&loaded);
        assert!(
            active.editor_theme.as_ref().unwrap()["tokenColors"]
                .as_array()
                .unwrap()
                .len()
                > 10
        );
        assert_eq!(active.tokens.background, "#282a36");
        Fixture::click(cx, window, "vsx-search");
        settle(&fixture, cx, &root).await;
        root.read_with(cx, |v, _| {
            assert!(!v.vsx_ui.rows.is_empty() && v.vsx_ui.total > 0)
        });
        fixture.cleanup();
    }
}
