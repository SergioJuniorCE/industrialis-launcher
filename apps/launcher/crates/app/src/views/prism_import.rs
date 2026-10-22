//! First-run and menu-driven Prism Launcher import flow.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, Context, Div, FontWeight, KeyDownEvent, Window};
use industrialis_backend::pack_files::{fetch_gtnh_versions, EmitProgress, PackProgress};
use industrialis_backend::prism::{
    self, PrismImportItem, PrismInstall, PrismInstance, IMPORT_CANCELLED,
};
use industrialis_core::background_processes::{Operation, ProcessToken};

use super::{
    pack_catalog::{self, VersionChoice},
    Dialog, LauncherRoot,
};
use crate::input::FieldState;
use crate::state::format_bytes;
use crate::ui::{badge, btn, icon, tint, BadgeKind, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrismStep {
    Ask,
    Select,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PickTarget {
    Instance(String),
    All,
}

#[derive(Debug, Clone)]
struct PrismRow {
    instance: PrismInstance,
    size: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct PrismImportDialog {
    onboarding: bool,
    step: PrismStep,
    detect_root: Option<PathBuf>,
    loading: bool,
    install: Option<PrismInstall>,
    rows: Vec<PrismRow>,
    selected: HashSet<String>,
    versions: HashMap<String, String>,
    catalog: Option<Vec<VersionChoice>>,
    catalog_loading: bool,
    catalog_error: Option<String>,
    picking: Option<PickTarget>,
    prism_running: bool,
    show_path: bool,
    pub(super) path: FieldState,
    pub(super) search: FieldState,
    error: Option<String>,
    generation: u64,
}

impl PrismImportDialog {
    fn empty(onboarding: bool, detect_root: Option<PathBuf>) -> Self {
        Self {
            onboarding,
            step: if onboarding {
                PrismStep::Ask
            } else {
                PrismStep::Select
            },
            detect_root,
            loading: false,
            install: None,
            rows: Vec::new(),
            selected: HashSet::new(),
            versions: HashMap::new(),
            catalog: None,
            catalog_loading: false,
            catalog_error: None,
            picking: None,
            prism_running: false,
            show_path: false,
            path: FieldState::empty(),
            search: FieldState::empty(),
            error: None,
            generation: 0,
        }
    }

    fn version_label(&self, folder: &str) -> String {
        self.versions
            .get(folder)
            .cloned()
            .unwrap_or_else(|| "Imported (no updates)".into())
    }

    fn row_name(&self, folder: &str) -> String {
        self.rows
            .iter()
            .find(|row| row.instance.folder == folder)
            .map(|row| {
                if row.instance.name.is_empty() {
                    row.instance.folder.clone()
                } else {
                    row.instance.name.clone()
                }
            })
            .unwrap_or_else(|| folder.to_string())
    }
}

#[derive(Debug, Clone)]
struct VersionOption {
    value: Option<String>,
    label: String,
    meta: String,
    selected: bool,
}

fn version_options(
    choices: Option<&[VersionChoice]>,
    search: &str,
    selected: Option<&str>,
) -> Vec<VersionOption> {
    let mut options = vec![VersionOption {
        value: None,
        label: "Imported (no updates)".into(),
        meta: "Frozen: no update checks, updates, or reinstalls".into(),
        selected: selected.is_none(),
    }];
    if let Some(choices) = choices {
        let search = search.trim().to_ascii_lowercase();
        for choice in choices.iter().filter(|choice| {
            search.is_empty() || choice.name.to_ascii_lowercase().contains(&search)
        }) {
            options.push(VersionOption {
                value: Some(choice.name.clone()),
                label: choice.name.clone(),
                meta: format!(
                    "{} · {}",
                    choice.date,
                    if choice.stable { "stable" } else { "beta" }
                ),
                selected: selected == Some(choice.name.as_str()),
            });
        }
    }
    options
}

/// Recompute the "name suggests version" hint once the catalog is available.
fn apply_suggestions(dialog: &mut PrismImportDialog) {
    let Some(catalog) = &dialog.catalog else {
        return;
    };
    let keys: Vec<String> = catalog.iter().map(|choice| choice.name.clone()).collect();
    for row in &mut dialog.rows {
        if row.instance.gtnh {
            row.instance.suggested_version = prism::suggest_gtnh_version(&row.instance.name, &keys);
        }
    }
}

fn java_type_for(choices: Option<&Vec<VersionChoice>>, version: &str) -> Option<String> {
    let choice = choices?.iter().find(|choice| choice.name == version)?;
    Some(if choice.java17 { "java17+" } else { "java8" }.to_string())
}

#[derive(Debug, Clone)]
pub(super) struct ImportVersionDialog {
    id: String,
    name: String,
    current: Option<String>,
    versions: Option<Vec<VersionChoice>>,
    loading: bool,
    saving: bool,
    error: Option<String>,
    pub(super) search: FieldState,
    generation: u64,
}

impl LauncherRoot {
    /// First-run entry: only for an empty library whose marker is unset.
    pub(crate) fn maybe_prompt_prism_import(&mut self, cx: &mut Context<Self>) {
        if self.closing.is_some() || self.dialog.is_some() {
            return;
        }
        if !self.data.instances.is_empty() || self.data.settings.prism_import_prompted {
            return;
        }
        self.open_prism_import(None, true, cx);
    }

    pub(super) fn open_prism_import(
        &mut self,
        detect_root: Option<PathBuf>,
        onboarding: bool,
        cx: &mut Context<Self>,
    ) {
        if self.closing.is_some() || (onboarding && self.data.settings.prism_import_prompted) {
            return;
        }
        self.workspace_ui.menu = None;
        self.add_menu = None;
        self.dialog = Some(Dialog::PrismImport(PrismImportDialog::empty(
            onboarding,
            detect_root,
        )));
        self.focused_field = None;
        cx.notify();
        if !onboarding {
            self.start_prism_detection(cx);
        }
    }

    fn start_prism_detection(&mut self, cx: &mut Context<Self>) {
        let (detect_root, generation) = match &mut self.dialog {
            Some(Dialog::PrismImport(dialog)) => {
                dialog.step = PrismStep::Select;
                dialog.loading = true;
                dialog.error = None;
                dialog.install = None;
                dialog.rows.clear();
                dialog.selected.clear();
                dialog.versions.clear();
                dialog.catalog = None;
                dialog.catalog_loading = false;
                dialog.catalog_error = None;
                dialog.prism_running = false;
                dialog.picking = None;
                dialog.generation += 1;
                (dialog.detect_root.clone(), dialog.generation)
            }
            _ => return,
        };
        cx.notify();
        let rt = self.data.rt.clone();
        let task = rt.spawn(async move {
            match detect_root {
                Some(root) => prism::resolve_manual(&root).await,
                None => prism::detect().await,
            }
        });
        cx.spawn(async move |view, cx| {
            let install = task.await.ok().flatten();
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::PrismImport(dialog)) = &mut v.dialog else {
                    return;
                };
                if dialog.generation != generation {
                    return;
                }
                dialog.loading = false;
                dialog.install = install;
                if dialog.install.is_some() {
                    v.start_prism_discovery(cx);
                } else {
                    dialog.error = Some(if dialog.detect_root.is_some() {
                        "No Prism instances found in that folder.".into()
                    } else {
                        "No Prism Launcher installation was found.".into()
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_prism_discovery(&mut self, cx: &mut Context<Self>) {
        let (install, generation) = match &mut self.dialog {
            Some(Dialog::PrismImport(dialog)) => {
                let Some(install) = dialog.install.clone() else {
                    return;
                };
                dialog.loading = true;
                dialog.catalog_loading = true;
                dialog.catalog_error = None;
                dialog.rows.clear();
                dialog.selected.clear();
                dialog.versions.clear();
                dialog.picking = None;
                dialog.generation += 1;
                (install, dialog.generation)
            }
            _ => return,
        };
        cx.notify();

        // Instances are discovered without waiting for the network. The
        // version catalog only adds suggestions and enables version pickers,
        // so an offline import can proceed frozen.
        let rt = self.data.rt.clone();
        let rows_task = rt.spawn(async move {
            let rows = prism::discover(&install, &[]).await;
            (rows, prism::is_prism_process_running())
        });
        cx.spawn(async move |view, cx| {
            let (rows, running) = match rows_task.await {
                Ok(value) => value,
                Err(error) => (Err(anyhow::Error::from(error)), false),
            };
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::PrismImport(dialog)) = &mut v.dialog else {
                    return;
                };
                if dialog.generation != generation {
                    return;
                }
                dialog.loading = false;
                dialog.prism_running = running;
                match rows {
                    Ok(rows) => {
                        dialog.selected = rows
                            .iter()
                            .filter(|instance| instance.eligible && instance.gtnh)
                            .map(|instance| instance.folder.clone())
                            .collect();
                        dialog.rows = rows
                            .into_iter()
                            .map(|instance| PrismRow {
                                instance,
                                size: None,
                            })
                            .collect();
                        if dialog.rows.iter().all(|row| !row.instance.eligible) {
                            dialog.error =
                                Some("No importable instances were found in that folder.".into());
                        }
                        apply_suggestions(dialog);
                        v.start_prism_sizes(cx);
                    }
                    Err(error) => dialog.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();

        let catalog_task = rt.spawn(fetch_gtnh_versions());
        cx.spawn(async move |view, cx| {
            let result = match catalog_task.await {
                Ok(value) => value,
                Err(error) => Err(anyhow::Error::from(error)),
            };
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::PrismImport(dialog)) = &mut v.dialog else {
                    return;
                };
                if dialog.generation != generation {
                    return;
                }
                dialog.catalog_loading = false;
                match result {
                    Ok(catalog) => {
                        dialog.catalog = Some(pack_catalog::choices(&catalog));
                        dialog.catalog_error = None;
                        apply_suggestions(dialog);
                    }
                    Err(error) => {
                        dialog.catalog = None;
                        dialog.catalog_error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_prism_sizes(&mut self, cx: &mut Context<Self>) {
        let (targets, generation) = match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => (
                dialog
                    .rows
                    .iter()
                    .map(|row| (row.instance.folder.clone(), row.instance.path.clone()))
                    .collect::<Vec<_>>(),
                dialog.generation,
            ),
            _ => return,
        };
        if targets.is_empty() {
            return;
        }
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            for (folder, path) in targets {
                let bytes = rt
                    .spawn(async move { industrialis_backend::fs_utils::dir_size(&path).await })
                    .await
                    .unwrap_or(0);
                let _ = view.update(cx, |v, cx| {
                    let Some(Dialog::PrismImport(dialog)) = &mut v.dialog else {
                        return;
                    };
                    if dialog.generation != generation {
                        return;
                    }
                    if let Some(row) = dialog
                        .rows
                        .iter_mut()
                        .find(|row| row.instance.folder == folder)
                    {
                        row.size = Some(bytes);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn scan_prism_path(&mut self, cx: &mut Context<Self>) {
        let path = match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => dialog.path.text.trim().to_string(),
            _ => return,
        };
        if path.is_empty() {
            if let Some(Dialog::PrismImport(dialog)) = &mut self.dialog {
                dialog.error = Some("Enter a Prism data folder or instances folder.".into());
            }
            cx.notify();
            return;
        }
        if let Some(Dialog::PrismImport(dialog)) = &mut self.dialog {
            dialog.detect_root = Some(PathBuf::from(path));
            dialog.show_path = true;
        }
        self.start_prism_detection(cx);
    }

    fn toggle_prism_row(&mut self, folder: &str, cx: &mut Context<Self>) {
        let Some(Dialog::PrismImport(dialog)) = &mut self.dialog else {
            return;
        };
        let Some(row) = dialog.rows.iter().find(|row| row.instance.folder == folder) else {
            return;
        };
        if !row.instance.eligible {
            return;
        }
        if dialog.selected.contains(folder) {
            dialog.selected.remove(folder);
        } else {
            dialog.selected.insert(folder.to_string());
        }
        cx.notify();
    }

    fn pick_prism_version(
        &mut self,
        target: PickTarget,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(Dialog::PrismImport(dialog)) = &mut self.dialog else {
            return;
        };
        match target {
            PickTarget::Instance(folder) => match value {
                Some(version) => {
                    dialog.versions.insert(folder, version);
                }
                None => {
                    dialog.versions.remove(&folder);
                }
            },
            PickTarget::All => {
                dialog.versions.clear();
                if let Some(version) = value {
                    for row in dialog
                        .rows
                        .iter()
                        .filter(|row| row.instance.eligible && row.instance.gtnh)
                    {
                        dialog
                            .versions
                            .insert(row.instance.folder.clone(), version.clone());
                    }
                }
            }
        }
        dialog.picking = None;
        cx.notify();
    }

    fn mark_prism_prompted(&mut self, cx: &mut Context<Self>) {
        if self.data.settings.prism_import_prompted {
            return;
        }
        self.data.settings.prism_import_prompted = true;
        let paths = self.data.paths.clone();
        let settings = self.data.settings.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    industrialis_backend::settings::save_launcher_settings(&paths, &settings).await
                })
                .await;
            if !matches!(result, Ok(Ok(()))) {
                let _ = view.update(cx, |v, cx| {
                    v.data.status = "Could not save the Prism import preference".into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn finish_prism_fresh(&mut self, cx: &mut Context<Self>) {
        self.mark_prism_prompted(cx);
        self.dialog = None;
        self.focused_field = None;
        cx.notify();
    }

    /// Enter in the dialog: advance the intro, scan the typed folder, or
    /// start the import.
    pub(super) fn submit_prism_dialog(&mut self, cx: &mut Context<Self>) {
        let (step, picking, has_install) = match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => (
                dialog.step,
                dialog.picking.is_some(),
                dialog.install.is_some(),
            ),
            _ => return,
        };
        match step {
            PrismStep::Ask => self.start_prism_detection(cx),
            PrismStep::Select if picking => {}
            PrismStep::Select if !has_install => self.scan_prism_path(cx),
            PrismStep::Select => self.start_prism_import(cx),
        }
    }

    fn start_prism_import(&mut self, cx: &mut Context<Self>) {
        let (onboarding, rows) = match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => {
                if dialog.loading {
                    return;
                }
                if dialog.selected.is_empty() {
                    return;
                }
                (dialog.onboarding, dialog.rows.clone())
            }
            _ => return,
        };
        let rows: Vec<PrismRow> = rows
            .into_iter()
            .filter(|row| row.instance.eligible && self.prism_selected(row))
            .collect();
        if rows.is_empty() {
            return;
        }
        let mut taken = HashSet::new();
        let mut items: Vec<(String, PrismImportItem)> = Vec::new();
        let mut errors = Vec::new();
        for row in &rows {
            let desired = if row.instance.name.trim().is_empty() {
                row.instance.folder.clone()
            } else {
                row.instance.name.clone()
            };
            match prism::unique_target_id(&self.data.paths, &desired, &taken) {
                Ok(id) => {
                    taken.insert(id.clone());
                    let version = self.prism_version(row);
                    items.push((
                        id.clone(),
                        PrismImportItem {
                            source_dir: row.instance.path.clone(),
                            target_id: id,
                            name: if row.instance.name.trim().is_empty() {
                                row.instance.folder.clone()
                            } else {
                                row.instance.name.clone()
                            },
                            icon_source: row.instance.icon_source.clone(),
                            install_default_icon: row.instance.gtnh
                                && row.instance.icon_source.is_none(),
                            java_type: self.prism_catalog().and_then(|choices| {
                                version
                                    .as_deref()
                                    .and_then(|version| java_type_for(Some(&choices), version))
                            }),
                            version,
                            gtnh: row.instance.gtnh,
                            group: Some(row.instance.group.clone()),
                        },
                    ));
                }
                Err(error) => errors.push(format!("{}: {error:#}", row.instance.name)),
            }
        }
        if items.is_empty() {
            if let Some(Dialog::PrismImport(dialog)) = &mut self.dialog {
                dialog.error = Some(format!(
                    "Could not prepare the import: {}",
                    errors.join("; ")
                ));
            }
            cx.notify();
            return;
        }
        if onboarding {
            self.mark_prism_prompted(cx);
        }
        self.dialog = None;
        self.focused_field = None;
        self.workspace_ui.menu = None;
        self.add_menu = None;
        let cancel = Arc::new(AtomicBool::new(false));
        for (id, _) in &items {
            self.busy.insert(format!("import:{id}"));
            self.processes_ui
                .import_cancel
                .insert(id.clone(), cancel.clone());
        }
        self.data.status = format!("Importing {} instance(s) from Prism…", items.len());
        cx.notify();

        enum Event {
            Begin(String, String),
            Progress(PackProgress),
            Done(String, Option<String>),
            AllDone(Option<String>),
        }
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit_sender = sender.clone();
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let emit: EmitProgress = Arc::new(move |progress| {
            let mut last = last.lock().unwrap();
            if progress.log_line.is_some()
                || last.elapsed() >= Duration::from_millis(100)
                || progress.pct >= 1.0
            {
                *last = Instant::now();
                let _ = emit_sender.send(Event::Progress(progress));
            }
        });
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let worker_cancel = cancel.clone();
        rt.spawn(async move {
            let mut last_ok = None;
            for (id, item) in items {
                let _ = sender.send(Event::Begin(id.clone(), item.name.clone()));
                let result = if worker_cancel.load(Ordering::Relaxed) {
                    Err(anyhow::anyhow!(IMPORT_CANCELLED))
                } else {
                    prism::import_instance(&paths, &item, emit.clone(), worker_cancel.clone()).await
                };
                match &result {
                    Ok(()) => last_ok = Some(id.clone()),
                    Err(_) => {}
                }
                let _ = sender.send(Event::Done(
                    id,
                    result.err().map(|error| format!("{error:#}")),
                ));
            }
            let _ = sender.send(Event::AllDone(last_ok));
        });
        let mut runs: HashMap<String, ProcessToken> = HashMap::new();
        cx.spawn(async move |view, cx| {
            while let Some(event) = receiver.recv().await {
                let finished = matches!(event, Event::AllDone(_));
                let updated = view.update(cx, |v, cx| {
                    match event {
                        Event::Begin(id, name) => {
                            let token = v.processes_ui.history.start(Operation::Import, &id, &name);
                            v.processes_ui.selected = Some(token.key.clone());
                            v.follow_process_output(&token.key);
                            runs.insert(token.key.clone(), token);
                        }
                        Event::Progress(progress) => {
                            v.data.status = progress.log_line.clone().unwrap_or_else(|| {
                                format!("{} {}%", progress.stage, (progress.pct * 100.0) as u32)
                            });
                            if let Some(id) = progress.id.as_ref() {
                                if let Some(token) = runs.get(&format!("import:{id}")) {
                                    v.processes_ui.history.progress(
                                        token,
                                        &progress.stage,
                                        progress.pct,
                                        progress.log_line.clone(),
                                        (
                                            progress.speed_mbps,
                                            progress.downloaded_mb,
                                            progress.total_mb,
                                        ),
                                    );
                                    v.follow_process_output(&token.key);
                                }
                            }
                        }
                        Event::Done(id, error) => {
                            let succeeded = error.is_none();
                            if let Some(token) = runs.get(&format!("import:{id}")) {
                                v.finish_instance_process(token, error.as_deref(), cx);
                            }
                            v.processes_ui.import_cancel.remove(&id);
                            v.busy.remove(&format!("import:{id}"));
                            if succeeded {
                                v.refresh(cx);
                            }
                        }
                        Event::AllDone(last_ok) => {
                            if let Some(id) = last_ok {
                                v.select_instance(id, cx);
                            }
                            v.data.status = "Prism import finished".into();
                            v.refresh(cx);
                        }
                    }
                    cx.notify();
                    true
                });
                if updated.is_err() || finished {
                    break;
                }
            }
        })
        .detach();
    }

    fn prism_selected(&self, row: &PrismRow) -> bool {
        match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => dialog.selected.contains(&row.instance.folder),
            _ => false,
        }
    }

    fn prism_version(&self, row: &PrismRow) -> Option<String> {
        match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => dialog.versions.get(&row.instance.folder).cloned(),
            _ => None,
        }
    }

    fn prism_catalog(&self) -> Option<Vec<VersionChoice>> {
        match &self.dialog {
            Some(Dialog::PrismImport(dialog)) => dialog.catalog.clone(),
            _ => None,
        }
    }

    pub(super) fn prism_import_dialog(
        &mut self,
        dialog: &PrismImportDialog,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme.clone();
        let t = &theme;
        let title = if dialog.step == PrismStep::Ask {
            "Import from Prism?"
        } else if dialog.onboarding {
            "Import instances from Prism"
        } else {
            "Import from Prism"
        };
        let mut body = div().flex().flex_col().gap_3().min_h_0();
        if dialog.step == PrismStep::Ask {
            body = body
                .child(
                    div()
                        .text_sm()
                        .child("Prism Launcher instances can be copied into Industrialis Launcher."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.muted_foreground())
                        .child("Saves, configs, mods, and player data are copied. Nothing in Prism is changed, moved, or registered. Accounts are not copied because Prism stores its tokens in the OS keychain."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.muted_foreground())
                        .child("GTNH instances are pre-selected. Other instances can be imported too, but stay frozen with updates disabled."),
                );
            let footer = div()
                .flex()
                .gap_2()
                .child(
                    btn(t, "Start fresh", ButtonVariant::Ghost)
                        .debug_selector(|| "prism-import-fresh".into())
                        .on_click(
                            cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.finish_prism_fresh(cx)),
                        ),
                )
                .child(
                    btn(t, "Import from Prism", ButtonVariant::Primary)
                        .debug_selector(|| "prism-import-start".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                            v.start_prism_detection(cx)
                        })),
                );
            return crate::ui::dialog_shell_with_width(&theme, title, 520.0, body, footer)
                .on_key_down(cx.listener(
                    |this: &mut Self, event: &KeyDownEvent, _: &mut Window, cx| {
                        if event.keystroke.key == "escape" {
                            this.close_dialog(cx);
                        }
                    },
                ));
        }

        if dialog.loading {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("loader-circle", 16.0))
                    .child("Looking for Prism Launcher…"),
            );
        } else if let Some(install) = &dialog.install {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child(format!("Found: {}", install.instances_dir.display())),
                    )
                    .child(
                        btn(t, "Change folder…", ButtonVariant::Ghost)
                            .text_xs()
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                if let Some(Dialog::PrismImport(dialog)) = &mut v.dialog {
                                    dialog.show_path = !dialog.show_path;
                                    dialog.error = None;
                                }
                                cx.notify();
                            })),
                    ),
            );
            if dialog.show_path {
                body = body.child(self.prism_path_field(cx));
            }
            if dialog.prism_running {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(t.muted_foreground())
                        .child("Prism Launcher is running. Close it before importing so the copy is consistent; the import never writes to Prism."),
                );
            }
            if let Some(error) = &dialog.catalog_error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(t.destructive())
                        .child(format!(
                            "Could not load GTNH versions ({error}). Imports stay frozen until a version is set later."
                        )),
                );
            }
            if dialog.picking.is_some() {
                body = body.child(self.prism_version_picker(dialog, cx));
            } else {
                let has_other = dialog
                    .rows
                    .iter()
                    .any(|row| row.instance.eligible && !row.instance.gtnh);
                if has_other {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .child("Non-GTNH instances are imported without update checks, updates, or reinstalls."),
                    );
                }
                if dialog.rows.iter().any(|row| row.instance.gtnh) {
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(t.muted_foreground())
                                    .child(if dialog.catalog_loading {
                                        "Loading GTNH versions… You can import now and set versions later."
                                    } else {
                                        "GTNH versions are assigned per instance."
                                    }),
                            )
                            .child(
                                btn(t, "Set all GTNH versions…", ButtonVariant::Ghost)
                                    .text_xs()
                                    .debug_selector(|| "prism-version-all".into())
                                    .when(dialog.catalog.is_none(), |button| {
                                        button.opacity(0.5).cursor_default()
                                    })
                                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                                        let available = matches!(
                                            &v.dialog,
                                            Some(Dialog::PrismImport(dialog))
                                                if dialog.catalog.is_some()
                                        );
                                        if available {
                                            if let Some(Dialog::PrismImport(dialog)) = &mut v.dialog
                                            {
                                                dialog.picking = Some(PickTarget::All);
                                                dialog.search = FieldState::empty();
                                            }
                                            cx.notify();
                                        }
                                    })),
                            ),
                    );
                }
                body = body.child(
                    div()
                        .id("prism-instance-list")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .max_h(px(320.0))
                        .overflow_y_scroll()
                        .children(
                            dialog
                                .rows
                                .iter()
                                .map(|row| self.prism_row_element(dialog, row, cx)),
                        ),
                );
            }
        } else {
            body = body
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            dialog
                                .error
                                .clone()
                                .unwrap_or_else(|| "No Prism Launcher installation was found.".into()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.muted_foreground())
                        .child("Choose the Prism data folder (the folder containing prismlauncher.cfg) or its instances folder."),
                )
                .child(self.prism_path_field(cx));
        }
        if let Some(error) = &dialog.error {
            if dialog.install.is_some() && dialog.step == PrismStep::Select {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(t.destructive())
                        .child(error.clone()),
                );
            }
        }

        let footer = if dialog.picking.is_some() {
            div().child(
                btn(t, "Back", ButtonVariant::Ghost)
                    .debug_selector(|| "prism-version-back".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| {
                        if let Some(Dialog::PrismImport(dialog)) = &mut v.dialog {
                            dialog.picking = None;
                        }
                        cx.notify();
                    })),
            )
        } else if dialog.install.is_some() {
            let count = dialog.selected.len();
            let disabled = count == 0 || dialog.loading;
            div()
                .flex()
                .gap_2()
                .child(if dialog.onboarding {
                    btn(t, "Start fresh", ButtonVariant::Ghost)
                        .debug_selector(|| "prism-import-fresh".into())
                        .on_click(
                            cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.finish_prism_fresh(cx)),
                        )
                        .into_any_element()
                } else {
                    btn(t, "Cancel", ButtonVariant::Ghost)
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx)))
                        .into_any_element()
                })
                .child(
                    btn(
                        t,
                        &format!(
                            "Import {count} instance{}",
                            if count == 1 { "" } else { "s" }
                        ),
                        ButtonVariant::Primary,
                    )
                    .debug_selector(|| "prism-import-submit".into())
                    .when(disabled, |button| button.opacity(0.5).cursor_default())
                    .on_click(
                        cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.start_prism_import(cx)),
                    ),
                )
        } else {
            div()
                .flex()
                .gap_2()
                .child(if dialog.onboarding {
                    btn(t, "Start fresh", ButtonVariant::Ghost)
                        .debug_selector(|| "prism-import-fresh".into())
                        .on_click(
                            cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.finish_prism_fresh(cx)),
                        )
                        .into_any_element()
                } else {
                    btn(t, "Cancel", ButtonVariant::Ghost)
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx)))
                        .into_any_element()
                })
                .child(
                    btn(t, "Scan folder", ButtonVariant::Primary)
                        .debug_selector(|| "prism-scan-submit".into())
                        .on_click(
                            cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.scan_prism_path(cx)),
                        ),
                )
        };

        crate::ui::dialog_shell_with_width(&theme, title, 680.0, body, footer).on_key_down(
            cx.listener(
                |this: &mut Self, event: &KeyDownEvent, _: &mut Window, cx| {
                    if event.keystroke.key == "escape" {
                        if let Some(Dialog::PrismImport(dialog)) = &mut this.dialog {
                            if dialog.picking.is_some() {
                                dialog.picking = None;
                                cx.notify();
                                return;
                            }
                        }
                        this.close_dialog(cx);
                    }
                },
            ),
        )
    }

    fn prism_path_field(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .gap_2()
            .items_center()
            .child(div().flex_1().child(self.field(
                "prism-path",
                "Prism data folder or instances folder",
                cx,
            )))
            .child(
                btn(&self.theme, "Scan", ButtonVariant::Outline)
                    .debug_selector(|| "prism-scan-folder".into())
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.scan_prism_path(cx))),
            )
    }

    fn prism_row_element(
        &self,
        dialog: &PrismImportDialog,
        row: &PrismRow,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = &self.theme;
        let instance = &row.instance;
        let folder = instance.folder.clone();
        let selected = dialog.selected.contains(&instance.folder);
        let eligible = instance.eligible;
        let meta = format!(
            "{}{}{}{}",
            if instance.minecraft_version.is_empty() {
                "Unknown Minecraft".to_string()
            } else {
                format!("MC {}", instance.minecraft_version)
            },
            if instance.loader.is_empty() {
                String::new()
            } else {
                format!(" · {}", instance.loader)
            },
            if instance.group.is_empty() {
                String::new()
            } else {
                format!(" · {}", instance.group)
            },
            row.size
                .map(|bytes| format!(" · {}", format_bytes(bytes)))
                .unwrap_or_else(|| " · …".into()),
        );
        let mut head = btn(t, "", ButtonVariant::Ghost)
            .id(format!("prism-row:{folder}"))
            .debug_selector({
                let folder = folder.clone();
                move || format!("prism-row:{folder}").into()
            })
            .aria_label(format!("Toggle import for {}", instance.name))
            .w_full()
            .h_auto()
            .p_2()
            .justify_start()
            .flex()
            .items_center()
            .gap_2()
            .child(icon(if selected { "check" } else { "square" }, 16.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_ellipsis()
                            .child(if instance.name.is_empty() {
                                instance.folder.clone()
                            } else {
                                instance.name.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(t.muted_foreground())
                            .child(meta),
                    ),
            );
        if instance.gtnh {
            head = head.child(badge(t, "GTNH", BadgeKind::Normal));
        } else if eligible {
            head = head.child(
                badge(t, "Other", BadgeKind::Danger)
                    .h(px(20.0))
                    .py_0()
                    .text_size(px(10.0)),
            );
        }
        if !eligible {
            head = head.child(
                div()
                    .text_size(px(10.0))
                    .text_color(t.destructive())
                    .child("No mmc-pack.json"),
            );
        }
        if let Some(suggested) = &instance.suggested_version {
            head = head.child(
                div()
                    .text_size(px(10.0))
                    .text_color(t.muted_foreground())
                    .child(format!("Name suggests {suggested}")),
            );
        }
        let mut card = div()
            .rounded(px(6.0))
            .border_1()
            .border_color(if selected {
                tint(t.primary(), 0.45)
            } else {
                tint(t.border(), 0.8)
            })
            .bg(if selected {
                tint(t.primary(), 0.07)
            } else {
                gpui::rgba(0x00000000)
            })
            .flex()
            .flex_col()
            .child(
                head.when(!eligible, |button| button.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        v.toggle_prism_row(&folder, cx)
                    })),
            );
        if eligible && instance.gtnh {
            let folder = instance.folder.clone();
            card = card.child(
                div().px_2().pb_2().child(
                    btn(
                        t,
                        &format!("GTNH version: {}", dialog.version_label(&instance.folder)),
                        ButtonVariant::Outline,
                    )
                    .id(format!("prism-version:{folder}"))
                    .debug_selector({
                        let folder = folder.clone();
                        move || format!("prism-version:{folder}")
                    })
                    .w_full()
                    .justify_start()
                    .text_xs()
                    .when(dialog.catalog.is_none(), |button| button.opacity(0.65))
                    .on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _, cx| {
                            if let Some(Dialog::PrismImport(dialog)) = &mut v.dialog {
                                let available = dialog.catalog.is_some();
                                if let Some(row) =
                                    dialog.rows.iter().find(|row| row.instance.folder == folder)
                                {
                                    if available {
                                        dialog.picking =
                                            Some(PickTarget::Instance(row.instance.folder.clone()));
                                        dialog.search = FieldState::empty();
                                    }
                                }
                            }
                            cx.notify();
                        },
                    )),
                ),
            );
        }
        card
    }

    fn prism_version_picker(&mut self, dialog: &PrismImportDialog, cx: &mut Context<Self>) -> Div {
        let theme = self.theme.clone();
        let t = &theme;
        let (heading, target, selected) = match &dialog.picking {
            Some(PickTarget::All) => (
                "Set every GTNH instance to the same version".to_string(),
                PickTarget::All,
                None,
            ),
            Some(PickTarget::Instance(folder)) => (
                format!("Choose a version for {}", dialog.row_name(folder)),
                PickTarget::Instance(folder.clone()),
                dialog.versions.get(folder).cloned(),
            ),
            None => return div(),
        };
        let choices = dialog.catalog.as_deref();
        let options = version_options(choices, &dialog.search.text, selected.as_deref());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .min_h_0()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(heading))
            .child(
                div().text_xs().text_color(t.muted_foreground()).child(
                    "“Imported (no updates)” keeps the copy frozen. Choosing an official version enables update checks, updates, and clean reinstall.",
                ),
            )
            .child(self.field("prism-version-search", "Search versions", cx))
            .child(
                div()
                    .id("prism-version-list")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .children(options.into_iter().map(|option| {
                        let value = option.value.clone();
                        let target = target.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                crate::ui::list_item(t, &option.label, option.selected)
                                    .id(format!(
                                        "prism-version-choice:{}",
                                        option.label
                                    ))
                                    .debug_selector({
                                        let label = option.label.clone();
                                        move || {
                                            format!("prism-version-choice:{label}").into()
                                        }
                                    })
                                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                        v.pick_prism_version(target.clone(), value.clone(), cx)
                                    })),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .text_size(px(10.0))
                                    .text_color(t.muted_foreground())
                                    .child(option.meta),
                            )
                    })),
            )
    }

    pub(super) fn open_import_version_dialog(&mut self, id: &str, cx: &mut Context<Self>) {
        static TOKEN: AtomicU64 = AtomicU64::new(1);
        let Some(info) = self.data.selected_instance(Some(id)) else {
            return;
        };
        if !info.settings.imported || !info.settings.imported_gtnh {
            return;
        }
        let token = TOKEN.fetch_add(1, Ordering::Relaxed);
        self.dialog = Some(Dialog::ImportVersion(ImportVersionDialog {
            id: info.id.clone(),
            name: super::workspace::display_name(info),
            current: if info.settings.pack_version.is_empty() {
                None
            } else {
                Some(info.settings.pack_version.clone())
            },
            versions: None,
            loading: true,
            saving: false,
            error: None,
            search: FieldState::empty(),
            generation: token,
        }));
        self.focused_field = None;
        cx.notify();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(fetch_gtnh_versions()).await;
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::ImportVersion(dialog)) = &mut v.dialog else {
                    return;
                };
                if dialog.generation != token {
                    return;
                }
                dialog.loading = false;
                match result {
                    Ok(Ok(catalog)) => dialog.versions = Some(pack_catalog::choices(&catalog)),
                    Ok(Err(error)) => dialog.error = Some(format!("{error:#}")),
                    Err(error) => dialog.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn submit_import_version(&mut self, value: Option<String>, cx: &mut Context<Self>) {
        let Some(Dialog::ImportVersion(dialog)) = self.dialog.clone() else {
            return;
        };
        if dialog.saving {
            return;
        }
        let java = dialog.versions.as_ref().and_then(|choices| {
            value
                .as_deref()
                .and_then(|v| java_type_for(Some(choices), v))
        });
        let paths = self.data.paths.clone();
        let id = dialog.id.clone();
        let result_id = id.clone();
        let result_value = value.clone();
        let rt = self.data.rt.clone();
        if let Some(Dialog::ImportVersion(dialog)) = &mut self.dialog {
            dialog.saving = true;
            dialog.error = None;
        }
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    prism::assign_pack_version(&paths, &id, value.as_deref(), java.as_deref()).await
                })
                .await;
            let failed = !matches!(result, Ok(Ok(())));
            let _ = view.update(cx, |v, cx| {
                if failed {
                    if let Some(Dialog::ImportVersion(dialog)) = &mut v.dialog {
                        if dialog.id == result_id {
                            dialog.saving = false;
                            dialog.error = Some(match result {
                                Ok(Err(error)) => format!("{error:#}"),
                                Err(error) => error.to_string(),
                                _ => unreachable!(),
                            });
                        }
                    }
                } else {
                    v.dialog = None;
                    v.focused_field = None;
                    v.data.status = if result_value.is_some() {
                        "GTNH version assigned; update checks enabled".into()
                    } else {
                        "GTNH version cleared; imported copy stays frozen".into()
                    };
                    v.refresh(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn import_version_dialog(
        &mut self,
        dialog: &ImportVersionDialog,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme.clone();
        let t = &theme;
        let mut body = div().flex().flex_col().gap_2().min_h_0().child(
            div()
                .text_xs()
                .text_color(t.muted_foreground())
                .child(format!(
                    "Choose the official GTNH version for {}.",
                    dialog.name
                )),
        );
        body = body.child(div().text_xs().text_color(t.muted_foreground()).child(
            "Assigning a version enables update checks, updates, and clean reinstall. “Imported (no updates)” keeps this copy frozen.",
        ));
        if dialog.loading {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("loader-circle", 16.0))
                    .child("Loading GTNH versions…"),
            );
        } else {
            body = body.child(self.field("import-version-search", "Search versions", cx));
            let options = version_options(
                dialog.versions.as_deref(),
                &dialog.search.text,
                dialog.current.as_deref(),
            );
            body = body.child(
                div()
                    .id("import-version-list")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .children(options.into_iter().map(|option| {
                        let value = option.value.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                crate::ui::list_item(t, &option.label, option.selected)
                                    .debug_selector({
                                        let label = option.label.clone();
                                        move || format!("import-version-choice:{label}").into()
                                    })
                                    .when(dialog.saving, |item| item.opacity(0.5).cursor_default())
                                    .on_click(cx.listener(
                                        move |v, _: &gpui::ClickEvent, _, cx| {
                                            v.submit_import_version(value.clone(), cx)
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .text_size(px(10.0))
                                    .text_color(t.muted_foreground())
                                    .child(option.meta),
                            )
                    })),
            );
            if let Some(error) = &dialog.error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(t.destructive())
                        .child(error.clone()),
                );
            }
        }
        let footer = div().child(
            btn(t, "Cancel", ButtonVariant::Ghost)
                .debug_selector(|| "import-version-cancel".into())
                .when(dialog.saving, |button| button.opacity(0.5).cursor_default())
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx))),
        );
        crate::ui::dialog_shell_with_width(
            &theme,
            if dialog.current.is_some() {
                "Change GTNH version"
            } else {
                "Set GTNH version"
            },
            460.0,
            body,
            footer,
        )
        .on_key_down(cx.listener(
            |this: &mut Self, event: &KeyDownEvent, _: &mut Window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_dialog(cx);
                }
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::TestAppContext;

    fn prism_fixture(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "industrialis-prism-ui-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let gtnh = root.join("instances/GTNH 2.8.0");
        std::fs::create_dir_all(gtnh.join(".minecraft/config/GregTech")).unwrap();
        std::fs::write(
            gtnh.join("instance.cfg"),
            "name=GTNH 2.8.0\nOverrideMemory=true\nMinMemAlloc=512\nMaxMemAlloc=6144\n",
        )
        .unwrap();
        std::fs::write(
            gtnh.join("mmc-pack.json"),
            r#"{"formatVersion":1,"components":[
                {"uid":"net.minecraft","version":"1.7.10"},
                {"uid":"net.minecraftforge","version":"10.13.4.1614"}
            ]}"#,
        )
        .unwrap();
        let plain = root.join("instances/Plain pack");
        std::fs::create_dir_all(plain.join(".minecraft")).unwrap();
        std::fs::write(plain.join("instance.cfg"), "name=Plain pack\n").unwrap();
        std::fs::write(
            plain.join("mmc-pack.json"),
            r#"{"formatVersion":1,"components":[
                {"uid":"net.minecraft","version":"1.20.1"}
            ]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("instances/instgroups.json"),
            r#"{"formatVersion":1,"groups":{"GTNH":{"hidden":false,"instances":["GTNH 2.8.0"]}}}"#,
        )
        .unwrap();
        root
    }

    #[gpui::test]
    async fn onboarding_prompt_imports_selected_instances_and_writes_the_marker(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&[]);
        let prism_root = prism_fixture("onboarding");
        let settings_path = fixture.paths.launcher_settings_path();
        let (window, root) = fixture.open(cx, |_| {});
        root.update(cx, |v, cx| {
            v.maybe_prompt_prism_import(cx);
            let Some(Dialog::PrismImport(dialog)) = &mut v.dialog else {
                panic!("first-run Prism prompt did not open");
            };
            assert_eq!(dialog.step, PrismStep::Ask);
            dialog.detect_root = Some(prism_root.clone());
        });

        Fixture::click(cx, window, "prism-import-start");
        fixture
            .until(cx, &root, |v| {
                matches!(
                    &v.dialog,
                    Some(Dialog::PrismImport(dialog))
                        if !dialog.loading && !dialog.rows.is_empty()
                )
            })
            .await;
        root.read_with(cx, |v, _| {
            let Some(Dialog::PrismImport(dialog)) = &v.dialog else {
                panic!("import dialog closed unexpectedly");
            };
            assert!(dialog.selected.contains("GTNH 2.8.0"));
            assert!(!dialog.selected.contains("Plain pack"));
            assert!(dialog.versions.is_empty(), "imports default to frozen");
            assert!(dialog.rows.iter().any(|row| row.instance.gtnh));
        });

        Fixture::click(cx, window, "prism-row:Plain pack");
        Fixture::click(cx, window, "prism-import-submit");
        fixture
            .until(cx, &root, |v| v.data.instances.len() == 2)
            .await;
        let (gtnh, plain) = root.read_with(cx, |v, _| {
            assert!(v.data.settings.prism_import_prompted);
            let gtnh = v
                .data
                .instances
                .iter()
                .find(|info| info.id == "GTNH_2.8.0")
                .cloned()
                .expect("GTNH copy imported");
            let plain = v
                .data
                .instances
                .iter()
                .find(|info| info.id == "Plain_pack")
                .cloned()
                .expect("non-GTNH copy imported");
            (gtnh, plain)
        });
        assert!(gtnh.settings.imported);
        assert!(gtnh.settings.imported_gtnh);
        assert!(gtnh.settings.pack_version.is_empty());
        assert_eq!(gtnh.group, "GTNH");
        assert_eq!(gtnh.settings.max_ram_mb, 6144);
        assert!(plain.settings.imported);
        assert!(!plain.settings.imported_gtnh);
        assert!(plain.settings.pack_version.is_empty());
        assert!(gtnh.icon_path.is_some(), "bundled GTNH icon installed");
        assert!(prism_root
            .join("instances/GTNH 2.8.0/mmc-pack.json")
            .exists());

        fixture
            .until(cx, &root, |_| {
                std::fs::read_to_string(&settings_path)
                    .map(|raw| raw.contains("prism_import_prompted"))
                    .unwrap_or(false)
            })
            .await;
        let _ = std::fs::remove_dir_all(fixture.paths.base());
        let _ = std::fs::remove_dir_all(prism_root);
    }

    #[gpui::test]
    async fn start_fresh_writes_the_marker_without_importing(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&[]);
        let settings_path = fixture.paths.launcher_settings_path();
        let (window, root) = fixture.open(cx, |_| {});
        root.update(cx, |v, cx| v.maybe_prompt_prism_import(cx));
        Fixture::click(cx, window, "prism-import-fresh");
        root.read_with(cx, |v, _| {
            assert!(v.dialog.is_none());
            assert!(v.data.settings.prism_import_prompted);
            assert!(v.data.instances.is_empty());
        });
        fixture
            .until(cx, &root, |_| {
                std::fs::read_to_string(&settings_path)
                    .map(|raw| raw.contains("prism_import_prompted"))
                    .unwrap_or(false)
            })
            .await;
        let _ = std::fs::remove_dir_all(fixture.paths.base());
    }
}
