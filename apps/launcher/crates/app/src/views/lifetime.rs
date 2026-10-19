use super::*;
use crate::ui::btn;

impl LauncherRoot {
    pub(crate) fn request_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            if self.closing.is_none() && self.close_allowed(cx) {
                crate::desktop_window::hide(window, cx);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            self.request_close(cx);
        }
    }
    pub(crate) fn request_close(&mut self, cx: &mut Context<Self>) {
        if self.closing.is_some() || !self.close_allowed(cx) {
            return;
        }
        self.begin_close(cx);
    }
    pub(super) fn begin_close(&mut self, cx: &mut Context<Self>) {
        self.closing = Some(None);
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            // A maintenance workflow can contain copy followed by update. Allow
            // the whole workflow to finish before closing backend admission.
            loop {
                let ready = match view.update(cx, |v, _| {
                    v.busy.is_empty()
                        && v.data.launching.is_empty()
                        && !v.groups_ui.busy
                        && !v.icons_ui.busy
                }) {
                    Ok(ready) => ready,
                    Err(_) => return,
                };
                if ready {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
            let result = rt
                .spawn(async move {
                    industrialis_backend::lifecycle::for_profile(&paths)
                        .shutdown(&paths)
                        .await
                })
                .await;
            let _ = view.update(cx, |v, cx| match result {
                Ok(Ok(())) => {
                    crate::native_quit::allow_after_drain();
                    cx.quit();
                }
                error => {
                    let message = match error {
                        Ok(Err(e)) => format!("{e:#}"),
                        Err(e) => e.to_string(),
                        _ => unreachable!(),
                    };
                    v.closing = Some(Some(message));
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn closing_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut view = div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .p_6()
            .bg(self.theme.window_background())
            .text_color(self.theme.foreground())
            .child("Closing launcher");
        if let Some(Some(error)) = &self.closing {
            view = view
                .child(format!("Could not finish saving: {error}"))
                .child(
                    btn(&self.theme, "Retry", ButtonVariant::Primary)
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.begin_close(cx))),
                );
        } else {
            view = view.child(
                "Finishing file operations and saving game logs. Running games will stay open.",
            );
        }
        view
    }
    pub(crate) fn monitor_restored_games(&mut self, cx: &mut Context<Self>) {
        let games: Vec<_> = self
            .data
            .running
            .iter()
            .map(|(id, (pid, _))| (id.clone(), *pid))
            .collect();
        for (id, pid) in games {
            if !self.data.instances.iter().any(|instance| instance.id == id) {
                continue;
            }
            self.restoring.insert(id.clone());
            let paths = self.data.paths.clone();
            let rt = self.data.rt.clone();
            let lines: Arc<Mutex<Vec<LogLine>>> = Default::default();
            let output = lines.clone();
            let emit: EmitLaunchLog = Arc::new(move |_, stream, line| {
                output.lock().unwrap().push(LogLine { stream, line })
            });
            cx.spawn(async move |view, cx| {
                let target = id.clone();
                let work = rt.spawn(async move {
                    industrialis_backend::launch::monitor_restored_game(&paths, &target, emit).await
                });
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let drained = std::mem::take(&mut *lines.lock().unwrap());
                    if view
                        .update(cx, |v, cx| {
                            v.append_console_lines(&id, drained);
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    if work.is_finished() {
                        break;
                    }
                }
                let result = work.await;
                let drained = std::mem::take(&mut *lines.lock().unwrap());
                let _ = view.update(cx, |v, cx| {
                    v.append_console_lines(&id, drained);
                    v.restoring.remove(&id);
                    match result {
                        Ok(Ok(true)) => {
                            if v.data
                                .running
                                .get(&id)
                                .is_some_and(|(current, _)| *current == pid)
                                && !v.data.owned_running.contains_key(&id)
                            {
                                v.data.running.remove(&id);
                            }
                            if v.closing.is_none() {
                                v.refresh(cx);
                            }
                        }
                        Ok(Ok(false)) => {}
                        Ok(Err(e)) => v.data.status = format!("Could not recover {id}: {e:#}"),
                        Err(e) => v.data.status = format!("Game recovery task failed: {e}"),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
}
