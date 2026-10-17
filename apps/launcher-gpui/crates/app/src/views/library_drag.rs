//! Live insertion preview; only the final drop writes the shared group order.
use super::{library_groups::DraggedInstance, LauncherRoot};
use gpui::{canvas, div, prelude::*, px, Bounds, Context, Div, DragMoveEvent, Pixels};
use industrialis_backend::{groups::GroupChange, instances::InstanceInfo};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DropTarget {
    pub group: String,
    pub before: Option<String>,
}
#[derive(Default)]
struct Geometry {
    tiles: HashMap<String, Bounds<Pixels>>,
    placeholder: Option<(DropTarget, Bounds<Pixels>)>,
}
#[derive(Default)]
pub(super) struct LibraryDrag {
    pub source: Option<String>,
    pub target: Option<DropTarget>,
    pub focus: Option<gpui::FocusHandle>,
    height: f32,
    geometry: Rc<RefCell<Geometry>>,
}
impl LibraryDrag {
    pub fn reset(&mut self) {
        self.source = None;
        self.target = None;
    }
    pub fn measure_tile(&self, id: String) -> gpui::Canvas<()> {
        let geometry = self.geometry.clone();
        canvas(
            move |bounds, _, _| {
                geometry.borrow_mut().tiles.insert(id, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }
    fn placeholder(
        &self,
        target: DropTarget,
        theme: &crate::theme::ActiveTheme,
    ) -> gpui::Stateful<Div> {
        let geometry = self.geometry.clone();
        div()
            .id("instance-drop-placeholder")
            .debug_selector(|| "instance-drop-placeholder".into())
            .aria_label("Instance drop position")
            .relative()
            .w(px(144.))
            .h(px(self.height.max(117.)))
            .flex_shrink_0()
            .rounded(px(6.))
            .border_2()
            .border_dashed()
            .border_color(theme.primary())
            .bg(crate::ui::tint(theme.primary(), 0.06))
            .child(
                canvas(
                    move |bounds, _, _| {
                        geometry.borrow_mut().placeholder = Some((target, bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }
}
impl LauncherRoot {
    pub(super) fn begin_library_drag(
        &mut self,
        drag: &DraggedInstance,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let geometry = self.workspace_ui.drag.geometry.borrow();
        let height = geometry
            .tiles
            .get(&drag.id)
            .map(|b| f32::from(b.size.height))
            .unwrap_or(117.);
        drop(geometry);
        self.workspace_ui.drag.height = height;
        self.workspace_ui.drag.source = Some(drag.id.clone());
        self.workspace_ui.drag.target = None;
        self.workspace_ui.menu = None;
        // The source tile disappears from the insertion preview. Keep keyboard
        // dispatch rooted in the library so Escape can cancel the gesture.
        if let Some(focus) = &self.workspace_ui.drag.focus {
            window.focus(focus, cx);
        }
        cx.notify();
    }
    pub(super) fn append_library_drag(
        &mut self,
        group: String,
        event: &DragMoveEvent<DraggedInstance>,
        cx: &mut Context<Self>,
    ) {
        if event.bounds.contains(&event.event.position)
            && !self.instance_is_busy(&event.drag(cx).id)
            && !self.groups_ui.busy
        {
            self.set_library_drop(
                Some(DropTarget {
                    group,
                    before: None,
                }),
                cx,
            );
        }
    }
    fn set_library_drop(&mut self, target: Option<DropTarget>, cx: &mut Context<Self>) {
        if self.workspace_ui.drag.target != target {
            self.workspace_ui.drag.target = target;
            cx.notify();
        }
    }
    pub(super) fn leave_library_drag(
        &mut self,
        event: &DragMoveEvent<DraggedInstance>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            self.set_library_drop(None, cx);
        }
    }
    fn track_library_drop(
        &mut self,
        group: String,
        ids: &[String],
        event: &DragMoveEvent<DraggedInstance>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            return;
        }
        let source = event.drag(cx).id.clone();
        if self.instance_is_busy(&source) || self.groups_ui.busy {
            self.set_library_drop(None, cx);
            return;
        }
        let geometry = self.workspace_ui.drag.geometry.borrow();
        // Retain the target while hovering its own slot. Otherwise the shifted
        // neighbors would repeatedly exchange places beneath a stationary pointer.
        if let Some((target, bounds)) = &geometry.placeholder {
            if self.workspace_ui.drag.target.as_ref() == Some(target)
                && target.group == group
                && bounds.contains(&event.event.position)
            {
                return;
            }
        }
        let p = event.event.position;
        let closest = ids
            .iter()
            .enumerate()
            .filter_map(|(i, id)| geometry.tiles.get(id).map(|b| (i, b)))
            .min_by(|(_, a), (_, b)| {
                let distance = |r: &Bounds<Pixels>| {
                    let c = r.center();
                    (f32::from(p.x - c.x)).powi(2) + (f32::from(p.y - c.y)).powi(2)
                };
                distance(a).total_cmp(&distance(b))
            });
        let index = closest
            .map(|(i, bounds)| i + usize::from(p.x > bounds.center().x || p.y > bounds.bottom()))
            .unwrap_or(0);
        let before = ids.iter().skip(index).find(|id| **id != source).cloned();
        drop(geometry);
        self.set_library_drop(Some(DropTarget { group, before }), cx);
    }
    pub(super) fn drop_library_drag(&mut self, id: &str, group: &str, cx: &mut Context<Self>) {
        let target = self.workspace_ui.drag.target.clone();
        self.workspace_ui.drag.reset();
        if !self.instance_is_busy(id) && !self.groups_ui.busy {
            if let Some(target) = target.filter(|t| t.group == group) {
                self.edit_groups(
                    GroupChange::Place {
                        id: id.into(),
                        group: target.group,
                        before: target.before,
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }
    pub(super) fn library_group_grid(
        &self,
        group: &str,
        mut items: Vec<&InstanceInfo>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let preview = self.workspace_ui.drag.target.as_ref();
        let target_here = preview.filter(|t| t.group == group).cloned();
        let source = self.workspace_ui.drag.source.as_deref();
        if target_here.is_some() {
            items.retain(|info| Some(info.id.as_str()) != source);
        }
        let ids: Vec<_> = items.iter().map(|i| i.id.clone()).collect();
        let insertion = target_here.as_ref().map(|t| {
            t.before
                .as_ref()
                .and_then(|before| ids.iter().position(|id| id == before))
                .unwrap_or(ids.len())
        });
        let mut children = Vec::new();
        for index in 0..=items.len() {
            if insertion == Some(index) {
                children.push(
                    self.workspace_ui
                        .drag
                        .placeholder(target_here.clone().unwrap(), &self.theme)
                        .into_any_element(),
                );
            }
            if let Some(info) = items.get(index) {
                // Reserve the source's geometry during cross-group moves so the
                // target group does not jump vertically while the pointer moves.
                children.push(
                    div()
                        .when(
                            preview.is_some() && Some(info.id.as_str()) == source,
                            |tile| tile.opacity(0.25),
                        )
                        .child(self.instance_card(info, cx))
                        .into_any_element(),
                );
            }
        }
        let hover_group = group.to_string();
        let drop_group = group.to_string();
        div()
            .id(format!("instance-group:{group}"))
            .debug_selector({
                let group = group.to_string();
                move || format!("instance-drop-zone:{group}").into()
            })
            .role(gpui::accesskit::Role::ListBox)
            .aria_label(if group.is_empty() {
                self.workspace_ui.ungrouped.clone()
            } else {
                group.into()
            })
            .relative()
            .flex()
            .flex_wrap()
            .gap_2()
            .px_1()
            .pb_1()
            .min_h(px(32.))
            .on_drag_move(
                cx.listener(move |v, event: &DragMoveEvent<DraggedInstance>, _, cx| {
                    v.track_library_drop(hover_group.clone(), &ids, event, cx);
                }),
            )
            .on_drop(cx.listener(move |v, drag: &DraggedInstance, _, cx| {
                v.drop_library_drag(&drag.id, &drop_group, cx);
            }))
            .children(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::{point, MouseButton, TestAppContext, VisualTestContext};

    #[gpui::test]
    async fn dashed_preview_is_stable_and_drop_persists_its_actual_position(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["a", "b", "c"]);
        let (handle, root) = fixture.open(cx, |_| {});
        cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let source = visual.debug_bounds("instance-tile:c").unwrap().center();
            let first = visual.debug_bounds("instance-tile:a").unwrap();
            let target = first.origin + point(px(25.), px(40.));
            visual.simulate_mouse_down(source, MouseButton::Left, Default::default());
            visual.simulate_mouse_move(
                source + point(px(12.), px(10.)),
                MouseButton::Left,
                Default::default(),
            );
            visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
            let ghost = visual
                .debug_bounds("instance-drop-placeholder")
                .expect("Drag must show a landing slot");
            assert_eq!(ghost.origin.x, first.origin.x);
            assert_eq!(ghost.size.width, px(144.));
            assert!(
                root.read_with(&visual, |v, _| !v.groups_ui.busy),
                "Preview must not write order"
            );
            for _ in 0..5 {
                visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
                assert_eq!(
                    visual.debug_bounds("instance-drop-placeholder").unwrap(),
                    ghost
                );
            }
            visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
        }
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        let groups = fixture
            .rt
            .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
            .unwrap();
        assert_eq!(groups.instance_order[""], vec!["c", "a", "b"]);
        cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        assert!(VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("instance-drop-placeholder")
            .is_none());
        fixture.cleanup();
    }
    #[gpui::test]
    async fn drop_into_an_empty_group_moves_only_on_release(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["a", "b"]);
        fixture
            .rt
            .block_on(industrialis_backend::fs_utils::write_json(
                &fixture.paths.groups_path(),
                &serde_json::json!({"formatVersion":1, "groups":{
                    "Main":{"instances":["a","b"]}, "Empty":{"instances":[]}
                }}),
            ))
            .unwrap();
        let (handle, root) = fixture.open(cx, |_| {});
        cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let source = visual.debug_bounds("instance-tile:a").unwrap().center();
            let grid = visual.debug_bounds("instance-drop-zone:Empty").unwrap();
            let destination = grid.origin + point(px(40.), px(15.));
            visual.simulate_mouse_down(source, MouseButton::Left, Default::default());
            visual.simulate_mouse_move(
                source + point(px(12.), px(10.)),
                MouseButton::Left,
                Default::default(),
            );
            visual.simulate_mouse_move(destination, MouseButton::Left, Default::default());
            assert!(visual.debug_bounds("instance-drop-placeholder").is_some());
            let saved = fixture
                .rt
                .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
                .unwrap();
            assert_eq!(saved.instance_order["Main"], vec!["a", "b"]);
            for _ in 0..3 {
                visual.simulate_mouse_move(destination, MouseButton::Left, Default::default());
            }
            visual.simulate_mouse_up(destination, MouseButton::Left, Default::default());
        }
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        let saved = fixture
            .rt
            .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
            .unwrap();
        assert_eq!(saved.instance_order["Empty"], vec!["a"]);
        assert_eq!(saved.instance_order["Main"], vec!["b"]);
        fixture.cleanup();
    }
    #[gpui::test]
    async fn right_half_drop_appends_and_escape_and_outside_drops_do_not_write(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["a", "b", "c"]);
        let (handle, root) = fixture.open(cx, |_| {});
        cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let before = fixture.paths.groups_path();
        for escape in [true, false] {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            // With details closed the library fills the window. Move above its
            // actual bounds instead of assuming the right half is another pane.
            let library = visual.debug_bounds("instance-library").unwrap();
            let outside = point(library.right() - px(24.), library.top() - px(12.));
            let a = visual.debug_bounds("instance-tile:a").unwrap().center();
            let c = visual.debug_bounds("instance-tile:c").unwrap();
            let target = c.center() + point(px(45.), px(0.));
            visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
            visual.simulate_mouse_move(
                a + point(px(12.), px(10.)),
                MouseButton::Left,
                Default::default(),
            );
            visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
            assert!(visual.debug_bounds("instance-drop-placeholder").is_some());
            if escape {
                visual.simulate_keystrokes("escape");
            } else {
                visual.simulate_mouse_move(outside, MouseButton::Left, Default::default());
            }
            assert!(
                visual.debug_bounds("instance-drop-placeholder").is_none(),
                "escape={escape}"
            );
            visual.simulate_mouse_up(outside, MouseButton::Left, Default::default());
            assert!(
                !before.exists(),
                "Cancelled preview must not create the group file"
            );
        }
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let a = visual.debug_bounds("instance-tile:a").unwrap().center();
            let c = visual.debug_bounds("instance-tile:c").unwrap();
            let target = c.center() + point(px(45.), px(0.));
            visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
            visual.simulate_mouse_move(
                a + point(px(12.), px(10.)),
                MouseButton::Left,
                Default::default(),
            );
            visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
            visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
        }
        fixture.until(cx, &root, |v| !v.groups_ui.busy).await;
        let groups = fixture
            .rt
            .block_on(industrialis_backend::groups::groups_state(&fixture.paths))
            .unwrap();
        assert_eq!(groups.instance_order[""], vec!["b", "c", "a"]);
        fixture.cleanup();
    }
}
