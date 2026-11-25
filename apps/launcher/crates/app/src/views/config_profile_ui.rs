//! Config profile editor, profile pickers and the apply action.

use super::LauncherRoot;
use crate::input::{FieldState, NativeField, NativeFieldEvent};
use crate::ui::{btn, card, tint, ButtonVariant};
use gpui::{div, prelude::*, px, Context, Div, FontWeight};
use industrialis_backend::config_profiles::{
    load_profiles, save_profiles, ConfigOverride, ConfigProfile, ProfilesFile, PROFILES_VERSION,
};

pub(super) struct ProfileRow {
    pub(super) file: FieldState,
    pub(super) key: FieldState,
    pub(super) value: FieldState,
}

pub(super) struct ConfigProfilesUi {
    pub(super) profiles: Vec<ConfigProfile>,
    pub(super) selected: usize,
    name: FieldState,
    rows: Vec<ProfileRow>,
    load_started: bool,
    loading: bool,
    saving: bool,
    error: Option<String>,
    status: Option<String>,
}

impl Default for ConfigProfilesUi {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            selected: 0,
            name: FieldState::empty(),
            rows: Vec::new(),
            load_started: false,
            loading: false,
            saving: false,
            error: None,
            status: None,
        }
    }
}

impl ConfigProfilesUi {
    fn select(&mut self, index: usize) {
        self.selected = index;
        match self.profiles.get(index) {
            Some(profile) => {
                self.name = FieldState::new(&profile.name);
                self.rows = profile
                    .overrides
                    .iter()
                    .map(|rule| ProfileRow {
                        file: FieldState::new(&rule.file),
                        key: FieldState::new(&rule.key),
                        value: FieldState::new(&rule.value),
                    })
                    .collect();
            }
            None => {
                self.name = FieldState::empty();
                self.rows.clear();
            }
        }
    }

    fn draft(&self) -> ConfigProfile {
        ConfigProfile {
            name: self.name.text.trim().to_string(),
            overrides: self
                .rows
                .iter()
                .filter_map(|row| {
                    let file = row.file.text.trim();
                    let key = row.key.text.trim();
                    if file.is_empty() && key.is_empty() && row.value.text.trim().is_empty() {
                        return None;
                    }
                    Some(ConfigOverride {
                        file: file.to_string(),
                        key: key.to_string(),
                        value: row.value.text.clone(),
                    })
                })
                .collect(),
        }
    }

    fn blocking_error(&self) -> Option<String> {
        let draft = self.draft();
        if draft.name.is_empty() {
            return Some("Give the profile a name.".into());
        }
        if self.profiles.iter().enumerate().any(|(index, profile)| {
            index != self.selected && profile.name.eq_ignore_ascii_case(&draft.name)
        }) {
            return Some(format!("Another profile is already named {:?}.", draft.name));
        }
        for (index, row) in self.rows.iter().enumerate() {
            let empty_file = row.file.text.trim().is_empty();
            let empty_key = row.key.text.trim().is_empty();
            if empty_file != empty_key {
                return Some(format!(
                    "Override {} needs both a config path and a key.",
                    index + 1
                ));
            }
        }
        None
    }
}

