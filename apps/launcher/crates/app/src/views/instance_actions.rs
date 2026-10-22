//! Instance maintenance dialogs and a single ordered progress/completion stream.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, Context, Div, Window};
use industrialis_backend::install::{preview_update, reinstall_instance, update_instance};
use industrialis_backend::instance_ops::copy_instance_with_progress;
use industrialis_backend::pack_files::{
    fetch_gtnh_versions, EmitProgress, PackProgress, UpdateModPreview,
};
use industrialis_backend::paths::validate_instance_id;
use industrialis_core::background_processes::{Operation, ProcessToken, Processes};

use super::{Dialog, LauncherRoot};
use crate::input::FieldState;
use crate::ui::{btn, progress_bar, ButtonVariant};

/// Both copy workflows notify only after a current worker finishes successfully.
/// Progress (including 100%) never completes a run or plays a notification.
fn finish_process(
    history: &mut Processes,
    token: &ProcessToken,
    error: Option<&str>,
    on_copy_complete: impl FnOnce(),
) {
    if history.finish(token, error)
        && error.is_none()
        && history
            .get(&token.key)
            .is_some_and(|process| process.operation == Operation::Copy)
    {
        on_copy_complete();
    }
}

#[cfg(test)]
mod completion_sound_tests {
    use super::*;
    use industrialis_core::background_processes::Status;

    #[test]
    fn actual_copy_notifies_once_after_finalization_and_failure_stays_silent() {
        let fixture = super::super::test_support::Fixture::new(&["source"]);
        let source = fixture
            .paths
            .instance_dir("source")
            .unwrap()
            .join(".minecraft/config/copy.cfg");
        std::fs::write(&source, b"copy notification acceptance").unwrap();
        let progress = Arc::new(Mutex::new(Vec::new()));
        let collected = progress.clone();
        let emit: EmitProgress = Arc::new(move |p| collected.lock().unwrap().push(p));
        let mut history = Processes::default();
        let token = history.start(Operation::Copy, "destination", "Copied source");
        let result = fixture.rt.block_on(copy_instance_with_progress(
            &fixture.paths,
            "source",
            "destination",
            "Copied source",
            emit.clone(),
        ));
        assert!(result.is_ok(), "{result:?}");
        assert!(progress.lock().unwrap().iter().any(|p| p.stage == "done"));
        for p in progress.lock().unwrap().iter() {
            history.progress(
                &token, &p.stage, p.pct, p.log_line.clone(), (None, None, None),
            );
        }
        assert_eq!(history.get(&token.key).unwrap().status, Status::Running);
        let copied = fixture
            .paths
            .instance_dir("destination")
            .unwrap()
            .join(".minecraft/config/copy.cfg");
        let mut cues = 0;
        finish_process(&mut history, &token, None, || {
            assert_eq!(
                std::fs::read(&copied).unwrap(),
                std::fs::read(&source).unwrap(),
            );
            assert!(fixture.paths.settings_path("destination").unwrap().is_file());
            cues += 1;
        });
        finish_process(&mut history, &token, None, || cues += 1);
        history.progress(&token, "done", 1.0, None, (None, None, None));
        assert_eq!(cues, 1);
        assert_eq!(history.get(&token.key).unwrap().status, Status::Done);

        // An existing destination is rejected by the real copy worker.
        let retry = history.start(Operation::Copy, "destination", "Collision");
        let failed = fixture
            .rt
            .block_on(copy_instance_with_progress(
                &fixture.paths,
                "source",
                "destination",
                "Collision",
                emit,
            ))
            .unwrap_err()
            .to_string();
        finish_process(&mut history, &retry, Some(&failed), || cues += 1);
        finish_process(&mut history, &retry, None, || cues += 1);
        assert_eq!(history.get(&retry.key).unwrap().status, Status::Failed);
        assert_eq!(cues, 1);
        fixture.cleanup();
    }

