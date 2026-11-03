//! Background process list and virtualized operation logs.

use super::{LauncherRoot, Tab};
use crate::ui::{
    badge, btn, empty_state, icon, progress_bar, spinner, tint, BadgeKind, ButtonVariant,
};
use gpui::{
    div, prelude::*, px, uniform_list, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    UniformListScrollHandle, Window,
};
use industrialis_core::background_processes::{
    stage_label, Operation, Process, Processes, Status,
};

fn line_count_label(count: usize) -> String {
    format!("{count} line{}", if count == 1 { "" } else { "s" })
}

fn elapsed_label(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs >= 3600 {
        format!(
            "{}h {:02}m {:02}s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    } else if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

#[derive(Default)]
pub(super) struct ProcessesUi {
    pub delete_cancel: std::collections::HashMap<String,std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub import_cancel: std::collections::HashMap<String,std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub history: Processes,
    pub selected: Option<String>,
    pub dropdown: bool,
    scroll: UniformListScrollHandle,
}

impl LauncherRoot {
    pub(super) fn follow_process_output(&self, key: &str) {
        if self.tab == Tab::Processes && self.processes_ui.selected.as_deref() == Some(key) {
            self.processes_ui.scroll.scroll_to_bottom();
        }
    }

    pub(super) fn select_process(&mut self, key: String, cx: &mut Context<Self>) {
        self.processes_ui.selected = Some(key);
        self.processes_ui.dropdown = false;
        self.tab = Tab::Processes;
        self.processes_ui.scroll.scroll_to_bottom();
        cx.notify();
    }
    fn dismiss_process(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.processes_ui.history.dismiss(key)
            && self.processes_ui.selected.as_deref() == Some(key)
        {
            self.processes_ui.selected = None;
        }
        cx.notify();
    }
    fn process_card(&self, key: String, p: &Process, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let selected = self.processes_ui.selected.as_ref() == Some(&key);
        let select = key.clone();
        let description = p
            .logs
            .last()
            .map(|line| {
                line.split_once("] ")
                    .map(|(_, text)| text)
                    .unwrap_or(line.as_str())
            })
            .unwrap_or_else(|| stage_label(&p.stage))
            .to_string();
        let mut card = div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(t.border())
            .bg(if selected {
                tint(t.primary(), 0.1)
            } else {
                gpui::rgba(0x00000000)
            })
            .child(
                crate::ui::button_base("").id(format!("process-row:{key}")).aria_label(p.name.clone()).h_auto().items_stretch().rounded_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .cursor_pointer()
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                            v.select_process(select.clone(), cx)
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(p.status == Status::Running, |el| {
                                el.child(spinner(
                                    format!("process-spinner:{key}"),
                                    "loader-circle",
                                    14.0,
                                ))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(p.name.clone()),
                            )
                            .child(
                                badge(t, &line_count_label(p.logs.len()), BadgeKind::Normal)
                                    .h(px(20.0))
                                    .py_0()
                                    .text_size(px(10.0)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(px(11.0))
                            .text_color(t.muted_foreground())
                            .text_ellipsis()
                            .child(description),
                    )
                    .when(p.status == Status::Running, |el| {
                        el.child(progress_bar(t, p.pct)).child(
                            div()
                                .text_xs()
                                .text_color(t.muted_foreground())
                                .child(p.progress_text()),
                        )
                    }),
            );
        if p.status==Status::Running && p.operation==Operation::Delete {
            let id=p.id.clone();card=card.child(btn(t,"Cancel deletion",ButtonVariant::Outline).on_click(cx.listener(move|v,_:&gpui::ClickEvent,_,cx|{if let Some(cancel)=v.processes_ui.delete_cancel.get(&id){cancel.store(true,std::sync::atomic::Ordering::Release);v.data.status="Cancel requested. Already deleted files cannot be restored.".into();cx.notify();}})));
        }
        if p.status == Status::Running && p.operation == Operation::Import {
            let id = p.id.clone();
            card = card.child(
                btn(t, "Cancel import", ButtonVariant::Outline)
                    .debug_selector(|| "process-cancel-import".into())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if let Some(cancel) = v.processes_ui.import_cancel.get(&id) {
                            cancel.store(true, std::sync::atomic::Ordering::Release);
                            v.data.status =
                                "Cancel requested. The current copy stops and later imports in the batch are skipped."
                                    .into();
                            cx.notify();
                        }
                    })),
            );
        }
        if p.status == Status::Failed && p.operation == Operation::Import {
            card = card.child(
                div().flex().justify_end().px_2().pb(px(6.0)).child(
                    btn(t, "Retry import", ButtonVariant::Outline)
                        .debug_selector(|| "process-retry-import".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            v.open_prism_import(None, false, cx);
                        })),
                ),
            );
        }
        if p.status != Status::Running {
            card = card.child(
                div().flex().justify_end().px_2().pb(px(6.0)).child(
                    btn(t, "", ButtonVariant::Ghost)
                        .id(format!("process-dismiss:{key}"))
                        .aria_label(format!("Dismiss {}", p.name))
                        .h(px(28.0))
                        .px(px(10.0))
                        .text_xs()
                        .child(icon("x", 14.0))
                        .child("Dismiss")
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                v.dismiss_process(&key, cx)
                            }),
                        ),
                ),
            );
        }
        card
    }
    pub(super) fn processes_view(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let items = self.processes_ui.history.sorted();
        let running = self.processes_ui.history.running_count();
        let selected = self
            .processes_ui
            .selected
            .as_deref()
            .and_then(|key| self.processes_ui.history.get(key));
        let summary = if running > 0 {
            format!("{running} running · {} total", items.len())
        } else if !items.is_empty() {
            format!("{} recent", items.len())
        } else {
            "No active or recent processes".into()
        };
        let sidebar = div().w(px(280.0)).max_w(gpui::relative(0.32)).flex_shrink_0().min_h_0().flex().flex_col().border_r_1().border_color(t.border())
            .child(div().px_3().py_2().border_b_1().border_color(t.border()).child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child("Background processes"))
                .child(div().text_size(px(11.0)).text_color(t.muted_foreground()).child(summary)))
            .child(div().id("process-history").flex_1().min_h_0().overflow_y_scroll().children(items.iter().map(|(key,p)| self.process_card((*key).into(),p,cx)))
                .when(items.is_empty(), |el| el.child(div().p_3().text_xs().text_color(t.muted_foreground()).child("Install a pack, update a pack, or delete an instance to see progress here."))));
        let detail = if let Some(p) = selected {
            let instance = p.id.clone();
            let detail_spinner = format!(
                "process-detail-spinner:{}",
                self.processes_ui.selected.as_deref().unwrap_or_default()
            );
            let meta = |label: &'static str, value: String| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child(value),
                    )
            };
            let mut header = div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .border_b_1()
                .border_color(t.border())
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .when(p.status == Status::Running, |el| {
                            el.child(spinner(detail_spinner.clone(), "loader-circle", 16.0))
                        })
                        .child(div().font_weight(FontWeight::MEDIUM).child(p.name.clone()))
                        .child(badge(
                            t,
                            if p.status == Status::Done {
                                "Complete"
                            } else if p.status == Status::Failed {
                                "Failed"
                            } else if p.status == Status::Cancelled {
                                "Cancelled"
                            } else {
                                stage_label(&p.stage)
                            },
                            if p.status == Status::Failed {
                                BadgeKind::Danger
                            } else {
                                BadgeKind::Normal
                            },
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_4()
                        .child(meta("Operation", p.operation.label().to_string()))
                        .child(meta("Elapsed", elapsed_label(p.elapsed())))
                        .child(meta("Instance", p.id.clone())),
                );
            if p.status == Status::Running {
                header = header
                    .child(progress_bar(t, p.pct))
                    .child(div().text_xs().child(p.progress_text()));
            } else if p.status == Status::Done
                && matches!(
                    p.operation,
                    industrialis_core::background_processes::Operation::Update
                        | industrialis_core::background_processes::Operation::Reinstall
                )
                && self.data.selected_instance(Some(&p.id)).is_some()
            {
                header = header.child(
                    btn(t, "View instance", ButtonVariant::Primary)
                        .text_xs()
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                v.tab = Tab::Instances;
                                v.select_instance(instance.clone(), cx);
                            }),
                        ),
                );
            }
            let lines = p.logs.clone();
            let copy_text = lines.join("\n");
            let line_count = lines.len();
            let error_color = t.destructive();
            let normal_color = t.muted_foreground();
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .child(header)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(tint(t.border(), 0.5))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Log"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(t.muted_foreground())
                                        .child(line_count_label(line_count)),
                                ),
                        )
                        .child(
                            btn(t, "Copy log", ButtonVariant::Ghost)
                                .debug_selector(|| "process-copy-log".into())
                                .h(px(24.0))
                                .px(px(8.0))
                                .text_xs()
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(
                                        gpui::ClipboardItem::new_string(copy_text.clone()),
                                    )
                                }),
                        ),
                )
                .child(
                    uniform_list(
                        "background-process-log",
                        line_count,
                        move |range, _, _| {
                            range
                                .map(|index| {
                                    let line = &lines[index];
                                    let (stamp, text) = line
                                        .split_once("] ")
                                        .map(|(stamp, text)| (format!("{stamp}]"), text))
                                        .unwrap_or_else(|| (String::new(), line.as_str()));
                                    let is_error = text.contains("Error:");
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_3()
                                        .h(px(20.0))
                                        .whitespace_nowrap()
                                        .text_size(px(11.0))
                                        .font_family("Consolas")
                                        .child(
                                            div()
                                                .w(px(68.0))
                                                .flex_shrink_0()
                                                .text_right()
                                                .text_color(tint(normal_color, 0.55))
                                                .child(stamp),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_ellipsis()
                                                .text_color(if is_error {
                                                    error_color
                                                } else {
                                                    normal_color
                                                })
                                                .child(text.to_string()),
                                        )
                                })
                                .collect()
                        },
                    )
                    .track_scroll(&self.processes_ui.scroll)
                    .flex_1()
                    .min_h_0()
                    .bg(gpui::rgba(0x00000066)),
                )
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state(
                    t,
                    if items.is_empty() {
                        "Process logs will appear here."
                    } else {
                        "Select a process to view its full log."
                    },
                ))
        };
        div().flex().flex_1().min_h_0().child(sidebar).child(detail)
    }
    pub(super) fn processes_dropdown(&self, cx: &mut Context<Self>) -> Option<gpui::Deferred> {
        if !self.processes_ui.dropdown {
            return None;
        }
        let t = &self.theme;
        let running = self.processes_ui.history.running_count();
        let summary = if running > 0 {
            format!("{running} running")
        } else {
            "Recent processes".into()
        };
        let body = div()
            .absolute()
            .inset_0()
            .on_mouse_down(MouseButton::Left,cx.listener(|v, _: &MouseDownEvent, _: &mut Window, cx| {
                    v.processes_ui.dropdown = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .right(px(212.0))
                    .top(px(40.0))
                    .w(px(288.0))
                    .p_1()
                    .max_h(gpui::relative(0.7))
                    .bg(t.popover())
                    .border_1()
                    .border_color(t.border())
                    .rounded_md()
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_2()
                            .py_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(t.muted_foreground())
                                    .child(summary),
                            )
                            .child(
                                btn(t, "View all", ButtonVariant::Ghost)
                                    .h(px(24.0))
                                    .text_xs()
                                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                            v.processes_ui.dropdown = false;
                                            v.tab = Tab::Processes;
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("processes-popup")
                            .max_h(px(300.0))
                            .overflow_y_scroll()
                            .children(
                                self.processes_ui
                                    .history
                                    .sorted()
                                    .into_iter()
                                    .map(|(key, p)| self.process_card(key.into(), p, cx)),
                            ),
                    ),
            );
        Some(gpui::deferred(body).with_priority(20))
    }
}