impl LauncherRoot {
    pub(crate) fn load_config_profiles(&mut self, cx: &mut Context<Self>) {
        if self.config_profiles_ui.loading {
            return;
        }
        self.config_profiles_ui.loading = true;
        self.config_profiles_ui.load_started = true;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(async move { load_profiles(&paths).await }).await;
            let _ = view.update(cx, |v, cx| {
                v.config_profiles_ui.loading = false;
                match result {
                    Ok(Ok(file)) => {
                        v.config_profiles_ui.profiles = file.profiles;
                        let selected = v
                            .config_profiles_ui
                            .selected
                            .min(v.config_profiles_ui.profiles.len().saturating_sub(1));
                        v.config_profiles_ui.select(selected);
                        v.config_profiles_ui.error = None;
                    }
                    Ok(Err(error)) => v.config_profiles_ui.error = Some(format!("{error:#}")),
                    Err(error) => v.config_profiles_ui.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_config_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        self.config_profiles_ui.select(index);
        self.config_profiles_ui.error = None;
        self.config_profiles_ui.status = None;
        cx.notify();
    }

    fn add_config_profile(&mut self, cx: &mut Context<Self>) {
        let mut name = "New profile".to_string();
        let mut suffix = 2;
        while self
            .config_profiles_ui
            .profiles
            .iter()
            .any(|profile| profile.name == name)
        {
            name = format!("New profile {suffix}");
            suffix += 1;
        }
        self.config_profiles_ui.profiles.push(ConfigProfile {
            name: name.clone(),
            overrides: Vec::new(),
        });
        let index = self.config_profiles_ui.profiles.len() - 1;
        self.config_profiles_ui.select(index);
        self.config_profiles_ui.name = FieldState::new(&name);
        self.config_profiles_ui.error = None;
        self.config_profiles_ui.status = Some("New profile. Edit it, then save.".into());
        cx.notify();
    }

    fn add_profile_override(&mut self, cx: &mut Context<Self>) {
        self.config_profiles_ui.rows.push(ProfileRow {
            file: FieldState::empty(),
            key: FieldState::empty(),
            value: FieldState::empty(),
        });
        cx.notify();
    }

    fn remove_profile_override(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.config_profiles_ui.rows.len() {
            self.config_profiles_ui.rows.remove(index);
        }
        cx.notify();
    }

    fn save_config_profiles(&mut self, cx: &mut Context<Self>) {
        if self.config_profiles_ui.saving || self.config_profiles_ui.loading {
            return;
        }
        if let Some(error) = self.config_profiles_ui.blocking_error() {
            self.config_profiles_ui.error = Some(error);
            self.config_profiles_ui.status = None;
            cx.notify();
            return;
        }
        let mut profiles = self.config_profiles_ui.profiles.clone();
        let draft = self.config_profiles_ui.draft();
        let index = self.config_profiles_ui.selected;
        if index < profiles.len() {
            profiles[index] = draft;
        } else {
            profiles.push(draft);
        }
        self.persist_config_profiles(profiles, "Config profile saved", cx);
    }

    fn delete_config_profile(&mut self, cx: &mut Context<Self>) {
        if self.config_profiles_ui.saving {
            return;
        }
        let index = self.config_profiles_ui.selected;
        if index >= self.config_profiles_ui.profiles.len() {
            return;
        }
        let mut profiles = self.config_profiles_ui.profiles.clone();
        profiles.remove(index);
        let next = index.min(profiles.len().saturating_sub(1));
        self.persist_config_profiles(profiles, "Config profile deleted", cx);
        self.config_profiles_ui.select(next);
    }

    fn persist_config_profiles(
        &mut self,
        profiles: Vec<ConfigProfile>,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        self.config_profiles_ui.saving = true;
        self.config_profiles_ui.error = None;
        self.config_profiles_ui.status = None;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let file = ProfilesFile {
            version: PROFILES_VERSION,
            profiles,
        };
        let message = message.to_string();
        cx.spawn(async move |view, cx| {
            let saved = file.clone();
            let result = rt
                .spawn(async move { save_profiles(&paths, &saved).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.config_profiles_ui.saving = false;
                match result {
                    Ok(Ok(file)) => {
                        v.config_profiles_ui.profiles = file.profiles;
                        let selected = v
                            .config_profiles_ui
                            .selected
                            .min(v.config_profiles_ui.profiles.len().saturating_sub(1));
                        v.config_profiles_ui.select(selected);
                        v.config_profiles_ui.status = Some(message);
                    }
                    Ok(Err(error)) => v.config_profiles_ui.error = Some(format!("{error:#}")),
                    Err(error) => v.config_profiles_ui.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Applies a profile to an existing instance after pending settings saves.
    pub(super) fn apply_config_profile(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        if self.instance_is_busy(id) {
            return;
        }
        let id = id.to_string();
        let name = name.to_string();
        let target = id.clone();
        self.after_instance_settings_saved(
            id.clone(),
            move |view, cx| view.begin_apply_config_profile(target, name, cx),
            cx,
        );
    }

    fn begin_apply_config_profile(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        self.busy.insert(format!("maintenance:{id}"));
        self.data.status = format!("Applying config profile {name:?}…");
        cx.notify();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        cx.spawn(async move |view, cx| {
            let task_paths = paths.clone();
            let task_id = id.clone();
            let task_name = name.clone();
            let applied = rt
                .spawn(async move {
                    industrialis_backend::config_profiles::apply_profile(
                        &task_paths,
                        &task_id,
                        &task_name,
                    )
                    .await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("maintenance:{id}"));
                match applied {
                    Ok(Ok(report)) => {
                        v.data.status = report.summary(&name);
                        v.record_applied_profile(&id, &name);
                    }
                    Ok(Err(error)) => {
                        v.data.status = format!("Config profile failed: {error:#}")
                    }
                    Err(error) => v.data.status = format!("Config profile task failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn profile_field_value(&self, id: &str) -> String {
        if id == "profile-name" {
            return self.config_profiles_ui.name.text.clone();
        }
        let Some(rest) = id.strip_prefix("profile-row:") else {
            return String::new();
        };
        let mut parts = rest.split(':');
        let index = parts.next().and_then(|part| part.parse::<usize>().ok());
        let field = parts.next();
        let Some(row) = index.and_then(|index| self.config_profiles_ui.rows.get(index)) else {
            return String::new();
        };
        match field {
            Some("file") => row.file.text.clone(),
            Some("key") => row.key.text.clone(),
            Some("value") => row.value.text.clone(),
            _ => String::new(),
        }
    }

    fn set_profile_field(&mut self, id: &str, value: String) {
        if id == "profile-name" {
            self.config_profiles_ui.name = FieldState::new(&value);
            return;
        }
        let Some(rest) = id.strip_prefix("profile-row:") else {
            return;
        };
        let mut parts = rest.split(':');
        let index = parts.next().and_then(|part| part.parse::<usize>().ok());
        let field = parts.next();
        let Some(row) = index.and_then(|index| self.config_profiles_ui.rows.get_mut(index)) else {
            return;
        };
        let field = match field {
            Some("file") => &mut row.file,
            Some("key") => &mut row.key,
            Some("value") => &mut row.value,
            _ => return,
        };
        *field = FieldState::new(&value);
    }

    fn profile_field(
        &mut self,
        id: String,
        placeholder: &str,
        accessibility: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = self.profile_field_value(&id);
        if !self.native_fields.contains_key(&id) {
            let entity = cx.new(|cx| NativeField::new(&value, self.theme.clone(), placeholder, false, cx));
            let field_id = id.clone();
            cx.subscribe(&entity, move |v, _, event, cx| {
                match event {
                    NativeFieldEvent::Change(value) => {
                        v.set_profile_field(&field_id, value.clone());
                    }
                    NativeFieldEvent::Focus => v.focused_field = Some(field_id.clone()),
                    _ => {}
                }
                cx.notify();
            })
            .detach();
            self.native_fields.insert(id.clone(), entity);
        }
        let entity = self.native_fields[&id].clone();
        entity.update(cx, |state, _| {
            state.value = value;
            state.theme = self.theme.clone();
            state.enabled = true;
            state.accessibility_label = accessibility.to_string();
        });
        div()
            .flex_1()
            .min_w_0()
            .debug_selector({
                let id = id.clone();
                move || id.clone().into()
            })
            .child(entity)
    }

    pub(super) fn config_profile_card(
        &mut self,
        id: &str,
        applied: Option<&str>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        if !self.config_profiles_ui.load_started {
            self.load_config_profiles(cx);
        }
        let t = self.theme.clone();
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .text_xs()
                .text_color(t.muted_foreground())
                .child(match applied {
                    Some(name) => format!("Last applied: {name}"),
                    None => "No config profile applied yet.".to_string(),
                }),
        );
        let mut row = div().flex().flex_wrap().gap_1();
        let instance = id.to_string();
        for profile in &self.config_profiles_ui.profiles {
            let name = profile.name.clone();
            let selector = format!("instance-profile:{name}");
            let instance = instance.clone();
            row = row.child(
                btn(&t, &name, ButtonVariant::Outline)
                    .text_xs()
                    .debug_selector(move || selector.clone().into())
                    .when(!enabled, |button| button.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if enabled {
                            v.apply_config_profile(&instance, &name, cx);
                        }
                    })),
            );
        }
        body = body.child(row).child(
            div().text_xs().text_color(t.muted_foreground()).child(
                "Applying edits the instance's config values in place; anything the pack no longer has is skipped and reported.",
            ),
        );
        self.settings_section(id, "Config profile", None, body, cx)
    }

    pub(super) fn config_profiles_section(&mut self, cx: &mut Context<Self>) -> Div {
        if !self.config_profiles_ui.load_started {
            self.load_config_profiles(cx);
        }
        let t = self.theme.clone();
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .py(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Config profiles"),
            )
            .child(div().text_xs().text_color(t.muted_foreground()).child(
                "Named overrides that edit pack config values in place. Pick one when creating an instance, or apply it to an existing one.",
            ));
        let mut list = div().flex().flex_wrap().gap_2();
        for (index, profile) in self.config_profiles_ui.profiles.iter().enumerate() {
            let selected = index == self.config_profiles_ui.selected;
            let selector = format!("profile:{}", profile.name);
            list = list.child(
                btn(
                    &t,
                    &profile.name,
                    if selected {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Outline
                    },
                )
                .text_xs()
                .debug_selector(move || selector.clone().into())
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    v.select_config_profile(index, cx)
                })),
            );
        }
        list = list.child(
            btn(&t, "New profile", ButtonVariant::Outline)
                .text_xs()
                .debug_selector(|| "profile-add".into())
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.add_config_profile(cx))),
        );
        content = content.child(list);
        if self.config_profiles_ui.loading {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Loading profiles…"),
            );
        }
        if let Some(error) = &self.config_profiles_ui.error {
            content = content.child(div().text_xs().text_color(t.destructive()).child(error.clone()));
        }
        if let Some(status) = &self.config_profiles_ui.status {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child(status.clone()),
            );
        }
        if self.config_profiles_ui.profiles.is_empty() && !self.config_profiles_ui.loading {
            content = content.child(
                card(&t)
                    .p_3()
                    .child(div().text_xs().text_color(t.muted_foreground()).child(
                        "No config profiles yet. Create one to reuse the same config values across instances.",
                    )),
            );
            return content;
        }
        let mut editor = card(&t)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Profile name"),
            )
            .child(self.profile_field(
                "profile-name".into(),
                "GTNH quiet defaults",
                "Config profile name",
                cx,
            ));
        for index in 0..self.config_profiles_ui.rows.len() {
            editor = editor.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(tint(t.border(), 0.5))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(self.profile_field(
                                format!("profile-row:{index}:file"),
                                "config/GregTech/Pollution.cfg",
                                "Config file path",
                                cx,
                            ))
                            .child(self.profile_field(
                                format!("profile-row:{index}:key"),
                                "Activate Pollution",
                                "Config key",
                                cx,
                            ))
                            .child(
                                btn(&t, "×", ButtonVariant::Ghost)
                                    .id(format!("profile-remove:{index}"))
                                    .aria_label(format!("Remove override {}", index + 1))
                                    .on_click(cx.listener(
                                        move |v, _: &gpui::ClickEvent, _, cx| {
                                            v.remove_profile_override(index, cx)
                                        },
                                    )),
                            ),
                    )
                    .child(self.profile_field(
                        format!("profile-row:{index}:value"),
                        "false",
                        "Config value",
                        cx,
                    )),
            );
        }
        editor = editor
            .child(
                btn(&t, "Add override", ButtonVariant::Outline)
                    .text_xs()
                    .debug_selector(|| "profile-add-override".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                        v.add_profile_override(cx)
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        btn(&t, "Save profile", ButtonVariant::Primary)
                            .text_xs()
                            .debug_selector(|| "profile-save".into())
                            .when(self.config_profiles_ui.saving, |button| {
                                button.opacity(0.5).cursor_default()
                            })
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.save_config_profiles(cx)
                            })),
                    )
                    .child(
                        btn(&t, "Delete profile", ButtonVariant::Danger)
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                v.delete_config_profile(cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child(if self.config_profiles_ui.saving {
                                "Saving…"
                            } else {
                                "Changes apply to new installs and to Apply in instance settings."
                            }),
                    ),
            );
        content.child(editor)
    }
}
