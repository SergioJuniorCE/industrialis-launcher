//! Shared native text editing for forms and multiline editors.
use crate::theme::ActiveTheme;
use gpui::{div, prelude::*, px, AccessibleAction, EntityInputHandler, Focusable, IntoElement};

fn code_font_family() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "monospace"
    }
}

#[derive(Debug, Clone)]
pub struct FieldState {
    pub text: String,
}
impl FieldState {
    pub fn new(text: &str) -> Self {
        Self { text: text.into() }
    }
    pub fn empty() -> Self {
        Self::new("")
    }
}
/// Presentation and form events over the pinned GPUI native editing engine.
/// Initialization is deferred until rendering supplies the owning Window.
pub enum NativeFieldEvent {
    Change(String),
    Submit,
    Focus,
    Blur,
}
pub struct NativeField {
    pub value: String,
    pub theme: ActiveTheme,
    pub placeholder: String,
    pub accessibility_label: String,
    pub enabled: bool,
    pub multiline: bool,
    pub height: f32,
    pub code_editor: bool,
    pub code_language: String,
    pub fill_height: bool,
    pub soft_wrap: bool,
    applied_wrap: Option<bool>,
    applied_syntax: Option<(String, bool, String, String, Option<serde_json::Value>)>,
    focused: bool,
    engine: Option<Engine>,
    find_field: Option<gpui::Entity<NativeField>>,
    search_revision: u64,
    focus_on_render: bool,
    observed_search: Option<(bool, u64, String, usize)>,
    _subscriptions: Vec<gpui::Subscription>,
}
#[derive(Clone)]
enum Engine {
    Single(gpui::Entity<gpui_base::input::InputState>),
    Multi(gpui::Entity<gpui_base::input::TextareaState>),
    Code(gpui::Entity<gpui_base::input::EditorState>),
}
impl Engine {
    fn focus_handle(&self, cx: &gpui::App) -> gpui::FocusHandle {
        match self {
            Self::Single(entity) => entity.focus_handle(cx),
            Self::Multi(entity) => entity.focus_handle(cx),
            Self::Code(entity) => entity.focus_handle(cx),
        }
    }
    fn focus(&self, window: &mut gpui::Window, cx: &mut gpui::App) {
        match self {
            Self::Single(entity) => entity.update(cx, |state, cx| state.focus(window, cx)),
            Self::Multi(entity) => entity.update(cx, |state, cx| state.focus(window, cx)),
            Self::Code(entity) => entity.update(cx, |state, cx| state.focus(window, cx)),
        }
    }
    fn replace_from_accessibility(
        &self,
        value: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        // Treat assistive-technology replacement as editing, preserving change
        // events and undo. set_value is for model synchronization and suppresses
        // those events. Native input ranges are UTF-16, not bytes.
        match self {
            Self::Single(entity) => entity.update(cx, |state, cx| {
                let end = state.value().encode_utf16().count();
                state.replace_text_in_range(Some(0..end), value, window, cx);
            }),
            Self::Multi(entity) => entity.update(cx, |state, cx| {
                let end = state.value().encode_utf16().count();
                state.replace_text_in_range(Some(0..end), value, window, cx);
            }),
            Self::Code(entity) => entity.update(cx, |state, cx| {
                let end = state.value().encode_utf16().count();
                state.replace_text_in_range(Some(0..end), value, window, cx);
            }),
        }
    }
}
struct InputInitialized;
impl gpui::Global for InputInitialized {}
impl gpui::EventEmitter<NativeFieldEvent> for NativeField {}
impl NativeField {
    pub fn code_input_focused(&self) -> bool {
        self.code_editor && self.focused
    }
    pub fn find_input_focused(&self, window: &gpui::Window, cx: &gpui::App) -> bool {
        self.find_field.as_ref().is_some_and(|find| {
            find.read(cx)
                .engine
                .as_ref()
                .is_some_and(|engine| engine.focus_handle(cx).is_focused(window))
        })
    }
    pub fn new(
        value: &str,
        theme: ActiveTheme,
        placeholder: &str,
        multiline: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        if !cx.has_global::<InputInitialized>() {
            gpui_base::init(cx);
            cx.set_global(InputInitialized);
        }
        Self {
            value: value.into(),
            theme,
            placeholder: placeholder.into(),
            accessibility_label: if placeholder.is_empty() {
                "Text".into()
            } else {
                placeholder.into()
            },
            enabled: true,
            multiline,
            height: if multiline { 160. } else { 30. },
            code_editor: false,
            code_language: String::new(),
            fill_height: false,
            soft_wrap: true,
            applied_wrap: None,
            applied_syntax: None,
            focused: false,
            engine: None,
            find_field: None,
            search_revision: 0,
            focus_on_render: false,
            observed_search: None,
            _subscriptions: Vec::new(),
        }
    }
    fn event(
        &mut self,
        event: &gpui_base::input::InputEvent,
        value: String,
        cx: &mut gpui::Context<Self>,
    ) {
        use gpui_base::input::InputEvent;
        match event {
            InputEvent::Change => {
                self.value = value.clone();
                cx.emit(NativeFieldEvent::Change(value));
            }
            InputEvent::PressEnter { .. } if !self.multiline => cx.emit(NativeFieldEvent::Submit),
            InputEvent::Focus => {
                self.focused = true;
                cx.emit(NativeFieldEvent::Focus);
            }
            InputEvent::Blur => {
                self.focused = false;
                cx.emit(NativeFieldEvent::Blur);
            }
            _ => {}
        }
        cx.notify();
    }

