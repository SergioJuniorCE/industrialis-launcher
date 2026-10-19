use super::LauncherRoot;
use crate::{
    input::{FieldState, NativeField, NativeFieldEvent},
    state::format_bytes,
    ui::{btn, card, list_item, ButtonVariant},
};
use gpui::{div, prelude::*, Context, Div};
use industrialis_backend::{
    backup_manager::{BackupManager, Snapshot, State},
    backups::{self, LocalBackup},
    google_drive::{DriveStatus, GoogleDrive},
};
use std::{collections::HashMap, sync::Arc, time::Duration};

pub(super) struct BackupsUi {
    drive: Arc<GoogleDrive>,
    manager: Arc<BackupManager>,
    client: FieldState,
    status: Option<DriveStatus>,
    state: Option<State>,
    loading: bool,
    busy: bool,
    error: Option<String>,
    instances: HashMap<String, InstanceBackups>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppData;
    use gpui::{size, ClipboardItem, TestAppContext, VisualTestContext};
    use industrialis_backend::{instances, paths::LauncherPaths};
    #[gpui::test]
    async fn backup_panel_reads_existing_archives_and_saves_policy_in_place(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let paths = LauncherPaths::new(std::env::temp_dir().join(format!(
                "industrialis-backup-ui-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        let instance = paths.instance_dir("pack").unwrap();
        std::fs::create_dir_all(instance.join(".minecraft/backups")).unwrap();
        std::fs::write(instance.join("mmc-pack.json"), "{}").unwrap();
        std::fs::write(instance.join(".minecraft/backups/world.zip"), b"world").unwrap();
        std::fs::write(instance.join("instance.json"),br#"{"name":"Pack","pack_version":"test","future":{"keep":true},"backups_enabled":false}"#).unwrap();
        let data = AppData {
            paths: paths.clone(),
            rt: rt.clone(),
            instances: rt.block_on(instances::load_instances(&paths)).unwrap(),
            accounts: Vec::new(),
            settings: Default::default(),
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(gpui::px(1100.), gpui::px(850.)), |_, _| {
            let mut root = LauncherRoot::new(data);
            root.selected_id = Some("pack".into());
            root.instance_ui.detail_tab = super::super::DetailTab::Backups;
            root
        });
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        async fn settle(
            cx: &mut TestAppContext,
            root: &gpui::Entity<LauncherRoot>,
            rt: &Arc<tokio::runtime::Runtime>,
        ) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if !root.read_with(cx, |v, _| {
                    v.backups_ui
                        .instances
                        .get("pack")
                        .is_some_and(|s| s.loading)
                        || v.instance_ui.has_unsaved_changes()
                }) {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                rt.spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                    .await
                    .unwrap();
            }
        }
        settle(cx, &root, &rt).await;
        root.read_with(cx, |v, _| {
            assert_eq!(
                v.backups_ui.instances["pack"].local[0].file_name,
                "world.zip"
            )
        });
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual
                .debug_bounds("instance-toggle:pack:backups_enabled")
                .unwrap();
            visual.simulate_click(bounds.center(), Default::default());
        }
        settle(cx, &root, &rt).await;
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            let bounds = visual
                .debug_bounds("instance:pack:backup_retention_override")
                .unwrap();
            visual.simulate_click(bounds.center(), Default::default());
            visual.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("7".into())));
            visual.simulate_keystrokes("ctrl-a ctrl-v");
        }
        settle(cx, &root, &rt).await;
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(instance.join("instance.json")).unwrap())
                .unwrap();
        assert_eq!(raw["backups_enabled"], true);
        assert_eq!(raw["backup_retention_override"], 7);
        assert_eq!(raw["future"], serde_json::json!({"keep":true}));
        {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_keystrokes("ctrl-a backspace");
        }
        settle(cx, &root, &rt).await;
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(instance.join("instance.json")).unwrap())
                .unwrap();
        assert!(raw["backup_retention_override"].is_null());
        assert_eq!(
            std::fs::read(instance.join(".minecraft/backups/world.zip")).unwrap(),
            b"world"
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
#[derive(Default)]
struct InstanceBackups {
    local: Vec<LocalBackup>,
    cloud: Vec<Snapshot>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    delete: Option<String>,
}
enum Action {
    Refresh,
    Scan,
    Configure(String),
    Connect,
    Disconnect,
    Retry(Option<String>, Option<String>),
    Upload(String, String),
    Download(String, String),
    Delete(String, String),
    Folder(String),
}
impl BackupsUi {
    pub(super) fn new(paths: industrialis_backend::paths::LauncherPaths) -> Self {
        let drive = Arc::new(GoogleDrive::new(paths.clone()));
        let manager = BackupManager::new(paths, drive.clone());
        Self {
            drive,
            manager,
            client: FieldState::empty(),
            status: None,
            state: None,
            loading: false,
            busy: false,
            error: None,
            instances: Default::default(),
        }
    }
}
impl LauncherRoot {
    pub(crate) fn start_backups(&mut self, cx: &mut Context<Self>) {
        self.backups_ui.manager.start(&self.data.rt);
        self.refresh_backup_status(cx);
        cx.spawn(async move |view, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let Ok(closing) = view.update(cx, |v, cx| {
                if v.closing.is_none() {
                    v.refresh_backup_status(cx);
                }
                v.closing.is_some()
            }) else {
                break;
            };
            if closing {
                break;
            }
        })
        .detach();
    }
    fn refresh_backup_status(&mut self, cx: &mut Context<Self>) {
        if self.backups_ui.loading {
            return;
        }
        self.backups_ui.loading = true;
        let drive = self.backups_ui.drive.clone();
        let manager = self.backups_ui.manager.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let status = drive.status().await;
                    let state = manager.state().await;
                    (status, state)
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.backups_ui.loading = false;
                match result {
                    Ok((status, state)) => {
                        match status {
                            Ok(status) => {
                                if v.backups_ui.status.is_none()
                                    && v.backups_ui.client.text.is_empty()
                                {
                                    v.backups_ui.client = FieldState::new(
                                        status.client_id.as_deref().unwrap_or_default(),
                                    );
                                }
                                v.backups_ui.status = Some(status);
                            }
                            Err(e) => v.backups_ui.error = Some(format!("{e:#}")),
                        }
                        match state {
                            Ok(state) => v.backups_ui.state = Some(state),
                            Err(e) => v.backups_ui.error = Some(format!("{e:#}")),
                        }
                    }
                    Err(e) => v.backups_ui.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn reload_backups(&mut self, id: String, cx: &mut Context<Self>) {
        let state = self.backups_ui.instances.entry(id.clone()).or_default();
        if state.loading {
            return;
        }
        state.loaded = true;
        state.loading = true;
        let manager = self.backups_ui.manager.clone();
        let paths = self.data.paths.clone();
        let target = id.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let local = backups::list_local(&paths, &target).await;
                    let cloud = manager.list(&target).await;
                    (local, cloud)
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                let state = v.backups_ui.instances.entry(id).or_default();
                state.loading = false;
                state.error = None;
                match result {
                    Ok((local, cloud)) => {
                        match local {
                            Ok(local) => state.local = local,
                            Err(e) => state.error = Some(format!("{e:#}")),
                        }
                        match cloud {
                            Ok(cloud) => state.cloud = cloud,
                            Err(e) => state.error = Some(format!("{e:#}")),
                        }
                    }
                    Err(e) => state.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn backup_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.backups_ui.busy || self.closing.is_some() {
            return;
        }
        self.backups_ui.busy = true;
        self.backups_ui.error = None;
        // OAuth listens for backend shutdown and must remain cancellable on close.
        let waiting_browser = matches!(action, Action::Connect);
        if !waiting_browser {
            self.busy.insert("backup-action".into());
        }
        let manager = self.backups_ui.manager.clone();
        let drive = self.backups_ui.drive.clone();
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let connection_change = matches!(
                        action,
                        Action::Configure(_) | Action::Connect | Action::Disconnect
                    );
                    let _connection_guard = if connection_change {
                        Some(manager.provider_change().await)
                    } else {
                        None
                    };
                    match action {
                        Action::Refresh => {
                            drive.refresh_status().await?;
                        }
                        Action::Configure(client) => {
                            drive.configure(client).await?;
                        }
                        Action::Connect => {
                            drive.connect().await?;
                        }
                        Action::Disconnect => {
                            drive.disconnect().await?;
                        }
                        Action::Scan => manager.run_once().await?,
                        Action::Retry(id, snapshot) => manager.retry(id, snapshot).await?,
                        Action::Upload(id, name) => manager.upload_now(&id, &name).await?,
                        Action::Download(id, snapshot) => {
                            manager.download(&id, &snapshot).await?;
                        }
                        Action::Delete(id, snapshot) => manager.delete(&id, &snapshot).await?,
                        Action::Folder(id) => backups::open_folder(&paths, &id).await?,
                    }
                    manager.wake();
                    Ok::<_, anyhow::Error>(())
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.busy.remove("backup-action");
                v.backups_ui.busy = false;
                match result {
                    Ok(Ok(())) => {
                        v.data.status = "Backup action completed".into();
                        for state in v.backups_ui.instances.values_mut() {
                            state.loaded = false;
                            state.delete = None;
                        }
                    }
                    Ok(Err(e)) => v.backups_ui.error = Some(format!("{e:#}")),
                    Err(e) => v.backups_ui.error = Some(e.to_string()),
                }
                v.refresh_backup_status(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn backup_button(
        &self,
        label: &str,
        action: impl Fn(&mut LauncherRoot, &mut Context<LauncherRoot>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let enabled = !self.backups_ui.busy;
        let selector = format!("backup-button:{label}");
        btn(&self.theme, label, ButtonVariant::Outline)
            .debug_selector(move || selector.clone().into())
            .text_xs()
            .tab_stop(enabled)
            .when(!enabled, |b| b.opacity(0.5).cursor_default())
            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                if enabled {
                    action(v, cx);
                }
            }))
    }
    pub(super) fn backup_preferences(&mut self, cx: &mut Context<Self>) -> Div {
        if self.backups_ui.status.is_none() && !self.backups_ui.loading {
            self.refresh_backup_status(cx);
        }
        let t = self.theme.clone();
        let field_key = "backup-client";
        if !self.native_fields.contains_key(field_key) {
            let field = cx.new(|cx| {
                NativeField::new(
                    &self.backups_ui.client.text,
                    t.clone(),
                    "Desktop OAuth client ID",
                    false,
                    cx,
                )
            });
            cx.subscribe(&field, |v, _, event, cx| {
                if let NativeFieldEvent::Change(text) = event {
                    v.backups_ui.client.text = text.clone();
                    cx.notify();
                }
            })
            .detach();
            self.native_fields.insert(field_key.into(), field);
        }
        let field = self.native_fields[field_key].clone();
        field.update(cx, |f, _| {
            f.value = self.backups_ui.client.text.clone();
            f.enabled = !self.backups_ui.busy;
            f.theme = t.clone();
        });
        let connected = self.backups_ui.status.as_ref().is_some_and(|s| s.connected);
        let configured = self
            .backups_ui
            .status
            .as_ref()
            .is_some_and(|s| s.configured);
        let mut panel=card(&t).p_3().flex().flex_col().gap_3().child("Google Drive").child(div().text_xs().text_color(t.muted_foreground()).child(if connected{"Connected. Existing backups and queued uploads are available."}else{"Connect Drive to upload local backups automatically."})).child(field)
            .child(div().flex().flex_wrap().gap_2().child(self.backup_button("Save client ID",|v,cx|v.backup_action(Action::Configure(v.backups_ui.client.text.clone()),cx),cx))
                .when(configured&&!connected,|row|row.child(self.backup_button("Connect Google Drive",|v,cx|v.backup_action(Action::Connect,cx),cx)))
                .when(connected,|row|row.child(self.backup_button("Disconnect",|v,cx|v.backup_action(Action::Disconnect,cx),cx)))
                .child(self.backup_button("Refresh",|v,cx|v.backup_action(Action::Refresh,cx),cx))
                .child(self.backup_button("Scan backups now",|v,cx|v.backup_action(Action::Scan,cx),cx))
                .child(self.backup_button("Retry failed uploads",|v,cx|v.backup_action(Action::Retry(None,None),cx),cx)))
            .child(btn(&t,"Google Drive setup guide",ButtonVariant::Ghost).text_xs().on_click(|_,_,cx|cx.open_url("https://github.com/SergioJuniorCE/industrialis-launcher/blob/master/docs/cloud-backups.md")));
        if self.backups_ui.busy {
            panel = panel.child("Working… Complete authorization in your browser if prompted.");
        }
        if let Some(error) = self.backups_ui.error.as_ref().or_else(|| {
            self.backups_ui
                .status
                .as_ref()
                .and_then(|s| s.error.as_ref())
        }) {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(t.destructive())
                    .child(error.clone()),
            );
        }
        if let Some(error) = self.backups_ui.manager.error.lock().unwrap().clone() {
            panel = panel.child(div().text_xs().text_color(t.destructive()).child(error));
        }
        for progress in self.backups_ui.manager.progress.lock().unwrap().values() {
            panel = panel.child(format!(
                "{} {}: {} / {}",
                progress.direction,
                progress.file_name,
                format_bytes(progress.completed),
                format_bytes(progress.total)
            ));
        }
        panel = panel.child("Instance backup health");
        for instance in self.data.instances.clone() {
            let id = instance.id.clone();
            let name = if instance.settings.name.is_empty() {
                id.clone()
            } else {
                instance.settings.name.clone()
            };
            let statuses: Vec<_> = self
                .backups_ui
                .state
                .as_ref()
                .map(|s| {
                    s.snapshots
                        .values()
                        .filter(|s| s.summary.instance_id == id && !s.deleted)
                        .flat_map(|s| s.providers.values().map(|p| p.status.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            let status = if !instance.settings.backups_enabled {
                "Disabled"
            } else if !connected {
                "Disconnected"
            } else if statuses
                .iter()
                .any(|s| *s == "failed" || *s == "pending-retry")
            {
                "Needs retry"
            } else if statuses
                .iter()
                .any(|s| *s == "pending" || *s == "uploading")
            {
                "Queued / uploading"
            } else if statuses.is_empty() {
                "Waiting for a completed backup"
            } else {
                "Healthy"
            };
            let retention = instance
                .settings
                .backup_retention_override
                .unwrap_or(self.data.settings.backup_retention_limit);
            panel = panel.child(
                list_item(
                    &t,
                    &format!("{name} / {status} / keeps {retention} cloud backups"),
                    false,
                ).h_auto().py_2().justify_start()
                .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                    v.select_instance(id.clone(), cx);
                    v.tab = super::Tab::Instances;
                    v.instance_ui.detail_tab = super::DetailTab::Backups;
                    cx.notify();
                })),
            );
        }
        if let Some(state) = &self.backups_ui.state {
            let pending = state
                .snapshots
                .values()
                .flat_map(|s| s.providers.values())
                .filter(|d| d.status != "healthy")
                .count();
            panel = panel.child(format!("{pending} queued or failed uploads"));
            for entry in state.activities.iter().take(10) {
                panel = panel.child(div().text_xs().text_color(t.muted_foreground()).child(
                    format!(
                        "{} — {}",
                        entry["created_at"].as_str().unwrap_or_default(),
                        entry["message"].as_str().unwrap_or_default()
                    ),
                ));
            }
        }
        panel
    }
    pub(super) fn instance_backups_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        if !self
            .backups_ui
            .instances
            .entry(id.into())
            .or_default()
            .loaded
        {
            self.reload_backups(id.into(), cx);
        }
        let t = self.theme.clone();
        let state = &self.backups_ui.instances[id];
        let local = state.local.clone();
        let mut cloud = state.cloud.clone();
        if let Some(queue) = &self.backups_ui.state {
            cloud.retain(|entry| {
                !queue.snapshots.values().any(|s| {
                    s.deleted
                        && s.summary.instance_id == id
                        && s.summary.snapshot_id == entry.summary.snapshot_id
                })
            });
            for entry in queue
                .snapshots
                .values()
                .filter(|s| s.summary.instance_id == id && !s.deleted && !s.providers.is_empty())
            {
                if let Some(existing) = cloud
                    .iter_mut()
                    .find(|s| s.summary.snapshot_id == entry.summary.snapshot_id)
                {
                    *existing = entry.clone();
                } else {
                    cloud.push(entry.clone());
                }
            }
            cloud.sort_by(|a, b| b.summary.created_at.cmp(&a.summary.created_at));
        }
        let loading = state.loading;
        let error = state.error.clone();
        let deleting = state.delete.clone();
        let refresh = id.to_string();
        let folder = id.to_string();
        let retry = id.to_string();
        let mut panel=div().id("instance-backups").flex().flex_col().gap_3().p_4().overflow_y_scroll()
            .child(self.instance_backup_policy(id,cx)).child(div().flex().flex_wrap().gap_2()
                .child(self.backup_button("Refresh",move|v,cx|v.reload_backups(refresh.clone(),cx),cx))
                .child(self.backup_button("Open backups folder",move|v,cx|v.backup_action(Action::Folder(folder.clone()),cx),cx))
                .child(self.backup_button("Retry uploads",move|v,cx|v.backup_action(Action::Retry(Some(retry.clone()),None),cx),cx)))
            .child(div().text_xs().text_color(t.muted_foreground()).child("Restoring downloads an archive into this instance’s backups folder. Local archives are kept during cloud retention."));
        if loading {
            panel = panel.child("Loading backups…");
        }
        if let Some(error) = error.or_else(|| self.backups_ui.error.clone()) {
            panel = panel.child(div().text_xs().text_color(t.destructive()).child(error));
        }
        panel = panel.child("Local backups");
        if local.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .child("No local backups yet. In-game backup archives appear here."),
            );
        }
        for file in local {
            let instance = id.to_string();
            let name = file.file_name.clone();
            panel = panel.child(
                card(&t)
                    .id(format!("local-backup:{id}:{name}"))
                    .flex_shrink_0()
                    .p_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().text_xs().child(format!(
                        "{} · {} · {}",
                        file.file_name,
                        format_bytes(file.size_bytes),
                        file.modified_at
                    )))
                    .child(self.backup_button(
                        "Upload",
                        move |v, cx| {
                            let target = instance.clone();
                            let name = name.clone();
                            v.after_instance_settings_saved(
                                target.clone(),
                                move |v, cx| v.backup_action(Action::Upload(target, name), cx),
                                cx,
                            );
                        },
                        cx,
                    )),
            );
        }
        panel = panel.child("Cloud snapshots");
        if cloud.is_empty() {
            panel = panel.child(div().text_xs().child("No cloud snapshots available."));
        }
        for snapshot in cloud {
            let summary = snapshot.summary;
            let hash = summary.snapshot_id.clone();
            let download = id.to_string();
            let dhash = hash.clone();
            let remove = id.to_string();
            let rhash = hash.clone();
            let retry = id.to_string();
            let thash = hash.clone();
            let status = snapshot
                .providers
                .values()
                .map(|d| d.status.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let mut row = card(&t)
                .id(format!("cloud-backup:{id}:{hash}"))
                .flex_shrink_0()
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().child(format!(
                    "{} · {} · {} · {status}",
                    summary.file_name,
                    format_bytes(summary.size_bytes),
                    summary.created_at
                )))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(self.backup_button(
                            "Restore",
                            move |v, cx| {
                                v.backup_action(
                                    Action::Download(download.clone(), dhash.clone()),
                                    cx,
                                )
                            },
                            cx,
                        ))
                        .child(self.backup_button(
                            "Retry",
                            move |v, cx| {
                                v.backup_action(
                                    Action::Retry(Some(retry.clone()), Some(thash.clone())),
                                    cx,
                                )
                            },
                            cx,
                        ))
                        .child(self.backup_button(
                            "Delete from cloud",
                            move |v, cx| {
                                v.backups_ui
                                    .instances
                                    .entry(remove.clone())
                                    .or_default()
                                    .delete = Some(rhash.clone());
                                cx.notify();
                            },
                            cx,
                        )),
                );
            for delivery in snapshot.providers.values() {
                if let Some(error) = &delivery.last_error {
                    row = row.child(
                        div()
                            .text_xs()
                            .text_color(t.destructive())
                            .child(error.clone()),
                    );
                }
            }
            if deleting.as_deref() == Some(&hash) {
                let instance = id.to_string();
                let hash = hash.clone();
                let cancel = id.to_string();
                row = row.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child("Remove this cloud snapshot?")
                        .child(self.backup_button(
                            "Confirm delete",
                            move |v, cx| {
                                v.backup_action(Action::Delete(instance.clone(), hash.clone()), cx)
                            },
                            cx,
                        ))
                        .child(self.backup_button(
                            "Cancel",
                            move |v, cx| {
                                v.backups_ui
                                    .instances
                                    .entry(cancel.clone())
                                    .or_default()
                                    .delete = None;
                                cx.notify();
                            },
                            cx,
                        )),
                );
            }
            panel = panel.child(row);
        }
        div().size_full().child(panel.size_full())
    }
}
