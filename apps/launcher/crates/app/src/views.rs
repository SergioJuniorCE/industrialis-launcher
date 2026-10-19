//! Root view + tabs. Layout mirrors `App.tsx`: top toolbar, instance grid
//! with detail panel, processes / accounts / settings tabs, status bar.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Context, InteractiveElement, IntoElement, KeyDownEvent, Render,
    UniformListScrollHandle, Window,
};
use industrialis_backend::accounts::account_to_info;
use industrialis_backend::launch::{
    kill_instance, launch_instance_with_started, running_instances,
};
use industrialis_backend::launch_config::EmitLaunchLog;
use industrialis_backend::settings::{load_launcher_settings, save_launcher_preferences};
use industrialis_core::launch_log::{
    classify_launch_log_line, LaunchLogLevel, LaunchLogLine, LogStream,
};
use industrialis_core::launcher_settings::{
    resolve_default_account_id, LauncherSettingsData, ThemeMode, WindowMaterial,
};

mod accounts;
mod appearance_settings;
mod backups;
mod content;
mod custom_mods;
mod editor_window;
mod file_explorer;
mod file_actions;
mod explorer_search;
mod java_chooser;
mod library_drag;
mod library_groups;
mod library_icons;
mod lifetime;
mod minecraft_files;
mod pack_catalog;
mod preferences;
mod shell;
#[cfg(test)]
mod test_support;
mod themes;
mod updater;
mod vsx_themes;
use preferences::PreferencesUi;
mod workspace;
use workspace::WorkspaceUi;
mod console;
mod instance_actions;
mod processes;
use console::ConsoleUi;
use processes::ProcessesUi;
mod instance_settings;
use instance_settings::{DetailTab, InstanceUi};

use crate::input::FieldState;
use accounts::AccountsState;