    #[test]
    fn progress_stale_retries_and_dismissed_runs_do_not_notify() {
        let mut history = Processes::default();
        let old = history.start(Operation::Copy, "copy", "First attempt");
        history.progress(&old, "done", 1.0, None, (None, None, None));
        history.progress(&old, "done", 1.0, None, (None, None, None));
        assert_eq!(history.get(&old.key).unwrap().status, Status::Running);
        let current = history.start(Operation::Copy, "copy", "Retry");
        let mut cues = 0;
        finish_process(&mut history, &old, None, || cues += 1);
        assert_eq!(cues, 0);
        assert_eq!(history.get(&current.key).unwrap().status, Status::Running);
        finish_process(&mut history, &current, None, || cues += 1);
        finish_process(&mut history, &current, Some("late failure"), || cues += 1);
        assert!(history.dismiss(&current.key));
        finish_process(&mut history, &current, None, || cues += 1);
        let next = history.start(Operation::Copy, "copy", "Next copy");
        finish_process(&mut history, &next, None, || cues += 1);
        assert_eq!(cues, 2, "one cue for each successful current run");
    }

    #[test]
    fn copy_before_maintenance_notifies_even_when_later_maintenance_fails() {
        for operation in [Operation::Update, Operation::Reinstall] {
            let mut history = Processes::default();
            let copy = history.start(Operation::Copy, "destination", "Safety copy");
            let mut cues = 0;
            finish_process(&mut history, &copy, None, || cues += 1);
            let maintenance = history.start(operation, "destination", "Maintenance");
            finish_process(&mut history, &copy, Some("maintenance failed"), || {
                cues += 1
            });
            finish_process(&mut history, &maintenance, Some("maintenance failed"), || {
                cues += 1
            });
            assert_eq!(cues, 1);
            assert_eq!(history.get(&copy.key).unwrap().status, Status::Done);
            assert_eq!(history.get(&maintenance.key).unwrap().status, Status::Failed);
        }
    }

    #[test]
    fn other_successful_operations_and_failed_safety_copies_stay_silent() {
        let mut history = Processes::default();
        let mut cues = 0;
        for operation in [
            Operation::Install,
            Operation::Update,
            Operation::Reinstall,
            Operation::Delete,
        ] {
            let token = history.start(operation, "instance", "Other operation");
            finish_process(&mut history, &token, None, || cues += 1);
        }
        let copy = history.start(Operation::Copy, "destination", "Safety copy");
        finish_process(&mut history, &copy, Some("copy failed"), || cues += 1);
        finish_process(&mut history, &copy, Some("overall operation failed"), || cues += 1);
        assert_eq!(cues, 0);
    }
}

#[derive(Debug, Clone)]
pub(super) struct CopyDialog {
    pub id: String,
    pub name: FieldState,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MaintenanceKind {
    Update,
    Reinstall,
}

use super::pack_catalog::VersionChoice;

#[derive(Debug, Clone)]
pub(super) struct MaintenanceDialog {
    token: u64,
    id: String,
    kind: MaintenanceKind,
    current: String,
    version: String,
    java: String,
    versions: Vec<VersionChoice>,
    pub(super) search: FieldState,
    filter: super::pack_catalog::ReleaseFilter,
    loading: bool,
    analyzing: bool,
    preview: Option<UpdateModPreview>,
    keep: HashSet<String>,
    copy_before: bool,
    error: Option<String>,
}

#[derive(Clone)]
enum Work {
    Install {
        name: String,
        group: String,
        version: String,
        java: String,
    },
    Delete,
    Copy {
        name: String,
        destination: String,
    },
    Preview {
        version: String,
        java: String,
        token: u64,
    },
    Maintain {
        kind: MaintenanceKind,
        version: String,
        java: String,
        keep: Vec<String>,
        copy: Option<(String, String)>,
    },
}

impl MaintenanceDialog {
    pub(super) fn is_analyzing(&self) -> bool {
        self.analyzing
    }
}

impl LauncherRoot {
    pub(super) fn instance_is_busy(&self, id: &str) -> bool {
        self.closing.is_some()
            || self.restoring.contains(id)
            || self.data.is_running(id)
            || self
                .busy
                .iter()
                .any(|key| key.split_once(':').is_some_and(|(_, target)| target == id))
    }

    pub(super) fn open_copy(&mut self, id: String, cx: &mut Context<Self>) {
        let name = self
            .data
            .selected_instance(Some(&id))
            .map(|i| i.settings.name.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&id);
        let name = FieldState::new(&format!("{name} (copy)"));
        self.dialog = Some(Dialog::CopyInstance(CopyDialog {
            id,
            name,
            error: None,
        }));
        self.focused_field = Some("copy-name".into());
        cx.notify();
    }