    fn search_bar(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let Some(Engine::Code(editor)) = self.engine.clone() else {
            return None;
        };
        let state = editor.read(cx);
        if !state.search_session().open {
            return None;
        }
        let query = state.search_session().query.clone();
        let revision = state.search_activation_revision();
        let count = state.search_session().matcher.matched_ranges().len();
        if self.find_field.is_none() {
            let field = cx
                .new(|cx| NativeField::new(&query, self.theme.clone(), "Find in file", false, cx));
            let target = editor.clone();
            self._subscriptions
                .push(cx.subscribe(&field, move |_, _, event, cx| {
                    match event {
                        NativeFieldEvent::Change(query) => target.update(cx, |state, cx| {
                            state.set_search_query(query.clone(), true, cx)
                        }),
                        NativeFieldEvent::Submit => {
                            target.update(cx, |state, cx| {
                                state.next_search_match(cx);
                            });
                        }
                        _ => {}
                    }
                    cx.notify();
                }));
            self.find_field = Some(field);
        }
        let field = self.find_field.as_ref().unwrap().clone();
        let refocus = revision != self.search_revision;
        self.search_revision = revision;
        field.update(cx, |field, _| {
            field.value = query;
            field.theme = self.theme.clone();
            field.focus_on_render |= refocus;
        });
        let mut bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .child(div().flex_1().min_w_0().child(field))
            .child(div().text_xs().child(format!(
                "{count} {}",
                if count == 1 { "match" } else { "matches" }
            )));
        for (label, direction) in [("Prev", -1), ("Next", 1), ("Close", 0)] {
            let editor = editor.clone();
            bar = bar.child(
                crate::ui::btn(&self.theme, label, crate::ui::ButtonVariant::Ghost).on_click(
                    cx.listener(move |_, _: &gpui::ClickEvent, window, cx| {
                        editor.update(cx, |state, cx| match direction {
                            -1 => {
                                state.previous_search_match(cx);
                            }
                            1 => {
                                state.next_search_match(cx);
                            }
                            _ => {
                                state.close_search(cx);
                                state.focus(window, cx);
                            }
                        });
                        cx.notify();
                    }),
                ),
            );
        }
        let _ = window;
        Some(bar)
    }
}
impl gpui::Render for NativeField {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui_base::input::{InputEditorStyle, InputState, TextareaState};
        if self.engine.is_none() {
            if self.code_editor {
                let state = cx.new(|cx| {
                    gpui_base::input::EditorState::new(window, cx)
                        .language(self.code_language.clone())
                        .default_value(self.value.clone())
                        .line_number(true)
                        .searchable(true)
                        .soft_wrap(self.soft_wrap)
                });
                self._subscriptions
                    .push(cx.subscribe(&state, |this, entity, event, cx| {
                        this.event(event, entity.read(cx).value().to_string(), cx)
                    }));
                self._subscriptions
                    .push(cx.observe(&state, |field, editor, cx| {
                        let state = editor.read(cx);
                        let session = state.search_session();
                        let search = (
                            session.open,
                            state.search_activation_revision(),
                            session.query.clone(),
                            session.matcher.matched_ranges().len(),
                        );
                        if field.observed_search.as_ref() != Some(&search) {
                            field.observed_search = Some(search);
                            cx.notify();
                        }
                    }));
                self.applied_wrap = Some(self.soft_wrap);
                self.engine = Some(Engine::Code(state));
            } else if self.multiline {
                let state = cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .default_value(self.value.clone())
                        .placeholder(self.placeholder.clone())
                        .rows(6)
                });
                self._subscriptions
                    .push(cx.subscribe(&state, |this, entity, event, cx| {
                        this.event(event, entity.read(cx).value().to_string(), cx);
                    }));
                self.engine = Some(Engine::Multi(state));
            } else {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(self.value.clone())
                        .placeholder(self.placeholder.clone())
                });
                self._subscriptions
                    .push(cx.subscribe(&state, |this, entity, event, cx| {
                        this.event(event, entity.read(cx).value().to_string(), cx);
                    }));
                self.engine = Some(Engine::Single(state));
            }
        }
        let mut selection: gpui::Hsla = self.theme.foreground().into();
        selection.a = 0.25;
        let style = InputEditorStyle {
            foreground: self.theme.foreground().into(),
            muted_foreground: self.theme.muted_foreground().into(),
            background: if self.code_editor {
                self.theme.code_background().into()
            } else {
                self.theme.secondary().into()
            },
            border: self.theme.border().into(),
            caret: self.theme.foreground().into(),
            selection,
            editor_gutter_background: Some(self.theme.card().into()),
            editor_active_line: Some(self.theme.code_active_background().into()),
            editor_invisible: Some(self.theme.muted_foreground().into()),
            ..Default::default()
        };
        let child = match self.engine.as_ref().unwrap() {
            Engine::Single(entity) => {
                entity.update(cx, |state, cx| {
                    if state.value().as_ref() != self.value {
                        state.set_value(self.value.clone(), window, cx);
                    }
                    state.set_disabled(!self.enabled, cx);
                    state.set_editor_style(style);
                });
                entity.clone().into_any_element()
            }
            Engine::Code(entity) => {
                let syntax = (
                    self.code_language.clone(),
                    self.theme.dark,
                    self.theme.tokens.background.clone(),
                    self.theme.tokens.foreground.clone(),
                    self.theme.editor_theme.clone(),
                );
                let syntax_changed = self.applied_syntax.as_ref() != Some(&syntax);
                self.applied_syntax = Some(syntax);
                let wrap_changed = self.applied_wrap != Some(self.soft_wrap);
                self.applied_wrap = Some(self.soft_wrap);
                entity.update(cx, |state, cx| {
                    if syntax_changed {
                        state.set_highlighter_factory(
                            crate::code_highlighting::factory_for_theme(&self.theme),
                            cx,
                        );
                        state.set_highlighter(self.code_language.clone(), cx);
                    }
                    if state.value().as_ref() != self.value {
                        state.set_value(self.value.clone(), window, cx);
                    }
                    if wrap_changed {
                        state.set_soft_wrap(self.soft_wrap, window, cx);
                    }
                    state.set_disabled(!self.enabled, cx);
                    state.set_editor_style(style);
                });
                entity.clone().into_any_element()
            }
            Engine::Multi(entity) => {
                entity.update(cx, |state, cx| {
                    if state.value().as_ref() != self.value {
                        state.set_value(self.value.clone(), window, cx);
                    }
                    state.set_disabled(!self.enabled, cx);
                    state.set_editor_style(style);
                });
                entity.clone().into_any_element()
            }
        };
        let focus_engine = self.engine.as_ref().unwrap().clone();
        if self.focus_on_render {
            self.focus_on_render = false;
            focus_engine.focus(window, cx);
        }
        let search = self.search_bar(window, cx);
        let engine_focus = focus_engine.focus_handle(cx);
        let value_engine = focus_engine.clone();
        let enabled = self.enabled;
        div()
            .id(("native-field", cx.entity_id()))
            .role(if self.multiline || self.code_editor {
                gpui::accesskit::Role::MultilineTextInput
            } else {
                gpui::accesskit::Role::TextInput
            })
            .aria_label(self.accessibility_label.clone())
            .aria_placeholder(self.placeholder.clone())
            // Share the engine's focus identity so keyboard traversal reports
            // this named node. Only the engine contributes a keyboard tab stop.
            .track_focus(&engine_focus)
            .tab_stop(false)
            .when(window.is_a11y_active(), |frame| {
                frame.aria_value(self.value.clone())
            })
            .on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
                if enabled {
                    focus_engine.focus(window, cx);
                }
            })
            .when(enabled, |frame| {
                frame.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                    if let Some(gpui::accesskit::ActionData::Value(value)) = data {
                        value_engine.replace_from_accessibility(value, window, cx);
                    }
                })
            })
            .w_full()
            .min_w_0()
            .when(self.fill_height, |frame| frame.flex_1().h_full().min_h_0())
            .when(!self.fill_height, |frame| {
                frame.h(px(self.height)).flex_shrink_0()
            })
            .px_2()
            .py_1()
            .rounded(px(6.))
            .border_1()
            .border_color(if self.focused {
                self.theme.ring()
            } else {
                self.theme.border()
            })
            .bg(if self.code_editor {
                self.theme.code_background()
            } else {
                self.theme.secondary()
            })
            .when(self.code_editor, |frame| {
                frame.font_family(code_font_family())
            })
            .text_color(self.theme.foreground())
            .text_sm()
            .flex()
            .when(self.code_editor, |frame| frame.flex_col())
            .capture_key_down(
                cx.listener(|field, event: &gpui::KeyDownEvent, window, cx| {
                    if let Some(Engine::Code(editor)) = field.engine.clone() {
                        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                            && !event.keystroke.modifiers.alt
                            && event.keystroke.key == "f"
                        {
                            editor.update(cx, |state, cx| state.open_search(false, cx));
                            cx.stop_propagation();
                        } else if event.keystroke.key == "escape"
                            && editor.read(cx).search_session().open
                        {
                            editor.update(cx, |state, cx| {
                                state.close_search(cx);
                                state.focus(window, cx);
                            });
                            cx.stop_propagation();
                        }
                    }
                }),
            )
            .when_some(search, |frame, search| frame.child(search))
            .child(div().flex_1().min_h_0().min_w_0().flex().child(child))
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[gpui::test]
    fn code_find_focuses_a_real_input_and_never_edits_the_document(cx: &mut TestAppContext) {
        let source = "general {\n I:count=1\n S:label=count\n}\n";
        let handle = cx.open_window(size(px(500.), px(300.)), |_, cx| {
            let mut field = NativeField::new(
                source,
                ActiveTheme::from_settings("dark", "sandstone", &[]),
                "File contents",
                true,
                cx,
            );
            field.code_editor = true;
            field.code_language = "cfg".into();
            field.height = 280.;
            field
        });
        let field = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let engine = field.read_with(cx, |field, _| field.engine.as_ref().unwrap().clone());
        cx.update_window(handle.into(), |_, window, cx| engine.focus(window, cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_keystrokes("ctrl-f");
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let find = field
            .read_with(&visual, |field, _| field.find_field.clone())
            .expect("Ctrl+F must render the search input");
        visual.update(|window, cx| {
            let engine = find.read(cx).engine.as_ref().expect("rendered Find input");
            assert!(engine.focus_handle(cx).is_focused(window));
        });
        visual.simulate_keystrokes("ctrl-a");
        visual
            .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("count".into())));
        visual.simulate_keystrokes("ctrl-v");
        let Engine::Code(editor) = engine else {
            panic!("native code editor expected")
        };
        assert_eq!(
            editor.read_with(&visual, |editor, _| editor.search_session().query.clone()),
            "count"
        );
        assert_eq!(
            editor.read_with(&visual, |editor, _| editor
                .search_session()
                .matcher
                .matched_ranges()
                .len()),
            2
        );
        assert_eq!(
            field.read_with(&visual, |field, _| field.value.clone()),
            source
        );
        visual.simulate_keystrokes("escape");
        assert!(!editor.read_with(&visual, |editor, _| editor.search_session().open));
        assert_eq!(
            field.read_with(&visual, |field, _| field.value.clone()),
            source
        );
    }
    use gpui::{size, EntityInputHandler, Focusable, TestAppContext, VisualTestContext};
    #[gpui::test]
    fn accessible_replacement_emits_form_changes_and_preserves_undo_in_every_mode(
        cx: &mut TestAppContext,
    ) {
        for (multiline, code) in [(false, false), (true, false), (true, true)] {
            let old = if multiline {
                "A\u{1f600}B\noriginal"
            } else {
                "A\u{1f600}B"
            };
            let replacement = if multiline {
                "\u{4e2d}\nchanged"
            } else {
                "\u{4e2d}changed"
            };
            let theme = ActiveTheme::from_settings("dark", "monochrome", &[]);
            let handle = cx.open_window(size(px(400.), px(200.)), |_, cx| {
                let mut field = NativeField::new(old, theme, "Editor", multiline, cx);
                field.code_editor = code;
                field
            });
            let field = handle.root(cx).unwrap();
            cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            let engine = field.read_with(cx, |field, _| field.engine.as_ref().unwrap().clone());
            cx.update_window(handle.into(), |_, window, cx| {
                engine.focus(window, cx);
                engine.replace_from_accessibility(replacement, window, cx);
            })
            .unwrap();
            cx.run_until_parked();
            assert_eq!(
                field.read_with(cx, |field, _| field.value.clone()),
                replacement,
                "native SetValue must emit the same change event as editing"
            );
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_keystrokes("ctrl-z");
            assert_eq!(
                field.read_with(&visual, |field, _| field.value.clone()),
                old,
                "native SetValue must remain undoable"
            );
        }
    }
    #[gpui::test]
    fn changing_imported_syntax_themes_keeps_the_editor_and_undo_history(cx: &mut TestAppContext) {
        let source = "# comment\nI:count=1\n";
        let handle = cx.open_window(size(px(500.), px(300.)), |_, cx| {
            let mut field = NativeField::new(
                source,
                ActiveTheme::from_settings("dark", "monochrome", &[]),
                "File",
                true,
                cx,
            );
            field.code_editor = true;
            field.code_language = "cfg".into();
            field
        });
        let field = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let engine = field.read_with(cx, |field, _| field.engine.as_ref().unwrap().clone());
        let Engine::Code(original) = engine.clone() else {
            panic!("code engine")
        };
        cx.update_window(handle.into(), |_, window, cx| {
            engine.focus(window, cx);
            engine.replace_from_accessibility("# changed\nI:count=2\n", window, cx);
        })
        .unwrap();
        cx.run_until_parked();
        for color in ["#00ff00", "#ff00ff"] {
            field.update(cx,|field,cx| {
                field.theme.editor_theme = Some(serde_json::json!({"tokenColors":[{"scope":"comment","settings":{"foreground":color}}]}));
                cx.notify();
            });
            cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            field.read_with(cx, |field, _| {
                let Engine::Code(current) = field.engine.as_ref().unwrap() else {
                    panic!("code engine")
                };
                assert_eq!(current.entity_id(), original.entity_id());
                assert_eq!(field.value, "# changed\nI:count=2\n");
                assert_eq!(
                    field.applied_syntax.as_ref().unwrap().4.as_ref().unwrap()["tokenColors"][0]
                        ["settings"]["foreground"],
                    color
                );
            });
        }
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_keystrokes("ctrl-z");
        assert_eq!(
            field.read_with(&visual, |field, _| field.value.clone()),
            source
        );
    }
    #[gpui::test]
    fn native_selection_clipboard_undo_and_utf16_composition(cx: &mut TestAppContext) {
        let theme = ActiveTheme::from_settings("dark", "monochrome", &[]);
        let handle = cx.open_window(size(px(400.), px(100.)), |_, cx| {
            NativeField::new("A\u{1f600}B", theme, "Text", false, cx)
        });
        let field = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let engine = field.read_with(cx, |field, _| match field.engine.as_ref().unwrap() {
            Engine::Single(entity) => entity.clone(),
            _ => panic!(),
        });
        cx.update_window(handle.into(), |_, window, cx| {
            engine.update(cx, |state, cx| {
                window.focus(&state.focus_handle(cx), cx);
                state.replace_and_mark_text_in_range(
                    Some(1..3),
                    "\u{4e2d}",
                    Some(1..1),
                    window,
                    cx,
                );
                assert_eq!(state.marked_text_range(window, cx), Some(1..2));
                state.replace_text_in_range(None, "\u{6587}", window, cx);
                state.unmark_text(window, cx);
                assert_eq!(state.value().as_ref(), "A\u{6587}B");
            })
        })
        .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_keystrokes("ctrl-z");
        assert_eq!(
            engine.read_with(&visual, |s, _| s.value().to_string()),
            "A\u{1f600}B"
        );
        visual.simulate_keystrokes("ctrl-y ctrl-a ctrl-c");
        visual.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                "A\u{6587}B"
            )
        });
        visual.simulate_keystrokes("backspace ctrl-v");
        assert_eq!(
            engine.read_with(&visual, |s, _| s.value().to_string()),
            "A\u{6587}B"
        );
        assert_eq!(
            field.read_with(&visual, |s, _| s.value.clone()),
            "A\u{6587}B"
        );
    }
}
