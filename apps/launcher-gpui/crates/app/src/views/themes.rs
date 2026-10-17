use super::LauncherRoot;
use crate::{
    input::{NativeField, NativeFieldEvent},
    theme::ActiveTheme,
    ui::{btn, button_base, card, icon, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, FontWeight};
use industrialis_core::{
    launcher_settings::{ThemeMode, ThemeOverrides},
    theme::{self, ThemePreset},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct ThemesUi {
    drafts: HashMap<String, String>,
    invalid: HashSet<String>,
    editing: Option<String>,
    editor_open: bool,
    advanced_open: bool,
    delete: Option<String>,
    error: Option<String>,
}
fn preset(settings: &industrialis_core::launcher_settings::LauncherSettingsData) -> ThemePreset {
    settings
        .custom_theme_presets
        .iter()
        .find(|raw| raw["id"].as_str() == Some(&settings.theme_preset))
        .and_then(|raw| serde_json::from_value(raw.clone()).ok())
        .unwrap_or_else(|| theme::resolve_preset_or_default(&settings.theme_preset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppData;
    use gpui::{px, size, ClipboardItem, TestAppContext, VisualTestContext};
    use industrialis_backend::paths::LauncherPaths;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    #[gpui::test]
    async fn copying_an_imported_theme_keeps_the_saved_mode_and_matching_syntax(
        cx: &mut TestAppContext,
    ) {
        use super::super::test_support::Fixture;
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let fixture = Fixture::new(&[]);
            let (_,root) = fixture.open(cx,|v| {
                let mut raw = serde_json::to_value(theme::resolve_preset_or_default("monochrome")).unwrap();
                raw["id"] = "openvsx:test.copy:dark".into();
                raw["openvsx"] = serde_json::json!({"mode":"dark","namespace":"test"});
                raw["editorTheme"] = serde_json::json!({"tokenColors":[{"scope":"comment","settings":{"foreground":"#00ff00"}}]});
                v.data.settings.custom_theme_presets = vec![raw];
                v.data.settings.theme_preset = "openvsx:test.copy:dark".into();
                v.data.settings.theme_mode = mode; v.rebuild_theme();
            });
            root.update(cx, |v, cx| {
                v.themes_ui.drafts.insert("name".into(), "My copy".into());
                v.save_named_theme(cx);
            });
            fixture
                .until(cx, &root, |v| {
                    !v.settings_saving && v.settings_save_pending.is_none()
                })
                .await;
            let id = root.read_with(cx, |v, _| v.data.settings.theme_preset.clone());
            assert!(id.starts_with("custom-"));
            root.update(cx, |v, cx| {
                v.set_theme(
                    if mode == ThemeMode::Dark {
                        ThemeMode::Light
                    } else {
                        ThemeMode::Dark
                    },
                    "monochrome".into(),
                    cx,
                );
                v.apply_saved_theme(id.clone(), cx);
            });
            fixture
                .until(cx, &root, |v| {
                    !v.settings_saving && v.settings_save_pending.is_none()
                })
                .await;
            let loaded = fixture
                .rt
                .block_on(industrialis_backend::settings::load_launcher_settings(
                    &fixture.paths,
                ))
                .unwrap();
            assert_eq!(loaded.theme_mode, mode);
            assert_eq!(
                ActiveTheme::from_preferences(&loaded)
                    .editor_theme
                    .is_some(),
                mode == ThemeMode::Dark
            );
            assert_eq!(
                loaded.custom_theme_presets[1]["openvsx"]["namespace"],
                "test"
            );
            fixture.cleanup();
        }
    }
    #[gpui::test]
    async fn existing_custom_theme_edits_preserve_extension_fields(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let paths = LauncherPaths::new(std::env::temp_dir().join(format!(
                "industrialis-theme-ui-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        std::fs::create_dir_all(paths.base()).unwrap();
        let mut raw = serde_json::to_value(theme::resolve_preset_or_default("monochrome")).unwrap();
        raw["id"] = "custom-existing".into();
        raw["name"] = "My old theme".into();
        raw["builtin"] = false.into();
        raw["dark"]["background"] = "#123456".into();
        raw["dark"]["future_color"] = serde_json::json!({"keep":true});
        raw["future"] = serde_json::json!([1, 2]);
        let mut settings = industrialis_core::launcher_settings::LauncherSettingsData::default();
        settings.theme_preset = "custom-existing".into();
        settings.custom_theme_presets.push(raw);
        settings
            .theme_overrides
            .extra_fields
            .insert("future_override".into(), true.into());
        rt.block_on(industrialis_backend::settings::save_launcher_preferences(
            &paths, &settings,
        ))
        .unwrap();
        let data = AppData {
            paths: paths.clone(),
            rt: rt.clone(),
            instances: Vec::new(),
            accounts: Vec::new(),
            settings,
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(1100.), px(2200.)), |_, _| {
            let mut root = LauncherRoot::new(data);
            root.tab = super::super::Tab::Settings;
            root.preferences_ui.section = super::super::preferences::Section::Appearance;
            root.themes_ui.editor_open = true;
            root.themes_ui.advanced_open = true;
            root
        });
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        assert_eq!(
            root.read_with(cx, |v, _| v.theme.tokens.background.clone()),
            "#123456"
        );
        async fn settle(
            cx: &mut TestAppContext,
            root: &gpui::Entity<LauncherRoot>,
            rt: &Arc<tokio::runtime::Runtime>,
        ) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if !root.read_with(cx, |v, _| {
                    v.settings_saving || v.settings_save_pending.is_some()
                }) {
                    break;
                }
                assert!(Instant::now() < deadline);
                rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                    .await
                    .unwrap();
            }
        }
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let field = visual.debug_bounds("theme:background").unwrap();
            visual.simulate_click(field.center(), Default::default());
            visual
                .update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("#222222".into())));
            visual.simulate_keystrokes("ctrl-a ctrl-v");
        }
        settle(cx, &root, &rt).await;
        assert_eq!(
            root.read_with(cx, |v, _| v.theme.tokens.background.clone()),
            "#222222"
        );
        root.update(cx, |v, cx| {
            v.themes_ui.editing = Some("custom-existing".into());
            v.themes_ui.drafts.insert("name".into(), "Renamed".into());
            v.themes_ui
                .drafts
                .insert("description".into(), "Keep both modes".into());
            v.save_named_theme(cx);
        });
        settle(cx, &root, &rt).await;
        let reloaded = rt
            .block_on(industrialis_backend::settings::load_launcher_settings(
                &paths,
            ))
            .unwrap();
        assert_eq!(reloaded.custom_theme_presets[0]["name"], "Renamed");
        assert_eq!(
            reloaded.custom_theme_presets[0]["dark"]["background"],
            "#222222"
        );
        assert_eq!(
            reloaded.custom_theme_presets[0]["dark"]["future_color"],
            serde_json::json!({"keep":true})
        );
        assert_eq!(
            reloaded.custom_theme_presets[0]["future"],
            serde_json::json!([1, 2])
        );
        assert_eq!(
            reloaded.theme_overrides.extra_fields["future_override"],
            true
        );
        assert_eq!(
            ActiveTheme::from_preferences(&reloaded).tokens.background,
            "#222222"
        );
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let reset = visual.debug_bounds("theme-reset").unwrap();
            visual.simulate_click(reset.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        let reset = rt
            .block_on(industrialis_backend::settings::load_launcher_settings(
                &paths,
            ))
            .unwrap();
        assert!(reset.theme_overrides.background.is_none());
        assert_eq!(reset.theme_overrides.extra_fields["future_override"], true);
        assert_eq!(
            reset.custom_theme_presets[0]["future"],
            serde_json::json!([1, 2])
        );
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let delete = visual.debug_bounds("theme-delete:custom-existing").unwrap();
            visual.simulate_click(delete.center(), Default::default());
            let confirm = visual
                .debug_bounds("theme-confirm-delete:custom-existing")
                .unwrap();
            visual.simulate_click(confirm.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        let deleted = rt
            .block_on(industrialis_backend::settings::load_launcher_settings(
                &paths,
            ))
            .unwrap();
        assert!(deleted.custom_theme_presets.is_empty());
        assert_eq!(deleted.theme_preset, "monochrome");
        assert_eq!(
            deleted.theme_overrides.extra_fields["future_override"],
            true
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
fn contrast(theme: &ActiveTheme) -> f32 {
    fn luminance(color: gpui::Rgba) -> f32 {
        let value = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * value(color.r) + 0.7152 * value(color.g) + 0.0722 * value(color.b)
    }
    let a = luminance(theme.background());
    let b = luminance(theme.foreground());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
impl LauncherRoot {
    fn theme_field(&mut self, key: &str, initial: String, cx: &mut Context<Self>) -> Div {
        let value = self
            .themes_ui
            .drafts
            .entry(key.into())
            .or_insert(initial)
            .clone();
        let fid = format!("theme:{key}");
        if !self.native_fields.contains_key(&fid) {
            let field = cx.new(|cx| {
                NativeField::new(
                    &value,
                    self.theme.clone(),
                    if key == "radius" {
                        "0.375rem"
                    } else {
                        "Use preset default"
                    },
                    false,
                    cx,
                )
            });
            let token = key.to_string();
            cx.subscribe(&field, move |v, _, event, cx| {
                if let NativeFieldEvent::Change(text) = event {
                    v.themes_ui.drafts.insert(token.clone(), text.clone());
                    if token == "name" || token == "description" {
                        cx.notify();
                        return;
                    }
                    let valid = text.is_empty()
                        || if token == "radius" {
                            theme::validate_radius(text)
                        } else {
                            theme::validate_hex_color(text)
                        };
                    if valid {
                        v.themes_ui.invalid.remove(&token);
                        let mut raw =
                            serde_json::to_value(&v.data.settings.theme_overrides).unwrap();
                        raw[&token] = if text.is_empty() {
                            serde_json::Value::Null
                        } else {
                            text.clone().into()
                        };
                        v.data.settings.theme_overrides = serde_json::from_value(raw).unwrap();
                        v.rebuild_theme();
                        v.persist_launcher_preferences(cx);
                    } else {
                        v.themes_ui.invalid.insert(token.clone());
                    }
                    cx.notify();
                }
            })
            .detach();
            self.native_fields.insert(fid.clone(), field);
        }
        let field = self.native_fields[&fid].clone();
        field.update(cx, |f, _| {
            f.value = value;
            f.theme = self.theme.clone();
            f.accessibility_label = format!("Theme {}", key.replace('_', " "));
            if key == "name" {
                f.placeholder = "My theme".into();
            }
            if key == "description" {
                f.placeholder = "Optional description".into();
            }
        });
        div()
            .w_full()
            .debug_selector(move || fid.clone().into())
            .child(field)
    }
    fn save_named_theme(&mut self, cx: &mut Context<Self>) {
        let name = self
            .themes_ui
            .drafts
            .get("name")
            .map(|v| v.trim())
            .unwrap_or_default();
        if name.is_empty() || name.len() > 128 || !self.themes_ui.invalid.is_empty() {
            self.themes_ui.error =
                Some("Enter a theme name and correct invalid colors before saving.".into());
            cx.notify();
            return;
        }
        let id = self.themes_ui.editing.clone().unwrap_or_else(|| {
            format!(
                "custom-{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        });
        let mut base = preset(&self.data.settings);
        base.id = id.clone();
        base.name = name.into();
        base.description = self
            .themes_ui
            .drafts
            .get("description")
            .cloned()
            .unwrap_or_default();
        base.builtin = false;
        if self.data.settings.theme_mode == ThemeMode::Dark {
            base.dark = self.theme.tokens.clone();
        } else {
            base.light = self.theme.tokens.clone();
        }
        let mut raw = self
            .data
            .settings
            .custom_theme_presets
            .iter()
            .find(|raw| raw["id"].as_str() == Some(&id))
            .cloned()
            .unwrap_or_else(|| {
                self.data
                    .settings
                    .custom_theme_presets
                    .iter()
                    .find(|raw| raw["id"].as_str() == Some(&self.data.settings.theme_preset))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}))
            });
        // A copy reflects the appearance being saved, even if the user switched
        // to the imported variant's opposite brightness before customizing it.
        if self.themes_ui.editing.is_none() && raw["openvsx"].is_object() {
            raw["openvsx"]["mode"] = if self.theme.dark { "dark" } else { "light" }.into();
            if let Some(document) = &self.theme.editor_theme {
                raw["editorTheme"] = document.clone();
            } else {
                raw.as_object_mut().unwrap().remove("editorTheme");
            }
        }
        let next = serde_json::to_value(base).unwrap();
        for (key, value) in next.as_object().unwrap() {
            if key == "dark" || key == "light" {
                let object = raw
                    .as_object_mut()
                    .unwrap()
                    .entry(key.clone())
                    .or_insert_with(|| serde_json::json!({}));
                if !object.is_object() {
                    *object = serde_json::json!({});
                }
                for (token, value) in value.as_object().unwrap() {
                    object[token] = value.clone();
                }
            } else {
                raw[key] = value.clone();
            }
        }
        if let Some(existing) = self
            .data
            .settings
            .custom_theme_presets
            .iter_mut()
            .find(|raw| raw["id"].as_str() == Some(&id))
        {
            *existing = raw;
        } else {
            self.data.settings.custom_theme_presets.push(raw);
        }
        self.data.settings.theme_preset = id;
        let extras = self.data.settings.theme_overrides.extra_fields.clone();
        self.data.settings.theme_overrides = ThemeOverrides {
            extra_fields: extras,
            ..Default::default()
        };
        self.clear_theme_drafts();
        self.themes_ui.error = None;
        self.rebuild_theme();
        self.persist_launcher_preferences(cx);
        cx.notify();
    }
    pub(super) fn refresh_theme_inputs(&mut self) {
        self.themes_ui
            .drafts
            .retain(|key, _| key == "name" || key == "description");
        self.themes_ui.invalid.clear();
        self.native_fields.retain(|key, _| {
            !key.starts_with("theme:") || key == "theme:name" || key == "theme:description"
        });
    }
    pub(super) fn clear_theme_drafts(&mut self) {
        self.themes_ui.drafts.clear();
        self.themes_ui.invalid.clear();
        self.themes_ui.editing = None;
        self.native_fields
            .retain(|key, _| !key.starts_with("theme:"));
    }
    pub(super) fn create_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.clear_theme_drafts();
        self.themes_ui.editor_open = true;
        self.themes_ui.advanced_open = false;
        cx.notify();
    }
    pub(super) fn theme_gallery(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let t = &self.theme;
        let mut presets = theme::builtin_presets();
        presets.extend(
            self.data
                .settings
                .custom_theme_presets
                .iter()
                .filter_map(|raw| {
                    let id = raw["id"].as_str()?;
                    let mut preset = serde_json::from_value::<ThemePreset>(raw.clone())
                        .unwrap_or_else(|_| {
                            let mut fallback = theme::resolve_preset_or_default("monochrome");
                            fallback.id = id.into();
                            fallback.name = raw["name"].as_str().unwrap_or(id).into();
                            fallback
                        });
                    preset.builtin = false;
                    Some(preset)
                }),
        );
        div()
            .id("appearance-theme-gallery")
            .flex()
            .flex_wrap()
            .gap_2()
            .children(presets.into_iter().map(|preset| {
                let id = preset.id.clone();
                let builtin = preset.builtin;
                let selected = self.data.settings.theme_preset == id;
                let select = id.clone();
                let selector = if builtin {
                    format!("builtin-theme:{id}")
                } else {
                    format!("theme-select:{id}")
                };
                let mut tile = card(t)
                    .id(format!("theme-row:{id}"))
                    .w(gpui::relative(0.32))
                    .min_w(px(156.))
                    .flex_shrink_0()
                    .p_2()
                    .border_color(if selected { t.primary() } else { t.border() })
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        button_base("")
                            .id(selector.clone())
                            .debug_selector(move || selector.clone().into())
                            .aria_label(preset.name.clone())
                            .aria_selected(selected)
                            .h_auto()
                            .w_full()
                            .min_w_0()
                            .px_1()
                            .py_0()
                            .flex_col()
                            .items_stretch()
                            .gap_2()
                            .hover(|style| style.bg(t.muted()))
                            .child(super::appearance_settings::theme_swatches(
                                &preset, selected, t,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_ellipsis()
                                            .child(preset.name),
                                    )
                                    .when(selected, |row| row.child(icon("check", 12.))),
                            )
                            .on_click(cx.listener(move |v, _, _, cx| {
                                if builtin {
                                    v.clear_theme_drafts();
                                    v.set_theme(v.data.settings.theme_mode, select.clone(), cx);
                                } else {
                                    v.apply_saved_theme(select.clone(), cx);
                                }
                            })),
                    );
                if !builtin {
                    let edit = id.clone();
                    let remove = id.clone();
                    tile = tile.child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_1()
                            .child(
                                btn(t, "Edit", ButtonVariant::Ghost)
                                    .h(px(24.))
                                    .px_2()
                                    .text_xs()
                                    .aria_label(format!("Edit {}", preset.id))
                                    .debug_selector({
                                        let id = id.clone();
                                        move || format!("theme-edit:{id}").into()
                                    })
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        v.clear_theme_drafts();
                                        if let Some(raw) = v
                                            .data
                                            .settings
                                            .custom_theme_presets
                                            .iter()
                                            .find(|raw| raw["id"].as_str() == Some(&edit))
                                        {
                                            v.themes_ui.drafts.insert(
                                                "name".into(),
                                                raw["name"].as_str().unwrap_or_default().into(),
                                            );
                                            v.themes_ui.drafts.insert(
                                                "description".into(),
                                                raw["description"]
                                                    .as_str()
                                                    .unwrap_or_default()
                                                    .into(),
                                            );
                                            v.themes_ui.editing = Some(edit.clone());
                                            v.themes_ui.editor_open = true;
                                            v.themes_ui.advanced_open = true;
                                            v.set_theme(
                                                v.saved_theme_mode(&edit),
                                                edit.clone(),
                                                cx,
                                            );
                                        }
                                    })),
                            )
                            .child(
                                btn(t, "Delete", ButtonVariant::Ghost)
                                    .h(px(24.))
                                    .px_2()
                                    .text_xs()
                                    .aria_label(format!("Delete {}", preset.id))
                                    .id(format!("theme-delete:{id}"))
                                    .debug_selector({
                                        let id = id.clone();
                                        move || format!("theme-delete:{id}").into()
                                    })
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        v.themes_ui.delete = Some(remove.clone());
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                if self.themes_ui.delete.as_deref() == Some(&id) {
                    let confirm = id;
                    tile = tile.child(
                        div()
                            .border_t_1()
                            .border_color(t.border())
                            .pt_2()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_xs().child("Delete this saved theme?"))
                            .child(
                                btn(t, "Confirm delete", ButtonVariant::Danger)
                                    .h(px(26.))
                                    .text_xs()
                                    .debug_selector({
                                        let id = confirm.clone();
                                        move || format!("theme-confirm-delete:{id}").into()
                                    })
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        v.data
                                            .settings
                                            .custom_theme_presets
                                            .retain(|raw| raw["id"].as_str() != Some(&confirm));
                                        if v.data.settings.theme_preset == confirm {
                                            v.data.settings.theme_preset = "monochrome".into();
                                        }
                                        v.themes_ui.delete = None;
                                        v.clear_theme_drafts();
                                        v.rebuild_theme();
                                        v.persist_launcher_preferences(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                btn(t, "Cancel", ButtonVariant::Ghost)
                                    .h(px(24.))
                                    .text_xs()
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.themes_ui.delete = None;
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                tile
            }))
    }
    pub(super) fn custom_theme_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        if !self.themes_ui.editor_open {
            return div();
        }
        let mut panel = card(&t).p_3().flex().flex_col().gap_3().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                    if self.themes_ui.editing.is_some() {
                        "Edit theme"
                    } else {
                        "Create theme"
                    },
                ))
                .child(
                    btn(&t, "Close editor", ButtonVariant::Ghost)
                        .h(px(24.))
                        .text_xs()
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.themes_ui.editor_open = false;
                            cx.notify();
                        })),
                ),
        );
        panel = panel
            .child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Name"),
            )
            .child(self.theme_field("name", String::new(), cx))
            .child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Description"),
            )
            .child(self.theme_field("description", String::new(), cx))
            .child(
                btn(
                    &t,
                    if self.themes_ui.editing.is_some() {
                        "Save theme changes"
                    } else {
                        "Save as new theme"
                    },
                    ButtonVariant::Outline,
                )
                .debug_selector(|| "theme-save".into())
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.save_named_theme(cx))),
            );
        if let Some(error) = &self.themes_ui.error {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(t.destructive())
                    .child(error.clone()),
            );
        }
        panel = panel.child(
            btn(&t, "Advanced colors and borders", ButtonVariant::Outline)
                .debug_selector(|| "theme-advanced".into())
                .justify_start()
                .text_xs()
                .child(icon(
                    if self.themes_ui.advanced_open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    12.,
                ))
                .on_click(cx.listener(|v, _, _, cx| {
                    v.themes_ui.advanced_open = !v.themes_ui.advanced_open;
                    cx.notify();
                })),
        );
        if !self.themes_ui.advanced_open {
            return panel;
        }
        let overrides = serde_json::to_value(&self.data.settings.theme_overrides).unwrap();
        let tokens = serde_json::to_value(&self.theme.tokens).unwrap();
        panel=panel.child("Appearance overrides").child(div().text_xs().child("Leave a value empty to use the selected preset. Changes apply to the current appearance."));
        let mut fields = div().flex().flex_wrap().gap_3();
        for (key, default) in tokens.as_object().unwrap() {
            let initial = overrides[key].as_str().unwrap_or_default().to_string();
            let invalid = self.themes_ui.invalid.contains(key);
            fields = fields.child(
                div()
                    .w(gpui::relative(0.48))
                    .min_w(px(200.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().child(format!(
                        "{} ({})",
                        key.replace('_', " "),
                        default.as_str().unwrap_or_default()
                    )))
                    .child(self.theme_field(key, initial, cx))
                    .when(invalid, |row| {
                        row.child(
                            div()
                                .text_xs()
                                .text_color(t.destructive())
                                .child("Enter a valid hexadecimal color or radius."),
                        )
                    }),
            );
        }
        panel = panel.child(fields);
        let ratio = contrast(&self.theme);
        panel = panel
            .child(
                div()
                    .text_xs()
                    .when(ratio < 4.5, |label| label.text_color(t.destructive()))
                    .child(format!(
                        "Text contrast: {ratio:.1}:1{}",
                        if ratio < 4.5 { " — low contrast" } else { "" }
                    )),
            )
            .child(
                btn(&t, "Reset appearance overrides", ButtonVariant::Outline)
                    .debug_selector(|| "theme-reset".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                        let extras = v.data.settings.theme_overrides.extra_fields.clone();
                        v.data.settings.theme_overrides = ThemeOverrides {
                            extra_fields: extras,
                            ..Default::default()
                        };
                        v.clear_theme_drafts();
                        v.rebuild_theme();
                        v.persist_launcher_preferences(cx);
                        cx.notify();
                    })),
            );
        panel
    }
}
