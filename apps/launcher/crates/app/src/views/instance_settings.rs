//! Instance settings panels, with coalesced, serialized autosaves per instance.

#[cfg(test)]
mod acceptance_tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::{size, ClipboardItem, TestAppContext, VisualTestContext};

    #[gpui::test]
    async fn native_multiline_arguments_and_command_environment_edits_roundtrip_without_losing_extensions(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        std::fs::write(fixture.paths.settings_path("pack").unwrap(), serde_json::json!({
            "name":"pack", "override_java_args":true, "acceptanceExtension":true
        }).to_string()).unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Settings;
            v.preferences_ui.installations = Some(Vec::new());
        });
        root.update(cx, |v, _| {
            v.ensure_editor("pack");
            v.instance_ui.editors.get_mut("pack").unwrap().tab = SettingsTab::Java;
        });
        cx.simulate_window_resize(window.into(), size(px(1100.), px(1400.)));
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx)).unwrap();
        let args = "-Dfirst=one\n-Dsecond=two\n\"-Dpath=my game\"";
        cx.write_to_clipboard(ClipboardItem::new_string(args.into()));
        {
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            let bounds = visual.debug_bounds("instance:pack:jvm_args").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
            visual.simulate_keystrokes("ctrl-a ctrl-v");
        }
        cx.run_until_parked();
        root.update(cx, |v, cx| v.flush_instance_settings("pack", cx));
        fixture.until(cx, &root, |v| {
            let editor = &v.instance_ui.editors["pack"];
            !editor.saving && editor.pending.is_empty()
        }).await;
        let first = fixture.rt.block_on(industrialis_backend::settings::load_instance_settings(&fixture.paths, "pack")).unwrap();
        assert_eq!(first.jvm_args, args);
        assert_eq!(industrialis_backend::launch_config::split_command_args(&first.jvm_args), vec!["-Dfirst=one", "-Dsecond=two", "-Dpath=my game"]);
        root.update(cx, |v, cx| {
            let editor = v.instance_ui.editors.get_mut("pack").unwrap();
            *editor.field("wrapper_command") = FieldState::new("wrapper \"two words\"");
            editor.environment = vec![EnvRow { key: FieldState::new(" GAME_VALUE "), value: FieldState::new("value with spaces=kept") }];
            v.field_changed("pack", "wrapper_command", cx);
            v.field_changed("pack", "env-key-0", cx);
            v.flush_instance_settings("pack", cx);
        });
        fixture.until(cx, &root, |v| {
            let editor = &v.instance_ui.editors["pack"];
            !editor.saving && editor.pending.is_empty()
        }).await;
        let saved = fixture.rt.block_on(industrialis_backend::settings::load_instance_settings(&fixture.paths, "pack")).unwrap();
        assert_eq!(saved.jvm_args, args);
        assert_eq!(saved.wrapper_command, "wrapper \"two words\"");
        assert_eq!(saved.env_vars["GAME_VALUE"], "value with spaces=kept");
        assert_eq!(saved.extra_fields["acceptanceExtension"], true);
        fixture.cleanup();
    }
}

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{div, prelude::*, px, Context, Div, PathPromptOptions, Window};
use industrialis_core::instance_settings::InstanceSettings;
use serde_json::{json, Map, Value};

use super::{LauncherRoot, Tab};
use crate::input::{FieldState, NativeField, NativeFieldEvent};
use crate::ui::{btn, tab_btn, ButtonVariant};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum DetailTab {
    #[default]
    Info,
    Console,
    Files,
    Mods,
    Backups,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    General,
    Java,
    Commands,
    Environment,
}

struct EnvRow {
    key: FieldState,
    value: FieldState,
}

struct Editor {
    settings: InstanceSettings,
    fields: HashMap<String, FieldState>,
    environment: Vec<EnvRow>,
    tab: SettingsTab,
    pending: Map<String, Value>,
    generation: u64,
    saving: bool,
    invalid: HashSet<String>,
    error: Option<String>,
    java_busy: bool,
    java_result: Option<String>,
}