use crate::state::{AppData, LogLine};
use crate::theme::ActiveTheme;
use crate::ui::{section_title, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Instances,
    Processes,
    Accounts,
    Settings,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Instances => "Instances",
            Tab::Processes => "Processes",
            Tab::Accounts => "Accounts",
            Tab::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewInstanceDialog {
    name: FieldState,
    group: FieldState,
    version: String,
    java_type: String,
    versions: Option<Vec<pack_catalog::VersionChoice>>,
    search: FieldState,
    filter: pack_catalog::ReleaseFilter,
    token: u64,
    versions_loading: bool,
    installing: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Dialog {
    NewInstance(NewInstanceDialog),
    DeleteInstance { id: String, name: String },
    CopyInstance(instance_actions::CopyDialog),
    Maintenance(instance_actions::MaintenanceDialog),
    LauncherUpdate,
    Group(library_groups::GroupDialog),
    Icon(library_icons::IconDialog),
}

pub struct LauncherRoot {
    pub data: AppData,
    closing: Option<Option<String>>,
    restoring: HashSet<String>,
    pub theme: ActiveTheme,
    pub(crate) launcher_update: updater::UpdaterUi,
    backdrop: crate::appearance::BackdropState,
    settings_save_pending: Option<LauncherSettingsData>,
    settings_saving: bool,
    settings_save_error: Option<String>,
    settings_saved: LauncherSettingsData,
    refresh_generation: u64,
    profile_issues: Vec<String>,
    pub tab: Tab,
    pub selected_id: Option<String>,
    log_scroll: UniformListScrollHandle,
    window_handle: Option<gpui::AnyWindowHandle>,
    dialog: Option<Dialog>,
    modal_focus: Option<gpui::FocusHandle>,
    modal_visible: bool,
    focused_field: Option<String>,
    native_fields: HashMap<String, gpui::Entity<crate::input::NativeField>>,
    /// Long operations in flight (`install:<id>`), shown in the status bar.
    busy: HashSet<String>,
    java_ui: java_chooser::JavaChooser,
    settings_java: FieldState,
    settings_retention: FieldState,
    settings_width: FieldState,
    settings_height: FieldState,
    preferences_ui: PreferencesUi,
    themes_ui: themes::ThemesUi,
    vsx_ui: vsx_themes::OpenVsxUi,
    fallback_field: FieldState,
    accounts_ui: AccountsState,
    instance_ui: InstanceUi,
    workspace_ui: WorkspaceUi,
    groups_ui: library_groups::GroupsUi,
    icons_ui: library_icons::IconsUi,
    catalog_ui: pack_catalog::CatalogUi,
    files_ui: minecraft_files::FilesUi,
    mods_ui: custom_mods::ModsUi,
    content_ui: content::ContentUi,
    backups_ui: backups::BackupsUi,
    processes_ui: ProcessesUi,
    console_ui: ConsoleUi,
    operation_progress: HashMap<String, industrialis_backend::pack_files::PackProgress>,
}

impl LauncherRoot {
    pub(crate) fn set_profile_issues(&mut self, issues: Vec<String>) {
        self.profile_issues = issues;
    }
    pub fn new(data: AppData) -> Self {
        let theme = ActiveTheme::from_preferences(&data.settings);
        let settings_java = FieldState::new(
            data.settings
                .default_java_path
                .as_deref()
                .unwrap_or_default(),
        );
        let settings_retention = FieldState::new(&data.settings.backup_retention_limit.to_string());
        let workspace_ui = WorkspaceUi::load(&data.paths);
        let settings_width = FieldState::new(&data.settings.window.window_width.to_string());
        let settings_height = FieldState::new(&data.settings.window.window_height.to_string());
        let settings_saved = data.settings.clone();
        let backups_ui = backups::BackupsUi::new(data.paths.clone());
        Self {
            data,
            closing: None,
            restoring: Default::default(),
            theme,
            launcher_update: updater::UpdaterUi::new(cfg!(feature = "packaged-updates")),
            backdrop: crate::appearance::BackdropState::default(),
            settings_save_pending: None,
            settings_saving: false,
            settings_save_error: None,
            settings_saved,
            refresh_generation: 0,
            profile_issues: Vec::new(),
            tab: Tab::Instances,
            selected_id: None,
            log_scroll: UniformListScrollHandle::new(),
            window_handle: None,
            dialog: None,
            modal_focus: None,
            modal_visible: false,
            focused_field: None,
            native_fields: HashMap::new(),
            busy: HashSet::new(),
            java_ui: Default::default(),
            settings_java,
            settings_retention,
            settings_width,
            settings_height,
            preferences_ui: PreferencesUi::default(),
            themes_ui: Default::default(),
            vsx_ui: Default::default(),
            fallback_field: FieldState::empty(),
            accounts_ui: AccountsState::default(),
            instance_ui: InstanceUi::default(),
            workspace_ui,
            groups_ui: Default::default(),
            icons_ui: Default::default(),
            catalog_ui: Default::default(),
            files_ui: Default::default(),
            mods_ui: Default::default(),
            content_ui: Default::default(),
            backups_ui,
            processes_ui: ProcessesUi::default(),
            console_ui: ConsoleUi::default(),
            operation_progress: HashMap::new(),
        }
    }

    fn rebuild_theme(&mut self) {
        self.theme = ActiveTheme::from_preferences(&self.data.settings);
    }

    // ── actions ──

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_generation += 1;
        let generation = self.refresh_generation;
        self.data.status = "Refreshing…".to_string();
        cx.notify();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        cx.spawn(async move |view, cx| {
            let loaded = rt
                .spawn(async move {
                    let instances =
                        industrialis_backend::instances::load_instances_with_issues(&paths).await;
                    let accounts =
                        industrialis_backend::accounts::load_accounts_with_issues(&paths).await;
                    let running = running_instances(&paths).await;
                    let settings = load_launcher_settings(&paths).await;
                    (instances, accounts, running, settings)
                })
                .await;
            let (instances, accounts, running, settings) = match loaded {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    let _ = view.update(cx, |v, cx| {
                        if generation == v.refresh_generation {
                            v.data.status = format!("Refresh task failed: {e}");
                            v.profile_issues = vec![v.data.status.clone()];
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let _ = view.update(cx, |v: &mut LauncherRoot, cx| {
                if generation != v.refresh_generation {
                    return;
                }
                let mut issues = Vec::new();
                match instances {
                    Ok((instances, warnings)) => {
                        v.data.instances = instances;
                        v.refresh_groups(cx);
                        issues.extend(warnings);
                    }
                    Err(e) => issues.push(format!("Could not refresh instances: {e:#}")),
                }
                match accounts {
                    Ok((accounts, skipped)) => {
                        v.data.accounts = accounts.iter().map(account_to_info).collect();
                        if skipped > 0 {
                            issues.push(format!(
                                "{skipped} unrecognized account entries were kept for recovery."
                            ));
                        }
                    }
                    Err(e) => issues.push(format!("Could not refresh accounts: {e:#}")),
                }
                match running {
                    Ok(running) => v.data.replace_running(
                        running
                            .into_iter()
                            .map(|(id, pid, alive)| (id, (pid, alive)))
                            .collect(),
                    ),
                    Err(e) => issues.push(format!("Could not refresh running games: {e:#}")),
                }
                if let Err(e) = &settings {
                    issues.push(format!("Could not refresh preferences: {e:#}"));
                }
                if let Some(settings) = settings
                    .ok()
                    .filter(|_| !v.settings_saving && v.settings_save_pending.is_none())
                {
                    v.data.default_account_id =
                        resolve_default_account_id(&settings).map(str::to_string);
                    v.settings_saved = settings.clone();
                    v.data.settings = settings;
                    v.rebuild_theme();
                    v.settings_java = FieldState::new(
                        v.data
                            .settings
                            .default_java_path
                            .as_deref()
                            .unwrap_or_default(),
                    );
                    v.settings_retention =
                        FieldState::new(&v.data.settings.backup_retention_limit.to_string());
                }
                if let Some(selected) = v.selected_id.clone() {
                    if !v.data.instances.iter().any(|i| i.id == selected) {
                        v.selected_id = None;
                    }
                }
                v.profile_issues = issues.clone();
                v.data.status = if issues.is_empty() {
                    "Refreshed".into()
                } else {
                    issues.join("\n")
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn play(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(key) = self.files_ui.dirty_key(&id) {
            let target = id.clone();
            self.save_file_then(key, move |v, cx| v.play(target, cx), cx);
            return;
        }
        let target = id.clone();
        self.after_instance_settings_saved(id, move |v, cx| v.start_launch(target, cx), cx);
    }

    fn start_launch(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        if self.data.launching.contains(&id) || self.data.is_running(&id) {
            return;
        }
        if !self.ensure_launch_account(&id, cx) {
            return;
        }
        self.invalidate_console(&id);
        self.selected_id = Some(id.clone());
        self.tab = Tab::Instances;
        self.instance_ui.detail_tab = DetailTab::Console;
        self.data.launching.insert(id.clone());
        self.data.status = format!("Launching {id}…");
        cx.notify();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let shared: Arc<Mutex<Vec<LogLine>>> = Arc::new(Mutex::new(Vec::new()));
        let emit_shared = shared.clone();
        let emit: EmitLaunchLog = Arc::new(move |_id: String, stream: String, line: String| {
            emit_shared.lock().unwrap().push(LogLine { stream, line });
        });
        let done = Arc::new(AtomicBool::new(false));
        let done_task = done.clone();
        let started = Arc::new(AtomicU32::new(0));
        let notify_started = started.clone();
        let launch_settings = self
            .data
            .selected_instance(Some(&id))
            .map(|i| i.settings.clone())
            .unwrap_or_default();
        let hide_after_start =
            launch_settings.override_window && launch_settings.close_after_launch;
        let id_task = id.clone();
        let id_done = id_task.clone();
        cx.spawn(async move |view, cx| {
            let outcome = rt
                .spawn(async move {
                    let launch = launch_instance_with_started(
                        &paths,
                        &id_task,
                        emit,
                        Arc::new(move |pid| {
                            notify_started.store(pid, Ordering::SeqCst);
                        }),
                    )
                    .await;
                    let instances = industrialis_backend::instances::load_instances(&paths)
                        .await
                        .unwrap_or_default();
                    let running = running_instances(&paths).await.unwrap_or_default();
                    (launch, instances, running)
                })
                .await;
            done_task.store(true, Ordering::SeqCst);
            let _ = view.update(cx, |v: &mut LauncherRoot, cx| {
                v.data.game_exited(&id_done);
                v.forget_instance_editor(&id_done);
                match &outcome {
                    Ok((launch, instances, running)) => {
                        v.data.instances = instances.clone();
                        v.data.replace_running(
                            running
                                .iter()
                                .map(|(id, pid, alive)| (id.clone(), (*pid, *alive)))
                                .collect(),
                        );
                        v.data.status = match launch {
                            Ok(()) => format!("{id_done} exited cleanly"),
                            Err(e) => format!("Launch failed: {e:#}"),
                        };
                    }
                    Err(e) => v.data.status = format!("Launch task failed: {e}"),
                }
                let failed = !matches!(&outcome, Ok((Ok(()), _, _)));
                if failed
                    && (!launch_settings.override_console || launch_settings.show_console_on_error)
                {
                    v.tab = Tab::Instances;
                    v.selected_id = Some(id_done.clone());
                    v.instance_ui.detail_tab = DetailTab::Console;
                    if let Some(handle) = v.window_handle {
                        let _ = cx.update_window(handle, |_, window, _| window.activate_window());
                    }
                    cx.activate(true);
                } else if !failed
                    && launch_settings.override_console
                    && launch_settings.auto_close_console
                    && v.selected_id.as_ref() == Some(&id_done)
                {
                    v.instance_ui.detail_tab = DetailTab::Settings;
                }
                if !failed
                    && launch_settings.override_window
                    && launch_settings.quit_after_game_stop
                    && !v.data.has_active_games()
                {
                    v.request_close(cx);
                }
                cx.notify();
            });
        })
        .detach();
        let poll_id = id.clone();
        cx.spawn(async move |view, cx| {
            let mut handled_start = false;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let drained = {
                    let mut guard = shared.lock().unwrap();
                    std::mem::take(&mut *guard)
                };
                let finished = done.load(Ordering::SeqCst);
                let stop = view
                    .update(cx, |v: &mut LauncherRoot, cx| {
                        let follow = v.log_scroll.is_scrolled_to_end().unwrap_or(true);
                        v.append_console_lines(&poll_id, drained);
                        if !handled_start && started.load(Ordering::SeqCst) != 0 {
                            if !finished {
                                v.data
                                    .game_started(&poll_id, started.load(Ordering::SeqCst));
                            }
                            handled_start = true;
                            if hide_after_start && !finished {
                                if let Some(handle) = v.window_handle {
                                    let _ = cx.update_window(handle, |_, window, cx| {
                                        crate::desktop_window::hide(window, cx)
                                    });
                                }
                            }
                        }
                        // Follow the live tail while launching; leave the
                        // scroll position alone once the game exits.
                        if v.selected_id.as_deref() == Some(&poll_id)
                            && v.data.is_running(&poll_id)
                            && follow
                        {
                            v.log_scroll.scroll_to_bottom();
                        }
                        cx.notify();
                    })
                    .is_err()
                    || finished;
                if stop {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn kill(&mut self, id: String, cx: &mut Context<Self>) {
        self.data.status = format!("Stopping {id}…");
        cx.notify();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { kill_instance(&paths, &id).await })
                .await;
            let _ = view.update(cx, |v: &mut LauncherRoot, cx| {
                v.data.status = match result {
                    Ok(Ok(true)) => "Stopped".to_string(),
                    Ok(Ok(false)) => "Not running".to_string(),
                    Ok(Err(e)) => format!("Stop failed: {e:#}"),
                    Err(e) => format!("Stop task failed: {e}"),
                };
                cx.notify();
            });
            let _ = view.update(cx, |v: &mut LauncherRoot, cx| v.refresh(cx));
        })
        .detach();
    }

    pub fn set_theme(&mut self, mode: ThemeMode, preset: String, cx: &mut Context<Self>) {
        self.refresh_theme_inputs();
        self.data.settings.theme_mode = mode;
        self.data.settings.theme_preset = preset;
        self.rebuild_theme();
        self.persist_launcher_preferences(cx);
        cx.notify();
    }

    fn set_window_material(&mut self, material: WindowMaterial, cx: &mut Context<Self>) {
        self.data.settings.window_material = material;
        self.persist_launcher_preferences(cx);
        cx.notify();
    }

    /// Coalesce edits made during a save and write the latest snapshot next.
    /// Rapid material/theme changes must not restore an older disk preference.
    fn persist_launcher_preferences(&mut self, cx: &mut Context<Self>) {
        self.refresh_generation += 1;
        self.settings_save_pending = Some(self.data.settings.clone());
        self.settings_save_error = None;
        self.start_preferences_save(cx);
    }

    fn start_preferences_save(&mut self, cx: &mut Context<Self>) {
        if self.settings_saving {
            return;
        }
        let Some(settings) = self.settings_save_pending.take() else {
            return;
        };
        self.settings_saving = true;
        self.settings_save_error = None;
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        let retry_settings = settings.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { save_launcher_preferences(&paths, &settings).await })
                .await;
            let _ = view.update(cx, |v: &mut LauncherRoot, cx| {
                v.settings_saving = false;
                v.refresh_generation += 1;
                match result {
                    Ok(Ok(())) => {
                        v.settings_saved = retry_settings;
                        v.data.status = "Settings saved".into();
                        v.start_preferences_save(cx);
                    }
                    failure => {
                        // A newer edit wins; otherwise retain the failed snapshot
                        // for an explicit retry. Do not loop on a disk error.
                        v.settings_save_pending.get_or_insert(retry_settings);
                        let message = match failure {
                            Ok(Err(e)) => format!("Save failed: {e:#}"),
                            Err(e) => format!("Save task failed: {e}"),
                            _ => unreachable!(),
                        };
                        v.settings_save_error = Some(message.clone());
                        v.data.status = message;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn discard_failed_preferences(&mut self, cx: &mut Context<Self>) {
        if self.settings_saving || self.settings_save_error.is_none() {
            return;
        }
        let mut saved = self.settings_saved.clone();
        // Account operations own these fields independently of preference saves.
        saved.default_account_id = self.data.settings.default_account_id.clone();
        saved.active_account_id = self.data.settings.active_account_id.clone();
        self.settings_save_pending = None;
        self.settings_save_error = None;
        self.refresh_generation += 1;
        self.settings_java =
            FieldState::new(saved.default_java_path.as_deref().unwrap_or_default());
        self.settings_retention = FieldState::new(&saved.backup_retention_limit.to_string());
        self.settings_width = FieldState::new(&saved.window.window_width.to_string());
        self.settings_height = FieldState::new(&saved.window.window_height.to_string());
        self.data.settings = saved;
        self.rebuild_theme();
        self.data.status = "Unsaved preference changes discarded".into();
        cx.notify();
    }

    fn close_allowed(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.protect_file_drafts(cx) {
            return false;
        }
        if self.launcher_update.in_progress() {
            return false;
        }
        if self.groups_ui.busy
            || self.icons_ui.busy
            || self.settings_saving
            || self.settings_save_pending.is_some()
            || self.accounts_ui.is_busy()
            || self.instance_ui.has_unsaved_changes()
        {
            self.data.status = if self.settings_save_error.is_some() {
                "Settings have not been saved. Open Settings to retry.".into()
            } else {
                "Finish saving settings before closing the launcher.".into()
            };
            cx.notify();
            return false;
        }
        true
    }

    // ── fields + dialogs ──

    fn field_state(&self, id: &str) -> &FieldState {
        match (&self.dialog, id) {
            (Some(Dialog::NewInstance(d)), "new-name") => &d.name,
            (Some(Dialog::NewInstance(d)), "new-group") => &d.group,
            (Some(Dialog::NewInstance(d)), "new-version-search") => &d.search,
            (Some(Dialog::Maintenance(d)), "maintenance-search") => &d.search,
            (Some(Dialog::CopyInstance(d)), "copy-name") => &d.name,
            (Some(Dialog::Group(d)), "group-name") => &d.name,
            (_, "account-name") => &self.accounts_ui.offline_username,
            (_, "settings-java") => &self.settings_java,
            (_, "settings-retention") => &self.settings_retention,
            (_, "settings-width") => &self.settings_width,
            (_, "settings-height") => &self.settings_height,
            _ => &self.fallback_field,
        }
    }

    fn field_state_mut(&mut self, id: &str) -> &mut FieldState {
        match (&mut self.dialog, id) {
            (Some(Dialog::NewInstance(d)), "new-name") => &mut d.name,
            (Some(Dialog::NewInstance(d)), "new-group") => &mut d.group,
            (Some(Dialog::NewInstance(d)), "new-version-search") => &mut d.search,
            (Some(Dialog::Maintenance(d)), "maintenance-search") => &mut d.search,
            (Some(Dialog::CopyInstance(d)), "copy-name") => &mut d.name,
            (Some(Dialog::Group(d)), "group-name") => &mut d.name,
            (_, "account-name") => &mut self.accounts_ui.offline_username,
            (_, "settings-java") => &mut self.settings_java,
            (_, "settings-retention") => &mut self.settings_retention,
            (_, "settings-width") => &mut self.settings_width,
            (_, "settings-height") => &mut self.settings_height,
            _ => &mut self.fallback_field,
        }
    }

    fn field(
        &mut self,
        field_id: &'static str,
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        use crate::input::{NativeField, NativeFieldEvent};
        let value = self.field_state(field_id).text.clone();
        if !self.native_fields.contains_key(field_id) {
            let entity =
                cx.new(|cx| NativeField::new(&value, self.theme.clone(), placeholder, false, cx));
            cx.subscribe(&entity, move |this, _, event, cx| {
                if field_id == "group-name" && this.groups_ui.busy
                    || field_id == "account-name" && this.accounts_ui.is_busy()
                {
                    return;
                }
                match event {
                    NativeFieldEvent::Change(value) => {
                        *this.field_state_mut(field_id) = FieldState::new(value)
                    }
                    NativeFieldEvent::Focus => this.focused_field = Some(field_id.into()),
                    NativeFieldEvent::Blur => {}
                    NativeFieldEvent::Submit if field_id.ends_with("search") => {}
                    NativeFieldEvent::Submit if field_id == "account-name" => {
                        this.submit_offline_account(cx)
                    }
                    NativeFieldEvent::Submit if field_id.starts_with("settings-") => {
                        this.save_launcher_settings_form(cx)
                    }
                    NativeFieldEvent::Submit => this.dialog_submit(cx),
                }
                cx.notify();
            })
            .detach();
            self.native_fields.insert(field_id.into(), entity);
        }
        let entity = self.native_fields[field_id].clone();
        let enabled = !(field_id == "group-name" && self.groups_ui.busy
            || field_id == "account-name" && self.accounts_ui.is_busy());
        entity.update(cx, |state, _| {
            state.value = value;
            state.theme = self.theme.clone();
            state.enabled = enabled;
            state.accessibility_label = match field_id {
                "account-name" => "Offline account username",
                "settings-java" => "Default Java executable",
                "settings-retention" => "Backup retention",
                "settings-width" => "Launcher width",
                "settings-height" => "Launcher height",
                "new-name" => "Instance name",
                "new-group" => "Instance group",
                "new-version-search" => "Search pack versions",
                "maintenance-search" => "Search replacement versions",
                "copy-name" => "Copied instance name",
                "group-name" => "Group name",
                _ => placeholder,
            }
            .into();
        });
        div()
            .w_full()
            .debug_selector(move || format!("field-{field_id}").into())
            .child(entity)
    }

    fn close_dialog(&mut self, cx: &mut Context<Self>) {
        if (self.icons_ui.busy && matches!(self.dialog, Some(Dialog::Icon(_))))
            || self.launcher_update.in_progress()
            || (self.groups_ui.busy && matches!(self.dialog, Some(Dialog::Group(_))))
        {
            return;
        }
        if matches!(&self.dialog, Some(Dialog::Maintenance(d)) if d.is_analyzing()) {
            return;
        }
        self.dialog = None;
        self.processes_ui.dropdown = false;
        self.focused_field = None;
        cx.notify();
    }

    fn dialog_submit(&mut self, cx: &mut Context<Self>) {
        if matches!(self.dialog, Some(Dialog::Group(_))) {
            self.submit_group_dialog(cx);
            return;
        }
        if matches!(self.dialog, Some(Dialog::NewInstance(_))) {
            self.submit_install(cx);
        } else if matches!(self.dialog, Some(Dialog::CopyInstance(_))) {
            self.submit_copy(cx);
        }
    }

    fn open_new_instance(&mut self, cx: &mut Context<Self>) {
        self.open_new_instance_in_group(String::new(), cx);
    }
    fn open_new_instance_in_group(&mut self, group: String, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        self.workspace_ui.menu = None;
        self.dialog = Some(Dialog::NewInstance(NewInstanceDialog {
            name: FieldState::empty(),
            group: FieldState::new(&group),
            version: String::new(),
            java_type: "java17+".to_string(),
            versions: None,
            search: FieldState::empty(),
            filter: pack_catalog::ReleaseFilter::All,
            token: 0,
            versions_loading: true,
            installing: false,
            error: None,
        }));
        self.focused_field = Some("new-name".to_string());
        cx.notify();
        self.load_new_catalog(cx);
    }

    fn submit_install(&mut self, cx: &mut Context<Self>) {
        let (name, group, version, java_type) = match self.dialog.as_ref() {
            Some(Dialog::NewInstance(state))
                if !state.installing
                    && !state.versions_loading
                    && state.versions.as_ref().is_some_and(|versions| {
                        versions.iter().any(|version| {
                            version.name == state.version && version.available(&state.java_type)
                        })
                    }) =>
            {
                (
                    state.name.text.clone(),
                    state.group.text.clone(),
                    state.version.clone(),
                    state.java_type.clone(),
                )
            }
            _ => return,
        };
        if version.is_empty() {
            return;
        }
        let id_base = if name.trim().is_empty() {
            format!("gtnh-{version}")
        } else {
            name.clone()
        };
        let id = match industrialis_backend::paths::validate_instance_id(&id_base) {
            Ok(id) => id,
            Err(e) => {
                if let Some(Dialog::NewInstance(state)) = self.dialog.as_mut() {
                    state.installing = false;
                    state.error = Some(format!("{e:#}"));
                }
                cx.notify();
                return;
            }
        };
        self.begin_install(id, name, group, version, java_type, cx);
    }

    fn open_delete(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let target = id.clone();
        self.after_instance_settings_saved(id, move |v, cx| v.open_delete_saved(target, cx), cx);
    }

    fn open_delete_saved(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) {
            return;
        }
        let name = self
            .data
            .selected_instance(Some(&id))
            .map(|i| {
                if i.settings.name.is_empty() {
                    i.id.clone()
                } else {
                    i.settings.name.clone()
                }
            })
            .unwrap_or_else(|| id.clone());
        self.dialog = Some(Dialog::DeleteInstance { id, name });
        cx.notify();
    }

    fn submit_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(Dialog::DeleteInstance { id, .. }) = self.dialog.clone() {
            self.begin_delete(id, cx);
        }
    }

    fn save_launcher_settings_form(&mut self, cx: &mut Context<Self>) {
        let java = self.settings_java.text.trim().to_string();
        if java.len() > 4096 {
            self.data.status = "Invalid default Java path".to_string();
            cx.notify();
            return;
        }
        let retention: u32 = match self.settings_retention.text.trim().parse() {
            Ok(n) if (1..=1000).contains(&n) => n,
            _ => {
                self.data.status = "Backup retention must be 1–1000".to_string();
                cx.notify();
                return;
            }
        };
        let width = industrialis_core::launcher_window::validate_launcher_window_dimension(
            &self.settings_width.text,
            "Width",
            800,
            7680,
        );
        let height = industrialis_core::launcher_window::validate_launcher_window_dimension(
            &self.settings_height.text,
            "Height",
            600,
            4320,
        );
        let (Some(width), Some(height)) = (width.value, height.value) else {
            self.data.status = width
                .error
                .or(height.error)
                .unwrap_or_else(|| "Invalid window size".into());
            cx.notify();
            return;
        };
        self.data.settings.window.window_width = width;
        self.data.settings.window.window_height = height;
        self.data.settings.default_java_path = if java.is_empty() { None } else { Some(java) };
        self.data.settings.backup_retention_limit = retention;
        self.persist_launcher_preferences(cx);
        cx.notify();
    }

    // ── render helpers ──

    /// Log colors mirror the web `launchLogLevelClass` (fixed palette, not theme).
    fn log_line_color(entry: &LogLine) -> gpui::Rgba {
        let level = classify_launch_log_line(&LaunchLogLine {
            stream: match entry.stream.as_str() {
                "stderr" => LogStream::Stderr,
                "system" => LogStream::System,
                _ => LogStream::Stdout,
            },
            line: entry.line.clone(),
        });
        match level {
            LaunchLogLevel::Error => gpui::rgb(0xf87171),
            LaunchLogLevel::Warn => gpui::rgb(0xfacc15),
            LaunchLogLevel::Info => gpui::rgb(0x4ade80),
            LaunchLogLevel::System => gpui::rgb(0x38bdf8),
        }
    }

    fn log_line_div(entry: &LogLine) -> gpui::Div {
        div()
            .text_size(px(11.0))
            .line_height(px(18.0))
            .h(px(18.0))
            .whitespace_nowrap()
            .text_color(Self::log_line_color(entry))
            .child(entry.line.clone())
    }

    fn render_dialog(&mut self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let t = self.theme.clone();
        let dialog = self.dialog.clone()?;
        match dialog {
            Dialog::NewInstance(state) => Some(self.new_instance_dialog(&t, &state, cx)),
            Dialog::DeleteInstance { id, name } => Some(self.delete_dialog(&t, &id, &name, cx)),
            Dialog::CopyInstance(state) => Some(self.copy_dialog(&state, cx)),
            Dialog::Maintenance(state) => Some(self.maintenance_dialog(&state, cx)),
            Dialog::LauncherUpdate => Some(self.launcher_update_dialog(cx)),
            Dialog::Group(state) => Some(self.group_dialog(&state, cx)),
            Dialog::Icon(state) => Some(self.icon_dialog(&state, cx)),
        }
    }

    fn dialog_frame(
        &self,
        title: &str,
        body: gpui::Div,
        footer: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let frame = crate::ui::dialog_shell(&self.theme, title, body, footer);
        frame.on_key_down(cx.listener(
            |this: &mut Self, event: &KeyDownEvent, _window: &mut Window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_dialog(cx);
                }
            },
        ))
    }

    fn dialog_footer(
        &self,
        cancel_label: &str,
        confirm_label: &str,
        confirm: ButtonVariant,
        on_confirm: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                crate::ui::btn(&self.theme, cancel_label, ButtonVariant::Ghost).on_click(
                    cx.listener(
                        |this: &mut Self, _event: &gpui::ClickEvent, _window: &mut Window, cx| {
                            this.close_dialog(cx)
                        },
                    ),
                ),
            )
            .child(
                crate::ui::btn(&self.theme, confirm_label, confirm).on_click(cx.listener(
                    move |this: &mut Self, _event: &gpui::ClickEvent, _window: &mut Window, cx| {
                        on_confirm(this, cx)
                    },
                )),
            )
    }

    fn delete_dialog(
        &mut self,
        t: &ActiveTheme,
        id: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let body = div().flex().flex_col().gap_2().child(
            div().text_sm().text_color(t.foreground()).child(format!(
                "Delete instance “{name}” ({id})? Saves and configs will be removed."
            )),
        );
        let footer = self.dialog_footer(
            "Cancel",
            "Delete",
            ButtonVariant::Danger,
            |this, cx| this.submit_delete(cx),
            cx,
        );
        self.dialog_frame("Delete instance", body, footer, cx)
    }
}

impl Render for LauncherRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_handle = Some(window.window_handle());
        if !cx.has_active_drag() {
            self.workspace_ui.drag.reset();
        }
        if self.closing.is_some() {
            return self.closing_view(cx).into_any_element();
        }
        self.backdrop.sync(
            window,
            self.data.settings.window_material,
            self.data.settings.theme_mode,
        );
        self.theme.window_material = self.backdrop.effective;
        self.theme.window_tint_opacity = self.data.settings.window_tint_opacity;
        let modal = self.dialog.is_some();
        let modal_focus = self
            .modal_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if modal && !self.modal_visible {
            window.focus(&modal_focus, cx);
        }
        self.modal_visible = modal;
        let t = self.theme.clone();
        let content = match self.tab {
            Tab::Instances => self.instances_view(window, cx).into_any_element(),
            Tab::Processes => self.processes_view(cx).into_any_element(),
            Tab::Accounts => self.accounts_view(cx).into_any_element(),
            Tab::Settings => self.settings_view(cx).into_any_element(),
        };
        let capture_focus = modal_focus.clone();
        let mut recovery = div().id("profile-recovery");
        if !self.profile_issues.is_empty() {
            recovery = div()
                .id("profile-recovery")
                .role(gpui::accesskit::Role::Alert)
                .p_3()
                .bg(t.secondary())
                .child(
                    div().flex().flex_col().gap_1().children(
                        self.profile_issues
                            .iter()
                            .cloned()
                            .map(|issue| div().child(issue)),
                    ),
                )
                .child(
                    crate::ui::btn(&t, "Retry profile reads", ButtonVariant::Secondary)
                        .debug_selector(|| "profile-retry".into())
                        .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.refresh(cx))),
                );
        }
        let mut root = div()
            .capture_key_down(cx.listener(move |v, event: &KeyDownEvent, window, cx| {
                let code_focus = v
                    .native_fields
                    .values()
                    .any(|field| field.read(cx).code_input_focused());
                if event.keystroke.key == "tab" && code_focus && !event.keystroke.modifiers.control
                {
                    return;
                }
                if event.keystroke.key == "tab"
                    && (!event.keystroke.modifiers.control || code_focus)
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.alt
                {
                    for _ in 0..512 {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        if !modal || capture_focus.contains_focused(window, cx) {
                            break;
                        }
                    }
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" {
                    if v.workspace_ui.drag.source.is_some() {
                        cx.stop_active_drag(window);
                        v.workspace_ui.drag.reset();
                        cx.notify();
                    } else if code_focus
                        || v.native_fields
                            .values()
                            .any(|field| field.read(cx).find_input_focused(window, cx))
                    {
                        // Let the native editor handle Find/IME Escape before
                        // global popup dismissal consumes the event.
                        return;
                    } else if modal {
                        v.close_dialog(cx);
                    } else {
                        v.workspace_ui.menu = None;
                        v.accounts_ui.switcher_open = false;
                        v.processes_ui.dropdown = false;
                        cx.notify();
                    }
                    cx.stop_propagation();
                } else if modal && !capture_focus.contains_focused(window, cx) {
                    window.focus(&capture_focus, cx);
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(t.window_background())
            .text_color(t.foreground())
            .font_family("Segoe UI Variable")
            .text_size(px(13.0))
            .line_height(gpui::relative(1.4))
            .child(self.toolbar(window, cx))
            .child(recovery)
            .child(div().flex_1().min_h_0().flex().flex_col().child(content))
            .child(self.footer());
        if let Some(popup) = self
            .dialog
            .is_none()
            .then(|| self.processes_dropdown(cx))
            .flatten()
        {
            root = root.child(popup);
        }
        if let Some(dialog) = self.render_dialog(cx) {
            root = root.child(
                div()
                    .id("active-dialog")
                    .role(gpui::accesskit::Role::Dialog)
                    .aria_label("Launcher dialog")
                    .track_focus(&modal_focus)
                    .tab_group()
                    .absolute()
                    .inset_0()
                    .child(dialog),
            );
        } else if let Some(menu) = self.workspace_menu(window, cx) {
            root = root.child(menu);
        }
        root.into_any_element()
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use gpui::{size, TestAppContext, VisualTestContext};
    #[gpui::test]
    async fn profile_read_errors_survive_activity_and_clear_only_after_successful_retry(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let paths =
            industrialis_backend::paths::LauncherPaths::new(std::env::temp_dir().join(format!(
                "industrialis-recovery-ui-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        std::fs::create_dir_all(paths.base()).unwrap();
        let mut settings = LauncherSettingsData::default();
        settings.theme_preset = "midnight".into();
        let original = serde_json::to_vec(&settings).unwrap();
        std::fs::write(paths.launcher_settings_path(), &original).unwrap();
        let data = AppData {
            paths: paths.clone(),
            rt: rt.clone(),
            instances: vec![],
            accounts: vec![],
            settings,
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(1100.), px(800.)), |_, _| LauncherRoot::new(data));
        let root = handle.root(cx).unwrap();
        root.update(cx, |v, cx| {
            v.set_profile_issues(vec!["Startup read failed; original files kept".into()]);
            v.data.status = "Unrelated operation completed".into();
            cx.notify();
        });
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        assert!(VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("profile-retry")
            .is_some());
        std::fs::write(paths.launcher_settings_path(), b"{invalid").unwrap();
        root.update(cx, |v, cx| v.refresh(cx));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            if root.read_with(cx, |v, _| {
                v.profile_issues
                    .iter()
                    .any(|issue| issue.contains("refresh preferences"))
            }) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        assert_eq!(
            std::fs::read(paths.launcher_settings_path()).unwrap(),
            b"{invalid"
        );
        assert_eq!(
            root.read_with(cx, |v, _| v.data.settings.theme_preset.clone()),
            "midnight"
        );
        std::fs::write(paths.launcher_settings_path(), &original).unwrap();
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let retry = visual.debug_bounds("profile-retry").unwrap();
            visual.simulate_click(retry.center(), Default::default());
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !root.read_with(cx, |v, _| v.profile_issues.is_empty()) {
            cx.run_until_parked();
            assert!(std::time::Instant::now() < deadline);
            rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        assert_eq!(
            std::fs::read(paths.launcher_settings_path()).unwrap(),
            original
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
