//! InstanceWorkspace, library tiles, header, Info panel and bottom action bar.
use super::{
    instance_actions::MaintenanceKind,
    instance_tile::{is_context_menu_key, TILE_HEIGHT, TILE_WIDTH},
    DetailTab, LauncherRoot,
};
use crate::state::format_bytes;
use crate::ui::{
    btn, card, icon, icon_btn, mix, progress_bar, spinner, tab_btn, tint, ButtonVariant,
};
use gpui::{
    div, prelude::*, px, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Window,
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
        let path = paths.base().join("launcher-workspace.json");
        let saved: Value = std::fs::read(&path)
            .or_else(|_| std::fs::read(paths.base().join("gpui-workspace.json")))
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
pub(super) fn imported_frozen(info: &InstanceInfo) -> bool {
    info.settings.imported
        && (info.settings.pack_version.is_empty() || !info.settings.imported_gtnh)
}
pub(super) fn pack_version(info: &InstanceInfo) -> &str {
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
            let item = btn(&self.theme, "", ButtonVariant::MenuItem)
                .id("library-new-group")
                .debug_selector(|| "library-new-group".into())
                .aria_label("New group")
                .role(gpui::accesskit::Role::MenuItem)
                .tab_stop(!disabled)
                .w_full()
                .justify_start()
                .text_xs()
                .px_2()
                .gap_2()
                .when(disabled, |button| button.opacity(0.5).cursor_default())
                .child(icon("folder", 14.0))
                .child("New group")
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    if !disabled {
                        v.open_group_dialog(super::library_groups::GroupDialogKind::Create, cx);
                    }
                }));
            let import_disabled = self.closing.is_some();
            let import = btn(&self.theme, "", ButtonVariant::MenuItem)
                .id("library-import-prism")
                .debug_selector(|| "library-import-prism".into())
                .aria_label("Import from Prism…")
                .role(gpui::accesskit::Role::MenuItem)
                .tab_stop(!import_disabled)
                .w_full()
                .justify_start()
                .text_xs()
                .px_2()
                .gap_2()
                .when(import_disabled, |button| {
                    button.opacity(0.5).cursor_default()
                })
                .child(icon("archive", 14.0))
                .child("Import from Prism…")
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
            ]
            .into_iter()
            .map(|(glyph, label, action)| {
                let id = id.clone();
                let disabled = (busy && !matches!(action, 1 | 5 | 6 | 7 | 12))
                    || (action == 7 && self.groups_ui.busy)
                    || (action == 12 && self.catalog_ui.loading)
                    || (frozen && matches!(action, 0 | 4 | 12));
                btn(t, "", ButtonVariant::MenuItem)
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
        Some(self.library_menu_overlay(menu, *position, 328., window, cx))
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
                .max(52.)))
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

    /// Card order shared by the library grid and the nine launch slots.
    /// Named groups sort alphabetically, the ungrouped section comes last, and
    /// each group keeps its saved card order. Transient copy/install cards are
    /// not part of the library and do not take a slot.
    pub(super) fn library_order(&self) -> Vec<(String, Vec<&InstanceInfo>)> {
        let creating: HashSet<_> = self
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
            .map(|(_, p)| p.id.as_str())
            .collect();
        let mut groups: BTreeMap<String, Vec<&InstanceInfo>> = BTreeMap::new();
        for info in &self.data.instances {
            if !creating.contains(info.id.as_str()) {
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
        for (group, items) in &mut ordered {
            if let Some(order) = self.workspace_ui.order.get(group) {
                items.sort_by_key(|info| {
                    order
                        .iter()
                        .position(|id| id == &info.id)
                        .unwrap_or(usize::MAX)
                });
            }
        }
        ordered
    }

    pub(super) fn launch_slot_ids(&self) -> Vec<String> {
        self.library_order()
            .into_iter()
            .flat_map(|(_, items)| items.into_iter().map(|info| info.id.clone()))
            .collect()
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
        let ordered = self.library_order();
        let slot_hints = self.launch_slot_hints();
        let mut list = div().flex().flex_col().gap_4().px_5().pt_5().pb_6();
        if let Some(error) = &self.catalog_ui.error {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(tint(t.destructive(), 0.4))
                    .bg(tint(t.destructive(), 0.08))
                    .text_size(px(12.0))
                    .text_color(t.destructive())
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
                    .gap_2()
                    .text_color(t.primary())
                    .child(spinner("library-creating-spinner", "loader-circle", 14.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.foreground())
                            .child(
                                if creating.iter().all(|(_, p)| p.operation == Operation::Copy) {
                                    "Copies in progress"
                                } else {
                                    "Instances in progress"
                                },
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(t.muted_foreground())
                            .child(creating.len().to_string()),
                    ),
            );
            list = list.child(div().flex().flex_wrap().gap_3().px_1().pb_2().children(
                creating.iter().map(|(key, p)| {
                    let key = (*key).to_string();
                    div()
                        .w(px(TILE_WIDTH))
                        .h(px(TILE_HEIGHT))
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .px(px(14.0))
                        .py(px(14.0))
                        .rounded(px(10.0))
                        .bg(tint(t.primary(), 0.07))
                        .border_1()
                        .border_color(tint(t.primary(), 0.3))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .size(px(56.0))
                                .rounded(px(14.0))
                                .bg(t.secondary())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(t.primary())
                                .child(icon("copy", 22.0)),
                        )
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .w_full()
                                        .text_center()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_ellipsis()
                                        .child(p.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(t.muted_foreground())
                                        .text_ellipsis()
                                        .child(p.progress_text()),
                                ),
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
        for (group, items) in ordered {
            let collapsed = self.workspace_ui.collapsed.contains(&group);
            let label = if group.is_empty() {
                self.workspace_ui.ungrouped.clone()
            } else {
                group.clone()
            };
            let mut section = div().flex().flex_col().gap_2().child(self.group_header(
                group.clone(),
                label,
                items.len(),
                collapsed,
                cx,
            ));
            if !collapsed {
                section = section.child(self.library_group_grid(&group, items, &slot_hints, cx));
            } else if self
                .workspace_ui
                .drag
                .target
                .as_ref()
                .is_some_and(|target| target.group == group)
            {
                section =
                    section.child(self.library_group_grid(&group, Vec::new(), &slot_hints, cx));
            }
            list = list.child(section);
        }
        if self.data.instances.is_empty() && creating.is_empty() {
            list = list.child(
                card(&t)
                    .mt_2()
                    .p_6()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground())
                                    .child("No instances installed"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(t.muted_foreground())
                                    .child(
                                        "Install a GT New Horizons release or import an existing Prism pack to begin.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                btn(&t, "Install GTNH pack", ButtonVariant::Primary)
                                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                        v.open_new_instance(cx);
                                    })),
                            )
                            .child(
                                btn(&t, "Import from Prism…", ButtonVariant::Outline)
                                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                        v.open_prism_import(None, false, cx);
                                    })),
                            ),
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
                                && is_context_menu_key(&event.keystroke)
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
        let pack_update = (!imported_frozen(&info))
            .then(|| self.pack_status(pack_version(&info)))
            .filter(|status| {
                status.status == industrialis_core::pack_version::PackUpdateStatus::UpdateAvailable
            });
        let update_id = info.id.clone();
        let header = div()
            .pt_4()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_3()
            .bg(t.window_surface(mix(t.card(), t.background(), 0.72)))
            .border_b_1()
            .border_color(tint(t.border(), 0.85))
            .child(
                div()
                    .px_5()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.instance_avatar(&info, 48.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.foreground())
                                    .text_ellipsis()
                                    .child(display_name(&info)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .font_family(crate::input::code_font_family())
                                    .text_size(px(11.0))
                                    .text_color(t.muted_foreground())
                                    .child(pack_version(&info).to_string())
                                    .child(format_bytes(info.size_bytes))
                                    .when(!info.group.is_empty(), |row| {
                                        row.child(info.group.clone())
                                    }),
                            ),
                    )
                    .when_some(pack_update, |row, status| {
                        let target = update_id.clone();
                        let label = status
                            .latest_version
                            .map(|ver| format!("Update to {ver}"))
                            .unwrap_or_else(|| "Update pack".into());
                        row.child(
                            btn(&t, "", ButtonVariant::Outline)
                                .aria_label(label.clone())
                                .h(px(30.0))
                                .px(px(10.0))
                                .text_size(px(12.0))
                                .border_color(tint(t.primary(), 0.6))
                                .text_color(t.primary())
                                .child(icon("download", 13.0))
                                .child(label)
                                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                    v.open_maintenance(target.clone(), MaintenanceKind::Update, cx);
                                })),
                        )
                    })
                    .child(
                        icon_btn(&t, "x", ButtonVariant::Ghost)
                            .debug_selector(|| "instance-details-close".into())
                            .aria_label("Close instance details")
                            .size(px(32.0))
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
                    .id("instance-detail-tabs")
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .overflow_x_scroll()
                    .children(
                        [
                            (DetailTab::Info, "Info"),
                            (DetailTab::Settings, "Settings"),
                            (DetailTab::Console, "Logs"),
                            (DetailTab::Files, "Files"),
                            (DetailTab::Mods, "Add-ons"),
                            (DetailTab::Backups, "Backups"),
                        ]
                        .into_iter()
                        .map(|(tab, label)| {
                            tab_btn(&t, label, self.instance_ui.detail_tab == tab)
                                .flex_shrink_0()
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
            .bg(t.window_surface(mix(t.card(), t.background(), 0.22)))
            .child(header)
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
        .unwrap_or_else(|| "No default account (set in Accounts)".into());
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
        let update_label = if imported_frozen(info) {
            "Frozen import".to_string()
        } else {
            use industrialis_core::pack_version::PackUpdateStatus;
            let status = self.pack_status(pack_version(info));
            match status.status {
                PackUpdateStatus::UpdateAvailable => status
                    .latest_version
                    .map(|ver| format!("Update available ({ver})"))
                    .unwrap_or_else(|| "Update available".into()),
                PackUpdateStatus::UpToDate => "Up to date".into(),
                _ if self.catalog_ui.loading => "Checking…".into(),
                _ => "Unavailable".into(),
            }
        };
        let mut rows = vec![
            ("Pack version", pack_version(info).into()),
            ("Update status", update_label),
            ("Disk size", format_bytes(info.size_bytes)),
            (
                "Group",
                if info.group.is_empty() {
                    self.workspace_ui.ungrouped.clone()
                } else {
                    info.group.clone()
                },
            ),
            ("Java runtime", java),
            (
                "Memory",
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
        rows.push(("Instance ID", info.id.clone()));
        let imported = info.settings.imported;
        let frozen = imported_frozen(info);
        let gtnh_import = imported && info.settings.imported_gtnh;
        let show_reinstall = !imported || (gtnh_import && !frozen);
        let version_id = id.clone();
        let reinstall_id = id.clone();
        let version_control = div()
            .flex()
            .items_center()
            .gap_4()
            .px_4()
            .py_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("GTNH version"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
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
                .h(px(32.0))
                .px(px(12.0))
                .text_size(px(12.0))
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
            .gap_4()
            .px_4()
            .py_3()
            .when(gtnh_import, |el| {
                el.border_t_1().border_color(tint(t.border(), 0.7))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Clean reinstall"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(t.muted_foreground())
                            .child("Fresh pack install while keeping saves, JourneyMap, options, and launcher overlays."),
                    ),
            )
            .child(
                btn(t, "", ButtonVariant::Outline)
                    .aria_label("Reinstall")
                    .h(px(32.0))
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .child(icon("refresh-cw", 13.0))
                    .child("Reinstall")
                    .tab_stop(enabled)
                    .when(!enabled, |el| el.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if enabled {
                            v.request_instance_action(reinstall_id.clone(), 4, cx);
                        }
                    })),
            );
        // Two-column spec sheet: label over value, hairline grid between cells.
        let cell = |key: &'static str, value: String, left_rule: bool| {
            let technical = matches!(
                key,
                "Pack version" | "Disk size" | "Memory" | "Play time" | "Instance ID"
            );
            div()
                .flex_1()
                .min_w_0()
                .px_4()
                .py(px(10.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .when(left_rule, |el| {
                    el.border_l_1().border_color(tint(t.border(), 0.8))
                })
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.muted_foreground())
                        .child(key.to_uppercase()),
                )
                .child(
                    div()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.foreground())
                        .when(technical, |el| {
                            el.font_family(crate::input::code_font_family())
                        })
                        .child(value),
                )
        };
        let specs = card(t)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(rows.chunks(2).enumerate().map(|(index, pair)| {
                        let mut row = div()
                            .w_full()
                            .flex()
                            .when(index > 0, |row| {
                                row.border_t_1().border_color(tint(t.border(), 0.8))
                            })
                            .child(cell(pair[0].0, pair[0].1.clone(), false));
                        row = match pair.get(1) {
                            Some((key, value)) => row.child(cell(key, value.clone(), true)),
                            None => row.child(
                                div()
                                    .flex_1()
                                    .border_l_1()
                                    .border_color(tint(t.border(), 0.8)),
                            ),
                        };
                        row
                    })),
            );
        let has_maintenance = gtnh_import || show_reinstall || (imported && !gtnh_import);
        div()
            .id("instance-info")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(specs)
            .when(has_maintenance, |el| {
                el.child(
                    card(t)
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .when(gtnh_import, |panel| panel.child(version_control))
                        .when(imported && !gtnh_import, |panel| {
                            panel.child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .text_size(px(12.0))
                                    .text_color(t.muted_foreground())
                                    .child("Imported from Prism as a non-GTNH instance. Update checks, updates, and reinstall stay unavailable."),
                            )
                        })
                        .when(show_reinstall, |panel| panel.child(reinstall)),
                )
            })
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
            .border_color(tint(t.border(), 0.85))
            .bg(t.window_surface(mix(t.card(), t.background(), 0.72)))
            .px_5()
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(p.stage.clone())
                            .child(
                                div()
                                    .text_color(t.muted_foreground())
                                    .child(format!("{}%", (p.pct * 100.0) as u32)),
                            ),
                    )
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
            .h(px(38.0))
            .flex_1()
            .text_size(px(14.0))
            .font_weight(FontWeight::SEMIBOLD)
            .child(if starting {
                spinner(format!("launch-spinner:{id}"), "loader-circle", 16.0).into_any_element()
            } else {
                icon(if running { "square" } else { "play" }, 16.0).into_any_element()
            })
            .child(if starting {
                "Launching…"
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
                    .size(px(38.0))
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
    fn update_badge_keeps_instance_tiles_the_same_height(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["current", "outdated"]);
        for (id, version) in [("current", "2.9.0"), ("outdated", "2.8.0")] {
            std::fs::write(
                fixture.paths.settings_path(id).unwrap(),
                serde_json::json!({
                    "name": id,
                    "pack_version": version,
                    "pack_java_type": "java17+"
                })
                .to_string(),
            )
            .unwrap();
        }
        let (window, _root) = fixture.open(cx, |v| {
            v.catalog_ui.versions = Some(std::collections::HashMap::from([
                (
                    "2.8.0".to_string(),
                    industrialis_core::pack_version::PackVersionMeta {
                        title: "Stable release".to_string(),
                        release_date: "2025/01/01".to_string(),
                        max_java_version: 25,
                    },
                ),
                (
                    "2.9.0".to_string(),
                    industrialis_core::pack_version::PackVersionMeta {
                        title: "Stable release".to_string(),
                        release_date: "2026/01/01".to_string(),
                        max_java_version: 25,
                    },
                ),
            ]));
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let current = visual.debug_bounds("instance-tile:current").unwrap();
        let outdated = visual.debug_bounds("instance-tile:outdated").unwrap();
        assert_eq!(current.size.height, outdated.size.height);
        assert!(visual.debug_bounds("instance-update:outdated").is_some());
        assert!(visual.debug_bounds("instance-update:current").is_none());
        fixture.cleanup();
    }

    #[gpui::test]
    fn keyboard_opens_the_tile_context_menu_without_a_menu_button(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["a"]);
        let (window, root) = fixture.open(cx, |_| {});
        Fixture::click(cx, window, "instance-tile:a");
        let tile = {
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            visual.simulate_keystrokes("shift-f10");
            visual.debug_bounds("instance-tile:a").unwrap()
        };
        root.read_with(cx, |v, _| match &v.workspace_ui.menu {
            Some((MenuTarget::Instance(id), at)) => {
                assert_eq!(id, "a");
                assert!(
                    tile.contains(at),
                    "menu opens at the tile, not the window corner"
                );
            }
            _ => panic!("Shift+F10 on a focused tile must open its context menu"),
        });
        fixture.cleanup();
    }

    #[gpui::test]
    fn updating_instance_renders_a_progress_badge(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        let (window, _root) = fixture.open(cx, |v| {
            let token = v
                .processes_ui
                .history
                .start(Operation::Update, "pack", "GTNH SSP");
            v.processes_ui.history.progress(
                &token,
                "updating",
                0.5,
                Some("Half way".into()),
                (None, None, None),
            );
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let badge = visual
            .debug_bounds("instance-progress:pack")
            .expect("active instance must show its progress badge");
        let tile = visual.debug_bounds("instance-tile:pack").unwrap();
        assert_eq!(badge.size.width, px(28.0));
        assert_eq!(tile.size.height, px(TILE_HEIGHT));
        fixture.cleanup();
    }

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
        let path = directory.join("launcher-workspace.json");
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
        assert!(loaded.collapsed.is_empty()); // Legacy launcher shadow state is ignored.
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
