//! Persisted console loading, copy/crash-copy, and guarded clearing.

use super::LauncherRoot;
use crate::state::LogLine;
use crate::ui::{btn, ButtonVariant};
use gpui::{
    div, prelude::*, px, uniform_list, ClipboardItem, Context, Div, Window,
};
use industrialis_backend::console_log::{clear_console_log, read_console_log};
use industrialis_core::launch_log::{
    classify_launch_log_line, extract_latest_crash_lines, format_launch_log, LaunchLogLevel,
    LaunchLogLine, LogStream, CRASH_EXCERPT_MAX_LINES,
};
use std::collections::HashMap;

#[derive(Default)]
struct ConsoleState {
    generation: u64,
    loading: bool,
    copying: bool,
    clearing: bool,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Default)]
pub(super) struct ConsoleUi {
    states: HashMap<String, ConsoleState>,
}

fn core_lines(lines: &[LogLine]) -> Vec<LaunchLogLine> {
    lines
        .iter()
        .map(|entry| LaunchLogLine {
            stream: match entry.stream.as_str() {
                "stderr" => LogStream::Stderr,
                "system" => LogStream::System,
                _ => LogStream::Stdout,
            },
            line: entry.line.clone(),
        })
        .collect()
}

impl LauncherRoot {
    pub(super) fn invalidate_console(&mut self, id: &str) {
        let state = self.console_ui.states.entry(id.into()).or_default();
        state.generation += 1;
        state.loading = false;
        state.error = None;
        state.message = None;
    }
    pub(super) fn append_console_lines(&mut self, id: &str, lines: Vec<LogLine>) {
        if !lines.is_empty() {
            self.invalidate_console(id);
            self.data.push_logs(id, lines);
        }
    }
    pub(super) fn select_instance(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected_id = Some(id.clone());
        self.log_scroll.scroll_to_bottom();
        self.focused_field = None;
        self.load_console(id, cx);
        cx.notify();
    }
    fn load_console(&mut self, id: String, cx: &mut Context<Self>) {
        let state = self.console_ui.states.entry(id.clone()).or_default();
        if state.loading {
            return;
        }
        state.generation += 1;
        let request = state.generation;
        state.loading = true;
        state.error = None;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let target = id.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { read_console_log(&paths, &target, false).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let Some(state) = v
                    .console_ui
                    .states
                    .get_mut(&id)
                    .filter(|s| s.generation == request)
                else {
                    return;
                };
                state.loading = false;
                match result {
                    Ok(Ok(lines)) => {
                        if !v.data.is_running(&id) || v.data.logs.get(&id).is_none_or(Vec::is_empty)
                        {
                            v.data.logs.insert(
                                id.clone(),
                                lines
                                    .into_iter()
                                    .map(|e| LogLine {
                                        stream: match e.stream {
                                            LogStream::Stdout => "stdout",
                                            LogStream::Stderr => "stderr",
                                            LogStream::System => "system",
                                        }
                                        .into(),
                                        line: e.line,
                                    })
                                    .collect(),
                            );
                            if v.selected_id.as_ref() == Some(&id) {
                                v.log_scroll.scroll_to_bottom();
                            }
                        }
                    }
                    failure => {
                        state.error = Some(match failure {
                            Ok(Err(e)) => format!("Could not load console: {e:#}"),
                            Err(e) => format!("Console task failed: {e}"),
                            _ => unreachable!(),
                        })
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn copy_console(&mut self, id: String, crash: bool, cx: &mut Context<Self>) {
        let state = self.console_ui.states.entry(id.clone()).or_default();
        if state.copying || state.clearing {
            return;
        }
        state.copying = true;
        state.error = None;
        state.message = None;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let target = id.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { read_console_log(&paths, &target, true).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                let mut source = match result {
                    Ok(Ok(lines)) => lines,
                    failure => {
                        let state = v.console_ui.states.entry(id.clone()).or_default();
                        state.copying = false;
                        state.error = Some(match failure {
                            Ok(Err(e)) => format!("Copy failed: {e:#}"),
                            Err(e) => format!("Copy failed: {e}"),
                            _ => unreachable!(),
                        });
                        cx.notify();
                        return;
                    }
                };
                if source.is_empty() {
                    source =
                        core_lines(v.data.logs.get(&id).map(Vec::as_slice).unwrap_or_default());
                }
                if crash {
                    source = extract_latest_crash_lines(&source, CRASH_EXCERPT_MAX_LINES);
                }
                let text = format_launch_log(&source);
                let fallback = crash
                    && !source
                        .iter()
                        .any(|e| classify_launch_log_line(e) == LaunchLogLevel::Error);
                let state = v.console_ui.states.entry(id.clone()).or_default();
                state.copying = false;
                if text.is_empty() {
                    state.error = Some("Copy failed: no console output available.".into());
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    state.message = Some(
                        if fallback {
                            "Launch copied"
                        } else if crash {
                            "Crash copied"
                        } else {
                            "Copied"
                        }
                        .into(),
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn clear_console(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        self.invalidate_console(&id);
        let state = self.console_ui.states.get_mut(&id).unwrap();
        if state.copying || state.clearing {
            return;
        }
        state.clearing = true;
        self.busy.insert(format!("console:{id}"));
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let target = id.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { clear_console_log(&paths, &target).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove(&format!("console:{id}"));
                let state = v.console_ui.states.entry(id.clone()).or_default();
                state.clearing = false;
                match result {
                    Ok(Ok(())) => {
                        v.data.logs.remove(&id);
                        state.message = Some("Console cleared".into());
                    }
                    failure => {
                        state.error = Some(match failure {
                            Ok(Err(e)) => format!("Clear failed: {e:#}"),
                            Err(e) => format!("Clear failed: {e}"),
                            _ => unreachable!(),
                        })
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn console_view(&self, id: &str, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let lines = self.data.logs.get(id).cloned().unwrap_or_default();
        let state = self.console_ui.states.get(id);
        let loading = state.is_some_and(|s| s.loading);
        let copying = state.is_some_and(|s| s.copying);
        let clearing = state.is_some_and(|s| s.clearing);
        let blocked = copying || clearing || loading || lines.is_empty();
        let mut controls =
            div()
                .flex()
                .flex_wrap()
                .gap(px(2.0))
                .px_3()
                .pb(px(2.0))
                .children([("Copy", false), ("Copy crash", true)].into_iter().map(
                    |(label, crash)| {
                        let id = id.to_string();
                        let feedback =
                            state
                                .and_then(|state| state.message.as_deref())
                                .filter(|message| {
                                    if crash {
                                        *message == "Crash copied" || *message == "Launch copied"
                                    } else {
                                        *message == "Copied"
                                    }
                                });
                        btn(
                            t,
                            if copying {
                                "Preparing…"
                            } else {
                                feedback.unwrap_or(label)
                            },
                            ButtonVariant::Ghost,
                        )
                        .h(px(28.0))
                        .px(px(10.0))
                        .text_xs()
                        .when(blocked, |el| el.opacity(0.5).cursor_default())
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                if !blocked {
                                    v.copy_console(id.clone(), crash, cx);
                                }
                            }),
                        )
                    },
                ));
        let id_click = id.to_string();
        let clear_blocked = blocked || self.instance_is_busy(id);
        controls = controls.child(
            btn(
                t,
                if clearing { "Clearing…" } else { "Clear" },
                ButtonVariant::Ghost,
            )
            .h(px(28.0))
            .px(px(10.0))
            .text_xs()
            .when(clear_blocked, |el| el.opacity(0.5).cursor_default())
            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                    if !clear_blocked {
                        let target = id_click.clone();
                        v.after_instance_settings_saved(
                            id_click.clone(),
                            move |v, cx| v.clear_console(target, cx),
                            cx,
                        );
                    }
                }),
            ),
        );
        let mut panel = div().flex_1().min_h_0().flex().flex_col().child(controls);
        if loading {
            panel = panel.child(div().text_xs().child("Loading saved console…"));
        }
        if let Some(error) = state.and_then(|s| s.error.clone()) {
            panel = panel.child(div().text_xs().text_color(t.destructive()).child(error));
        }
        if lines.len() >= industrialis_core::log_buffer::MAX_RETAINED_LOG_LINES {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("Latest 5,000 lines retained"),
            );
        }
        let empty = lines.is_empty();
        panel.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .px_3()
                .py_2()
                .bg(gpui::rgba(0x00000099))
                .when(empty, |el| {
                    el.child(
                        div()
                            .text_size(px(11.0))
                            .font_family("Consolas")
                            .text_color(t.muted_foreground())
                            .child("No log output yet."),
                    )
                })
                .child(
                    uniform_list("console-log", lines.len(), move |range, _, _| {
                        range
                            .map(|index| {
                                LauncherRoot::log_line_div(&lines[index]).font_family("Consolas")
                            })
                            .collect()
                    })
                    .track_scroll(&self.log_scroll)
                    .flex_1()
                    .min_h_0()
                    .text_size(px(11.0)),
                ),
        )
    }
}