impl Editor {
    fn new(settings: InstanceSettings) -> Self {
        let mut vars: Vec<_> = settings.env_vars.iter().collect();
        vars.sort_by_key(|(key, _)| *key);
        let environment = vars
            .into_iter()
            .map(|(key, value)| EnvRow {
                key: FieldState::new(key),
                value: FieldState::new(value),
            })
            .collect();
        Self {
            settings,
            fields: HashMap::new(),
            environment,
            tab: SettingsTab::General,
            pending: Map::new(),
            generation: 0,
            saving: false,
            invalid: HashSet::new(),
            error: None,
            java_busy: false,
            java_result: None,
        }
    }

    fn field(&mut self, key: &str) -> &mut FieldState {
        if let Some(index) = key
            .strip_prefix("env-key-")
            .and_then(|s| s.parse::<usize>().ok())
        {
            return &mut self.environment[index].key;
        }
        if let Some(index) = key
            .strip_prefix("env-value-")
            .and_then(|s| s.parse::<usize>().ok())
        {
            return &mut self.environment[index].value;
        }
        self.fields.entry(key.to_string()).or_insert_with(|| {
            let value = serde_json::to_value(&self.settings).unwrap();
            let text = match &value[key] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            FieldState::new(&text)
        })
    }
}

#[derive(Default)]
pub(super) struct InstanceUi {
    pub detail_tab: DetailTab,
    editors: HashMap<String, Editor>,
}

impl InstanceUi {
    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.editors
            .values()
            .any(|e| e.saving || !e.pending.is_empty() || !e.invalid.is_empty() || e.java_busy)
    }
}

fn numeric_key(key: &str) -> bool {
    matches!(
        key,
        "min_ram_mb"
            | "max_ram_mb"
            | "perm_gen_mb"
            | "window_width"
            | "window_height"
            | "backup_retention_override"
    )
}

impl LauncherRoot {
    fn ensure_editor(&mut self, id: &str) {
        if !self.instance_ui.editors.contains_key(id) {
            if let Some(info) = self.data.selected_instance(Some(id)) {
                self.instance_ui
                    .editors
                    .insert(id.to_string(), Editor::new(info.settings.clone()));
            }
        }
    }

    pub(super) fn forget_instance_editor(&mut self, id: &str) {
        self.instance_ui.editors.remove(id);
    }

