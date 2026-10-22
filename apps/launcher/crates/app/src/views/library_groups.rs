use super::*;
use crate::ui::btn;
use industrialis_backend::groups::{GroupChange, GroupsState};

#[derive(Default)]
pub(super) struct GroupsUi {
    pub busy: bool,
    pub generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::{size, MouseButton, TestAppContext, VisualTestContext};
    use industrialis_backend::{instances, paths::LauncherPaths};
    use std::time::Instant;

    #[gpui::test]
    async fn blank_library_context_creates_groups_without_overriding_header_or_tile_menus(
        cx: &mut TestAppContext,
    ) {
        for ids in [&[][..], &["a", "b"][..]] {
            let fixture = Fixture::new(ids);
            let (window, root) = fixture.open(cx, |_| {});
            cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            let background = {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                assert!(
                    visual.debug_bounds("library-new-group").is_none(),
                    "no permanent New group button"
                );
                let library = visual.debug_bounds("instance-library").unwrap();
                let position = library.origin + gpui::point(px(24.), library.size.height - px(24.));
                visual.simulate_mouse_down(position, MouseButton::Right, Default::default());
                visual.simulate_mouse_up(position, MouseButton::Right, Default::default());
                assert!(visual.debug_bounds("library-new-group").is_some());
                visual.simulate_keystrokes("escape");
                // Blank-space focus also supports the keyboard context menu.
                visual.simulate_keystrokes("shift-f10");
                assert!(visual.debug_bounds("library-new-group").is_some());
                position
            };
            assert!(root.read_with(cx, |v, _| matches!(
                &v.workspace_ui.menu,
                Some((super::super::workspace::MenuTarget::Library, _))
            )));
            Fixture::click(cx, window, "library-new-group");
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                let field = visual.debug_bounds("field-group-name").unwrap();
                visual.simulate_click(field.center(), Default::default());
                visual.simulate_keystrokes("n e w enter");
            }
            fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
            let saved = fixture
                .rt
                .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
                .unwrap();
            assert!(saved.instance_order.contains_key("new"));
            assert!(root.read_with(cx, |v, _| v.dialog.is_none()
                && v.workspace_ui.menu.is_none()));
            // Headers retain their group menu; right-click must not bubble to the library.
            cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                let header = visual.debug_bounds("group-header:new").unwrap();
                visual.simulate_mouse_down(header.center(), MouseButton::Right, Default::default());
                visual.simulate_mouse_up(header.center(), MouseButton::Right, Default::default());
                assert!(visual.debug_bounds("group-menu-rename").is_some());
                assert!(visual.debug_bounds("library-new-group").is_none());
                visual.simulate_keystrokes("escape");
                if !ids.is_empty() {
                    let tile = visual.debug_bounds("instance-tile:a").unwrap();
                    visual.simulate_mouse_down(
                        tile.center(),
                        MouseButton::Right,
                        Default::default(),
                    );
                    visual.simulate_mouse_up(tile.center(), MouseButton::Right, Default::default());
                    assert!(visual.debug_bounds("instance-menu-action:12").is_some());
                    assert!(visual.debug_bounds("library-new-group").is_none());
                    visual.simulate_keystrokes("escape");
                }
                // Disk usage updates automatically; there is no toolbar action.
                assert!(visual.debug_bounds("library-refresh-sizes").is_none());
                assert!(visual.debug_bounds("library-new-group").is_none());
                // Opening/dismissing the blank-space menu never creates another group.
                visual.simulate_mouse_down(background, MouseButton::Right, Default::default());
                visual.simulate_mouse_up(background, MouseButton::Right, Default::default());
                assert!(visual.debug_bounds("library-new-group").is_some());
                visual.simulate_click(
                    background - gpui::point(px(0.), px(96.)),
                    Default::default(),
                );
                assert!(visual.debug_bounds("library-new-group").is_none());
            }
            assert!(root.read_with(cx, |v, _| v.workspace_ui.menu.is_none()
                && v.dialog.is_none()));
            assert_eq!(
                fixture
                    .rt
                    .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
                    .unwrap()
                    .instance_order
                    .len(),
                saved.instance_order.len()
            );
            fixture.cleanup();
        }
    }

    #[gpui::test]
    async fn group_context_rename_and_create_use_the_selected_group_without_collapsing(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["a", "b"]);
        fixture.rt.block_on(industrialis_backend::fs_utils::write_json(&fixture.paths.groups_path(),
            &serde_json::json!({"formatVersion":1,"future":true,"groups":{"Games":{"instances":["a"]},"Empty":{"instances":[]}},"ungrouped":{"name":"Main","instances":["b"]}}))).unwrap();
        let (window, root) = fixture.open(cx, |_| {});
        fn right_click(
            cx: &mut TestAppContext,
            window: gpui::WindowHandle<LauncherRoot>,
            selector: &'static str,
        ) {
            cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            let bounds = visual.debug_bounds(selector).expect("group header");
            visual.simulate_mouse_down(bounds.center(), MouseButton::Right, Default::default());
            visual.simulate_mouse_up(bounds.center(), MouseButton::Right, Default::default());
        }
        right_click(cx, window, "group-header:Games");
        root.read_with(cx,|v,_| {
            assert!(!v.workspace_ui.collapsed.contains("Games"));
            assert!(matches!(&v.workspace_ui.menu,Some((super::super::workspace::MenuTarget::Group(group),_)) if group=="Games"));
        });
        Fixture::click(cx, window, "group-menu-rename");
        root.update(cx, |v, cx| {
            let Some(Dialog::Group(d)) = &mut v.dialog else {
                panic!("rename dialog")
            };
            assert!(matches!(&d.kind,GroupDialogKind::Rename(group) if group=="Games"));
            d.name = FieldState::new("Renamed");
            v.submit_group_dialog(cx);
        });
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.paths.groups_path()).unwrap()).unwrap();
        assert_eq!(
            saved["groups"]["Renamed"]["instances"],
            serde_json::json!(["a"])
        );
        assert_eq!(saved["future"], true);

        // The header remains keyboard-focusable after removing the visible Rename button.
        Fixture::click(cx, window, "group-header:Empty");
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            visual.simulate_keystrokes("shift-f10");
            assert!(visual.debug_bounds("group-menu-create-instance").is_some());
            visual.simulate_keystrokes("escape");
        }
        assert!(root.read_with(cx, |v, _| v.workspace_ui.menu.is_none()));

        // Hold catalog work unpolled; selecting a group must be immediate and
        // independent of network availability. No live network request runs here.
        let before_create = std::fs::read(fixture.paths.groups_path()).unwrap();
        let catalog_rt = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        root.update(cx, |v, _| v.data.rt = catalog_rt.clone());
        for (selector, group) in [
            ("group-header:Renamed", "Renamed"),
            ("group-header:Empty", "Empty"),
            ("group-header:", ""),
        ] {
            right_click(cx, window, selector);
            Fixture::click(cx, window, "group-menu-create-instance");
            root.read_with(cx, |v, _| {
                let Some(Dialog::NewInstance(d)) = &v.dialog else {
                    panic!("install dialog")
                };
                assert_eq!(d.group.text, group);
                assert!(d.versions_loading);
                assert!(v.workspace_ui.menu.is_none());
                assert!(v.busy.is_empty());
            });
            Fixture::click(cx, window, "field-new-group");
            {
                let mut visual = VisualTestContext::from_window(window.into(), cx);
                visual.update(|_, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()))
                });
                visual.simulate_keystrokes("ctrl-a ctrl-c");
                if !group.is_empty() {
                    visual.update(|_, cx| {
                        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), group)
                    });
                }
                visual.simulate_keystrokes("escape");
            }
            assert!(root.read_with(cx, |v, _| v.dialog.is_none()));
        }
        Fixture::click(cx, window, "toolbar-add");
        Fixture::click(cx, window, "add-menu-install");
        root.read_with(cx, |v, _| {
            assert!(matches!(&v.dialog,Some(Dialog::NewInstance(d)) if d.group.text.is_empty()))
        });
        assert_eq!(
            std::fs::read(fixture.paths.groups_path()).unwrap(),
            before_create
        );
        fixture.cleanup();
    }
    #[gpui::test]
    async fn group_dialog_assignment_clear_rename_collision_and_delete_persist_without_touching_instances(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["a", "b"]);
        let before: Vec<_> = ["a", "b"]
            .iter()
            .map(|id| std::fs::read(fixture.paths.settings_path(id).unwrap()).unwrap())
            .collect();
        let (_, root) = fixture.open(cx, |_| {});
        for (kind, name) in [
            (GroupDialogKind::Assign("a".into()), "Games"),
            (GroupDialogKind::Assign("b".into()), "Games"),
            (GroupDialogKind::Assign("b".into()), ""),
            (GroupDialogKind::Rename("Games".into()), "Renamed"),
        ] {
            root.update(cx, |v, cx| {
                v.open_group_dialog(kind, cx);
                if let Some(Dialog::Group(dialog)) = &mut v.dialog {
                    dialog.name = FieldState::new(name);
                }
                v.submit_group_dialog(cx);
            });
            fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
            assert!(root.read_with(cx, |v, _| v.dialog.is_none()));
        }
        let loaded = fixture
            .rt
            .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
            .unwrap();
        assert_eq!(loaded.instance_groups["a"], "Renamed");
        assert!(!loaded.instance_groups.contains_key("b"));
        root.update(cx, |v, cx| {
            v.open_group_dialog(GroupDialogKind::Create, cx);
            if let Some(Dialog::Group(dialog)) = &mut v.dialog {
                dialog.name = FieldState::new("renamed");
            }
            v.submit_group_dialog(cx);
        });
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        assert!(root.read_with(
            cx,
            |v, _| matches!(&v.dialog, Some(Dialog::Group(d)) if d.error.is_some())
        ));
        root.update(cx, |v, cx| {
            v.open_group_dialog(GroupDialogKind::Delete("Renamed".into()), cx);
            v.submit_group_dialog(cx);
        });
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        root.update(cx, |v, cx| {
            v.open_group_dialog(GroupDialogKind::Rename("".into()), cx);
            if let Some(Dialog::Group(dialog)) = &mut v.dialog {
                dialog.name = FieldState::new("Loose");
            }
            v.submit_group_dialog(cx);
        });
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        let reloaded = super::super::workspace::WorkspaceUi::load(&fixture.paths);
        assert_eq!(reloaded.ungrouped, "Loose");
        assert_eq!(reloaded.order[""], vec!["b", "a"]);
        for (index, id) in ["a", "b"].iter().enumerate() {
            assert_eq!(
                std::fs::read(fixture.paths.settings_path(id).unwrap()).unwrap(),
                before[index]
            );
            assert!(fixture
                .paths
                .instance_dir(id)
                .unwrap()
                .join("mmc-pack.json")
                .exists());
        }
        fixture.cleanup();
    }

    #[gpui::test]
    async fn keyboard_create_and_pointer_drag_persist_to_shared_group_file(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let root_dir = std::env::temp_dir().join(format!(
            "industrialis-group-ui-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = LauncherPaths::new(root_dir.clone());
        for id in ["a", "b", "c"] {
            let dir = paths.instance_dir(id).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("mmc-pack.json"), "{}").unwrap();
        }
        rt.block_on(industrialis_backend::fs_utils::write_json(
            &paths.groups_path(),
            &serde_json::json!({"formatVersion":1,"groups":{"Main":{"instances":["a","b","c"]}}}),
        ))
        .unwrap();
        let data = AppData {
            instances: rt.block_on(instances::load_instances(&paths)).unwrap(),
            paths: paths.clone(),
            rt: rt.clone(),
            accounts: Vec::new(),
            settings: Default::default(),
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(1100.0), px(750.0)), |_, _| LauncherRoot::new(data));
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let library = visual.debug_bounds("instance-library").unwrap();
            let empty = library.origin + gpui::point(px(24.), library.size.height - px(24.));
            visual.simulate_mouse_down(empty, MouseButton::Right, Default::default());
            visual.simulate_mouse_up(empty, MouseButton::Right, Default::default());
            let add = visual.debug_bounds("library-new-group").unwrap();
            visual.simulate_click(add.center(), Default::default());
            let field = visual.debug_bounds("field-group-name").unwrap();
            visual.simulate_click(field.center(), Default::default());
            visual.simulate_keystrokes("m o d s enter");
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while root.update(cx, |v, _| v.groups_ui.busy) {
            cx.run_until_parked();
            assert!(Instant::now() < deadline);
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        assert!(rt
            .block_on(industrialis_backend::groups::groups_state(&paths))
            .unwrap()
            .instance_order
            .contains_key("mods"));
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let source = visual.debug_bounds("instance-tile:c").unwrap().center();
            let target = visual.debug_bounds("instance-tile:a").unwrap().center();
            visual.simulate_mouse_down(source, MouseButton::Left, Default::default());
            visual.simulate_mouse_move(
                source + gpui::point(px(10.0), px(10.0)),
                MouseButton::Left,
                Default::default(),
            );
            visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
            visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while root.update(cx, |v, _| v.groups_ui.busy) {
            cx.run_until_parked();
            assert!(Instant::now() < deadline);
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        assert_eq!(
            rt.block_on(industrialis_backend::groups::groups_state(&paths))
                .unwrap()
                .instance_order["Main"],
            vec!["c", "a", "b"]
        );
        let loaded = super::super::workspace::WorkspaceUi::load(&paths);
        assert_eq!(loaded.order["Main"], vec!["c", "a", "b"]);
        std::fs::remove_dir_all(root_dir).unwrap();
    }
}
#[derive(Debug, Clone)]
pub(super) enum GroupDialogKind {
    Create,
    Rename(String),
    Assign(String),
    Delete(String),
}
#[derive(Debug, Clone)]
pub(super) struct GroupDialog {
    pub kind: GroupDialogKind,
    pub name: FieldState,
    error: Option<String>,
}
#[derive(Clone)]
pub(super) struct DraggedInstance {
    pub id: String,
    pub name: String,
    pub theme: ActiveTheme,
}
impl Render for DraggedInstance {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_3()
            .rounded_md()
            .bg(self.theme.card())
            .text_color(self.theme.foreground())
            .child(self.name.clone())
    }
}
impl LauncherRoot {
    pub(super) fn group_header(
        &self,
        group: String,
        label: String,
        count: usize,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let click = group.clone();
        let drop_group = group.clone();
        let hover_group = group.clone();
        let context_group = group.clone();
        let keyboard_group = group.clone();
        let delete = group.clone();
        let anchor = std::rc::Rc::new(std::cell::Cell::new(gpui::point(px(0.), px(44.))));
        let measured = anchor.clone();
        let delete_label = format!("Delete group {label}");
        div()
            .flex()
            .items_center()
            .gap_1()
            .bg(crate::ui::tint(self.theme.card(), 0.9))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |v, event: &gpui::MouseDownEvent, _, cx| {
                    v.open_group_menu(context_group.clone(), event.position, cx);
                    cx.stop_propagation();
                }),
            )
            .on_drag_move(cx.listener(
                move |v, event: &gpui::DragMoveEvent<DraggedInstance>, _, cx| {
                    v.append_library_drag(hover_group.clone(), event, cx)
                },
            ))
            .on_drop(cx.listener(move |v, drag: &DraggedInstance, _, cx| {
                v.drop_library_drag(&drag.id, &drop_group, cx);
            }))
            .child(
                btn(&self.theme, "", ButtonVariant::Ghost)
                    .id(format!("group-collapse:{group}"))
                    .debug_selector({
                        let group = group.clone();
                        move || format!("group-header:{group}").into()
                    })
                    .on_key_down(cx.listener(move |v, event: &gpui::KeyDownEvent, _, cx| {
                        if (event.keystroke.key == "f10" && event.keystroke.modifiers.shift)
                            || event.keystroke.key == "menu"
                        {
                            v.open_group_menu(keyboard_group.clone(), anchor.get(), cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                measured
                                    .set(bounds.origin + gpui::point(px(0.), bounds.size.height))
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .aria_label(format!(
                        "{} group {}",
                        if collapsed { "Expand" } else { "Collapse" },
                        label
                    ))
                    .flex_1()
                    .min_w_0()
                    .justify_start()
                    .child(crate::ui::icon(
                        if collapsed {
                            "chevron-right"
                        } else {
                            "chevron-down"
                        },
                        14.0,
                    ))
                    .child(div().text_xs().text_ellipsis().child(label))
                    .child(crate::ui::badge(
                        &self.theme,
                        &count.to_string(),
                        crate::ui::BadgeKind::Normal,
                    ))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.edit_groups(
                            GroupChange::Collapse {
                                group: click.clone(),
                                collapsed: !v.workspace_ui.collapsed.contains(&click),
                            },
                            cx,
                        )
                    })),
            )
            .when(!group.is_empty(), |row| {
                row.child(
                    btn(&self.theme, "Delete", ButtonVariant::Ghost)
                        .id(format!("group-delete:{group}"))
                        .aria_label(delete_label)
                        .text_xs()
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                            v.open_group_dialog(GroupDialogKind::Delete(delete.clone()), cx)
                        })),
                )
            })
    }
    fn open_group_menu(
        &mut self,
        group: String,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.closing.is_some() || self.dialog.is_some() {
            return;
        }
        self.workspace_ui.menu = Some((super::workspace::MenuTarget::Group(group), position));
        cx.notify();
    }
    pub(super) fn group_menu_items(&self, group: &str, cx: &mut Context<Self>) -> gpui::Div {
        div().flex().flex_col().children(
            [("Create instance", false), ("Rename group", true)]
                .into_iter()
                .map(|(label, rename)| {
                    let target = group.to_owned();
                    let disabled = self.groups_ui.busy || self.closing.is_some();
                    let selector = if rename {
                        "group-menu-rename"
                    } else {
                        "group-menu-create-instance"
                    };
                    btn(&self.theme, label, ButtonVariant::Ghost)
                        .id(selector)
                        .debug_selector(move || selector.into())
                        .role(gpui::accesskit::Role::MenuItem)
                        .tab_stop(!disabled)
                        .w_full()
                        .justify_start()
                        .text_xs()
                        .when(disabled, |button| button.opacity(0.5).cursor_default())
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                            if disabled {
                                return;
                            }
                            if rename {
                                v.open_group_dialog(GroupDialogKind::Rename(target.clone()), cx);
                            } else {
                                v.open_new_instance_in_group(target.clone(), cx);
                            }
                        }))
                }),
        )
    }
    fn apply_groups(&mut self, state: GroupsState) {
        self.workspace_ui.order = state.instance_order;
        self.workspace_ui.collapsed = state.collapsed;
        self.workspace_ui.ungrouped = state.ungrouped_name;
        for instance in &mut self.data.instances {
            instance.group = state
                .instance_groups
                .get(&instance.id)
                .cloned()
                .unwrap_or_default();
        }
    }
    pub(crate) fn refresh_groups(&mut self, cx: &mut Context<Self>) {
        if self.groups_ui.busy {
            return;
        }
        self.groups_ui.generation += 1;
        let generation = self.groups_ui.generation;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { industrialis_backend::groups::groups_state(&paths).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                if generation != v.groups_ui.generation || v.groups_ui.busy {
                    return;
                }
                match result {
                    Ok(Ok(state)) => v.apply_groups(state),
                    Ok(Err(e)) => v.data.status = format!("Could not load groups: {e:#}"),
                    Err(e) => v.data.status = format!("Group task failed: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn edit_groups(&mut self, change: GroupChange, cx: &mut Context<Self>) {
        if self.groups_ui.busy || self.closing.is_some() {
            return;
        }
        self.groups_ui.busy = true;
        self.groups_ui.generation += 1;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result =
                rt.spawn(async move {
                    industrialis_backend::groups::change_groups(&paths, change).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.groups_ui.busy = false;
                match result {
                    Ok(Ok(state)) => {
                        v.apply_groups(state);
                        if matches!(v.dialog, Some(Dialog::Group(_))) {
                            v.dialog = None;
                            v.focused_field = None;
                        }
                        v.data.status = "Groups saved".into();
                    }
                    error => {
                        let error = match error {
                            Ok(Err(e)) => format!("{e:#}"),
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        };
                        if let Some(Dialog::Group(dialog)) = &mut v.dialog {
                            dialog.error = Some(error.clone());
                        }
                        v.data.status = format!("Could not save groups: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn open_group_dialog(&mut self, kind: GroupDialogKind, cx: &mut Context<Self>) {
        if self.groups_ui.busy {
            return;
        }
        let name = match &kind {
            GroupDialogKind::Rename(group) if group.is_empty() => {
                self.workspace_ui.ungrouped.clone()
            }
            GroupDialogKind::Rename(group) => group.clone(),
            GroupDialogKind::Assign(id) => self
                .data
                .selected_instance(Some(id))
                .map(|i| i.group.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.workspace_ui.menu = None;
        self.dialog = Some(Dialog::Group(GroupDialog {
            kind,
            name: FieldState::new(&name),
            error: None,
        }));
        cx.notify();
    }
    pub(super) fn submit_group_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Group(dialog)) = &self.dialog else {
            return;
        };
        let name = dialog.name.text.trim().to_string();
        let change = match &dialog.kind {
            GroupDialogKind::Create => GroupChange::Create(name),
            GroupDialogKind::Rename(old) => GroupChange::Rename {
                old: old.clone(),
                name,
            },
            GroupDialogKind::Assign(id) => GroupChange::Assign {
                id: id.clone(),
                group: name,
            },
            GroupDialogKind::Delete(group) => GroupChange::Delete(group.clone()),
        };
        self.edit_groups(change, cx);
    }
    pub(super) fn group_dialog(
        &mut self,
        dialog: &GroupDialog,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let title = match &dialog.kind {
            GroupDialogKind::Create => "Create group",
            GroupDialogKind::Rename(_) => "Rename group",
            GroupDialogKind::Assign(_) => "Change instance group",
            GroupDialogKind::Delete(_) => "Delete group",
        };
        let mut body = div().flex().flex_col().gap_3();
        if let GroupDialogKind::Delete(group) = &dialog.kind {
            body = body.child(format!(
                "Delete {group}? Its instances will move to {}.",
                self.workspace_ui.ungrouped
            ));
        } else {
            body = body.child(self.field("group-name", "Group name", cx));
            if matches!(dialog.kind, GroupDialogKind::Assign(_)) {
                body=body.child("Enter a name to create a group, or leave it empty to move to the ungrouped section.");
                let mut names: Vec<_> = self
                    .workspace_ui
                    .order
                    .keys()
                    .filter(|g| !g.is_empty())
                    .cloned()
                    .collect();
                names.sort();
                body = body.child(div().flex().flex_wrap().gap_2().children(
                    names.into_iter().map(|name| {
                        let value = name.clone();
                        btn(&self.theme, &name, ButtonVariant::Ghost).on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _, cx| {
                                if let Some(Dialog::Group(dialog)) = &mut v.dialog {
                                    dialog.name = FieldState::new(&value);
                                }
                                cx.notify();
                            },
                        ))
                    }),
                ));
            }
        }
        if let Some(error) = &dialog.error {
            body = body.child(error.clone());
        }
        let footer = if self.groups_ui.busy {
            div().child("Saving groups...")
        } else {
            self.dialog_footer(
                "Cancel",
                "Delete",
                ButtonVariant::Primary,
                |v, cx| v.submit_group_dialog(cx),
                cx,
            )
        };
        self.dialog_frame(title, body, footer, cx)
    }
}
