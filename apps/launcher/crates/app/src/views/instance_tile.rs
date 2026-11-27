//! Library instance tiles and status indicators.
use super::{
    instance_actions::MaintenanceKind,
    workspace::{display_name, imported_frozen, pack_version, MenuTarget},
    LauncherRoot,
};
use crate::state::format_bytes;
use crate::ui::{btn, icon, mix, progress_ring, spinner, tint, ButtonVariant};
use gpui::{
    div, img, prelude::*, px, Animation, AnimationExt, Context, Div, FontWeight, MouseButton,
    MouseDownEvent,
};
use industrialis_backend::instances::InstanceInfo;
use industrialis_core::{
    background_processes::{Operation, Process, Status},
    pack_version::PackUpdateStatus,
};
use std::path::PathBuf;

pub(super) const TILE_WIDTH: f32 = 176.0;
pub(super) const TILE_HEIGHT: f32 = 148.0;
const TILE_ICON: f32 = 60.0;

pub(super) fn is_context_menu_key(keystroke: &gpui::Keystroke) -> bool {
    (keystroke.key == "f10" && keystroke.modifiers.shift) || keystroke.key == "menu"
}

impl LauncherRoot {
    pub(super) fn instance_avatar(&self, info: &InstanceInfo, size: f32) -> Div {
        let t = &self.theme;
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(px((size * 0.2).round()))
            .overflow_hidden()
            .bg(mix(t.primary(), t.secondary(), 0.1))
            .border_1()
            .border_color(mix(t.primary(), t.border(), 0.22))
            .flex()
            .items_center()
            .justify_center()
            .font_family(crate::input::code_font_family())
            .text_size(px((size * 0.4).round()))
            .font_weight(FontWeight::BOLD)
            .text_color(t.primary())
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
        slot_hint: Option<&str>,
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
            });
        let update = self.pack_status(pack_version(info));
        let update_available =
            !imported_frozen(info) && update.status == PackUpdateStatus::UpdateAvailable;
        let idle_caption = !starting && active.is_none();
        let caption = if starting {
            "Launching…".to_owned()
        } else if let Some((_, process)) = active {
            format!(
                "{} · {}",
                process.operation.label(),
                process.progress_text()
            )
        } else {
            format!("{} · {}", pack_version(info), format_bytes(info.size_bytes))
        };
        let update_label = update_available.then(|| {
            update.latest_version.as_deref().map_or_else(
                || "Pack update available".to_owned(),
                |version| format!("Pack update available: {version}"),
            )
        });
        let drag_owner = cx.entity().downgrade();
        let dragged = super::library_groups::DraggedInstance {
            id: info.id.clone(),
            name: display_name(info),
            theme: t.clone(),
        };
        let keyboard_id = info.id.clone();
        let selector = format!("instance-tile:{}", info.id);
        let hover_bg = mix(t.accent(), t.card(), 0.55);
        let hover_border = t.control_border(t.card());
        let mut tile = div()
            .id(selector.clone())
            .role(gpui::accesskit::Role::ListBoxOption)
            .aria_label(display_name(info))
            .aria_selected(selected)
            .focusable()
            .tab_stop(true)
            .focus_visible(|style| style.border_2().border_color(t.primary()))
            .on_key_down(cx.listener(move |v, event: &gpui::KeyDownEvent, _, cx| {
                if is_context_menu_key(&event.keystroke) {
                    if let Some(bounds) = v.workspace_ui.drag.tile_bounds(&menu_id) {
                        v.workspace_ui.menu =
                            Some((MenuTarget::Instance(menu_id.clone()), bounds.center()));
                        cx.notify();
                        cx.stop_propagation();
                    }
                } else if matches!(event.keystroke.key.as_str(), "enter" | "space")
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
            .w(px(TILE_WIDTH))
            .h(px(TILE_HEIGHT))
            .overflow_hidden()
            .flex_shrink_0()
            .px(px(12.0))
            .pt(px(18.0))
            .pb(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(if selected { t.primary() } else { t.border() })
            .bg(t.card())
            .flex()
            .flex_col()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .when(selected, |el| el.bg(mix(t.primary(), t.card(), 0.06)))
            .when(!selected, |el| {
                el.hover(move |s| s.bg(hover_bg).border_color(hover_border))
            })
            .child(self.tile_icon(info, active, update_label, running, cx))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.card_foreground())
                            .text_center()
                            .w_full()
                            .text_ellipsis()
                            .child(display_name(info)),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(16.0))
                            .font_family(crate::input::code_font_family())
                            .text_size(px(11.0))
                            .text_color(t.muted_foreground())
                            .child(
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(6.0))
                                    .when(!idle_caption, |el| el.text_color(t.primary()))
                                    .when(idle_caption, |el| {
                                        el.group_hover("instance-tile", |s| s.invisible())
                                    })
                                    .child(div().min_w_0().text_ellipsis().child(caption)),
                            )
                            // Hover swaps the caption for a hint, so the menu is
                            // discoverable without a permanent button.
                            .when(idle_caption, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .invisible()
                                        .group_hover("instance-tile", |s| s.visible())
                                        .child("Right-click for options"),
                                )
                            }),
                    ),
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
        if let Some(hint) = slot_hint.filter(|_| idle_caption) {
            let chip = format!("launch-slot:{}", info.id);
            tile = tile.child(
                div()
                    .debug_selector(move || chip.clone().into())
                    .absolute()
                    .top(px(10.0))
                    .left(px(12.0))
                    .font_family(crate::input::code_font_family())
                    .text_size(px(11.0))
                    .text_color(tint(t.muted_foreground(), 0.72))
                    .child(hint.to_string()),
            );
        }
        tile.into_any_element()
    }

    fn tile_icon(
        &self,
        info: &InstanceInfo,
        active: Option<(&str, &Process)>,
        update_label: Option<String>,
        running: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = &self.theme;
        let mut avatar = div()
            .relative()
            .size(px(TILE_ICON))
            .flex_shrink_0()
            .child(self.instance_avatar(info, TILE_ICON));
        let status_badge = if let Some((key, p)) = active {
            let process_key = key.to_owned();
            let selector = format!("instance-progress:{}", info.id);
            let ring_key = format!("instance-progress-ring:{}", info.id);
            let spinner_key = format!("instance-progress-spinner:{}", info.id);
            let glyph = match p.operation {
                Operation::Update | Operation::Install => "download",
                Operation::Reinstall => "refresh-cw",
                Operation::Delete => "trash-2",
                Operation::Copy => "copy",
                Operation::Import => "archive",
            };
            let label = format!(
                "{} {:.0}% — open process",
                p.operation.label(),
                p.pct * 100.0
            );
            Some(
                btn(t, "", ButtonVariant::Ghost)
                    .id(selector.clone())
                    .aria_label(label)
                    .debug_selector(move || selector.clone().into())
                    .absolute()
                    .right(px(-12.0))
                    .top(px(-12.0))
                    .size(px(28.0))
                    .px_0()
                    .gap(px(0.0))
                    .rounded_full()
                    .bg(mix(t.primary(), t.card(), 0.16))
                    .border_color(gpui::rgba(0x00000000))
                    .text_color(t.primary())
                    .child(
                        div()
                            .absolute()
                            .top(px(-4.0))
                            .left(px(-4.0))
                            .size(px(36.0))
                            .rounded_full()
                            .bg(t.primary())
                            .with_animation(
                                format!("instance-progress-halo:{}", info.id),
                                Animation::new(std::time::Duration::from_millis(1600)).repeat(),
                                |halo, phase| {
                                    let pulse = ((phase * std::f32::consts::TAU).sin() + 1.0) / 2.0;
                                    halo.opacity(0.06 + 0.12 * pulse)
                                },
                            ),
                    )
                    .child(spinner(spinner_key, glyph, 15.0))
                    .child(div().absolute().inset_0().child(progress_ring(
                        ring_key,
                        p.pct as f32,
                        tint(t.primary(), 0.28),
                        t.primary(),
                    )))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        cx.stop_propagation();
                        v.select_process(process_key.clone(), cx);
                    }))
                    .into_any_element(),
            )
        } else if let Some(label) = update_label {
            let update_id = info.id.clone();
            let update_selector = format!("instance-update:{}", info.id);
            Some(
                btn(t, "", ButtonVariant::Primary)
                    .id(update_selector.clone())
                    .aria_label(label)
                    .debug_selector(move || update_selector.clone().into())
                    .absolute()
                    .right(px(-10.0))
                    .top(px(-10.0))
                    .size(px(24.0))
                    .px_0()
                    .rounded_full()
                    .bg(t.primary())
                    .border_2()
                    .border_color(t.card())
                    .text_color(t.primary_foreground())
                    .child(icon("download", 12.0))
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        cx.stop_propagation();
                        v.open_maintenance(update_id.clone(), MaintenanceKind::Update, cx);
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };
        if let Some(badge) = status_badge {
            avatar = avatar.child(badge);
        }
        if running {
            avatar = avatar.child(
                // Presence dot on the icon's corner, ringed in the card colour.
                div()
                    .absolute()
                    .right(px(-3.0))
                    .bottom(px(-3.0))
                    .size(px(14.0))
                    .rounded_full()
                    .bg(t.card())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(8.0)).rounded_full().bg(gpui::rgb(0x37d67a))),
            );
        }
        avatar
    }
}
