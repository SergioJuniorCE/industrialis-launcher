//! InstanceWorkspace, library tiles, header, Info panel and bottom action bar.
use super::{instance_actions::MaintenanceKind, DetailTab, LauncherRoot};
use crate::state::format_bytes;
use crate::ui::{
    badge, btn, icon, icon_btn, mix, progress_bar, tab_btn, tint, BadgeKind, ButtonVariant,
};
use gpui::{
    div, img, prelude::*, px, relative, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};
use industrialis_backend::{instances::InstanceInfo, paths::LauncherPaths};
use industrialis_core::{
    background_processes::{Operation, Status},
    instance_settings::format_play_time,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};

#[derive(Clone)]
pub(super) enum MenuTarget {
    Library,
    Instance(String),
    Group(String),
    File(
        String,
        industrialis_backend::minecraft_files::MinecraftEntry,
    ),
}

pub(super) struct WorkspaceUi {
    pub(super) drag: super::library_drag::LibraryDrag,
    fraction: f32,
    resizing: bool,
    pub(super) collapsed: HashSet<String>,
    pub(super) order: HashMap<String, Vec<String>>,
    pub(super) ungrouped: String,
    pub(super) menu: Option<(MenuTarget, Point<Pixels>)>,
    path: PathBuf,
    write_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    write_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl WorkspaceUi {
    pub fn load(paths: &LauncherPaths) -> Self {
        let path = paths.base().join("gpui-workspace.json");
        let saved: Value = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let groups: Value = std::fs::read(paths.groups_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut order = HashMap::new();
        let mut collapsed = HashSet::new();
        if let Some(entries) = groups["groups"].as_object() {
            for (name, entry) in entries {
                order.insert(
                    name.clone(),
                    serde_json::from_value(entry["instances"].clone()).unwrap_or_default(),
                );
                if entry["hidden"] == true {
                    collapsed.insert(name.clone());
                }
            }
        }
        order.insert(
            String::new(),
            serde_json::from_value(groups["ungrouped"]["instances"].clone()).unwrap_or_default(),
        );
        if groups["ungrouped"]["hidden"] == true {
            collapsed.insert(String::new());
        }

        Self {
            drag: Default::default(),
            fraction: saved["fraction"]
                .as_f64()
                .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                .unwrap_or(0.54) as f32,
            resizing: false,
            collapsed,
            order,
            menu: None,
            ungrouped: groups["ungrouped"]["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Ungrouped")
                .into(),
            path,
            write_lock: Default::default(),
            write_generation: Default::default(),
        }
    }
}

pub(super) fn display_name(info: &InstanceInfo) -> String {
    if info.settings.name.is_empty() {
        format!("GTNH {}", pack_version(info))
    } else {
        info.settings.name.clone()
    }
}
fn imported_frozen(info: &InstanceInfo) -> bool {
    info.settings.imported
        && (info.settings.pack_version.is_empty() || !info.settings.imported_gtnh)
}
fn pack_version(info: &InstanceInfo) -> &str {
    if info.settings.pack_version.is_empty() {
        if info.settings.imported {
            "Imported"
        } else {
            &info.id
        }
    } else {
        &info.settings.pack_version
    }
}
fn library_width(width: f32, fraction: f32) -> f32 {
    let available = (width - 12.0).max(580.0);
    (available * fraction).clamp(260.0, (available * 0.68).min(available - 320.0).max(260.0))
}

async fn write_snapshot(
    path: PathBuf,
    value: Value,
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    request: u64,
) -> anyhow::Result<()> {
    let _guard = lock.lock().await;
    if generation.load(std::sync::atomic::Ordering::SeqCst) != request {
        return Ok(());
    }
    industrialis_backend::fs_utils::write_json(&path, &value).await
}

impl LauncherRoot {
    fn open_library_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.closing.is_some() || self.dialog.is_some() {
            return;
        }
        self.workspace_ui.menu = Some((MenuTarget::Library, position));
        self.add_menu = None;
        cx.notify();
    }
    fn request_instance_action(&mut self, id: String, action: u8, cx: &mut Context<Self>) {
        self.workspace_ui.menu = None;
        // Imported instances without an assigned GTNH version stay frozen:
        // no update checks, updates, or reinstalls.
        if matches!(action, 0 | 4 | 12)
            && self
                .data
                .selected_instance(Some(&id))
                .is_some_and(imported_frozen)
        {
            return;
        }
        if let Some(draft) = self
            .files_ui
            .dirty_key(&id)
            .filter(|_| matches!(action, 0 | 2 | 3 | 4))
        {
            let target = id.clone();
            self.save_file_then(
                draft,
                move |v, cx| v.request_instance_action(target, action, cx),
                cx,
            );
            return;
        }
        if action == 1 {
            self.open_instance_directory(id, cx);
        } else if action == 12 {
            self.manual_pack_check(id, cx);
        } else if action == 10 {
            self.open_icon_dialog(id, cx);
        } else if action == 7 {
            self.open_group_dialog(super::library_groups::GroupDialogKind::Assign(id), cx);
        } else if action == 8 || action == 9 {
            self.edit_groups(
                industrialis_backend::groups::GroupChange::Move {
                    id,
                    up: action == 8,
                },
                cx,
            );
        } else if action == 5 {
            self.select_instance(id, cx);
            self.instance_ui.detail_tab = DetailTab::Settings;
        } else if action == 6 {
            if self.data.is_running(&id) {
                self.kill(id, cx);
            } else {
                self.play(id, cx);
            }
        } else if !self.instance_is_busy(&id) {
            self.after_instance_settings_saved(
                id.clone(),
                move |v, cx| {
                    if !v.instance_is_busy(&id) {
                        match action {
                            0 => v.open_maintenance(id, MaintenanceKind::Update, cx),
                            2 => v.open_copy(id, cx),
                            4 => v.open_maintenance(id, MaintenanceKind::Reinstall, cx),
                            _ => v.open_delete(id, cx),
                        }
                    }
                },
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn workspace_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Deferred> {
        let (target, position) = self.workspace_ui.menu.as_ref()?;
        if self.tab != super::Tab::Instances && !matches!(target, MenuTarget::File(..)) {
            return None;
        }
        if matches!(target, MenuTarget::Library) {
            let disabled = self.groups_ui.busy || self.closing.is_some();
            let item = btn(&self.theme, "New group", ButtonVariant::Ghost)
                .id("library-new-group")
                .debug_selector(|| "library-new-group".into())
                .role(gpui::accesskit::Role::MenuItem)
                .tab_stop(!disabled)
                .w_full()
                .justify_start()
                .text_xs()
                .when(disabled, |button| button.opacity(0.5).cursor_default())
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    if !disabled {
                        v.open_group_dialog(super::library_groups::GroupDialogKind::Create, cx);
                    }
                }));
            let import_disabled = self.closing.is_some();
            let import = btn(&self.theme, "Import from Prism…", ButtonVariant::Ghost)
                .id("library-import-prism")
                .debug_selector(|| "library-import-prism".into())
                .role(gpui::accesskit::Role::MenuItem)
                .tab_stop(!import_disabled)
                .w_full()
                .justify_start()
                .text_xs()
                .gap_2()
                .when(import_disabled, |button| {
                    button.opacity(0.5).cursor_default()
                })
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    if !import_disabled {
                        v.open_prism_import(None, false, cx);
                    }
                }));
            return Some(self.library_menu_overlay(
                div().child(item).child(import),
                *position,
                92.,
                window,
                cx,
            ));
        }
        if let MenuTarget::Group(group) = target {
            return Some(self.library_menu_overlay(
                self.group_menu_items(group, cx),
                *position,
                80.,
                window,
                cx,
            ));
        }
        if let MenuTarget::File(id, entry) = target {
            return Some(self.library_menu_overlay(
                self.file_menu_items(id, entry, cx),
                *position,
                144.,
                window,
                cx,
            ));
        }
        let MenuTarget::Instance(id) = target else {
            return None;
        };
        let t = &self.theme;
        let busy = self.instance_is_busy(id);
        let frozen = self
            .data
            .selected_instance(Some(id))
            .is_some_and(imported_frozen);
        let menu = div().children(
            [
                ("folder-open", "Open folder", 1),
                ("sliders-horizontal", "Settings", 5),
                ("play", "Launch / Stop", 6),
                ("copy", "Copy instance…", 2),
                ("arrow-up-circle", "Update pack…", 0),
                ("refresh-cw", "Check for pack updates", 12),
                ("refresh-cw", "Clean reinstall…", 4),
                ("trash-2", "Delete instance", 3),
                ("image", "Change icon...", 10),
                ("folder-open", "Change group...", 7),
                ("chevron-up", "Move earlier", 8),
                ("chevron-down", "Move later", 9),
            ]
            .into_iter()
            .map(|(glyph, label, action)| {
                let id = id.clone();
                let disabled = (busy && !matches!(action, 1 | 5 | 6 | 7 | 8 | 9 | 12))
                    || (matches!(action, 7 | 8 | 9) && self.groups_ui.busy)
                    || (action == 12 && self.catalog_ui.loading)
                    || (frozen && matches!(action, 0 | 4 | 12));
                btn(t, "", ButtonVariant::Ghost)
                    .id(format!("instance-menu-action:{action}"))
                    .aria_label(label)
                    .debug_selector(move || format!("instance-menu-action:{action}").into())
                    .tab_stop(!disabled)
                    .w_full()
                    .justify_start()
                    .text_xs()
                    .px_2()
                    .gap_2()
                    .child(icon(glyph, 14.0))
                    .child(label)
                    .when(disabled, |el| el.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if !disabled {
                            v.request_instance_action(id.clone(), action, cx);
                        }
                    }))
            }),
        );
        Some(self.library_menu_overlay(menu, *position, 408., window, cx))
    }
    fn library_menu_overlay(
        &self,
        body: Div,
        position: Point<Pixels>,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Deferred {
        let menu = div()
            .id("library-context-menu")
            .role(gpui::accesskit::Role::Menu)
            .absolute()
            .left(px(f32::from(position.x)
                .min(f32::from(window.viewport_size().width) - 212.)
                .max(0.)))
            .top(px(f32::from(position.y)
                .min(f32::from(window.viewport_size().height) - height)
                .max(44.)))
            .w(px(208.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(self.theme.border())
            .bg(self.theme.popover())
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|v, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left {
                    v.workspace_ui.menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(body);
        gpui::deferred(div().absolute().inset_0().child(menu)).with_priority(20)
    }
    fn save_workspace(&self, cx: &mut Context<Self>) {
        let path = self.workspace_ui.path.clone();
        let value = json!({"fraction": self.workspace_ui.fraction});
        let rt = self.data.rt.clone();
        let lock = self.workspace_ui.write_lock.clone();
        let generation = self.workspace_ui.write_generation.clone();
        let request = generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let Ok(lease) = industrialis_backend::lifecycle::admit(&self.data.paths) else {
            return;
        };
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let _lease = lease;
                    write_snapshot(path, value, lock, generation, request).await
                })
                .await;
            if !matches!(result, Ok(Ok(()))) {
                let _ = view.update(cx, |v, cx| {
                    v.data.status = "Could not save workspace layout".into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn instance_avatar(&self, info: &InstanceInfo, size: f32) -> Div {
        let t = &self.theme;
        div()
            .size(px(size))
            .rounded(px(if size >= 56.0 { 12.0 } else { 8.0 }))
            .overflow_hidden()
            .bg(t.secondary())
            .border_1()
            .border_color(tint(t.primary(), 0.18))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(if size >= 56.0 { 16.0 } else { 14.0 }))
            .font_weight(FontWeight::SEMIBOLD)
            .child(if let Some(path) = &info.icon_path {
                img(PathBuf::from(path))
                    .size(px(size))
                    .object_fit(gpui::ObjectFit::Cover)
                    .into_any_element()
            } else {
                div()
                    .child(
                        display_name(info)
                            .chars()
                            .next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string(),
                    )
                    .into_any_element()
            })
    }

    pub(super) fn instance_card(
        &self,
        info: &InstanceInfo,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = &self.theme;
        let selected = self.selected_id.as_deref() == Some(&info.id);
        let id = info.id.clone();
        let menu_id = id.clone();
        let right_id = id.clone();
        let running = self.data.running.get(&id).is_some_and(|(_, alive)| *alive);
        let starting = self.data.launching.contains(&id);
        let active = self
            .processes_ui
            .history
            .sorted()
            .into_iter()
            .find(|(_, p)| {
                p.id == id && p.status == Status::Running && p.operation != Operation::Copy
            })
            .map(|(_, p)| p);
        let caption = if let Some(p) = active {
            format!("{} · {}", p.operation.label(), p.progress_text())
        } else {
            format!("{} · {}", pack_version(info), format_bytes(info.size_bytes))
        };
        let drag_owner = cx.entity().downgrade();
        let dragged = super::library_groups::DraggedInstance {
            id: info.id.clone(),
            name: display_name(info),
            theme: t.clone(),
        };
        let keyboard_id = info.id.clone();
        let selector = format!("instance-tile:{}", info.id);
        let mut tile = div()
            .id(selector.clone())
            .role(gpui::accesskit::Role::ListBoxOption)
            .aria_label(display_name(info))
            .aria_selected(selected)
            .focusable()
            .tab_stop(true)
            .focus_visible(|style| style.border_2().border_color(t.primary()))
            .on_key_down(cx.listener(move |v, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space")
                    && !event.keystroke.modifiers.modified()
                {
                    v.select_instance(keyboard_id.clone(), cx);
                    if event.keystroke.key == "enter" && !v.instance_is_busy(&keyboard_id) {
                        v.play(keyboard_id.clone(), cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .debug_selector(move || selector.clone().into())
            .when(
                !self.instance_is_busy(&info.id) && !self.groups_ui.busy,
                |tile| {
                    tile.on_drag(dragged, move |drag, _, window, cx| {
                        if let Some(root) = drag_owner.upgrade() {
                            root.update(cx, |v, cx| v.begin_library_drag(drag, window, cx));
                        }
                        cx.new(|_| drag.clone())
                    })
                },
            )
            .relative()
            .group("instance-tile")
            .child(self.workspace_ui.drag.measure_tile(info.id.clone()))
            .w(px(144.0))
            .flex_shrink_0()
            .p(px(10.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if selected {
                tint(t.primary(), 0.65)
            } else {
                t.border()
            })
            .bg(t.card())
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(selected, |el| {
                el.bg(tint(t.primary(), 0.13)).child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.0))
                        .bg(tint(t.primary(), 0.86)),
                )
            })
            .when(!selected, |el| el.hover(|s| s.bg(tint(t.accent(), 0.7))))
            .child(self.instance_avatar(info, 56.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.card_foreground())
                            .text_center()
                            .w_full()
                            .text_ellipsis()
                            .child(display_name(info)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.muted_foreground())
                            .text_center()
                            .child(caption),
                    ),
            )
            .child(
                icon_btn(t, "ellipsis", ButtonVariant::Ghost)
                    .aria_label(format!("Instance actions: {}", display_name(info)))
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .size(px(28.0))
                    .bg(tint(t.background(), 0.86))
                    .text_color(t.muted_foreground())
                    .on_click(cx.listener(move |v, e: &gpui::ClickEvent, _, cx| {
                        cx.stop_propagation();
                        v.workspace_ui.menu =
                            Some((MenuTarget::Instance(menu_id.clone()), e.position()));
                        cx.notify();
                    })),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |v, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    v.workspace_ui.menu =
                        Some((MenuTarget::Instance(right_id.clone()), e.position));
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |v, e: &gpui::ClickEvent, _, cx| {
                v.select_instance(id.clone(), cx);
                if e.click_count() == 2 && !v.instance_is_busy(&id) {
                    v.play(id.clone(), cx);
                }
            }));
        if !imported_frozen(info)
            && self.pack_status(pack_version(info)).status
                == industrialis_core::pack_version::PackUpdateStatus::UpdateAvailable
        {
            tile = tile.child(div().text_xs().child("Update available"));
        }
        if running {
            tile = tile.child(
                div()
                    .absolute()
                    .right(px(40.0))
                    .top(px(10.0))
                    .size(px(8.0))
                    .rounded_full()
                    .bg(gpui::rgb(0x37d67a)),
            );
        }
        if starting {
            tile = tile.child(
                div()
                    .text_size(px(10.0))
                    .text_color(t.muted_foreground())
                    .child("Launching…"),
            );
        }
        if let Some(p) = active {
            tile = tile.child(progress_bar(t, p.pct).h(px(4.0)));
        }
        tile.into_any_element()
    }

    pub(super) fn instances_view(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        let drag_focus = self
            .workspace_ui
            .drag
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let width = f32::from(window.viewport_size().width);
        let library = library_width(width, self.workspace_ui.fraction);
        let creating: Vec<_> = self
            .processes_ui
            .history
            .sorted()
            .into_iter()
            .filter(|(_, p)| {
                p.status == Status::Running
                    && matches!(
                        p.operation,
                        Operation::Copy | Operation::Install | Operation::Import
                    )
            })
            .collect();
        let ids: HashSet<_> = creating.iter().map(|(_, p)| p.id.as_str()).collect();
        let mut groups: BTreeMap<String, Vec<&InstanceInfo>> = BTreeMap::new();
        for info in &self.data.instances {
            if !ids.contains(info.id.as_str()) {
                groups.entry(info.group.clone()).or_default().push(info);
            }
        }
        for group in self.workspace_ui.order.keys().filter(|g| !g.is_empty()) {
            groups.entry(group.clone()).or_default();
        }
        groups.entry(String::new()).or_default();
        let ungrouped = groups.remove("");
        let mut ordered: Vec<_> = groups.into_iter().collect();
        ordered.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        if let Some(items) = ungrouped {
            ordered.push((String::new(), items));
        }
        let mut list = div().flex().flex_col().gap_2().p_2();
        if let Some(error) = &self.catalog_ui.error {
            list = list.child(
                div()
                    .text_xs()
                    .child(format!("Could not check pack updates: {error}")),
            );
        }
        if !creating.is_empty() {
            list = list.child(
                div()
                    .px_1()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(icon("loader-circle", 14.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.muted_foreground())
                            .child(
                                if creating.iter().all(|(_, p)| p.operation == Operation::Copy) {
                                    "COPIES IN PROGRESS"
                                } else {
                                    "INSTANCES IN PROGRESS"
                                },
                            ),
                    )
                    .child(
                        badge(&t, &creating.len().to_string(), BadgeKind::Normal)
                            .h(px(20.0))
                            .py_0(),
                    ),
            );
            list = list.child(div().flex().flex_wrap().gap_2().px_1().pb_1().children(
                creating.iter().map(|(key, p)| {
                    let key = (*key).to_string();
                    div()
                        .w(px(144.0))
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(tint(t.primary(), 0.05))
                        .border_1()
                        .border_color(tint(t.primary(), 0.2))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(56.0))
                                .rounded(px(12.0))
                                .bg(t.secondary())
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon("copy", 22.0)),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child(p.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(t.muted_foreground())
                                .child(p.progress_text()),
                        )
                        .child(progress_bar(&t, p.pct).h(px(4.0)))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |v, _: &MouseDownEvent, _, cx| {
                                v.select_process(key.clone(), cx)
                            }),
                        )
                }),
            ));
        }
        for (group, mut items) in ordered {
            if let Some(order) = self.workspace_ui.order.get(&group) {
                items.sort_by_key(|i| {
                    order
                        .iter()
                        .position(|id| id == &i.id)
                        .unwrap_or(usize::MAX)
                });
            }
            let collapsed = self.workspace_ui.collapsed.contains(&group);
            let label = if group.is_empty() {
                self.workspace_ui.ungrouped.to_uppercase()
            } else {
                group.to_uppercase()
            };
            let mut section = div().flex().flex_col().gap_1().child(self.group_header(
                group.clone(),
                label,
                items.len(),
                collapsed,
                cx,
            ));
            if !collapsed {
                section = section.child(self.library_group_grid(&group, items, cx));
            } else if self
                .workspace_ui
                .drag
                .target
                .as_ref()
                .is_some_and(|target| target.group == group)
            {
                section = section.child(self.library_group_grid(&group, Vec::new(), cx));
            }
            list = list.child(section);
        }
        if self.data.instances.is_empty() && creating.is_empty() {
            list = list.child(
                div()
                    .m_2()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("No instances installed"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child("Add a pack instance to start building your launcher library."),
                    ),
            );
        }
        let selected = self
            .selected_id
            .clone()
            .filter(|id| self.data.selected_instance(Some(id)).is_some());
        let detail = selected.map(|id| self.detail_view(&id, width - library - 12., cx));
        let details_open = detail.is_some();
        if !details_open {
            self.workspace_ui.resizing = false;
        }
        div()
            .debug_selector(|| "instance-workspace".into())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .on_mouse_move(cx.listener(move |v, e: &MouseMoveEvent, _, cx| {
                if v.workspace_ui.resizing && e.pressed_button == Some(MouseButton::Left) {
                    v.workspace_ui.fraction =
                        library_width(width, (f32::from(e.position.x) - 6.0) / (width - 12.0))
                            / (width - 12.0);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|v, _: &MouseUpEvent, _, cx| {
                    if v.workspace_ui.resizing {
                        v.workspace_ui.resizing = false;
                        v.save_workspace(cx);
                    }
                }),
            )
            .child(
                div()
                    .id("instance-library")
                    .debug_selector(|| "instance-library".into())
                    .aria_label("Instance library")
                    .role(gpui::accesskit::Role::Group)
                    .track_focus(&drag_focus)
                    .tab_stop(true)
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|v, event: &MouseDownEvent, window, cx| {
                            if let Some(focus) = &v.workspace_ui.drag.focus {
                                window.focus(focus, cx);
                            }
                            v.open_library_menu(event.position, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_key_down(
                        cx.listener(move |v, event: &gpui::KeyDownEvent, window, cx| {
                            if drag_focus.is_focused(window)
                                && ((event.keystroke.key == "f10"
                                    && event.keystroke.modifiers.shift)
                                    || event.keystroke.key == "menu")
                            {
                                v.open_library_menu(gpui::point(px(16.), px(52.)), cx);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_drag_move(cx.listener(
                        |v,
                         event: &gpui::DragMoveEvent<super::library_groups::DraggedInstance>,
                         _,
                         cx| v.leave_library_drag(event, cx),
                    ))
                    .when(details_open, |el| el.w(px(library)).flex_shrink_0())
                    .when(!details_open, |el| el.flex_1().min_w_0())
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .when(details_open, |el| {
                el.child(
                    div()
                        .debug_selector(|| "instance-divider".into())
                        .w(px(12.0))
                        .h_full()
                        .flex_shrink_0()
                        .cursor_col_resize()
                        .flex()
                        .items_center()
                        .justify_center()
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .left(px(6.0))
                                .top_0()
                                .bottom_0()
                                .w(px(1.0))
                                .bg(tint(t.border(), 0.68)),
                        )
                        .hover(|s| s.bg(tint(t.primary(), 0.1)))
                        .child(
                            div()
                                .w(px(4.0))
                                .h(px(36.0))
                                .rounded_full()
                                .bg(tint(t.border(), 0.58)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _, cx| {
                                v.workspace_ui.resizing = true;
                                cx.notify();
                            }),
                        ),
                )
            })
            .children(detail)
    }

    fn detail_view(&mut self, id: &str, panel_width: f32, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        let Some(info) = self.data.selected_instance(Some(id)).cloned() else {
            return div();
        };
        let close_id = info.id.clone();
        let header = div()
            .px_4()
            .py_3()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .bg(t.window_surface(mix(t.card(), t.background(), 0.72)))
            .border_b_1()
            .border_color(tint(t.border(), 0.82))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.instance_avatar(&info, 40.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(display_name(&info)),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_xs()
                                    .text_color(t.muted_foreground())
                                    .child(format!(
                                        "{} / {}{}",
                                        pack_version(&info),
                                        format_bytes(info.size_bytes),
                                        if info.group.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" / {}", info.group)
                                        }
                                    )),
                            ),
                    )
                    .child(
                        icon_btn(&t, "x", ButtonVariant::Ghost)
                            .debug_selector(|| "instance-details-close".into())
                            .aria_label("Close instance details")
                            .size(px(28.0))
                            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                if v.selected_id.as_ref() == Some(&close_id) {
                                    v.selected_id = None;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .h(px(32.0))
                    .border_b_1()
                    .border_color(tint(t.border(), 0.76))
                    .children(
                        [
                            (DetailTab::Info, "Info", "info"),
                            (DetailTab::Settings, "Settings", "sliders-horizontal"),
                            (DetailTab::Console, "Logs", "terminal"),
                            (DetailTab::Files, "Files", "folder-open"),
                            (DetailTab::Mods, "Add-ons", "boxes"),
                            (DetailTab::Backups, "Backups", "copy"),
                        ]
                        .into_iter()
                        .map(|(tab, label, glyph)| {
                            tab_btn(&t, label, self.instance_ui.detail_tab == tab)
                                .flex_1()
                                .gap(px(6.0))
                                .child(icon(glyph, 12.0))
                                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    v.instance_ui.detail_tab = tab;
                                    cx.notify();
                                }))
                        }),
                    ),
            );
        let content = match self.instance_ui.detail_tab {
            DetailTab::Info => self.instance_info(&info, cx).into_any_element(),
            DetailTab::Settings => self.instance_settings_view(id, cx).into_any_element(),
            DetailTab::Console => self.console_view(id, cx).into_any_element(),
            DetailTab::Files => self.files_view(id, panel_width, cx).into_any_element(),
            DetailTab::Mods => self.add_ons_view(id, cx).into_any_element(),
            DetailTab::Backups => self.instance_backups_view(id, cx).into_any_element(),
        };
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .children(
                (!imported_frozen(&info))
                    .then(|| self.pack_status_view(&info.id, pack_version(&info), cx)),
            )
            .child(content)
            .child(self.detail_actions(&info, cx))
    }

    fn instance_info(&self, info: &InstanceInfo, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let t = &self.theme;
        let id = info.id.clone();
        let enabled = !self.instance_is_busy(&id);
        let account = super::accounts::resolve_launch_account(
            &self.data.accounts,
            self.data.default_account_id.as_deref(),
            &info.settings,
        )
        .map(|a| {
            format!(
                "{} ({}, {})",
                a.username,
                if a.account_type == "offline" {
                    "offline"
                } else {
                    "Microsoft"
                },
                if info.settings.override_account {
                    "instance override"
                } else {
                    "default"
                }
            )
        })
        .unwrap_or_else(|| "No default account - set one in Accounts".into());
        let java = if info.settings.override_java_location {
            info.settings
                .java_path
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Invalid instance override".into())
        } else {
            self.data
                .settings
                .default_java_path
                .clone()
                .unwrap_or_else(|| "Auto-detect".into())
        };
        let mut rows = vec![
            ("Pack version", pack_version(info).into()),
            ("Instance ID", info.id.clone()),
            ("Size", format_bytes(info.size_bytes)),
            (
                "Group",
                if info.group.is_empty() {
                    self.workspace_ui.ungrouped.clone()
                } else {
                    info.group.clone()
                },
            ),
            ("Java", java),
            (
                "RAM",
                if info.settings.override_memory {
                    format!(
                        "{}-{} MB",
                        info.settings.min_ram_mb, info.settings.max_ram_mb
                    )
                } else {
                    "Default (4096-6144 MB)".into()
                },
            ),
        ];
        if info.settings.show_game_time
            && (info.settings.override_game_time || info.settings.total_play_seconds > 0)
        {
            rows.push((
                "Play time",
                format_play_time(info.settings.total_play_seconds as i64),
            ));
        }
        rows.push(("Account", account));
        let imported = info.settings.imported;
        let frozen = imported_frozen(info);
        let gtnh_import = imported && info.settings.imported_gtnh;
        let show_reinstall = !imported || (gtnh_import && !frozen);
        let version_id = id.clone();
        let reinstall_id = id.clone();
        let version_control = div()
            .flex()
            .items_center()
            .gap_3()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("GTNH version"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child(if frozen {
                                "This copy was imported from Prism and stays frozen. Set an official GTNH version to enable update checks, updates, and reinstall."
                            } else {
                                "Imported from Prism. Change the version if this copy was imported with the wrong one."
                            }),
                    ),
            )
            .child(
                btn(
                    t,
                    if frozen {
                        "Set version…"
                    } else {
                        "Change version…"
                    },
                    ButtonVariant::Outline,
                )
                .debug_selector(|| "import-set-version".into())
                .h(px(28.0))
                .px(px(10.0))
                .text_xs()
                .tab_stop(enabled)
                .when(!enabled, |el| el.opacity(0.5).cursor_default())
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    if enabled {
                        v.open_import_version_dialog(&version_id, cx);
                    }
                })),
            );
        let reinstall = div()
            .flex()
            .items_center()
            .gap_3()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Clean reinstall"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child("Fresh pack install while keeping saves, JourneyMap, options, and launcher overlays."),
                    ),
            )
            .child(
                btn(t, "", ButtonVariant::Outline)
                    .aria_label("Reinstall")
                    .h(px(28.0))
                    .px(px(10.0))
                    .text_xs()
                    .child(icon("refresh-cw", 14.0))
                    .child("Reinstall")
                    .tab_stop(enabled)
                    .when(!enabled, |el| el.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if enabled {
                            v.request_instance_action(reinstall_id.clone(), 4, cx);
                        }
                    })),
            );
        div().id("instance-info").flex_1().min_h_0().overflow_y_scroll().px_4().pt_3().pb_4().flex().flex_col().gap_3()
            .when(gtnh_import, |el| el.child(version_control))
            .when(imported && !gtnh_import, |el| el.child(
                div()
                    .py_2()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Imported from Prism as a non-GTNH instance. Update checks, updates, and reinstall stay unavailable."),
            ))
            .when(show_reinstall, |el| el.child(reinstall))
            .child(div().flex().flex_wrap().children(rows.into_iter().map(|(key, value)| div().w(relative(0.5)).min_w_0().py(px(10.0)).pr_3().border_t_1().border_color(tint(t.border(), 0.66))
                .flex().items_center().justify_between().gap_3().child(div().text_xs().flex_shrink_0().text_color(t.muted_foreground()).child(key)).child(div().flex_1().min_w_0().text_right().text_ellipsis().text_xs().font_weight(FontWeight::MEDIUM).child(value)))))
    }

    fn detail_actions(&self, info: &InstanceInfo, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let id = info.id.clone();
        let running = self.data.running.get(&id).is_some_and(|(_, alive)| *alive);
        let starting = self.data.launching.contains(&id);
        let busy = self.instance_is_busy(&id);
        let mut bar = div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(tint(t.border(), 0.8))
            .bg(t.window_surface(mix(t.card(), t.background(), 0.54)))
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .gap_2();
        if let Some(p) = self.operation_progress.get(&id) {
            return bar.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(div().text_sm().child(format!(
                        "{} {}%",
                        p.stage,
                        (p.pct * 100.0) as u32
                    )))
                    .child(progress_bar(t, p.pct).h(px(6.0))),
            );
        }
        let launch_disabled = starting || (!running && busy);
        bar = bar.child(
            btn(
                t,
                "",
                if running {
                    ButtonVariant::Danger
                } else {
                    ButtonVariant::Primary
                },
            )
            .aria_label(if starting {
                "Launching"
            } else if running {
                "Stop"
            } else {
                "Launch"
            })
            .flex_1()
            .child(icon(
                if starting {
                    "loader-circle"
                } else if running {
                    "square"
                } else {
                    "play"
                },
                16.0,
            ))
            .child(if starting {
                "Launching..."
            } else if running {
                "Stop"
            } else {
                "Launch"
            })
            .when(launch_disabled, |el| el.opacity(0.5).cursor_default())
            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                if !launch_disabled {
                    if running {
                        v.kill(id.clone(), cx);
                    } else {
                        v.play(id.clone(), cx);
                    }
                }
            })),
        );
        let mut actions = vec![
            ("arrow-up-circle", "Update pack", 0),
            ("folder-open", "Open instance folder", 1),
            ("copy", "Copy instance", 2),
            ("trash-2", "Delete instance", 3),
        ];
        if imported_frozen(info) {
            actions.retain(|(_, _, action)| *action != 0);
        }
        for (glyph, label, action) in actions {
            let id = info.id.clone();
            bar = bar.child(
                icon_btn(t, glyph, ButtonVariant::Outline)
                    .aria_label(label)
                    .tab_stop(!busy || action == 1)
                    .when(busy && action != 1, |el| el.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if action == 1 || !v.instance_is_busy(&id) {
                            v.request_instance_action(id.clone(), action, cx);
                        }
                    })),
            );
        }
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::{size, TestAppContext, VisualTestContext};

    #[gpui::test]
    async fn closing_details_expands_library_and_reopening_preserves_split(
        cx: &mut TestAppContext,
    ) {
        for width in [800.0, 1100.0, 1600.0] {
            let fixture = Fixture::new(&["pack", "other"]);
            let (window, root) = fixture.open_sized(cx, size(px(width), px(850.0)), |v| {
                v.workspace_ui.fraction = 0.6;
            });
            cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                assert_eq!(
                    visual.debug_bounds("instance-library"),
                    visual.debug_bounds("instance-workspace")
                );
                assert!(visual.debug_bounds("instance-divider").is_none());
                assert!(visual.debug_bounds("instance-details-close").is_none());
            }
            Fixture::click(cx, window, "instance-tile:pack");
            let split_bounds = {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                let library = visual.debug_bounds("instance-library").unwrap();
                let workspace = visual.debug_bounds("instance-workspace").unwrap();
                assert!(library.size.width < workspace.size.width);
                assert!(visual.debug_bounds("instance-divider").is_some());
                library
            };
            Fixture::click(cx, window, "instance-details-close");
            assert!(root.read_with(cx, |v, _| v.selected_id.is_none()));
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                assert_eq!(
                    visual.debug_bounds("instance-library"),
                    visual.debug_bounds("instance-workspace")
                );
                assert!(visual.debug_bounds("instance-divider").is_none());
                assert!(visual.debug_bounds("instance-details-close").is_none());
            }
            Fixture::click(cx, window, "instance-tile:other");
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                assert_eq!(
                    visual.debug_bounds("instance-library").unwrap(),
                    split_bounds
                );
                assert!(visual.debug_bounds("instance-divider").is_some());
                assert!(visual.debug_bounds("instance-details-close").is_some());
            }
            assert!(
                root.read_with(cx, |v, _| v.selected_id.as_deref() == Some("other")
                    && (v.workspace_ui.fraction - 0.6).abs() < f32::EPSILON)
            );
            // A removed/stale selection must not reserve an empty details pane.
            root.update(cx, |v, cx| {
                v.selected_id = Some("removed".into());
                v.workspace_ui.resizing = true;
                cx.notify();
            });
            cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                assert_eq!(
                    visual.debug_bounds("instance-library"),
                    visual.debug_bounds("instance-workspace")
                );
                assert!(visual.debug_bounds("instance-divider").is_none());
            }
            assert!(!root.read_with(cx, |v, _| v.workspace_ui.resizing));
            fixture.cleanup();
        }
    }

    #[test]
    fn split_preserves_reference_panel_limits_at_minimum_and_large_windows() {
        for width in [800.0, 1100.0, 1600.0] {
            for fraction in [-1.0, 0.54, 2.0] {
                let library = library_width(width, fraction);
                assert!(library >= 260.0);
                assert!(width - 12.0 - library >= 320.0);
                assert!(library <= (width - 12.0) * 0.68);
            }
        }
    }

    #[tokio::test]
    async fn older_save_cannot_replace_newer_layout_and_collapsed_groups() {
        let directory = std::env::temp_dir().join(format!(
            "industrialis-workspace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = LauncherPaths::new(directory.clone());
        let lock = Default::default();
        let generation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(2));
        let path = directory.join("gpui-workspace.json");
        write_snapshot(
            path.clone(),
            json!({"fraction": 0.64, "collapsed": ["Production"]}),
            lock,
            generation.clone(),
            2,
        )
        .await
        .unwrap();
        write_snapshot(
            path.clone(),
            json!({"fraction": 0.4, "collapsed": []}),
            Default::default(),
            generation,
            1,
        )
        .await
        .unwrap();
        let loaded = WorkspaceUi::load(&paths);
        assert!((loaded.fraction - 0.64).abs() < 0.001);
        assert!(loaded.collapsed.is_empty()); // Legacy GPUI shadow state is ignored.
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