    fn edit_setting(&mut self, id: &str, key: &str, value: Value, cx: &mut Context<Self>) {
        if self.instance_is_busy(id) {
            return;
        }
        self.ensure_editor(id);
        let Some(editor) = self.instance_ui.editors.get_mut(id) else {
            return;
        };
        let mut next = serde_json::to_value(&editor.settings).unwrap();
        next[key] = value.clone();
        match serde_json::from_value(next) {
            Ok(settings) => editor.settings = settings,
            Err(e) => {
                editor.error = Some(e.to_string());
                cx.notify();
                return;
            }
        }
        editor.pending.insert(key.to_string(), value);
        self.refresh_generation += 1;
        editor.generation += 1;
        editor.error = None;
        let generation = editor.generation;
        let id = id.to_string();
        cx.notify();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = view.update(cx, |v, cx| {
                if v.instance_ui
                    .editors
                    .get(&id)
                    .is_some_and(|e| e.generation == generation)
                {
                    v.flush_instance_settings(&id, cx);
                }
            });
        })
        .detach();
    }

    fn field_changed(&mut self, id: &str, key: &str, cx: &mut Context<Self>) {
        let Some(editor) = self.instance_ui.editors.get_mut(id) else {
            return;
        };
        if key.starts_with("env-") {
            let env: HashMap<_, _> = editor
                .environment
                .iter()
                .filter_map(|row| {
                    let key = row.key.text.trim();
                    (!key.is_empty()).then(|| (key.to_string(), row.value.text.clone()))
                })
                .collect();
            self.edit_setting(id, "env_vars", json!(env), cx);
            return;
        }
        let text = editor.field(key).text.clone();
        let value = if key == "backup_retention_override" && text.trim().is_empty() {
            editor.invalid.remove(key);
            Value::Null
        } else if numeric_key(key) {
            match text.parse::<u32>() {
                Ok(value) if key != "backup_retention_override" || (1..=1000).contains(&value) => {
                    editor.invalid.remove(key);
                    json!(value)
                }
                _ => {
                    editor.invalid.insert(key.to_string());
                    cx.notify();
                    return;
                }
            }
        } else if key == "java_path" {
            if text.trim().is_empty() {
                Value::Null
            } else {
                json!(text)
            }
        } else {
            json!(text)
        };
        self.edit_setting(id, key, value, cx);
    }

    pub(super) fn flush_instance_settings(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(editor) = self.instance_ui.editors.get_mut(id) else {
            return;
        };
        if editor.saving || editor.pending.is_empty() || !editor.invalid.is_empty() {
            return;
        }
        let patch = std::mem::take(&mut editor.pending);
        editor.saving = true;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let id = id.to_string();
        let target = id.clone();
        let retry_patch = patch.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    industrialis_backend::settings::patch_instance_settings(&paths, &target, &patch)
                        .await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(editor) = v.instance_ui.editors.get_mut(&id) else {
                    return;
                };
                editor.saving = false;
                v.refresh_generation += 1;
                match result {
                    Ok(Ok(saved)) => {
                        if let Some(info) = v.data.instances.iter_mut().find(|i| i.id == id) {
                            info.settings = saved;
                        }
                        editor.error = None;
                        v.flush_instance_settings(&id, cx);
                    }
                    failure => {
                        let mut pending = retry_patch;
                        pending.extend(std::mem::take(&mut editor.pending));
                        editor.pending = pending;
                        editor.error = Some(match failure {
                            Ok(Err(e)) => format!("{e:#}"),
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Maintenance and launch wait for the latest editor writes, even when
    /// the user changes tabs or selects another instance during a save.
    pub(super) fn after_instance_settings_saved(
        &mut self,
        id: String,
        action: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.flush_instance_settings(&id, cx);
        cx.spawn(async move |view, cx| {
            let mut action = Some(action);
            loop {
                let stop = view
                    .update(cx, |v, cx| {
                        if let Some(editor) = v.instance_ui.editors.get(&id) {
                            if !editor.invalid.is_empty() || editor.error.is_some() {
                                v.data.status =
                                    "Resolve the instance settings error before continuing.".into();
                                cx.notify();
                                return true;
                            }
                            if editor.java_busy || editor.saving || !editor.pending.is_empty() {
                                v.flush_instance_settings(&id, cx);
                                return false;
                            }
                        }
                        if let Some(action) = action.take() {
                            action(v, cx);
                        }
                        true
                    })
                    .unwrap_or(true);
                if stop {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
            }
        })
        .detach();
    }

    fn instance_field(
        &mut self,
        id: &str,
        key: &str,
        placeholder: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        self.ensure_editor(id);
        let fid = format!("instance:{id}:{key}");
        let field = self
            .instance_ui
            .editors
            .get_mut(id)
            .unwrap()
            .field(key)
            .text
            .clone();
        if !self.native_fields.contains_key(&fid) {
            let multiline = key == "java_args" || key == "jvm_args";
            let entity = cx
                .new(|cx| NativeField::new(&field, self.theme.clone(), placeholder, multiline, cx));
            let instance = id.to_string();
            let setting = key.to_string();
            let focus_id = fid.clone();
            cx.subscribe(&entity, move |v, _, event, cx| {
                if v.instance_is_busy(&instance) {
                    return;
                }
                match event {
                    NativeFieldEvent::Change(value) => {
                        let Some(editor) = v.instance_ui.editors.get_mut(&instance) else {
                            return;
                        };
                        *editor.field(&setting) = FieldState::new(value);
                        v.field_changed(&instance, &setting, cx);
                    }
                    NativeFieldEvent::Submit | NativeFieldEvent::Blur => {
                        v.flush_instance_settings(&instance, cx)
                    }
                    NativeFieldEvent::Focus => v.focused_field = Some(focus_id.clone()),
                }
                cx.notify();
            })
            .detach();
            self.native_fields.insert(fid.clone(), entity);
        }
        let entity = self.native_fields[&fid].clone();
        entity.update(cx, |state, _| {
            state.value = field;
            state.theme = self.theme.clone();
            state.enabled = enabled;
        });
        div()
            .w_full()
            .debug_selector(move || fid.clone().into())
            .child(entity)
    }

    fn setting_toggle(
        &self,
        id: &str,
        key: &str,
        label: &str,
        checked: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let id = id.to_string();
        let key = key.to_string();
        let selector = format!("instance-toggle:{id}:{key}");
        crate::ui::checkbox(&self.theme, label, checked)
            .debug_selector(move || selector.clone().into())
            .tab_stop(enabled)
            .when(!enabled, |el| el.opacity(0.5).cursor_default())
            .on_click(
                cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                    if enabled {
                        v.edit_setting(&id, &key, json!(!checked), cx);
                    }
                }),
            )
    }

    fn settings_section(
        &self,
        id: &str,
        title: &str,
        override_key: Option<(&str, bool)>,
        body: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        let enabled = !self.instance_is_busy(id);
        let mut heading = div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_sm().child(title.to_string()));
        if let Some((key, checked)) = override_key {
            heading = heading.child(self.setting_toggle(id, key, "Override", checked, enabled, cx));
        }
        crate::ui::card(&self.theme)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(heading)
            .child(body)
    }

    fn labeled_setting(
        &mut self,
        id: &str,
        key: &str,
        label: &str,
        hint: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().child(label.to_string()))
            .child(self.instance_field(id, key, hint, enabled, cx))
    }

    pub(super) fn choose_instance_java(&mut self, id: &str, path: String, cx: &mut Context<Self>) {
        self.ensure_editor(id);
        if self.instance_is_busy(id) {
            return;
        }
        self.instance_ui
            .editors
            .get_mut(id)
            .unwrap()
            .fields
            .insert("java_path".into(), FieldState::new(&path));
        self.edit_setting(id, "java_path", json!(path), cx);
    }
    fn java_action(&mut self, id: String, detect: bool, cx: &mut Context<Self>) {
        self.ensure_editor(&id);
        let editor = self.instance_ui.editors.get_mut(&id).unwrap();
        if editor.java_busy {
            return;
        }
        editor.java_busy = true;
        let path = editor
            .settings
            .java_path
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.data.settings.default_java_path.clone());
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    if detect {
                        industrialis_backend::java::detect_java()
                            .await
                            .first()
                            .map(|j| j.path.clone())
                            .ok_or_else(|| anyhow::anyhow!("No Java installations found"))
                    } else {
                        industrialis_backend::java::test_java(path.as_deref()).await
                    }
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(editor) = v.instance_ui.editors.get_mut(&id) else {
                    return;
                };
                editor.java_busy = false;
                match result {
                    Ok(Ok(result)) if detect => {
                        editor
                            .fields
                            .insert("java_path".into(), FieldState::new(&result));
                        v.edit_setting(&id, "java_path", json!(result), cx);
                    }
                    Ok(Ok(result)) => editor.java_result = Some(result),
                    Ok(Err(e)) => editor.java_result = Some(format!("Failed: {e:#}")),
                    Err(e) => editor.java_result = Some(format!("Failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn browse_instance_java(&mut self, id: String, cx: &mut Context<Self>) {
        self.ensure_editor(&id);
        let Some(editor) = self.instance_ui.editors.get_mut(&id) else {
            return;
        };
        if editor.java_busy {
            return;
        }
        editor.java_busy = true;
        cx.notify();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Java executable".into()),
        });
        cx.spawn(async move |view, cx| {
            let result = picker.await;
            let _ = view.update(cx, |v, cx| {
                if let Some(editor) = v.instance_ui.editors.get_mut(&id) {
                    editor.java_busy = false;
                }
                if v.instance_is_busy(&id) || v.data.selected_instance(Some(&id)).is_none() {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            let path = path.to_string_lossy().into_owned();
                            v.ensure_editor(&id);
                            v.instance_ui
                                .editors
                                .get_mut(&id)
                                .unwrap()
                                .fields
                                .insert("java_path".into(), FieldState::new(&path));
                            v.edit_setting(&id, "java_path", json!(path), cx);
                        }
                    }
                    Ok(Err(e)) => v.data.status = format!("File picker failed: {e:#}"),
                    Err(e) => v.data.status = format!("File picker failed: {e}"),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn instance_backup_policy(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        self.ensure_editor(id);
        let Some(editor) = self.instance_ui.editors.get(id) else {
            return div();
        };
        let settings = editor.settings.clone();
        let error = editor.error.clone();
        let invalid = editor.invalid.contains("backup_retention_override");
        let enabled = !self.instance_is_busy(id);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.setting_toggle(
                id,
                "backups_enabled",
                "Upload backups automatically",
                settings.backups_enabled,
                enabled,
                cx,
            ))
            .child(self.labeled_setting(
                id,
                "backup_retention_override",
                "Cloud retention (blank uses launcher default)",
                "Default",
                enabled,
                cx,
            ))
            .when(invalid, |panel| {
                panel.child("Retention must be 1?1000, or blank for the launcher default.")
            })
            .when(error.is_some(), |panel| {
                panel.child(error.unwrap_or_default())
            })
    }

    pub(super) fn instance_settings_view(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        self.ensure_editor(id);
        let editor = &self.instance_ui.editors[id];
        let settings = editor.settings.clone();
        let tab = editor.tab;
        let error = editor.error.clone();
        let invalid = !editor.invalid.is_empty();
        let saving = editor.saving || !editor.pending.is_empty();
        let java_busy = editor.java_busy;
        let java_result = editor.java_result.clone();
        let env_count = editor.environment.len();
        let enabled = !self.instance_is_busy(id);
        let t = self.theme.clone();
        let mut panel = div().id("instance-settings-scroll").flex_1().min_h_0().overflow_y_scroll().px_4().pt_3().pb_4().flex().flex_col().gap_2()
            .child(btn(&t, "Open launcher settings", ButtonVariant::Secondary).text_xs()
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| { v.tab = Tab::Settings; cx.notify(); })))
            .child(div().text_size(px(11.0)).text_color(t.muted_foreground()).child("Instance settings below override defaults only when each section's override box is checked."))
            .child(div().flex().flex_wrap().justify_center().gap(px(2.0)).children([
                (SettingsTab::General, "General"), (SettingsTab::Java, "Java"), (SettingsTab::Commands, "Custom Commands"), (SettingsTab::Environment, "Environment")
            ].into_iter().map(|(value, label)| {
                let id = id.to_string();
                tab_btn(&t, label, value == tab).text_xs()
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| { v.instance_ui.editors.get_mut(&id).unwrap().tab = value; cx.notify(); }))
            })));
        match tab {
            SettingsTab::General => {
                let name =
                    self.labeled_setting(id, "name", "Display name", "GTNH instance", enabled, cx);
                panel = panel.child(self.settings_section(id, "Instance", None, name, cx));
                let active = enabled && settings.override_window;
                let mut body = div().flex().flex_col().gap_2().child(self.setting_toggle(
                    id,
                    "launch_maximized",
                    "Start Minecraft maximized",
                    settings.launch_maximized,
                    active,
                    cx,
                ));
                if !settings.launch_maximized {
                    body = body
                        .child(self.labeled_setting(
                            id,
                            "window_width",
                            "Width (pixels)",
                            "854",
                            active,
                            cx,
                        ))
                        .child(self.labeled_setting(
                            id,
                            "window_height",
                            "Height (pixels)",
                            "480",
                            active,
                            cx,
                        ));
                }
                body = body
                    .child(self.setting_toggle(
                        id,
                        "close_after_launch",
                        if cfg!(windows) {
                            "When the game starts, minimize the launcher"
                        } else {
                            "When the game starts, hide the launcher"
                        },
                        settings.close_after_launch,
                        active,
                        cx,
                    ))
                    .child(self.setting_toggle(
                        id,
                        "quit_after_game_stop",
                        "When the game closes, quit the launcher",
                        settings.quit_after_game_stop,
                        active,
                        cx,
                    ));
                panel = panel.child(self.settings_section(
                    id,
                    "Game window",
                    Some(("override_window", settings.override_window)),
                    body,
                    cx,
                ));
                let active = enabled && settings.override_console;
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.setting_toggle(
                        id,
                        "show_console_on_error",
                        "When the game crashes, show the log panel",
                        settings.show_console_on_error,
                        active,
                        cx,
                    ))
                    .child(self.setting_toggle(
                        id,
                        "auto_close_console",
                        "When the game quits, hide the log panel",
                        settings.auto_close_console,
                        active,
                        cx,
                    ));
                panel = panel.child(self.settings_section(
                    id,
                    "Console / logs",
                    Some(("override_console", settings.override_console)),
                    body,
                    cx,
                ));
                let active = enabled && settings.override_game_time;
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.setting_toggle(
                        id,
                        "show_game_time",
                        "Show time spent playing this instance",
                        settings.show_game_time,
                        active,
                        cx,
                    ))
                    .child(self.setting_toggle(
                        id,
                        "record_game_time",
                        "Record time spent playing this instance",
                        settings.record_game_time,
                        active,
                        cx,
                    ));
                panel = panel.child(self.settings_section(
                    id,
                    "Play time",
                    Some(("override_game_time", settings.override_game_time)),
                    body,
                    cx,
                ));
                let mut options = vec![(None, "Use launcher default account".to_string())];
                options.extend(
                    self.data
                        .accounts
                        .iter()
                        .map(|a| (Some(a.id.clone()), a.username.clone())),
                );
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(options.into_iter().map(|(value, label)| {
                        let id = id.to_string();
                        let value_click = value.clone();
                        btn(
                            &t,
                            &label,
                            if value == settings.account_id {
                                ButtonVariant::Secondary
                            } else {
                                ButtonVariant::Ghost
                            },
                        )
                        .text_xs()
                        .tab_stop(enabled && settings.override_account)
                        .when(!enabled || !settings.override_account, |el| el.opacity(0.5))
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                if enabled && settings.override_account {
                                    v.edit_setting(&id, "account_id", json!(value_click), cx);
                                }
                            },
                        ))
                    }));
                panel = panel.child(self.settings_section(
                    id,
                    "Override default account",
                    Some(("override_account", settings.override_account)),
                    body,
                    cx,
                ));
                let body = self.labeled_setting(
                    id,
                    "join_server_address",
                    "Server address",
                    "play.example.com",
                    enabled && settings.join_server_on_launch,
                    cx,
                );
                panel = panel.child(self.settings_section(
                    id,
                    "Auto-join server",
                    Some(("join_server_on_launch", settings.join_server_on_launch)),
                    body,
                    cx,
                ));
            }
            SettingsTab::Java => {
                let active = enabled && settings.override_java_location;
                let mut body = self.labeled_setting(
                    id,
                    "java_path",
                    "Java executable",
                    "Uses launcher default when override is off",
                    active && !java_busy,
                    cx,
                );
                body = body.child(
                    div().flex().flex_wrap().gap_1().children(
                        [("Detect", 0), ("Browse", 1), ("Test settings", 2)]
                            .into_iter()
                            .map(|(label, mode)| {
                                let id = id.to_string();
                                btn(
                                    &t,
                                    if java_busy && mode != 1 {
                                        "Working…"
                                    } else {
                                        label
                                    },
                                    ButtonVariant::Secondary,
                                )
                                .text_xs()
                                .when(!active || java_busy, |el| el.opacity(0.5))
                                .on_click(cx.listener(
                                    move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                        if active && !java_busy {
                                            if mode == 1 {
                                                v.browse_instance_java(id.clone(), cx);
                                            } else {
                                                v.java_action(id.clone(), mode == 0, cx);
                                            }
                                        }
                                    },
                                ))
                            }),
                    ),
                );
                body = body.child(self.java_chooser(Some(id.into()), active && !java_busy, cx));
                if let Some(result) = java_result {
                    body = body.child(div().text_xs().child(result));
                }
                body = body.child(self.setting_toggle(
                    id,
                    "skip_java_compat",
                    "Skip Java compatibility checks",
                    settings.skip_java_compat,
                    active,
                    cx,
                ));
                panel = panel.child(self.settings_section(
                    id,
                    "Java installation",
                    Some(("override_java_location", settings.override_java_location)),
                    body,
                    cx,
                ));
                let active = enabled && settings.override_memory;
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.labeled_setting(
                        id,
                        "min_ram_mb",
                        "Minimum memory (MiB) · -Xms",
                        "4096",
                        active,
                        cx,
                    ))
                    .child(self.labeled_setting(
                        id,
                        "max_ram_mb",
                        "Maximum memory (MiB) · -Xmx",
                        "6144",
                        active,
                        cx,
                    ))
                    .child(self.labeled_setting(
                        id,
                        "perm_gen_mb",
                        "PermGen (MiB)",
                        "128",
                        active,
                        cx,
                    ));
                panel = panel.child(self.settings_section(
                    id,
                    "Memory",
                    Some(("override_memory", settings.override_memory)),
                    body,
                    cx,
                ));
                let body = self.labeled_setting(
                    id,
                    "jvm_args",
                    "Java arguments",
                    "-XX:+UseG1GC",
                    enabled && settings.override_java_args,
                    cx,
                );
                panel = panel.child(self.settings_section(
                    id,
                    "Java arguments",
                    Some(("override_java_args", settings.override_java_args)),
                    body,
                    cx,
                ));
            }
            SettingsTab::Commands => {
                let active = enabled && settings.override_commands;
                let body = div().flex().flex_col().gap_2()
                    .child(self.labeled_setting(id, "pre_launch_command", "Pre-launch command", "", active, cx))
                    .child(self.labeled_setting(id, "wrapper_command", "Wrapper command", "e.g. optirun", active, cx))
                    .child(self.labeled_setting(id, "post_exit_command", "Post-exit command", "", active, cx))
                    .child(div().text_xs().text_color(t.muted_foreground()).child("Available variables: $INST_NAME, $INST_ID, $INST_DIR, $INST_MC_DIR, $INST_JAVA"));
                panel = panel.child(self.settings_section(
                    id,
                    "Custom commands",
                    Some(("override_commands", settings.override_commands)),
                    body,
                    cx,
                ));
            }
            SettingsTab::Environment => {
                let active = enabled && settings.override_env;
                let mut body = div().flex().flex_col().gap_2();
                for index in 0..env_count {
                    let id_click = id.to_string();
                    body = body.child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                self.instance_field(
                                    id,
                                    &format!("env-key-{index}"),
                                    "VAR_NAME",
                                    active,
                                    cx,
                                )
                                .flex_1()
                                .min_w_0(),
                            )
                            .child(
                                self.instance_field(
                                    id,
                                    &format!("env-value-{index}"),
                                    "value",
                                    active,
                                    cx,
                                )
                                .flex_1()
                                .min_w_0(),
                            )
                            .child(btn(&t, "×", ButtonVariant::Ghost).id(format!("env-remove:{id}:{index}")).aria_label(format!("Remove environment variable {}", index + 1)).on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                    if active {
                                        v.focused_field = None;
                                        v.instance_ui
                                            .editors
                                            .get_mut(&id_click)
                                            .unwrap()
                                            .environment
                                            .remove(index);
                                        v.field_changed(&id_click, "env-vars", cx);
                                    }
                                },
                            ))),
                    );
                }
                let id_click = id.to_string();
                body = body.child(
                    btn(&t, "Add variable", ButtonVariant::Secondary)
                        .text_xs()
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                if active {
                                    v.instance_ui
                                        .editors
                                        .get_mut(&id_click)
                                        .unwrap()
                                        .environment
                                        .push(EnvRow {
                                            key: FieldState::empty(),
                                            value: FieldState::empty(),
                                        });
                                    cx.notify();
                                }
                            },
                        )),
                );
                panel = panel.child(self.settings_section(
                    id,
                    "Environment variables",
                    Some(("override_env", settings.override_env)),
                    body,
                    cx,
                ));
            }
        }
        panel = panel.child(
            div()
                .text_xs()
                .text_color(t.muted_foreground())
                .child(if invalid {
                    "Enter valid non-negative numbers before saving."
                } else if saving {
                    "Saving…"
                } else {
                    "All changes saved"
                }),
        );
        if let Some(error) = error {
            let id = id.to_string();
            panel = panel
                .child(div().text_xs().text_color(t.destructive()).child(error))
                .child(
                    btn(&t, "Retry save", ButtonVariant::Secondary).on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                            v.instance_ui.editors.get_mut(&id).unwrap().error = None;
                            v.flush_instance_settings(&id, cx);
                        },
                    )),
                );
        }
        panel
    }
}
