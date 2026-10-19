//! An instance editor window shares the root draft instead of copying buffers.
use super::LauncherRoot;
use gpui::{
    div, prelude::*, px, size, Bounds, Context, Render, TitlebarOptions, WeakEntity, Window,
    WindowBounds, WindowOptions,
};
pub(super) struct EditorWindow {
    root: WeakEntity<LauncherRoot>,
    id: String,
}
impl EditorWindow {
    fn new(root: WeakEntity<LauncherRoot>, id: String, cx: &mut Context<Self>) -> Self {
        if let Some(entity) = root.upgrade() {
            cx.observe(&entity, |_, _, cx| cx.notify()).detach();
        }
        Self { root, id }
    }
}
impl Render for EditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(root) = self.root.upgrade() {
            let keyboard_root = root.downgrade();
            root.update(cx, |v, cx| {
                let t = v.theme.clone();
                div()
                    .capture_key_down(move |event, window, cx| {
                        let code_focus = keyboard_root.upgrade().is_some_and(|root| {
                            root.read(cx)
                                .native_fields
                                .values()
                                .any(|field| field.read(cx).code_input_focused())
                        });
                        if event.keystroke.key == "tab"
                            && code_focus
                            && !event.keystroke.modifiers.control
                        {
                            return;
                        }
                        if event.keystroke.key == "tab"
                            && (!event.keystroke.modifiers.control || code_focus)
                            && !event.keystroke.modifiers.alt
                            && !event.keystroke.modifiers.platform
                        {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev(cx);
                            } else {
                                window.focus_next(cx);
                            }
                            cx.stop_propagation();
                        }
                    })
                    .size_full()
                    .bg(t.background())
                    .text_color(t.foreground())
                    .flex()
                    .flex_col()
                    .child(v.files_view(&self.id, f32::from(window.viewport_size().width), cx))
            })
        } else {
            div().child("The launcher has closed")
        }
    }
}
impl LauncherRoot {
    pub(super) fn focus_file_window(&self, key: &str, cx: &mut Context<Self>) {
        if let Some(handle) = self.files_ui.windows.get(key).copied() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
        }
    }
    pub(super) fn popout_file_editor(&mut self, id: String, cx: &mut Context<Self>) {
        if id.contains('\0') {
            self.focus_file_window(&id, cx);
            return;
        }
        let Some(rel) = self
            .files_ui
            .editors
            .get(&id)
            .and_then(|d| d.selected.clone())
        else {
            self.data.status = "Select a file to open in a separate window.".into();
            cx.notify();
            return;
        };
        let key = format!("{id}\0{rel}");
        if self.files_ui.windows.contains_key(&key) || self.files_ui.opening.contains(&key) {
            self.focus_file_window(&key, cx);
            return;
        }
        self.save_file_then(
            id.clone(),
            move |v, cx| {
                // Admission happens before the deferred native open, preventing double clicks.
                if !v.files_ui.opening.insert(key.clone()) {
                    return;
                }
                v.clear_file_inputs(&id);
                if let Some(draft) = v.files_ui.editors.remove(&id) {
                    v.files_ui.editors.insert(key.clone(), draft);
                }
                v.open_editor_window(key, cx);
            },
            cx,
        );
    }
    fn open_editor_window(&mut self, id: String, cx: &mut Context<Self>) {
        // An input engine belongs to its native window. Recreate its presentation
        // when moving the shared, already-saved draft between windows.
        self.clear_file_inputs(&id);
        let weak = cx.entity().downgrade();
        let target = id.clone();
        let bounds = Bounds::centered(None, size(px(900.), px(750.)), cx);
        let owner = cx.entity().downgrade();
        cx.defer(move |cx| {
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(id.replace('\0', " - ").into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(600.), px(500.))),
                    ..Default::default()
                },
                move |window, cx| {
                    let close_root = weak.clone();
                    let close_id = target.clone();
                    window.on_window_should_close(cx, move |_, cx| {
                        if let Some(root) = close_root.upgrade() {
                            root.update(cx, |v, cx| v.close_editor_window(close_id.clone(), cx));
                            false
                        } else {
                            true
                        }
                    });
                    cx.new(|cx| EditorWindow::new(weak, target, cx))
                },
            );

            if let Some(root) = owner.upgrade() {
                root.update(cx, |v, cx| {
                    v.files_ui.opening.remove(&id);
                    match result {
                        Ok(handle) => {
                            v.files_ui.windows.insert(id, handle);
                        }
                        Err(e) => {
                            if let Some(draft) = v.files_ui.editors.remove(&id) {
                                v.files_ui.editors.insert(
                                    super::minecraft_files::instance_id(&id).to_string(),
                                    draft,
                                );
                            }
                            v.data.status = format!("Could not open editor window: {e:#}");
                        }
                    }
                    cx.notify();
                });
            }
        });
        cx.notify();
    }
    pub(super) fn close_editor_window(&mut self, id: String, cx: &mut Context<Self>) {
        let target = id.clone();
        self.save_file_then(
            id,
            move |v, cx| {
                if let Some(handle) = v.files_ui.windows.remove(&target) {
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    });
                }
                v.clear_file_inputs(&target);
                v.files_ui.editors.remove(&target);
                cx.notify();
            },
            cx,
        );
    }
    fn clear_file_inputs(&mut self, id: &str) {
        self.native_fields.retain(|key, _| {
            !key.starts_with(&format!("minecraft:{id}:"))
                && !key.starts_with(&format!("forge:{id}:"))
        });
    }
}
