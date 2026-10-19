//! Per-instance file drafts. File changes save first; folder expansion preserves the draft.
use super::{DetailTab, LauncherRoot};
use crate::{
    input::{NativeField, NativeFieldEvent},
    ui::{btn, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div, KeyDownEvent};
use industrialis_backend::minecraft_files::{self as files, MinecraftEntry};
use industrialis_core::forge_config;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct FilesUi {
    pub(super) editors: HashMap<String, FileDraft>,
    pub(super) opening: HashSet<String>,
    pub(super) windows: HashMap<String, gpui::WindowHandle<super::editor_window::EditorWindow>>,
}
#[derive(Default)]
pub(super) struct FileDraft {
    no_wrap: bool,
    pub(super) explorer: super::file_explorer::Explorer,
    pub(super) selected: Option<String>,
    content: String,
    saved: String,
    loading: bool,
    saving: bool,
    large: bool,
    external: bool,
    generation: u64,
    pub(super) error: Option<String>,
    easy: bool,
    easy_values: HashMap<usize, String>,
    easy_list: Option<gpui::ListState>,
    easy_list_file: Option<String>,
    forge_cache: Option<(String, Option<std::sync::Arc<forge_config::Document>>)>,
    invalid: HashSet<usize>,
}
impl FileDraft {
    fn forge_document(&mut self) -> Option<std::sync::Arc<forge_config::Document>> {
        if !self
            .selected
            .as_deref()
            .is_some_and(forge_config::is_forge_file)
        {
            return None;
        }
        if self
            .forge_cache
            .as_ref()
            .is_none_or(|(source, _)| source != &self.content)
        {
            let doc = forge_config::parse(&self.content).map(std::sync::Arc::new);
            self.forge_cache = Some((self.content.clone(), doc));
            // Advanced edits can change descriptions and multiline row heights.
            // Keep the scroll anchor while invalidating measurements of old rows.
            if let Some(list) = &self.easy_list {
                list.remeasure();
            }
        }
        self.forge_cache.as_ref().and_then(|(_, doc)| doc.clone())
    }
    pub(super) fn dirty(&self) -> bool {
        self.selected.is_some() && (self.content != self.saved || !self.invalid.is_empty())
    }
}
pub(super) fn instance_id(key: &str) -> &str {
    key.split_once('\0').map_or(key, |(id, _)| id)
}
impl FilesUi {
    pub(super) fn dirty_key(&self, id: &str) -> Option<String> {
        self.editors
            .iter()
            .find(|(key, d)| instance_id(key) == id && (d.dirty() || d.saving))
            .map(|(key, _)| key.clone())
    }
    #[cfg(test)]
    pub(super) fn dirty(&self, id: &str) -> bool {
        self.dirty_key(id).is_some()
    }
    pub(super) fn unsaved_instance(&self) -> Option<String> {
        self.editors
            .iter()
            .find(|(_, d)| d.dirty() || d.saving)
            .map(|(id, _)| id.clone())
    }
}
impl LauncherRoot {
    pub(super) fn navigate_file(
        &mut self,
        id: String,
        entry: MinecraftEntry,
        cx: &mut Context<Self>,
    ) {
        if entry.is_dir {
            self.toggle_explorer_folder(id, entry.rel_path, cx);
            return;
        }
        if self
            .files_ui
            .editors
            .get(&id)
            .is_some_and(|d| d.loading || d.saving)
        {
            return;
        }
        let key = format!("{}\0{}", instance_id(&id), entry.rel_path);
        if self.files_ui.windows.contains_key(&key) || self.files_ui.opening.contains(&key) {
            self.focus_file_window(&key, cx);
            return;
        }
        let target = id.clone();
        self.save_file_then(id, move |v, cx| v.open_file_saved(target, entry, cx), cx);
    }
    fn open_file_saved(&mut self, id: String, entry: MinecraftEntry, cx: &mut Context<Self>) {
        if id.contains('\0')
            && entry.rel_path
                != self.files_ui.editors[&id]
                    .selected
                    .clone()
                    .unwrap_or_default()
        {
            return;
        }
        if entry.is_dir {
            self.toggle_explorer_folder(id, entry.rel_path, cx);
            return;
        }
        if !entry.editable {
            self.files_ui.editors.get_mut(&id).unwrap().error =
                Some("This file type cannot be edited in the launcher.".into());
            cx.notify();
            return;
        }
        let draft = self.files_ui.editors.get_mut(&id).unwrap();
        draft.generation += 1;
        let generation = draft.generation;
        draft.selected = Some(entry.rel_path.clone());
        draft.content.clear();
        draft.saved.clear();
        draft.easy_values.clear();
        draft.invalid.clear();
        draft.large = entry.too_large_to_edit;
        draft.external = false;
        draft.error = None;
        if draft.large {
            cx.notify();
            return;
        }
        draft.loading = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = instance_id(&id).to_string();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { files::read_file(&paths, &target, &entry.rel_path).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(d) = v.files_ui.editors.get_mut(&id) else {
                    return;
                };
                if d.generation != generation {
                    return;
                }
                d.loading = false;
                match result {
                    Ok(Ok(text)) => {
                        if !d
                            .selected
                            .as_deref()
                            .is_some_and(forge_config::is_forge_file)
                            || forge_config::parse(&text).is_none()
                        {
                            d.easy = false;
                        }
                        d.content = text.clone();
                        d.saved = text;
                    }
                    Ok(Err(e)) => d.error = Some(format!("{e:#}")),
                    Err(e) => d.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn save_file_then(
        &mut self,
        id: String,
        next: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(d) = self.files_ui.editors.get_mut(&id) else {
            next(self, cx);
            return;
        };
        if d.loading || d.saving {
            return;
        }
        if !d.invalid.is_empty() {
            d.error = Some("Fix the invalid Forge values before saving.".into());
            cx.notify();
            return;
        }
        if !d.dirty() {
            next(self, cx);
            return;
        }
        let Some(rel) = d.selected.clone() else {
            return;
        };
        let text = d.content.clone();
        let expected = d.saved.clone();
        d.saving = true;
        d.error = None;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = instance_id(&id).to_string();
        let content = text.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    files::write_file(&paths, &target, &rel, content, true, Some(expected)).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(d) = v.files_ui.editors.get_mut(&id) else {
                    return;
                };
                d.saving = false;
                match result {
                    Ok(Ok(())) => {
                        d.saved = text;
                        if let Some(rel) = &d.selected {
                            for entry in d
                                .explorer
                                .children
                                .values_mut()
                                .flatten()
                                .chain(d.explorer.search.results.entries.iter_mut())
                            {
                                if entry.rel_path == *rel {
                                    entry.has_persistent_override = true;
                                }
                            }
                        }
                        v.data.status = "File saved with persistent override".into();
                        next(v, cx);
                    }
                    Ok(Err(e)) => d.error = Some(format!("{e:#}")),
                    Err(e) => d.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn file_external(&mut self, id: String, cx: &mut Context<Self>) {
        let target = id.clone();
        self.save_file_then(
            id,
            move |v, cx| {
                let Some(rel) = v
                    .files_ui
                    .editors
                    .get(&target)
                    .and_then(|d| d.selected.clone())
                else {
                    return;
                };
                let paths = v.data.paths.clone();
                let rt = v.data.rt.clone();
                let instance = instance_id(&target).to_string();
                v.files_ui.editors.get_mut(&target).unwrap().loading = true;
                cx.spawn(async move |view, cx| {
                    let result =
                        rt.spawn(async move {
                            files::open_external_editor(&paths, &instance, &rel).await
                        })
                        .await;
                    let _ = view.update(cx, |v, cx| {
                        let Some(d) = v.files_ui.editors.get_mut(&target) else {
                            return;
                        };
                        d.loading = false;
                        match result {
                            Ok(Ok(())) => {
                                d.external = true;
                                d.error = None;
                            }
                            Ok(Err(e)) => d.error = Some(format!("{e:#}")),
                            Err(e) => d.error = Some(e.to_string()),
                        }
                        cx.notify();
                    });
                })
                .detach();
            },
            cx,
        );
    }
    fn remove_file_override(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(d) = self.files_ui.editors.get_mut(&id) else {
            return;
        };
        if d.dirty() || d.saving || d.loading {
            return;
        }
        let Some(rel) = d.selected.clone() else {
            return;
        };
        d.saving = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        let target = instance_id(&id).to_string();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { files::delete_persistent_file(&paths, &target, &rel).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(d) = v.files_ui.editors.get_mut(&id) else {
                    return;
                };
                d.saving = false;
                match result {
                    Ok(Ok(())) => {
                        d.external = false;
                        v.data.status = "Override removed; the current game file is kept".into();
                        v.refresh_explorer(id, cx);
                    }
                    Ok(Err(e)) => d.error = Some(format!("{e:#}")),
                    Err(e) => d.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn files_view(
        &mut self,
        id: &str,
        panel_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        if !self
            .files_ui
            .editors
            .entry(id.into())
            .or_default()
            .explorer
            .loaded
        {
            self.refresh_explorer(id.into(), cx);
        }
        let d = &self.files_ui.editors[id];
        let selected = d.selected.clone();
        let content = d.content.clone();
        let dirty = d.dirty();
        let large = d.large;
        let loading = d.loading || d.saving;
        let external = d.external;
        let error = d.error.clone();
        let has_error = error.is_some();
        let enabled = !loading
            && !self
                .busy
                .iter()
                .any(|key| key.ends_with(&format!(":{}", instance_id(id))));
        let easy = d.easy;
        let wide = panel_width >= 580.;
        let has_selection = selected.is_some();
        let target = id.to_string();
        let save_id = target.clone();
        let external_id = target.clone();
        let reset_id = target.clone();
        let reload_id = target.clone();
        let detached = id.contains('\0');
        let browser = self.file_explorer(id, panel_width, wide, has_selection, cx);
        let mut view = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(self.theme.card())
            .border_1()
            .border_color(self.theme.border())
            .rounded(self.theme.radius());
        if let Some(rel) = selected {
            view = view.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(format!("{}{}", rel, if dirty { " (unsaved)" } else { "" })),
            );
            if large {
                view = view.child("File exceeds the 2 MB editor limit. Use an external editor.");
            } else {
                let doc = self.files_ui.editors.get_mut(id).unwrap().forge_document();
                let can_easy = doc.is_some();
                let mut modes = div().flex().flex_shrink_0().gap_1();
                for (is_easy, label) in [(true, "Easy"), (false, "Advanced")] {
                    let mode_id = id.to_string();
                    let inactive = is_easy != easy;
                    modes = modes.child(
                        btn(
                            &self.theme,
                            label,
                            if !inactive {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            },
                        )
                        .debug_selector(move || {
                            if inactive && can_easy {
                                "minecraft-mode".into()
                            } else {
                                format!("minecraft-mode:{label}").into()
                            }
                        })
                        .when(is_easy && !can_easy, |button| {
                            button.opacity(0.6).cursor_default()
                        })
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _, cx| {
                                let d = v.files_ui.editors.get_mut(&mode_id).unwrap();
                                if d.invalid.is_empty() && (!is_easy || can_easy) {
                                    d.easy = is_easy;
                                }
                                cx.notify();
                            },
                        )),
                    );
                }
                view = view.child(modes);
                if !can_easy {
                    view = view.child(div().flex_shrink_0().text_xs().text_color(self.theme.muted_foreground())
                        .child("Easy mode supports Forge .cfg properties. This file uses Advanced mode."));
                }
                if easy && doc.is_some() {
                    view = view.child(self.forge_easy_view(id, doc.unwrap(), enabled, cx));
                } else {
                    let wrap_id = id.to_string();
                    let no_wrap = self.files_ui.editors[id].no_wrap;
                    view = view.child(
                        crate::ui::checkbox(&self.theme, "Wrap long lines", !no_wrap)
                            .debug_selector(|| "minecraft-wrap".into())
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.files_ui.editors.get_mut(&wrap_id).unwrap().no_wrap = !no_wrap;
                                cx.notify();
                            })),
                    );
                    let fid = format!("minecraft:{id}:{rel}");
                    if !self.native_fields.contains_key(&fid) {
                        let entity = cx.new(|cx| {
                            NativeField::new(
                                &content,
                                self.theme.clone(),
                                "File contents",
                                true,
                                cx,
                            )
                        });
                        let target = id.to_string();
                        cx.subscribe(&entity, move |v, _, event, cx| {
                            if let NativeFieldEvent::Change(text) = event {
                                if let Some(d) = v.files_ui.editors.get_mut(&target) {
                                    if !d.loading && !d.saving {
                                        d.content = text.clone();
                                        d.easy_values.clear();
                                        cx.notify();
                                    }
                                }
                            }
                        })
                        .detach();
                        self.native_fields.insert(fid.clone(), entity);
                    }
                    let entity = self.native_fields[&fid].clone();
                    entity.update(cx, |field, _| {
                        field.value = content;
                        field.theme = self.theme.clone();
                        field.enabled = enabled;
                        field.height = 240.;
                        field.fill_height = true;
                        field.code_editor = true;
                        field.code_language = crate::code_highlighting::language_for_path(&rel);
                        field.soft_wrap = !no_wrap;
                    });
                    view = view.child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .flex()
                            .overflow_hidden()
                            .debug_selector(|| "minecraft-content".into())
                            .child(entity),
                    );
                }
            }
            let save_id = id.to_string();
            view = view.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        btn(
                            &self.theme,
                            if loading { "Working..." } else { "Save" },
                            ButtonVariant::Primary,
                        )
                        .debug_selector(|| "minecraft-save".into())
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _, cx| {
                                v.save_file_then(save_id.clone(), |_, _| {}, cx)
                            },
                        )),
                    )
                    .child(
                        btn(&self.theme, "Reload / discard draft", ButtonVariant::Ghost).on_click(
                            cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                if let Some(d) = v.files_ui.editors.get(&reload_id) {
                                    if d.saving || d.loading {
                                        return;
                                    }
                                    let rel = d.selected.clone().unwrap();
                                    v.open_file_saved(
                                        reload_id.clone(),
                                        MinecraftEntry {
                                            name: rel.clone(),
                                            rel_path: rel,
                                            is_dir: false,
                                            has_persistent_override: false,
                                            editable: true,
                                            too_large_to_edit: false,
                                        },
                                        cx,
                                    );
                                }
                            }),
                        ),
                    )
                    .child(
                        btn(&self.theme, "External editor", ButtonVariant::Ghost).on_click(
                            cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.file_external(external_id.clone(), cx)
                            }),
                        ),
                    )
                    .child(
                        btn(&self.theme, "Remove override", ButtonVariant::Ghost).on_click(
                            cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.remove_file_override(reset_id.clone(), cx)
                            }),
                        ),
                    ),
            );
            view = view.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(self.theme.muted_foreground())
                    .child("Save keeps a persistent copy restored after pack updates."),
            );
            if external {
                view = view.child(
                    "External edits are mirrored to persistent-minecraft. Reload to see them here.",
                );
            }
            if let Some(error) = files::watch_error(&self.data.paths, instance_id(id), &rel) {
                view = view.child(format!("External mirror failed: {error}"));
            }
        } else {
            view = view.child(div().flex_1().flex().flex_col().justify_center().items_center().gap_2()
                .child("Select a file")
                .child(div().text_xs().text_color(self.theme.muted_foreground())
                    .child("Open a configuration file for Easy controls or the highlighted Advanced editor.")));
        }
        if let Some(error) = error {
            view = view.child(div().text_color(self.theme.destructive()).child(error));
        }
        let selector = format!("minecraft-editor:{id}");
        let resize_id = id.to_owned();
        let move_id = id.to_owned();
        let up_id = id.to_owned();
        let splitter = div()
            .w(px(5.))
            .h_full()
            .flex_shrink_0()
            .cursor_col_resize()
            .hover(|style| style.bg(crate::ui::tint(self.theme.primary(), 0.25)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |v, e: &gpui::MouseDownEvent, _, cx| {
                    let Some(draft) = v.files_ui.editors.get_mut(&resize_id) else {
                        return;
                    };
                    let tree = &mut draft.explorer;
                    tree.resizing = Some((
                        f32::from(e.position.x),
                        tree.width.min((panel_width * 0.55).max(220.)),
                    ));
                    cx.stop_propagation();
                }),
            );
        div()
            .id(selector.clone())
            .debug_selector(move || selector.clone().into())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .gap_0()
            .p_2()
            .overflow_hidden()
            .when(!wide, |frame| frame.flex_col().gap_2())
            .on_mouse_move(cx.listener(move |v, e: &gpui::MouseMoveEvent, _, cx| {
                let Some(draft) = v.files_ui.editors.get_mut(&move_id) else {
                    return;
                };
                let tree = &mut draft.explorer;
                if let Some((origin, width)) = tree.resizing {
                    if e.pressed_button == Some(gpui::MouseButton::Left) {
                        tree.width = (width + f32::from(e.position.x) - origin)
                            .clamp(220., (panel_width * 0.55).max(220.));
                        cx.notify();
                    } else {
                        tree.resizing = None;
                    }
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |v, _, _, _| {
                    if let Some(draft) = v.files_ui.editors.get_mut(&up_id) {
                        draft.explorer.resizing = None;
                    }
                }),
            )
            .on_key_down(cx.listener(move |v, e: &KeyDownEvent, _, cx| {
                if (e.keystroke.modifiers.control || e.keystroke.modifiers.platform)
                    && e.keystroke.key == "s"
                {
                    v.save_file_then(save_id.clone(), |_, _| {}, cx);
                    cx.stop_propagation();
                }
            }))
            .when(!detached, |frame| {
                frame
                    .child(browser)
                    .when(wide, |frame| frame.child(splitter))
            })
            .when(has_selection || wide || detached || has_error, |frame| {
                frame.child(view)
            })
    }
    fn change_forge_value(&mut self, id: &str, index: usize, text: String, cx: &mut Context<Self>) {
        let d = self.files_ui.editors.get_mut(id).unwrap();
        d.easy_values.insert(index, text.clone());
        match forge_config::update(&d.content, index, &text) {
            Ok(content) => {
                d.content = content;
                d.invalid.remove(&index);
                if d.invalid.is_empty() {
                    d.error = None;
                }
            }
            Err(e) => {
                d.invalid.insert(index);
                d.error = Some(e.to_string());
            }
        }
        cx.notify();
    }
    fn forge_easy_view(
        &mut self,
        id: &str,
        doc: std::sync::Arc<forge_config::Document>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let rel = self.files_ui.editors[id].selected.clone().unwrap();
        let draft = self.files_ui.editors.get_mut(id).unwrap();
        let state = draft
            .easy_list
            .get_or_insert_with(|| {
                gpui::ListState::new(doc.fields.len(), gpui::ListAlignment::Top, px(120.))
            })
            .clone();
        if draft.easy_list_file.as_ref() != Some(&rel) || state.item_count() != doc.fields.len() {
            state.reset(doc.fields.len());
            draft.easy_list_file = Some(rel.clone());
        }
        let target = id.to_owned();
        let body = gpui::list(
            state,
            cx.processor(move |v, index: usize, _, cx| {
                let id = target.as_str();
                let field = doc.fields[index].clone();
                let value = v.files_ui.editors[id]
                    .easy_values
                    .get(&index)
                    .cloned()
                    .unwrap_or(field.value);
                let mut row = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_shrink_0()
                    .flex_col()
                    .gap_1()
                    .child(format!("{} / {}", field.section, field.key));
                if !field.description.is_empty() {
                    row = row.child(
                        div()
                            .text_xs()
                            .text_color(v.theme.muted_foreground())
                            .child(field.description),
                    );
                }
                if field.kind == 'B' && !field.multiline {
                    let target = id.to_string();
                    let next = if value == "true" { "false" } else { "true" };
                    row = row.child(
                        btn(&v.theme, &value, ButtonVariant::Secondary)
                            .debug_selector(move || format!("forge-value:{index}").into())
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                if enabled {
                                    v.change_forge_value(&target, index, next.into(), cx);
                                }
                            })),
                    );
                } else {
                    let fid = format!("forge:{id}:{rel}:{index}");
                    if !v.native_fields.contains_key(&fid) {
                        let entity = cx.new(|cx| {
                            NativeField::new(
                                &value,
                                v.theme.clone(),
                                &format!("{} / {}", field.section, field.key),
                                field.multiline,
                                cx,
                            )
                        });
                        let target = id.to_string();
                        cx.subscribe(&entity, move |v, _, event, cx| {
                            if let NativeFieldEvent::Change(text) = event {
                                v.change_forge_value(&target, index, text.clone(), cx);
                            }
                        })
                        .detach();
                        v.native_fields.insert(fid.clone(), entity);
                    }
                    let entity = v.native_fields[&fid].clone();
                    entity.update(cx, |state, _| {
                        state.value = value;
                        state.theme = v.theme.clone();
                        state.enabled = enabled;
                    });
                    row = row.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .debug_selector(move || format!("forge-value:{index}").into())
                            .child(entity),
                    );
                }
                row.pb_3().into_any_element()
            }),
        )
        .flex_1()
        .min_h_0();
        div()
            .debug_selector(|| "forge-easy".into())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(body)
    }
    pub(super) fn protect_file_drafts(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(id) = self.files_ui.unsaved_instance() {
            self.selected_id = Some(instance_id(&id).to_string());
            if id.contains('\0') {
                self.focus_file_window(&id, cx);
            }
            self.tab = super::Tab::Instances;
            self.instance_ui.detail_tab = DetailTab::Files;
            self.data.status = "Save or reload the file draft before closing.".into();
            cx.notify();
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    async fn pointer_popout_survives_bubbling_mouse_up_after_draft_moves(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = super::super::test_support::Fixture::new(&["pack"]);
        std::fs::write(
            fixture
                .paths
                .instance_dir("pack")
                .unwrap()
                .join(".minecraft/config/a.cfg"),
            "general {\n B:enabled=true\n}\n",
        )
        .unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Files;
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        super::super::test_support::Fixture::click(cx, window, "minecraft-file:config");
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        super::super::test_support::Fixture::click(cx, window, "minecraft-file:config/a.cfg");
        fixture
            .until(cx, &root, |v| !v.files_ui.editors["pack"].loading)
            .await;
        super::super::test_support::Fixture::click(cx, window, "minecraft-popout");
        cx.run_until_parked();
        let child = root.read_with(cx, |v, _| v.files_ui.windows["pack\0config/a.cfg"]);
        cx.update_window(child.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        assert_eq!(cx.windows().len(), 2);
        root.update(cx, |v, cx| {
            v.close_editor_window("pack\0config/a.cfg".into(), cx)
        });
        cx.run_until_parked();
        fixture
            .until(cx, &root, |v| {
                v.files_ui
                    .editors
                    .values()
                    .all(|d| d.explorer.loading.is_empty())
            })
            .await;
        assert_eq!(cx.windows().len(), 1);
        fixture.cleanup();
    }
    #[gpui::test]
    async fn explorer_expands_nested_folders_without_saving_or_losing_the_open_draft(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = super::super::test_support::Fixture::new(&["pack", "other"]);
        let game = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft");
        std::fs::create_dir_all(game.join("config/nested")).unwrap();
        std::fs::create_dir_all(game.join("scripts")).unwrap();
        let original = "general {\n B:enabled=true\n}\n";
        std::fs::write(game.join("config/nested/tweaks.cfg"), original).unwrap();
        std::fs::write(game.join("scripts/example.zs"), "print(1);").unwrap();
        let (window, root) = fixture.open_sized(cx, gpui::size(px(1600.), px(900.)), |v| {
            v.selected_id = Some("pack".into());
            v.instance_ui.detail_tab = DetailTab::Files;
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        for path in ["config", "config/nested"] {
            super::super::test_support::Fixture::click(
                cx,
                window,
                if path == "config" {
                    "minecraft-file:config"
                } else {
                    "minecraft-file:config/nested"
                },
            );
            fixture
                .until(cx, &root, |v| {
                    v.files_ui.editors["pack"].explorer.loading.is_empty()
                })
                .await;
        }
        super::super::test_support::Fixture::click(
            cx,
            window,
            "minecraft-file:config/nested/tweaks.cfg",
        );
        fixture
            .until(cx, &root, |v| !v.files_ui.editors["pack"].loading)
            .await;
        root.update(cx, |v, _| {
            v.files_ui.editors.get_mut("pack").unwrap().content = "unsaved draft".into()
        });
        super::super::test_support::Fixture::click(cx, window, "minecraft-file:scripts");
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        super::super::test_support::Fixture::click(cx, window, "minecraft-file:config");
        assert!(root.read_with(cx, |v, _| {
            let draft = &v.files_ui.editors["pack"];
            draft.selected.as_deref() == Some("config/nested/tweaks.cfg")
                && draft.content == "unsaved draft"
                && draft.dirty()
        }));
        assert_eq!(
            std::fs::read_to_string(game.join("config/nested/tweaks.cfg")).unwrap(),
            original
        );
        assert!(!fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join("persistent-minecraft/config/nested/tweaks.cfg")
            .exists());
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            visual.simulate_keystrokes("right");
            assert!(visual
                .debug_bounds("minecraft-file:config/nested/tweaks.cfg")
                .is_some());
        }
        super::super::test_support::Fixture::click(cx, window, "minecraft-tree-collapse");
        std::fs::write(game.join("new.json"), "{}").unwrap();
        super::super::test_support::Fixture::click(cx, window, "minecraft-tree-refresh");
        fixture
            .until(cx, &root, |v| {
                v.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        assert!(
            root.read_with(cx, |v, _| v.files_ui.editors["pack"].explorer.children[""]
                .iter()
                .any(|entry| entry.name == "new.json"))
        );
        root.update(cx, |v, cx| {
            v.selected_id = Some("other".into());
            cx.notify();
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |v| {
                v.files_ui
                    .editors
                    .get("other")
                    .is_some_and(|draft| draft.explorer.loading.is_empty())
            })
            .await;
        assert!(
            root.read_with(cx, |v, _| v.files_ui.editors["other"].selected.is_none()
                && v.files_ui.editors["pack"].content == "unsaved draft")
        );
        fixture.cleanup();
    }
    #[gpui::test]
    async fn wide_file_browser_hit_bounds_match_painted_rows(cx: &mut gpui::TestAppContext) {
        let fixture = super::super::test_support::Fixture::new(&["pack"]);
        std::fs::remove_dir(
            fixture
                .paths
                .instance_dir("pack")
                .unwrap()
                .join(".minecraft/config"),
        )
        .unwrap();
        let file = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config-editor.cfg");
        std::fs::write(file, "general {\n B:enabled=true\n}\n").unwrap();
        let (window, root) = fixture.open_sized(cx, gpui::size(px(1600.), px(900.)), |root| {
            root.selected_id = Some("pack".into());
            root.instance_ui.detail_tab = DetailTab::Files;
            root.data.settings.theme_preset = "sandstone".into();
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |root| {
                root.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            let list = visual.debug_bounds("minecraft-file-list").unwrap();
            let row = visual
                .debug_bounds("minecraft-file:config-editor.cfg")
                .unwrap();
            assert!(list.contains(&row.center()));
            assert!(row.size.width > px(100.) && row.size.height >= px(28.));
            visual.simulate_click(row.center(), Default::default());
        }
        fixture
            .until(cx, &root, |root| !root.files_ui.editors["pack"].loading)
            .await;
        assert_eq!(
            root.read_with(cx, |root, _| root.files_ui.editors["pack"].selected.clone()),
            Some("config-editor.cfg".into())
        );
        fixture.cleanup();
    }
    #[gpui::test]
    async fn last_config_file_is_reachable_and_easy_controls_save_in_a_bounded_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = super::super::test_support::Fixture::new(&["pack"]);
        let config = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config");
        for index in 0..120 {
            std::fs::write(
                config.join(format!("setting-{index:03}.cfg")),
                "# Keep comment\ngeneral {\n B:enabled=true\n I:count=42\n}\n",
            )
            .unwrap();
        }
        let (window, root) = fixture.open(cx, |root| {
            root.selected_id = Some("pack".into());
            root.instance_ui.detail_tab = DetailTab::Files;
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        fixture
            .until(cx, &root, |root| {
                root.files_ui
                    .editors
                    .get("pack")
                    .is_some_and(|draft| draft.explorer.loading.is_empty())
            })
            .await;
        super::super::test_support::Fixture::click(cx, window, "minecraft-file:config");
        fixture
            .until(cx, &root, |root| {
                root.files_ui.editors["pack"]
                    .explorer
                    .children
                    .get("config")
                    .is_some_and(|entries| entries.len() == 120)
                    && root.files_ui.editors["pack"].explorer.loading.is_empty()
            })
            .await;
        root.update(cx, |root, cx| {
            root.files_ui.editors["pack"]
                .explorer
                .scroll
                .scroll_to_item(120, gpui::ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
            let list = visual.debug_bounds("minecraft-file-list").unwrap();
            let last = visual
                .debug_bounds("minecraft-file:config/setting-119.cfg")
                .unwrap();
            assert!(
                list.contains(&last.center()),
                "last file must scroll into the actual browser viewport"
            );
            assert!(list.size.height > px(100.));
            visual.simulate_click(last.center(), Default::default());
        }
        fixture
            .until(cx, &root, |root| !root.files_ui.editors["pack"].loading)
            .await;
        assert_eq!(
            root.read_with(cx, |root, _| root.files_ui.editors["pack"].selected.clone()),
            Some("config/setting-119.cfg".into())
        );
        super::super::test_support::Fixture::click(cx, window, "minecraft-mode");
        super::super::test_support::Fixture::click(cx, window, "forge-value:0");
        super::super::test_support::Fixture::click(cx, window, "minecraft-save");
        fixture
            .until(cx, &root, |root| !root.files_ui.editors["pack"].saving)
            .await;
        let saved = std::fs::read_to_string(config.join("setting-119.cfg")).unwrap();
        assert!(saved.contains("B:enabled=false") && saved.starts_with("# Keep comment"));
        assert_eq!(
            saved,
            std::fs::read_to_string(
                fixture
                    .paths
                    .instance_dir("pack")
                    .unwrap()
                    .join("persistent-minecraft/config/setting-119.cfg")
            )
            .unwrap()
        );
        fixture.cleanup();
    }
    use crate::state::AppData;
    use gpui::{size, ClipboardItem, TestAppContext, VisualTestContext};
    use industrialis_backend::{instances, paths::LauncherPaths};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    #[gpui::test]
    async fn pointer_editor_save_conflict_and_popout_keep_the_same_draft(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let paths = LauncherPaths::new(std::env::temp_dir().join(format!(
                "industrialis-file-ui-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        let instance = paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(instance.join(".minecraft/config")).unwrap();
        std::fs::write(instance.join("mmc-pack.json"), "{}").unwrap();
        let source="# Keep this comment\ngeneral {\n    B:enabled=true\n    I:number=5\n    S:list <\n        A\n     >\n}\n";
        let file = instance.join(".minecraft/config/a.cfg");
        std::fs::write(&file, source).unwrap();
        let data = AppData {
            paths: paths.clone(),
            rt: rt.clone(),
            instances: rt.block_on(instances::load_instances(&paths)).unwrap(),
            accounts: Vec::new(),
            settings: Default::default(),
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(1100.), px(850.)), |_, _| {
            let mut root = LauncherRoot::new(data);
            root.selected_id = Some("pack".into());
            root.instance_ui.detail_tab = DetailTab::Files;
            root
        });
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        async fn settle(
            cx: &mut TestAppContext,
            root: &gpui::Entity<LauncherRoot>,
            rt: &Arc<tokio::runtime::Runtime>,
        ) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if !root.read_with(cx, |v, _| {
                    v.files_ui
                        .editors
                        .values()
                        .any(|d| d.loading || d.saving || !d.explorer.loading.is_empty())
                }) {
                    break;
                }
                assert!(Instant::now() < deadline);
                rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                    .await
                    .unwrap();
            }
        }
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual.debug_bounds("minecraft-file:config").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual.debug_bounds("minecraft-file:config/a.cfg").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let mode = visual.debug_bounds("minecraft-mode").unwrap();
            visual.simulate_click(mode.center(), Default::default());
            let value = visual.debug_bounds("forge-value:0").unwrap();
            visual.simulate_click(value.center(), Default::default());
            assert!(root.read_with(&visual, |v, _| v.files_ui.editors["pack"]
                .content
                .contains("B:enabled=false")));
            for (index, selector, value) in [
                (1, "forge-value:1", "42"),
                (2, "forge-value:2", "B\nC value"),
            ] {
                visual.update(|window, cx| {
                    root.update(cx, |v, cx| {
                        v.files_ui.editors["pack"]
                            .easy_list
                            .as_ref()
                            .unwrap()
                            .scroll_to_reveal_item(index);
                        cx.notify();
                    });
                    window.draw(cx).clear(cx);
                });
                let bounds = visual.debug_bounds(selector).unwrap();
                assert!(
                    visual
                        .debug_bounds("forge-easy")
                        .unwrap()
                        .contains(&bounds.center()),
                    "Input must be visible before pointer editing"
                );
                visual.simulate_click(bounds.center(), Default::default());
                visual.simulate_keystrokes("ctrl-a");
                visual
                    .update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(value.into())));
                visual.simulate_keystrokes("ctrl-v");
            }
            let save = visual.debug_bounds("minecraft-save").unwrap();
            let frame = visual.debug_bounds("minecraft-editor:pack").unwrap();
            assert!(
                frame.contains(&save.center()) && save.bottom() <= frame.bottom(),
                "Save must stay inside the editor, above the launcher action bar"
            );
            visual.simulate_click(save.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# Keep this comment\ngeneral {\n    B:enabled=false\n    I:number=42\n    S:list <\n        B\n        C value\n     >\n}\n"
        );
        assert_eq!(
            std::fs::read_to_string(instance.join("persistent-minecraft/config/a.cfg")).unwrap(),
            std::fs::read_to_string(&file).unwrap()
        );
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let mode = visual.debug_bounds("minecraft-mode").unwrap();
            visual.simulate_click(mode.center(), Default::default());
            let text = visual.debug_bounds("minecraft-content").unwrap();
            visual.simulate_click(text.center(), Default::default());
            visual.simulate_keystrokes("ctrl-f");
            visual.update(|window, cx| {
                assert!(root
                    .read(cx)
                    .native_fields
                    .values()
                    .any(|field| field.read(cx).find_input_focused(window, cx)));
            });
            visual.simulate_keystrokes("escape");
            visual.update(|window, cx| {
                assert!(!root
                    .read(cx)
                    .native_fields
                    .values()
                    .any(|field| field.read(cx).find_input_focused(window, cx)));
            });
            visual.simulate_keystrokes("ctrl-a");
            visual.update(|_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(source.replacen("5", "9", 1)))
            });
            visual.simulate_keystrokes("ctrl-v");
            let save = visual.debug_bounds("minecraft-save").unwrap();
            visual.simulate_click(save.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        let raw_saved = source.replacen("5", "9", 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), raw_saved);
        assert_eq!(
            std::fs::read_to_string(instance.join("persistent-minecraft/config/a.cfg")).unwrap(),
            raw_saved
        );
        // Reload from disk after dropping the editor's in-memory draft.
        root.update(cx, |v, cx| {
            v.files_ui.editors.remove("pack");
            v.files_ui.editors.entry("pack".into()).or_default();
            v.open_file_saved(
                "pack".into(),
                MinecraftEntry {
                    name: "a.cfg".into(),
                    rel_path: "config/a.cfg".into(),
                    is_dir: false,
                    has_persistent_override: true,
                    editable: true,
                    too_large_to_edit: false,
                },
                cx,
            );
        });
        settle(cx, &root, &rt).await;
        assert_eq!(
            root.read_with(cx, |v, _| v.files_ui.editors["pack"].saved.clone()),
            raw_saved
        );
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let text = visual.debug_bounds("minecraft-content").unwrap();
            visual.simulate_click(text.center(), Default::default());
            visual.simulate_keystrokes("ctrl-a");
            visual.update(|_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string("my draft".into()))
            });
            visual.simulate_keystrokes("ctrl-v");
        }
        std::fs::write(&file, "external change").unwrap();
        root.update(cx, |v, cx| v.save_file_then("pack".into(), |_, _| {}, cx));
        settle(cx, &root, &rt).await;
        root.read_with(cx, |v, _| {
            let draft = &v.files_ui.editors["pack"];
            assert_eq!(draft.content, "my draft");
            assert!(draft.error.as_ref().unwrap().contains("changed outside"));
            assert!(draft.dirty());
        });
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "external change");
        root.update(cx, |v, cx| {
            assert!(!v.close_allowed(cx));
            let d = v.files_ui.editors.get_mut("pack").unwrap();
            d.content = d.saved.clone();
            d.error = None;
            v.popout_file_editor("pack".into(), cx);
        });
        cx.run_until_parked();
        std::fs::write(&file, raw_saved).unwrap();
        let window = root.read_with(cx, |v, _| v.files_ui.windows["pack\0config/a.cfg"]);
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        root.update(cx, |v, cx| {
            v.popout_file_editor("pack\0config/a.cfg".into(), cx)
        });
        assert_eq!(cx.windows().len(), 2);
        // The main browser remains usable and a different file gets its own
        // independent window, while repeated opens reuse the matching one.
        std::fs::write(instance.join(".minecraft/config/b.cfg"), "second file").unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual.debug_bounds("minecraft-file:config").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual.debug_bounds("minecraft-file:config/b.cfg").unwrap();
            visual.simulate_click(bounds.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        assert_eq!(
            root.read_with(cx, |v, _| v.files_ui.editors["pack"].saved.clone()),
            "second file"
        );
        root.update(cx, |v, cx| {
            v.popout_file_editor("pack".into(), cx);
            v.popout_file_editor("pack".into(), cx);
        });
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 3);
        root.update(cx, |v, cx| {
            v.files_ui
                .editors
                .get_mut("pack\0config/b.cfg")
                .unwrap()
                .content = "second edit".into();
            v.files_ui
                .editors
                .get_mut("pack\0config/a.cfg")
                .unwrap()
                .content = "first edit".into();
            v.close_editor_window("pack\0config/b.cfg".into(), cx);
        });
        settle(cx, &root, &rt).await;
        cx.run_until_parked();
        root.read_with(cx, |v, _| {
            if let Some(d) = v.files_ui.editors.get("pack\0config/b.cfg") {
                panic!("Second editor did not close: selected={:?}, content={:?}, saved={:?}, loading={}, saving={}, error={:?}", d.selected, d.content, d.saved, d.loading, d.saving, d.error);
            }
        });
        assert_eq!(
            std::fs::read_to_string(instance.join(".minecraft/config/b.cfg")).unwrap(),
            "second edit"
        );
        assert!(root.read_with(cx, |v, _| v.files_ui.dirty("pack")));
        // Launch must save the detached file's key, then reach the existing
        // account guard without recursively trying to save the main browser.
        root.update(cx, |v, cx| v.play("pack".into(), cx));
        settle(cx, &root, &rt).await;
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "first edit");
        assert!(!root.read_with(cx, |v, _| v.files_ui.dirty("pack")));
        root.update(cx, |v, cx| {
            v.close_editor_window("pack\0config/a.cfg".into(), cx)
        });
        settle(cx, &root, &rt).await;
        cx.run_until_parked();
        assert!(window.root(cx).is_err());
        assert_eq!(cx.windows().len(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "first edit");
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}

#[cfg(test)]
#[path = "explorer_search_tests.rs"]
mod explorer_search_tests;

#[cfg(test)]
#[path = "easy_performance_tests.rs"]
mod easy_performance_tests;