    fn unused_copy_id(&self, name: &str) -> anyhow::Result<String> {
        let base = validate_instance_id(name)?;
        for n in 0..10_000 {
            let candidate = if n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let path = self.data.paths.instance_dir(&candidate)?;
            if !path.try_exists()? && !self.instance_is_busy(&candidate) {
                return Ok(candidate);
            }
        }
        anyhow::bail!("Could not find an available copy name")
    }

    pub(super) fn submit_copy(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::CopyInstance(state)) = self.dialog.clone() else {
            return;
        };
        if self.instance_is_busy(&state.id) {
            return;
        }
        let name = state.name.text.trim().to_string();
        match self.unused_copy_id(&name) {
            Ok(destination) => {
                self.close_dialog(cx);
                self.run_instance_work(state.id, Work::Copy { name, destination }, cx);
            }
            Err(error) => {
                if let Some(Dialog::CopyInstance(d)) = &mut self.dialog {
                    d.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    pub(super) fn copy_dialog(&mut self, state: &CopyDialog, cx: &mut Context<Self>) -> Div {
        let mut body = div().flex().flex_col().gap_2().child("Name for the copied instance")
            .child(self.field("copy-name", "Instance name", cx))
            .child(div().text_xs().text_color(self.theme.muted_foreground()).child("Copies pack files, saves, custom mods, and settings. The original stays intact."));
        if let Some(error) = &state.error {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(self.theme.destructive())
                    .child(error.clone()),
            );
        }
        let footer = self.dialog_footer(
            "Cancel",
            "Copy instance",
            ButtonVariant::Primary,
            |v, cx| v.submit_copy(cx),
            cx,
        );
        self.dialog_frame("Copy instance", body, footer, cx)
    }

    pub(super) fn open_maintenance(
        &mut self,
        id: String,
        kind: MaintenanceKind,
        cx: &mut Context<Self>,
    ) {
        static TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let Some(info) = self.data.selected_instance(Some(&id)) else {
            return;
        };
        let token = TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.dialog = Some(Dialog::Maintenance(MaintenanceDialog {
            token,
            id,
            kind,
            current: info.settings.pack_version.clone(),
            version: info.settings.pack_version.clone(),
            java: info.settings.pack_java_type.clone(),
            versions: vec![],
            search: FieldState::empty(),
            filter: Default::default(),
            loading: true,
            analyzing: false,
            preview: None,
            keep: HashSet::new(),
            copy_before: false,
            error: None,
        }));
        self.focused_field = None;
        cx.notify();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(fetch_gtnh_versions()).await;
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::Maintenance(d)) = &mut v.dialog else {
                    return;
                };
                if d.token != token {
                    return;
                }
                d.loading = false;
                match result {
                    Ok(Ok(catalog)) => {
                        d.versions = super::pack_catalog::choices(&catalog);
                        if d.kind == MaintenanceKind::Update {
                            if let Some(first) = d.versions.iter().find(|v| v.java8 || v.java17) {
                                d.version = first.name.clone();
                            }
                        }
                        if let Some(version) = d.versions.iter().find(|v| v.name == d.version) {
                            if !version.java17 {
                                d.java = "java8".into();
                            } else if !version.java8 {
                                d.java = "java17+".into();
                            }
                        }
                    }
                    failure => {
                        d.error = Some(match failure {
                            Ok(Err(e)) => format!("{e:#}"),
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        })
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn selected_pack_available(d: &MaintenanceDialog) -> bool {
        d.versions
            .iter()
            .find(|v| v.name == d.version)
            .is_some_and(|v| if d.java == "java8" { v.java8 } else { v.java17 })
    }

    fn submit_maintenance(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Maintenance(d)) = self.dialog.clone() else {
            return;
        };
        if d.loading
            || d.analyzing
            || self.instance_is_busy(&d.id)
            || !Self::selected_pack_available(&d)
        {
            return;
        }
        if d.kind == MaintenanceKind::Update && d.preview.is_none() {
            if let Some(Dialog::Maintenance(state)) = &mut self.dialog {
                state.analyzing = true;
                state.error = None;
            }
            self.run_instance_work(
                d.id,
                Work::Preview {
                    version: d.version,
                    java: d.java,
                    token: d.token,
                },
                cx,
            );
            return;
        }
        let copy = if d.copy_before {
            let name = self
                .data
                .selected_instance(Some(&d.id))
                .map(|i| i.settings.name.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(&d.id);
            let name = format!("{name} (copy)");
            match self.unused_copy_id(&name) {
                Ok(id) => Some((id, name)),
                Err(e) => {
                    if let Some(Dialog::Maintenance(state)) = &mut self.dialog {
                        state.error = Some(e.to_string());
                    }
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        self.close_dialog(cx);
        self.run_instance_work(
            d.id,
            Work::Maintain {
                kind: d.kind,
                version: d.version,
                java: d.java,
                keep: d.keep.into_iter().collect(),
                copy,
            },
            cx,
        );
    }

    pub(super) fn maintenance_dialog(
        &mut self,
        d: &MaintenanceDialog,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme.clone();
        let t = &theme;
        let title = if d.kind == MaintenanceKind::Update {
            "Update pack"
        } else {
            "Clean reinstall"
        };
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .text_sm()
                .child(format!("Current pack: {}", d.current)),
        );
        if d.loading {
            body = body.child("Loading pack versions…");
        } else if d.analyzing {
            body = body.child("Analyzing pack and custom mods…");
            if let Some(p) = self.operation_progress.get(&d.id) {
                body = body.child(progress_bar(t, p.pct)).child(
                    div()
                        .text_xs()
                        .child(p.log_line.clone().unwrap_or_else(|| p.stage.clone())),
                );
            }
        } else if let Some(preview) = &d.preview {
            body = body
                .child(
                    div()
                        .text_sm()
                        .child(format!("{} → {}", d.current, d.version)),
                )
                .child(div().text_xs().child(format!(
                    "Pack mods: {} new, {} changed, {} removed",
                    preview.new_pack_mods_count,
                    preview.updated_pack_mods_count,
                    preview.removed_from_pack_count
                )))
                .child(div().text_sm().child("Custom mods to keep"))
                .child(
                    div()
                        .id("update-custom-mods")
                        .max_h(px(180.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(preview.custom_mods.iter().map(|entry| {
                            let identity = entry.entry.identity.clone();
                            let checked = d.keep.contains(&identity);
                            btn(
                                t,
                                &format!(
                                    "{} {}",
                                    if checked { "☑" } else { "☐" },
                                    entry.entry.filename
                                ),
                                ButtonVariant::Ghost,
                            )
                            .text_xs()
                            .on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                    if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                                        if !d.keep.remove(&identity) {
                                            d.keep.insert(identity.clone());
                                        }
                                        cx.notify();
                                    }
                                },
                            ))
                        })),
                );
            if preview.custom_mods.is_empty() {
                body = body.child(div().text_xs().child("No custom mods found."));
            }
            body = body.child(
                btn(t, "Change version", ButtonVariant::Ghost)
                    .text_xs()
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                        if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                            d.preview = None;
                            cx.notify();
                        }
                    })),
            );
        } else {
            body = body
                .child(div().text_sm().child("Pack version"))
                .child(self.field("maintenance-search", "Search versions", cx))
                .child(
                    div().flex().gap_2().children(
                        [
                            (super::pack_catalog::ReleaseFilter::All, "All"),
                            (super::pack_catalog::ReleaseFilter::Stable, "Stable"),
                            (super::pack_catalog::ReleaseFilter::Beta, "Beta"),
                        ]
                        .into_iter()
                        .map(|(filter, label)| {
                            btn(
                                t,
                                label,
                                if d.filter == filter {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                },
                            )
                            .text_xs()
                            .on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _, cx| {
                                    if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                                        d.filter = filter;
                                    }
                                    cx.notify();
                                },
                            ))
                        }),
                    ),
                )
                .child(
                    div()
                        .id("maintenance-versions")
                        .max_h(px(160.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(
                            d.versions
                                .iter()
                                .filter(|version| {
                                    d.filter.matches(version)
                                        && version
                                            .name
                                            .to_lowercase()
                                            .contains(&d.search.text.to_lowercase())
                                })
                                .map(|version| {
                                    let name = version.name.clone();
                                    let java8 = version.java8;
                                    let java17 = version.java17;
                                    btn(
                                        t,
                                        &version.name,
                                        if version.name == d.version {
                                            ButtonVariant::Secondary
                                        } else {
                                            ButtonVariant::Ghost
                                        },
                                    )
                                    .text_xs()
                                    .on_click(cx.listener(
                                        move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                            if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                                                d.version = name.clone();
                                                d.error = None;
                                                if !java17 {
                                                    d.java = "java8".into();
                                                } else if !java8 {
                                                    d.java = "java17+".into();
                                                }
                                                cx.notify();
                                            }
                                        },
                                    ))
                                }),
                        ),
                )
                .child(
                    div().flex().gap_1().children(
                        [("java8", "Java 8"), ("java17+", "Java 17+")]
                            .into_iter()
                            .map(|(value, label)| {
                                btn(
                                    t,
                                    label,
                                    if d.java == value {
                                        ButtonVariant::Secondary
                                    } else {
                                        ButtonVariant::Ghost
                                    },
                                )
                                .text_xs()
                                .on_click(cx.listener(
                                    move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                        if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                                            d.java = value.into();
                                            cx.notify();
                                        }
                                    },
                                ))
                            }),
                    ),
                );
            if !Self::selected_pack_available(d) && !d.versions.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(t.destructive())
                        .child("This pack does not provide the selected Java variant."),
                );
            }
        }
        if !d.loading && !d.analyzing {
            let checked = d.copy_before;
            body = body.child(btn(t, &format!("{} Copy instance before {title}", if checked { "☑" } else { "☐" }), ButtonVariant::Ghost).text_xs()
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                    if let Some(Dialog::Maintenance(d)) = &mut v.dialog { d.copy_before = !d.copy_before; cx.notify(); }
                })))
                .child(div().text_xs().text_color(t.muted_foreground()).child(if checked { "The operation runs on a new copy; the original stays intact." } else { "Replaces pack files. Preserves saves, JourneyMap, screenshots, resource packs, and player options." }));
        }
        if let Some(error) = &d.error {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(t.destructive())
                    .child(error.clone()),
            );
        }
        let label = if d.analyzing {
            "Analyzing…"
        } else if d.kind == MaintenanceKind::Update && d.preview.is_none() {
            "Analyze update"
        } else if d.kind == MaintenanceKind::Update {
            "Update pack"
        } else {
            "Reinstall"
        };
        let footer = self.dialog_footer(
            "Cancel",
            label,
            if d.kind == MaintenanceKind::Reinstall {
                ButtonVariant::Danger
            } else {
                ButtonVariant::Primary
            },
            |v, cx| v.submit_maintenance(cx),
            cx,
        );
        self.dialog_frame(title, body, footer, cx)
    }

    pub(super) fn finish_instance_process(
        &mut self,
        token: &ProcessToken,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let window = self.window_handle;
        finish_process(&mut self.processes_ui.history, token, error, || {
            if let Some(window) = window {
                cx.defer(move |cx| {
                    // A missing/closed window or unavailable system audio must
                    // never fail or delay the already completed copy.
                    let _ = window.update(cx, |_, window, _| window.play_system_bell());
                });
            }
        });
        self.follow_process_output(&token.key);
    }
    pub(super) fn begin_install(
        &mut self,
        id: String,
        name: String,
        group: String,
        version: String,
        java: String,
        cx: &mut Context<Self>,
    ) {
        self.close_dialog(cx);
        self.run_instance_work(
            id,
            Work::Install {
                name,
                group,
                version,
                java,
            },
            cx,
        );
    }
    pub(super) fn begin_delete(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        self.close_dialog(cx);
        self.run_instance_work(id, Work::Delete, cx);
    }
    fn run_instance_work(&mut self, id: String, work: Work, cx: &mut Context<Self>) {
        use industrialis_core::background_processes::Operation;
        if !matches!(work, Work::Preview { .. }) {
            if let Some(key) = self.files_ui.dirty_key(&id) {
                let target = id.clone();
                self.save_file_then(key, move |v, cx| v.run_instance_work(target, work, cx), cx);
                return;
            }
        }
        if self.instance_is_busy(&id) {
            return;
        }
        let destination = match &work {
            Work::Copy { destination, .. } => Some(destination.clone()),
            Work::Maintain {
                copy: Some((id, _)),
                ..
            } => Some(id.clone()),
            _ => None,
        };
        if destination
            .as_ref()
            .is_some_and(|id| self.instance_is_busy(id))
        {
            return;
        }
        let name = self
            .data
            .selected_instance(Some(&id))
            .map(|i| i.settings.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or(&id)
            .to_string();
        let initial = match &work {
            Work::Install { name, version, .. } => Some((
                Operation::Install,
                id.clone(),
                if name.trim().is_empty() {
                    format!("GTNH {version}")
                } else {
                    name.clone()
                },
            )),
            Work::Delete => Some((Operation::Delete, id.clone(), name.clone())),
            Work::Copy { name, destination } => {
                Some((Operation::Copy, destination.clone(), name.clone()))
            }
            Work::Maintain {
                copy: Some((destination, name)),
                ..
            } => Some((Operation::Copy, destination.clone(), name.clone())),
            Work::Maintain { kind, .. } => Some((
                if *kind == MaintenanceKind::Update {
                    Operation::Update
                } else {
                    Operation::Reinstall
                },
                id.clone(),
                name.clone(),
            )),
            Work::Preview { .. } => None,
        };
        let mut runs = std::collections::HashMap::new();
        if let Some((operation, target, name)) = initial {
            let token = self.processes_ui.history.start(operation, &target, &name);
            self.processes_ui.selected = Some(token.key.clone());
            self.follow_process_output(&token.key);
            runs.insert(token.key.clone(), token);
        }
        let delete_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if matches!(work, Work::Delete) {
            self.processes_ui
                .delete_cancel
                .insert(id.clone(), delete_cancel.clone());
        }
        let mut reserved = vec![id.clone()];
        if let Some(destination) = destination {
            reserved.push(destination);
        }
        for id in &reserved {
            self.busy.insert(format!("maintenance:{id}"));
        }
        self.data.status = format!("Working on {id}?");
        cx.notify();
        enum Event {
            Begin(Operation, String, String),
            Progress(PackProgress),
            FinishOne(Operation, String, Option<String>),
            Finished(Result<Option<UpdateModPreview>, String>),
        }
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit_sender = sender.clone();
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let emit: EmitProgress = Arc::new(move |p| {
            let mut last = last.lock().unwrap();
            if p.log_line.is_some()
                || last.elapsed() >= Duration::from_millis(100)
                || p.stage == "done"
            {
                *last = Instant::now();
                let _ = emit_sender.send(Event::Progress(p));
            }
        });
        let events = sender.clone();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let target = id.clone();
        let worker = work.clone();
        let task = rt.spawn(async move {
            let result = async {
                match worker {
                    Work::Install {
                        name,
                        group,
                        version,
                        java,
                    } => {
                        industrialis_backend::install::download_install(
                            &paths,
                            &target,
                            &version,
                            &java,
                            Some(&name),
                            if group.trim().is_empty() {
                                None
                            } else {
                                Some(group.trim())
                            },
                            emit,
                        )
                        .await?;
                        Ok(None)
                    }
                    Work::Delete => {
                        industrialis_backend::instance_ops::delete_instance_with_progress(
                            &paths,
                            &target,
                            emit,
                            delete_cancel,
                        )
                        .await?;
                        Ok(None)
                    }
                    Work::Copy { name, destination } => {
                        copy_instance_with_progress(&paths, &target, &destination, &name, emit)
                            .await?;
                        Ok(None)
                    }
                    Work::Preview { version, java, .. } => {
                        preview_update(&paths, &target, &version, Some(&java), emit)
                            .await
                            .map(Some)
                    }
                    Work::Maintain {
                        kind,
                        version,
                        java,
                        keep,
                        copy,
                    } => {
                        let target = if let Some((destination, name)) = copy {
                            let copied = copy_instance_with_progress(
                                &paths,
                                &target,
                                &destination,
                                &name,
                                emit.clone(),
                            )
                            .await;
                            let _ = events.send(Event::FinishOne(
                                Operation::Copy,
                                destination.clone(),
                                copied.as_ref().err().map(|e| format!("{e:#}")),
                            ));
                            copied?;
                            let _ = events.send(Event::Begin(
                                if kind == MaintenanceKind::Update {
                                    Operation::Update
                                } else {
                                    Operation::Reinstall
                                },
                                destination.clone(),
                                name,
                            ));
                            destination
                        } else {
                            target
                        };
                        if kind == MaintenanceKind::Update {
                            update_instance(&paths, &target, &version, Some(&java), &keep, emit)
                                .await?;
                        } else {
                            reinstall_instance(&paths, &target, &version, Some(&java), emit)
                                .await?;
                        }
                        Ok(None)
                    }
                }
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"));
            result
        });
        cx.spawn(async move |_, _| {
            let event = match task.await {
                Ok(result) => Event::Finished(result),
                Err(error) => Event::Finished(Err(format!("Operation task failed: {error}"))),
            };
            let _ = sender.send(event);
        })
        .detach();
        cx.spawn(async move |view, cx| {
            while let Some(event) = receiver.recv().await {
                let finished = matches!(event, Event::Finished(..));
                if view
                    .update(cx, |v, cx| {
                        match event {
                            Event::Begin(operation, id, name) => {
                                let token = v.processes_ui.history.start(operation, &id, &name);
                                v.processes_ui.selected = Some(token.key.clone());
                                v.follow_process_output(&token.key);
                                runs.insert(token.key.clone(), token);
                            }
                            Event::Progress(p) => {
                                v.data.status = p.log_line.clone().unwrap_or_else(|| {
                                    format!("{} {}%", p.stage, (p.pct * 100.0) as u32)
                                });
                                if let (Some(operation), Some(id)) =
                                    (Operation::parse(&p.operation), p.id.as_ref())
                                {
                                    if let Some(token) =
                                        runs.get(&format!("{}:{id}", operation.key()))
                                    {
                                        v.processes_ui.history.progress(
                                            token,
                                            &p.stage,
                                            p.pct,
                                            p.log_line.clone(),
                                            (p.speed_mbps, p.downloaded_mb, p.total_mb),
                                        );
                                        v.follow_process_output(&token.key);
                                    }
                                }
                                for target in &reserved {
                                    v.operation_progress.insert(target.clone(), p.clone());
                                }
                            }
                            Event::FinishOne(operation, id, error) => {
                                if let Some(token) = runs.get(&format!("{}:{id}", operation.key()))
                                {
                                    v.finish_instance_process(token, error.as_deref(), cx);
                                }
                            }
                            Event::Finished(result) => {
                                v.processes_ui.delete_cancel.remove(&id);
                                for token in runs.values() {
                                    v.finish_instance_process(
                                        token,
                                        result.as_ref().err().map(String::as_str),
                                        cx,
                                    );
                                }
                                for target in &reserved {
                                    v.busy.remove(&format!("maintenance:{target}"));
                                    v.operation_progress.remove(target);
                                }
                                if let Work::Preview { token, .. } = work {
                                    v.data.status = match &result {
                                        Err(e) => format!("Update analysis failed: {e}"),
                                        _ => "Update analysis finished".into(),
                                    };
                                    if let Some(Dialog::Maintenance(d)) = &mut v.dialog {
                                        if d.token == token {
                                            d.analyzing = false;
                                            match result {
                                                Ok(Some(preview)) => {
                                                    d.keep = preview
                                                        .custom_mods
                                                        .iter()
                                                        .map(|e| e.entry.identity.clone())
                                                        .collect();
                                                    d.preview = Some(preview);
                                                    d.error = None;
                                                }
                                                Err(e) => d.error = Some(e),
                                                _ => {}
                                            }
                                        }
                                    }
                                } else {
                                    for target in &reserved {
                                        v.forget_instance_editor(target);
                                    }
                                    match result {
                                        Ok(_) => {
                                            v.data.status = "Instance operation complete".into();
                                            if matches!(work, Work::Delete) {
                                                if v.selected_id.as_ref() == Some(&id) {
                                                    v.selected_id = None;
                                                }
                                            } else if let Some(target) = reserved.last() {
                                                v.select_instance(target.clone(), cx);
                                            }
                                        }
                                        Err(e) if e.starts_with(industrialis_backend::instance_ops::DELETE_CANCELLED) => v.data.status=e,
                                        Err(e) => {
                                            v.data.status =
                                                format!("Instance operation failed: {e}")
                                        }
                                    }
                                    // Use the shared generation-checked reader rather
                                    // than applying a stale, error-discarding snapshot.
                                    v.refresh(cx);
                                }
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                    || finished
                {
                    break;
                }
            }
        })
        .detach();
    }
}
