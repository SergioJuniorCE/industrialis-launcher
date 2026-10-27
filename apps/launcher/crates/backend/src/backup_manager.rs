//! Durable automatic backups using the original backup-state.json schema.
use crate::{
    backups::{self, BackupStore, Summary},
    paths::LauncherPaths,
};
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const PROVIDER: &str = "google-drive";
type Extras = serde_json::Map<String, Value>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delivery {
    pub status: String,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_failed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uploaded_at: Option<String>,
    #[serde(flatten)]
    extra: Extras,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::tests::{fixture, MemoryStore};
    use std::sync::atomic::Ordering;
    async fn policy(paths: &LauncherPaths, limit: u32) {
        std::fs::write(
            paths.instance_dir("pack").unwrap().join("mmc-pack.json"),
            b"{}",
        )
        .unwrap();
        let path = paths.settings_path("pack").unwrap();
        std::fs::write(path,serde_json::to_vec(&json!({"name":"Pack","pack_version":"test","backups_enabled":true,"backup_retention_override":limit})).unwrap()).unwrap();
    }
    #[tokio::test]
    async fn automatic_scan_waits_for_settled_archives_and_ignores_temporary_files() {
        let paths=fixture();policy(&paths,20).await;
        let store=Arc::new(MemoryStore::default());
        let manager=BackupManager::with_store(paths.clone(),store.clone(),None);
        let dir=paths.instance_backups_dir("pack").unwrap();
        std::fs::write(dir.join("world.zip"),b"completed world").unwrap();
        std::fs::write(dir.join("world.zip.part"),b"partial").unwrap();
        manager.run_once().await.unwrap();
        assert!(backups::list_remote("pack",store.as_ref()).await.unwrap().is_empty());
        let file=std::fs::OpenOptions::new().write(true).open(dir.join("world.zip")).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()-Duration::from_secs(90))).unwrap();
        manager.run_once().await.unwrap();
        let remote=backups::list_remote("pack",store.as_ref()).await.unwrap();
        assert_eq!(remote.len(),1);assert_eq!(remote[0].file_name,"world.zip");
        manager.run_once().await.unwrap();assert_eq!(backups::list_remote("pack",store.as_ref()).await.unwrap().len(),1);
        assert!(dir.join("world.zip.part").exists());drop(file);
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    struct SlowStore {inner:MemoryStore,active:std::sync::atomic::AtomicUsize,max:std::sync::atomic::AtomicUsize,started:std::sync::atomic::AtomicUsize,peer:tokio::sync::Notify}
    #[async_trait::async_trait]
    impl BackupStore for SlowStore {
        async fn list(&self,prefix:&str)->anyhow::Result<Vec<backups::RemoteObject>>{self.inner.list(prefix).await}
        async fn upload(&self,key:&str,source:backups::UploadSource,content_type:&str,progress:backups::Progress)->anyhow::Result<()> {
            // Hold the first two uploads until both are in flight so the overlap
            // assertion does not depend on how long unrelated file work takes.
            match self.started.fetch_add(1,Ordering::SeqCst) {
                0=>{let _=tokio::time::timeout(Duration::from_secs(5),self.peer.notified()).await;},
                1=>self.peer.notify_one(),
                _=>{},
            }
            let count=self.active.fetch_add(1,Ordering::SeqCst)+1;self.max.fetch_max(count,Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(30)).await;
            let result=self.inner.upload(key,source,content_type,progress).await;self.active.fetch_sub(1,Ordering::SeqCst);result
        }
        async fn download(&self,key:&str,path:&std::path::Path,progress:backups::Progress)->anyhow::Result<()>{self.inner.download(key,path,progress).await}
        async fn delete(&self,key:&str)->anyhow::Result<()>{self.inner.delete(key).await}
    }
    #[tokio::test]
    async fn uploads_two_instances_at_once_and_persists_every_delivery() {
        let paths=fixture();
        let store=Arc::new(SlowStore{inner:Default::default(),active:Default::default(),max:Default::default(),started:Default::default(),peer:Default::default()});
        let manager=BackupManager::with_store(paths.clone(),store.clone(),None);
        for id in ["one","two","three"]{
            let dir=paths.instance_dir(id).unwrap();std::fs::create_dir_all(dir.join(".minecraft/backups")).unwrap();
            std::fs::write(dir.join("mmc-pack.json"),"{}").unwrap();std::fs::write(paths.settings_path(id).unwrap(),br#"{"backups_enabled":true}"#).unwrap();
            std::fs::write(dir.join(".minecraft/backups/world.zip"),id).unwrap();manager.upload_now(id,"world.zip").await.unwrap();
        }
        manager.run_once().await.unwrap();assert_eq!(store.max.load(Ordering::SeqCst),2);
        let state=manager.state().await.unwrap();assert_eq!(state.snapshots.len(),3);
        assert!(state.snapshots.values().all(|s|s.providers[PROVIDER].status=="healthy"));
        assert!(manager.progress.lock().unwrap().is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn queue_retry_restart_retention_and_delete_preserve_existing_state_and_local_data() {
        let paths = fixture();
        policy(&paths, 1).await;
        let store = Arc::new(MemoryStore::default());
        let manager = BackupManager::with_store(paths.clone(), store.clone(), None);
        let dir = paths.instance_backups_dir("pack").unwrap();
        std::fs::write(dir.join("first.zip"), b"first").unwrap();
        std::fs::write(paths.backup_state_path(),br#"{"schema_version":1,"files":{},"snapshots":{},"activities":[],"retention":{},"future":{"keep":true}}"#).unwrap();
        manager.upload_now("pack", "first.zip").await.unwrap();
        let hash = backups::prepare(&paths, "pack", "first.zip", None)
            .await
            .unwrap()
            .summary
            .snapshot_id;
        store.fail.store(true, Ordering::SeqCst);
        // Retention listing succeeds; upload failure becomes a durable retry.
        manager.run_once().await.unwrap();
        let state = manager.state().await.unwrap();
        let delivery = &state.snapshots[&key("pack", &hash)].providers[PROVIDER];
        assert_eq!(delivery.status, "pending-retry");
        assert_eq!(delivery.attempts, 1);
        assert!(delivery.next_retry_at.is_some());
        manager.run_once().await.unwrap();
        assert_eq!(
            manager.state().await.unwrap().snapshots[&key("pack", &hash)].providers[PROVIDER]
                .attempts,
            1
        );
        change(&paths, {
            let hash = hash.clone();
            move |state| {
                state
                    .snapshots
                    .get_mut(&key("pack", &hash))
                    .unwrap()
                    .providers
                    .get_mut(PROVIDER)
                    .unwrap()
                    .status = "uploading".into();
                Ok(())
            }
        })
        .await
        .unwrap();
        store.fail.store(false, Ordering::SeqCst);
        let reopened = BackupManager::with_store(paths.clone(), store.clone(), None);
        reopened.run_once().await.unwrap();
        assert_eq!(
            reopened.list("pack").await.unwrap()[0].providers[PROVIDER].status,
            "healthy"
        );
        std::fs::write(dir.join("second.zip"), b"second").unwrap();
        reopened.upload_now("pack", "second.zip").await.unwrap();
        reopened.run_once().await.unwrap();
        let remote = backups::list_remote("pack", store.as_ref()).await.unwrap();
        assert_eq!(remote.len(), 1);
        assert_eq!(remote[0].file_name, "second.zip");
        assert_eq!(backups::list_local(&paths, "pack").await.unwrap().len(), 2);
        reopened
            .delete("pack", &remote[0].snapshot_id)
            .await
            .unwrap();
        reopened.run_once().await.unwrap();
        assert!(backups::list_remote("pack", store.as_ref())
            .await
            .unwrap()
            .is_empty());
        assert!(reopened.list("pack").await.unwrap().is_empty());
        let raw: Value =
            serde_json::from_slice(&std::fs::read(paths.backup_state_path()).unwrap()).unwrap();
        assert_eq!(raw["future"], json!({"keep":true}));
        assert_eq!(std::fs::read(dir.join("first.zip")).unwrap(), b"first");
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn corrupt_queue_is_never_replaced_and_retry_window_expires() {
        let paths = fixture();
        policy(&paths, 20).await;
        std::fs::write(paths.backup_state_path(), b"broken").unwrap();
        let manager =
            BackupManager::with_store(paths.clone(), Arc::new(MemoryStore::default()), None);
        assert!(manager.run_once().await.is_err());
        assert_eq!(std::fs::read(paths.backup_state_path()).unwrap(), b"broken");
        let mut delivery = Delivery::default();
        let time = now();
        failed(&mut delivery, "offline".into(), time);
        failed(
            &mut delivery,
            "offline".into(),
            time + chrono::Duration::hours(24),
        );
        assert_eq!(delivery.status, "failed");
        assert!(delivery.next_retry_at.is_none());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            status: "pending".into(),
            attempts: 0,
            first_failed_at: None,
            next_retry_at: None,
            last_error: None,
            uploaded_at: None,
            extra: Default::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(flatten)]
    pub summary: Summary,
    pub providers: BTreeMap<String, Delivery>,
    /// Tombstones prevent a still-present local archive from being re-uploaded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    #[serde(flatten)]
    extra: Extras,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileRecord {
    instance_id: String,
    file_name: String,
    size_bytes: u64,
    modified_at: String,
    sha256: String,
    #[serde(flatten)]
    extra: Extras,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    schema_version: u32,
    #[serde(default)]
    files: BTreeMap<String, FileRecord>,
    #[serde(default)]
    pub snapshots: BTreeMap<String, Snapshot>,
    #[serde(default)]
    pub activities: Vec<Value>,
    #[serde(default)]
    retention: BTreeMap<String, u32>,
    #[serde(flatten)]
    extra: Extras,
}
impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: 1,
            files: Default::default(),
            snapshots: Default::default(),
            activities: vec![],
            retention: Default::default(),
            extra: Default::default(),
        }
    }
}
fn key(id: &str, value: &str) -> String {
    format!("{id}\0{value}")
}
fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}
fn date(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
async fn read(paths: &LauncherPaths) -> anyhow::Result<State> {
    let raw = crate::profile_store::read_object(&paths.backup_state_path()).await?;
    let state: State = raw
        .map(serde_json::from_value)
        .transpose()
        .context("Invalid backup queue; the original file has been kept")?
        .unwrap_or_default();
    ensure!(
        state.schema_version == 1,
        "Unsupported backup queue schema; the original has been kept"
    );
    Ok(state)
}
async fn change<T: Send + 'static>(
    paths: &LauncherPaths,
    edit: impl FnOnce(&mut State) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let mut state = read(&paths).await?;
        let before = serde_json::to_value(&state)?;
        let result = edit(&mut state)?;
        let after = serde_json::to_value(state)?;
        if after != before {
            crate::fs_utils::write_json(&paths.backup_state_path(), &after).await?;
        }
        Ok(result)
    })
    .await
}
fn activity(state: &mut State, level: &str, message: String, id: &str, snapshot: Option<&str>) {
    state.activities.insert(0,json!({"id":uuid::Uuid::new_v4().to_string(),"created_at":date(now()),"level":level,"message":message,"instance_id":id,"provider_id":PROVIDER,"snapshot_id":snapshot}));
    state.activities.truncate(100);
}
fn failed(delivery: &mut Delivery, error: String, time: chrono::DateTime<chrono::Utc>) {
    delivery.attempts = delivery.attempts.saturating_add(1);
    let first = delivery.first_failed_at.get_or_insert_with(|| date(time));
    let expired = chrono::DateTime::parse_from_rfc3339(first)
        .map(|first| time.signed_duration_since(first).num_hours() >= 24)
        .unwrap_or(true);
    delivery.status = if expired { "failed" } else { "pending-retry" }.into();
    delivery.next_retry_at = (!expired).then(|| {
        date(
            time + chrono::Duration::seconds(
                (60u64.saturating_mul(1u64 << delivery.attempts.saturating_sub(1).min(6))).min(3600)
                    as i64,
            ),
        )
    });
    delivery.last_error = Some(error);
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub instance_id: String,
    pub enabled: bool,
    pub retention_limit: u32,
}
#[derive(Clone, Debug, Default)]
pub struct Transfer {
    pub instance_id: String,
    pub file_name: String,
    pub direction: String,
    pub completed: u64,
    pub total: u64,
}
pub struct BackupManager {
    paths: LauncherPaths,
    store: Arc<dyn BackupStore>,
    drive: Option<Arc<crate::google_drive::GoogleDrive>>,
    serial: tokio::sync::Mutex<()>,
    initialized: tokio::sync::Mutex<bool>,
    wake: tokio::sync::Notify,
    pub progress: Arc<Mutex<BTreeMap<String,Transfer>>>,
    pub error: Mutex<Option<String>>,
}
impl BackupManager {
    pub fn new(paths: LauncherPaths, drive: Arc<crate::google_drive::GoogleDrive>) -> Arc<Self> {
        Self::with_store(paths, drive.clone(), Some(drive))
    }
    fn with_store(
        paths: LauncherPaths,
        store: Arc<dyn BackupStore>,
        drive: Option<Arc<crate::google_drive::GoogleDrive>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            paths,
            store,
            drive,
            serial: Default::default(),
            initialized: Default::default(),
            wake: Default::default(),
            progress: Default::default(),
            error: Default::default(),
        })
    }
    pub fn start(self: &Arc<Self>, rt: &tokio::runtime::Runtime) {
        let this = self.clone();
        rt.spawn(async move {let lifecycle=crate::lifecycle::for_profile(&this.paths);loop {
            if lifecycle.is_closing(){break;}
            let result=this.run_once().await;*this.error.lock().unwrap()=result.err().map(|e|format!("{e:#}"));
            tokio::select! {_=lifecycle.closing()=>break,_=this.wake.notified()=>{},_=tokio::time::sleep(Duration::from_secs(15))=>{}}
        }});
    }
    pub async fn provider_change(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.serial.lock().await
    }
    pub fn wake(&self) {
        self.wake.notify_one();
    }
    async fn connected(&self) -> anyhow::Result<bool> {
        match &self.drive {
            Some(drive) => {
                let status = drive.status().await?;
                if let Some(error) = status.error {
                    anyhow::bail!(error)
                }
                Ok(status.connected)
            }
            None => Ok(true),
        }
    }
    async fn initialize(&self) -> anyhow::Result<()> {
        let mut initialized = self.initialized.lock().await;
        if *initialized {
            return Ok(());
        }
        let state = read(&self.paths).await?;
        if state.snapshots.values().any(|s| {
            s.providers
                .get(PROVIDER)
                .is_some_and(|d| d.status == "uploading")
        }) {
            change(&self.paths,|state| {for snapshot in state.snapshots.values_mut(){if let Some(d)=snapshot.providers.get_mut(PROVIDER){if d.status=="uploading" {d.status="pending-retry".into();d.next_retry_at=None;d.last_error=Some("The previous upload was interrupted; it will resume automatically.".into());}}}Ok(())}).await?;
        }
        *initialized = true;
        Ok(())
    }
    pub async fn state(&self) -> anyhow::Result<State> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        self.initialize().await?;
        read(&self.paths).await
    }
    pub async fn policies(&self) -> anyhow::Result<Vec<Policy>> {
        let global = crate::settings::load_launcher_settings(&self.paths).await?;
        let mut policies = Vec::new();
        for id in crate::instances::known_instance_ids(&self.paths).await? {
            let settings = crate::settings::load_instance_settings(&self.paths, &id).await?;
            policies.push(Policy {
                instance_id: id,
                enabled: settings.backups_enabled,
                retention_limit: settings
                    .backup_retention_override
                    .unwrap_or(global.backup_retention_limit)
                    .clamp(1, 1000),
            });
        }
        Ok(policies)
    }
    async fn remember(
        &self,
        prepared: &backups::Prepared,
        local_name: String,
        manual: bool,
    ) -> anyhow::Result<()> {
        let summary = prepared.summary.clone();
        let mtime = date(prepared.source_modified.into());
        change(&self.paths, move |state| {
            let fk = key(&summary.instance_id, &local_name);
            let sk = key(&summary.instance_id, &summary.snapshot_id);
            let file = state.files.entry(fk).or_insert_with(|| FileRecord {
                instance_id: summary.instance_id.clone(),
                file_name: local_name,
                size_bytes: summary.size_bytes,
                modified_at: mtime.clone(),
                sha256: summary.sha256.clone(),
                extra: Default::default(),
            });
            file.size_bytes = summary.size_bytes;
            file.modified_at = mtime;
            file.sha256 = summary.sha256.clone();
            let snapshot = state.snapshots.entry(sk).or_insert_with(|| Snapshot {
                summary: summary.clone(),
                providers: Default::default(),
                deleted: false,
                extra: Default::default(),
            });
            if manual {
                snapshot.deleted = false;
            }
            if !snapshot.deleted {
                snapshot.providers.entry(PROVIDER.into()).or_default();
            }
            Ok(())
        })
        .await
    }
    pub async fn upload_now(&self, id: &str, name: &str) -> anyhow::Result<()> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let _serial = self.serial.lock().await;
        self.initialize().await?;
        ensure!(
            self.policies()
                .await?
                .iter()
                .any(|p| p.instance_id == id && p.enabled),
            "Cloud backups are disabled for this instance"
        );
        ensure!(self.connected().await?, "Connect Google Drive first");
        let prepared = backups::prepare(&self.paths, id, name, None).await?;
        self.remember(&prepared, name.into(), true).await?;
        self.wake();
        Ok(())
    }
    pub async fn retry(&self, id: Option<String>, snapshot: Option<String>) -> anyhow::Result<()> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let _serial = self.serial.lock().await;
        self.initialize().await?;
        change(&self.paths, move |state| {
            for entry in state.snapshots.values_mut() {
                if entry.deleted
                    || id
                        .as_ref()
                        .is_some_and(|id| *id != entry.summary.instance_id)
                    || snapshot
                        .as_ref()
                        .is_some_and(|id| *id != entry.summary.snapshot_id)
                {
                    continue;
                }
                if let Some(d) = entry.providers.get_mut(PROVIDER) {
                    if d.status != "healthy" {
                        *d = Delivery {
                            extra: d.extra.clone(),
                            ..Default::default()
                        };
                    }
                }
            }
            Ok(())
        })
        .await?;
        self.wake();
        Ok(())
    }
    pub async fn list(&self, id: &str) -> anyhow::Result<Vec<Snapshot>> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let state = self.state().await?;
        let mut merged: BTreeMap<String, Snapshot> = state
            .snapshots
            .into_values()
            .filter(|s| s.summary.instance_id == id && !s.deleted && !s.providers.is_empty())
            .map(|s| (s.summary.snapshot_id.clone(), s))
            .collect();
        if self.connected().await? {
            for summary in backups::list_remote(id, self.store.as_ref()).await? {
                let entry = merged
                    .entry(summary.snapshot_id.clone())
                    .or_insert_with(|| Snapshot {
                        summary,
                        providers: Default::default(),
                        deleted: false,
                        extra: Default::default(),
                    });
                entry.providers.insert(
                    PROVIDER.into(),
                    Delivery {
                        status: "healthy".into(),
                        ..Default::default()
                    },
                );
            }
        }
        let mut result: Vec<_> = merged.into_values().collect();
        result.sort_by(|a, b| b.summary.created_at.cmp(&a.summary.created_at));
        Ok(result)
    }
    pub async fn download(&self, id: &str, snapshot: &str) -> anyhow::Result<backups::LocalBackup> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let _serial = self.serial.lock().await;
        ensure!(self.connected().await?, "Connect Google Drive first");
        let transfer_key=format!("download:{}:{}",id,snapshot);
        self.progress.lock().unwrap().insert(transfer_key.clone(),Transfer{instance_id:id.into(),file_name:snapshot.into(),direction:"Downloading".into(),..Default::default()});
        let progress=self.progress.clone();let key=transfer_key.clone();
        let result = backups::download(
            &self.paths,
            id,
            snapshot,
            self.store.as_ref(),
            Arc::new(move|done,total|{if let Some(p)=progress.lock().unwrap().get_mut(&key){p.completed=done;p.total=total;}}),
        )
        .await;
        self.progress.lock().unwrap().remove(&transfer_key);
        let result=result?;
        let id = id.to_string();
        let snapshot = snapshot.to_string();
        let name = result.file_name.clone();
        change(&self.paths, move |state| {
            activity(
                state,
                "info",
                format!("Restored {name} from Google Drive."),
                &id,
                Some(&snapshot),
            );
            Ok(())
        })
        .await?;
        Ok(result)
    }
    pub async fn delete(&self, id: &str, snapshot: &str) -> anyhow::Result<()> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let _serial = self.serial.lock().await;
        ensure!(self.connected().await?, "Connect Google Drive first");
        let remote = backups::list_remote(id, self.store.as_ref())
            .await?
            .into_iter()
            .find(|s| s.snapshot_id == snapshot);
        backups::delete_remote(id, snapshot, self.store.as_ref()).await?;
        let id = id.to_string();
        let snapshot = snapshot.to_string();
        change(&self.paths, move |state| {
            if let Some(summary) = remote {
                state
                    .snapshots
                    .entry(key(&id, &snapshot))
                    .or_insert_with(|| Snapshot {
                        summary,
                        providers: Default::default(),
                        deleted: true,
                        extra: Default::default(),
                    });
            }
            if let Some(entry) = state.snapshots.get_mut(&key(&id, &snapshot)) {
                entry.deleted = true;
                entry.providers.remove(PROVIDER);
            }
            activity(
                state,
                "info",
                "Removed cloud backup; the local archive has been kept.".into(),
                &id,
                Some(&snapshot),
            );
            Ok(())
        })
        .await
    }
    pub async fn run_once(&self) -> anyhow::Result<()> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let _serial = self.serial.lock().await;
        self.initialize().await?;
        if !self.connected().await? {
            return Ok(());
        }
        let policies = self.policies().await?;
        for policy in policies.iter().filter(|p| p.enabled) {
            let mut distinct = std::collections::HashSet::new();
            for file in backups::list_local(&self.paths, &policy.instance_id).await? {
                if distinct.len() >= policy.retention_limit as usize {
                    break;
                }
                let modified = chrono::DateTime::parse_from_rfc3339(&file.modified_at)?;
                if now().signed_duration_since(modified).num_seconds() < 60 {
                    continue;
                }
                let state = read(&self.paths).await?;
                if let Some(cached) = state.files.get(&key(&policy.instance_id, &file.file_name)) {
                    if cached.size_bytes == file.size_bytes
                        && cached.modified_at == file.modified_at
                        && state
                            .snapshots
                            .contains_key(&key(&policy.instance_id, &cached.sha256))
                    {
                        distinct.insert(cached.sha256.clone());
                        continue;
                    }
                }
                let prepared =
                    backups::prepare(&self.paths, &policy.instance_id, &file.file_name, None)
                        .await?;
                distinct.insert(prepared.summary.snapshot_id.clone());
                self.remember(&prepared, file.file_name, false).await?;
            }
        }
        let state = read(&self.paths).await?;
        // Bound concurrency to two different instances; retention stays serial per instance.
        let mut pending=Vec::new();
        for snapshot in state.snapshots.values().filter(|s| !s.deleted) {
            let Some(policy) = policies
                .iter()
                .find(|p| p.enabled && p.instance_id == snapshot.summary.instance_id)
            else {
                continue;
            };
            let Some(delivery) = snapshot.providers.get(PROVIDER) else {
                continue;
            };
            if !matches!(delivery.status.as_str(), "pending" | "pending-retry") {
                continue;
            }
            if delivery.next_retry_at.as_ref().is_some_and(|at| {
                chrono::DateTime::parse_from_rfc3339(at)
                    .map(|at| at > now())
                    .unwrap_or(true)
            }) {
                continue;
            }
            if crate::lifecycle::for_profile(&self.paths).is_closing() {
                break;
            }
            pending.push((snapshot,policy.retention_limit));
        }
        while !pending.is_empty() && !crate::lifecycle::for_profile(&self.paths).is_closing(){
            let (first,limit)=pending.remove(0);
            if let Some(index)=pending.iter().position(|(entry,_)|entry.summary.instance_id!=first.summary.instance_id){
                let(second,other_limit)=pending.remove(index);
                let(a,b)=tokio::join!(self.transfer(first,&state,limit),self.transfer(second,&state,other_limit));a?;b?;
            }else{self.transfer(first,&state,limit).await?;}
        }
        for policy in policies.iter().filter(|p| p.enabled) {
            let state = read(&self.paths).await?;
            if state.retention.get(&key(&policy.instance_id, PROVIDER))
                != Some(&policy.retention_limit)
            {
                self.retention(&policy.instance_id, policy.retention_limit)
                    .await?;
            }
        }
        Ok(())
    }
    async fn transfer(&self, snapshot: &Snapshot, state: &State, limit: u32) -> anyhow::Result<()> {
        let id = snapshot.summary.instance_id.clone();
        let hash = snapshot.summary.snapshot_id.clone();
        let sk = key(&id, &hash);
        let start = sk.clone();
        change(&self.paths, move |state| {
            let entry = state
                .snapshots
                .get_mut(&start)
                .context("Backup queue changed")?;
            entry
                .providers
                .get_mut(PROVIDER)
                .context("Backup provider changed")?
                .status = "uploading".into();
            Ok(())
        })
        .await?;
        let file = state
            .files
            .values()
            .find(|f| f.instance_id == id && f.sha256 == hash)
            .map(|f| f.file_name.clone())
            .unwrap_or_else(|| snapshot.summary.file_name.clone());
        self.progress.lock().unwrap().insert(sk.clone(),Transfer {
            instance_id: id.clone(),
            file_name: file.clone(),
            direction: "Uploading".into(),
            ..Default::default()
        });
        let progress = self.progress.clone();
        let progress_key=sk.clone();
        let result = async {
            let prepared = backups::prepare(
                &self.paths,
                &id,
                &file,
                Some(snapshot.summary.created_at.clone()),
            )
            .await?;
            ensure!(
                prepared.summary.snapshot_id == hash,
                "Queued backup changed; upload a new snapshot instead"
            );
            backups::upload(
                &prepared,
                self.store.as_ref(),
                Arc::new(move |done, total| {
                    if let Some(p) = progress.lock().unwrap().get_mut(&progress_key) {
                        p.completed = done;
                        p.total = total;
                    }
                }),
            )
            .await
        }
        .await;
        self.progress.lock().unwrap().remove(&sk);
        let success = result.is_ok();
        let error = result.err().map(|e| format!("{e:#}"));
        let aid = id.clone();
        let ahash = hash.clone();
        change(&self.paths, move |state| {
            let entry = state
                .snapshots
                .get_mut(&sk)
                .context("Backup queue changed")?;
            let delivery = entry
                .providers
                .get_mut(PROVIDER)
                .context("Backup provider changed")?;
            if let Some(error) = error {
                failed(delivery, error.clone(), now());
                activity(
                    state,
                    "error",
                    format!("Could not upload {file}: {error}"),
                    &aid,
                    Some(&ahash),
                );
            } else {
                delivery.status = "healthy".into();
                delivery.uploaded_at = Some(date(now()));
                delivery.last_error = None;
                delivery.next_retry_at = None;
                activity(
                    state,
                    "info",
                    format!("Uploaded {file} to Google Drive."),
                    &aid,
                    Some(&ahash),
                );
            }
            Ok(())
        })
        .await?;
        if success {
            self.retention(&id, limit).await?;
        }
        Ok(())
    }
    async fn retention(&self, id: &str, limit: u32) -> anyhow::Result<()> {
        for expired in backups::list_remote(id, self.store.as_ref())
            .await?
            .into_iter()
            .skip(limit as usize)
        {
            backups::delete_remote(id, &expired.snapshot_id, self.store.as_ref()).await?;
            let id = id.to_string();
            let hash = expired.snapshot_id.clone();
            change(&self.paths, move |state| {
                if let Some(s) = state.snapshots.get_mut(&key(&id, &hash)) {
                    s.deleted = true;
                    s.providers.remove(PROVIDER);
                }
                activity(
                    state,
                    "info",
                    format!(
                        "Removed {} from Google Drive due to retention.",
                        expired.file_name
                    ),
                    &id,
                    Some(&hash),
                );
                Ok(())
            })
            .await?;
        }
        let id = id.to_string();
        change(&self.paths, move |state| {
            state.retention.insert(key(&id, PROVIDER), limit);
            Ok(())
        })
        .await
    }
}
