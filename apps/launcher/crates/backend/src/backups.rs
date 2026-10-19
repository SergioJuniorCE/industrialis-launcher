//! Content-addressed backup manifests compatible with the existing Drive layout.
use crate::paths::LauncherPaths;
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalBackup {
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub snapshot_id: String,
    pub instance_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub created_at: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub logical_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub schema_version: u32,
    pub snapshot_id: String,
    pub instance_id: String,
    pub created_at: String,
    pub artifacts: Vec<Artifact>,
}
pub struct Prepared {
    pub source_path: PathBuf,
    pub source_size_bytes: u64,
    pub source_modified: std::time::SystemTime,
    pub manifest: Manifest,
    pub summary: Summary,
}
#[derive(Clone, Debug)]
pub struct RemoteObject {
    pub key: String,
    pub size_bytes: Option<u64>,
}
pub enum UploadSource {
    File(PathBuf),
    Bytes(Vec<u8>),
}
#[async_trait::async_trait]
pub trait BackupStore: Send + Sync {
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<RemoteObject>>;
    async fn upload(
        &self,
        key: &str,
        source: UploadSource,
        content_type: &str,
        progress: Progress,
    ) -> anyhow::Result<()>;
    /// Return only when all downloaded bytes are visible to subsequent readers.
    async fn download(
        &self,
        key: &str,
        destination: &Path,
        progress: Progress,
    ) -> anyhow::Result<()>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
}
pub fn no_progress() -> Progress {
    Arc::new(|_, _| {})
}
pub fn file_name(value: &str) -> anyhow::Result<&str> {
    ensure!(
        !value.is_empty()
            && value != "."
            && value != ".."
            && !value.contains(['\\', '/', ':', '\0'])
            && !value.ends_with(['.', ' ']),
        "Backup filename must name one file in the backups folder"
    );
    Ok(value)
}
fn hash_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn prefix(id: &str, snapshot: &str) -> anyhow::Result<String> {
    let normalized = crate::paths::validate_instance_id(id)?;
    ensure!(normalized == id, "Invalid backup instance ID");
    let id = normalized;
    ensure!(hash_id(snapshot), "Invalid backup snapshot ID");
    Ok(format!("instances/{id}/snapshots/{snapshot}"))
}
fn validate_manifest(raw: &[u8], key: &str) -> anyhow::Result<Manifest> {
    let m: Manifest = serde_json::from_slice(raw).context("Invalid backup manifest")?;
    ensure!(
        m.format == "industrialis-backup"
            && m.schema_version == 1
            && !m.artifacts.is_empty()
            && chrono::DateTime::parse_from_rfc3339(&m.created_at).is_ok(),
        "Invalid backup manifest"
    );
    ensure!(
        format!("{}/manifest.json", prefix(&m.instance_id, &m.snapshot_id)?) == key,
        "Backup manifest does not match its remote path"
    );
    for a in &m.artifacts {
        file_name(&a.file_name)?;
        ensure!(
            a.kind == "world-archive"
                && a.id == m.snapshot_id
                && a.sha256 == m.snapshot_id
                && a.logical_path == format!("artifacts/{}", a.file_name),
            "Invalid backup artifact"
        );
    }
    Ok(m)
}
fn iso(time: std::time::SystemTime) -> String {
    let date: chrono::DateTime<chrono::Utc> = time.into();
    date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn backup_path(
    paths: &LauncherPaths,
    id: &str,
    name: &str,
    create: bool,
) -> anyhow::Result<PathBuf> {
    file_name(name)?;
    crate::minecraft_files::scoped(paths, id, ".minecraft", &format!("backups/{name}"), create)
}
async fn digest(path: &Path) -> anyhow::Result<(u64, String)> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        total += n as u64;
    }
    Ok((total, format!("{:x}", hash.finalize())))
}
pub async fn list_local(paths: &LauncherPaths, id: &str) -> anyhow::Result<Vec<LocalBackup>> {
    let _lease = crate::lifecycle::admit(paths)?;
    let dir = paths.instance_backups_dir(id)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    crate::minecraft_files::scoped(paths, id, ".minecraft", "backups", false)?;
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut result = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let meta = tokio::fs::symlink_metadata(entry.path()).await?;
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if !meta.is_file()
            || [".tmp", ".part", ".partial", ".crdownload"]
                .iter()
                .any(|ext| lower.ends_with(ext))
        {
            continue;
        }
        file_name(&name)?;
        result.push(LocalBackup {
            file_name: name,
            size_bytes: meta.len(),
            modified_at: iso(meta.modified()?),
        });
    }
    result.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(result)
}
pub async fn prepare(
    paths: &LauncherPaths,
    id: &str,
    name: &str,
    created_at: Option<String>,
) -> anyhow::Result<Prepared> {
    let source = backup_path(paths, id, name, false)?;
    let before = tokio::fs::metadata(&source).await?;
    ensure!(before.is_file(), "Selected backup is not a file");
    let (size, sha256) = digest(&source).await?;
    let after = tokio::fs::metadata(&source).await?;
    ensure!(
        before.len() == after.len()
            && before.modified()? == after.modified()?
            && size == before.len(),
        "Backup changed while it was being read"
    );
    let safe_name = crate::paths::sanitize_name(name);
    file_name(&safe_name)?;
    let created_at = created_at.unwrap_or_else(|| iso(before.modified().unwrap()));
    let summary = Summary {
        snapshot_id: sha256.clone(),
        sha256: sha256.clone(),
        instance_id: crate::paths::validate_instance_id(id)?,
        file_name: safe_name.clone(),
        size_bytes: size,
        created_at: created_at.clone(),
    };
    let manifest = Manifest {
        format: "industrialis-backup".into(),
        schema_version: 1,
        snapshot_id: sha256.clone(),
        instance_id: summary.instance_id.clone(),
        created_at,
        artifacts: vec![Artifact {
            id: sha256.clone(),
            kind: "world-archive".into(),
            logical_path: format!("artifacts/{safe_name}"),
            file_name: safe_name,
            size_bytes: size,
            sha256,
            content_type: "application/octet-stream".into(),
        }],
    };
    Ok(Prepared {
        source_path: source,
        source_size_bytes: size,
        source_modified: before.modified()?,
        manifest,
        summary,
    })
}
async fn stable(prepared: &Prepared) -> anyhow::Result<()> {
    let meta = tokio::fs::metadata(&prepared.source_path).await?;
    ensure!(
        meta.len() == prepared.source_size_bytes && meta.modified()? == prepared.source_modified,
        "Backup changed during upload"
    );
    Ok(())
}
pub async fn upload(
    prepared: &Prepared,
    store: &dyn BackupStore,
    progress: Progress,
) -> anyhow::Result<Summary> {
    stable(prepared).await?;
    let prefix = prefix(
        &prepared.manifest.instance_id,
        &prepared.manifest.snapshot_id,
    )?;
    let artifact = &prepared.manifest.artifacts[0];
    let staging = TempDirectory::new()?;
    let source = staging.0.join("archive");
    let mut input = tokio::fs::File::open(&prepared.source_path).await?;
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&source)
        .await?;
    tokio::io::copy(&mut input, &mut output).await?;
    output.sync_all().await?;
    drop(output);
    let (size, hash) = digest(&source).await?;
    ensure!(
        size == artifact.size_bytes && hash == artifact.sha256,
        "Backup changed before upload"
    );
    store
        .upload(
            &format!("{prefix}/{}", artifact.logical_path),
            UploadSource::File(source),
            &artifact.content_type,
            progress,
        )
        .await?;
    stable(prepared).await?;
    // Verify content, too: a file can keep its timestamp and length after a save.
    let (size, hash) = digest(&prepared.source_path).await?;
    ensure!(
        size == artifact.size_bytes && hash == artifact.sha256,
        "Backup changed during upload"
    );
    store
        .upload(
            &format!("{prefix}/manifest.json"),
            UploadSource::Bytes(serde_json::to_vec_pretty(&prepared.manifest)?),
            "application/json",
            no_progress(),
        )
        .await?;
    Ok(prepared.summary.clone())
}
struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> anyhow::Result<Self> {
        let path =
            std::env::temp_dir().join(format!("industrialis-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn remote_manifest(
    store: &dyn BackupStore,
    key: &str,
    temp: &Path,
) -> anyhow::Result<Manifest> {
    let target = temp.join(format!("{}.json", uuid::Uuid::new_v4()));
    store.download(key, &target, no_progress()).await?;
    let meta = tokio::fs::metadata(&target).await?;
    ensure!(meta.len() <= 1024 * 1024, "Backup manifest is too large");
    let raw = tokio::fs::read(&target).await?;
    validate_manifest(&raw, key)
}
pub async fn list_remote(id: &str, store: &dyn BackupStore) -> anyhow::Result<Vec<Summary>> {
    let id = crate::paths::validate_instance_id(id)?;
    let root = format!("instances/{id}/snapshots/");
    let objects = store.list(&root).await?;
    let temp = TempDirectory::new()?;
    let mut result = Vec::new();
    for object in objects {
        if !object.key.starts_with(&root) || !object.key.ends_with("/manifest.json") {
            continue;
        }
        // Invalid manifests are ignored; transport failures remain actionable.
        let target = temp.0.join(format!("{}.json", uuid::Uuid::new_v4()));
        store.download(&object.key, &target, no_progress()).await?;
        if tokio::fs::metadata(&target).await?.len() > 1024 * 1024 {
            continue;
        }
        let raw = tokio::fs::read(target).await?;
        let Ok(m) = validate_manifest(&raw, &object.key) else {
            continue;
        };
        if m.instance_id != id {
            continue;
        }
        let a = &m.artifacts[0];
        result.push(Summary {
            snapshot_id: m.snapshot_id,
            instance_id: m.instance_id,
            file_name: a.file_name.clone(),
            size_bytes: a.size_bytes,
            created_at: m.created_at,
            sha256: a.sha256.clone(),
        });
    }
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(result)
}
pub async fn download(
    paths: &LauncherPaths,
    id: &str,
    snapshot: &str,
    store: &dyn BackupStore,
    progress: Progress,
) -> anyhow::Result<LocalBackup> {
    let _lease = crate::lifecycle::admit(paths)?;
    let prefix = prefix(id, snapshot)?;
    let temp = TempDirectory::new()?;
    let manifest = remote_manifest(store, &format!("{prefix}/manifest.json"), &temp.0).await?;
    let artifact = &manifest.artifacts[0];
    let downloaded = temp.0.join("artifact");
    store
        .download(
            &format!("{prefix}/{}", artifact.logical_path),
            &downloaded,
            progress,
        )
        .await?;
    let (size, hash) = digest(&downloaded).await?;
    ensure!(
        size == artifact.size_bytes && hash == artifact.sha256,
        "Cloud backup failed integrity verification"
    );
    // Publish a complete, verified file without replacing an existing backup.
    // A crash can leave only a .part file, which automatic discovery ignores.
    let stage = backup_path(paths, id, &format!("{}.part", uuid::Uuid::new_v4()), true)?;
    struct Stage(PathBuf);
    impl Drop for Stage {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _stage = Stage(stage.clone());
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .await?;
    let mut source = tokio::fs::File::open(&downloaded).await?;
    tokio::io::copy(&mut source, &mut output).await?;
    output.sync_all().await?;
    drop(output);
    let path = Path::new(&artifact.file_name);
    let stem = path.file_stem().unwrap().to_string_lossy();
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    let mut name = artifact.file_name.clone();
    let mut suffix = 0;
    loop {
        let target = backup_path(paths, id, &name, true)?;
        match tokio::fs::hard_link(&stage, &target).await {
            Ok(()) => {
                return Ok(LocalBackup {
                    file_name: name,
                    size_bytes: size,
                    modified_at: iso(tokio::fs::metadata(target).await?.modified()?),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix += 1;
                name = format!("{stem}-cloud-{suffix}{ext}");
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub async fn delete_remote(
    id: &str,
    snapshot: &str,
    store: &dyn BackupStore,
) -> anyhow::Result<()> {
    let root = format!("{}/", prefix(id, snapshot)?);
    let mut objects = store.list(&root).await?;
    for object in &objects {
        ensure!(
            object.key.starts_with(&root),
            "Backup provider returned an object outside the selected snapshot"
        );
    }
    objects.sort_by_key(|object| !object.key.ends_with("/manifest.json"));
    for object in objects {
        ensure!(
            object.key.starts_with(&root),
            "Backup provider returned an object outside the selected snapshot"
        );
        store.delete(&object.key).await?;
    }
    Ok(())
}
pub async fn open_folder(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    let probe = backup_path(paths, id, ".folder-probe", true)?;
    let dir = probe.parent().unwrap().to_path_buf();
    tokio::task::spawn_blocking(move || open::that(dir)).await??;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };
    #[derive(Default)]
    pub struct MemoryStore {
        pub objects: Mutex<BTreeMap<String, Vec<u8>>>,
        pub writes: Mutex<Vec<String>>,
        pub fail: AtomicBool,
        pub corrupt: AtomicBool,
    }
    #[async_trait::async_trait]
    impl BackupStore for MemoryStore {
        async fn list(&self, prefix: &str) -> anyhow::Result<Vec<RemoteObject>> {
            Ok(self
                .objects
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| RemoteObject {
                    key: k.clone(),
                    size_bytes: Some(v.len() as u64),
                })
                .collect())
        }
        async fn upload(
            &self,
            key: &str,
            source: UploadSource,
            _: &str,
            progress: Progress,
        ) -> anyhow::Result<()> {
            ensure!(
                !self.fail.load(Ordering::SeqCst),
                "temporary provider failure"
            );
            let raw = match source {
                UploadSource::File(path) => tokio::fs::read(path).await?,
                UploadSource::Bytes(raw) => raw,
            };
            progress(raw.len() as u64, raw.len() as u64);
            self.objects.lock().unwrap().insert(key.into(), raw);
            self.writes.lock().unwrap().push(key.into());
            Ok(())
        }
        async fn download(
            &self,
            key: &str,
            destination: &Path,
            progress: Progress,
        ) -> anyhow::Result<()> {
            let mut raw = self
                .objects
                .lock()
                .unwrap()
                .get(key)
                .context("remote object missing")?
                .clone();
            if self.corrupt.load(Ordering::SeqCst) && !key.ends_with("manifest.json") {
                raw.push(0);
            }
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .await?;
            use tokio::io::AsyncWriteExt;
            file.write_all(&raw).await?;
            file.flush().await?;
            progress(raw.len() as u64, raw.len() as u64);
            Ok(())
        }
        async fn delete(&self, key: &str) -> anyhow::Result<()> {
            ensure!(
                !self.fail.load(Ordering::SeqCst),
                "temporary provider failure"
            );
            self.objects.lock().unwrap().remove(key);
            Ok(())
        }
    }
    pub fn fixture() -> LauncherPaths {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-backups-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(paths.instance_backups_dir("pack").unwrap()).unwrap();
        paths
    }
    #[tokio::test]
    async fn manifest_commit_integrity_and_collision_do_not_change_local_archives() {
        let paths = fixture();
        let dir = paths.instance_backups_dir("pack").unwrap();
        std::fs::write(dir.join("world.zip"), b"world data").unwrap();
        std::fs::write(dir.join("unfinished.part"), b"partial").unwrap();
        let store = MemoryStore::default();
        let prepared = prepare(&paths, "pack", "world.zip", None).await.unwrap();
        upload(&prepared, &store, no_progress()).await.unwrap();
        assert!(store
            .writes
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .ends_with("manifest.json"));
        assert_eq!(list_remote("pack", &store).await.unwrap().len(), 1);
        let restored = download(
            &paths,
            "pack",
            &prepared.summary.snapshot_id,
            &store,
            no_progress(),
        )
        .await
        .unwrap();
        assert_eq!(restored.file_name, "world-cloud-1.zip");
        assert_eq!(std::fs::read(dir.join("world.zip")).unwrap(), b"world data");
        assert_eq!(list_local(&paths, "pack").await.unwrap().len(), 2);
        store.corrupt.store(true, Ordering::SeqCst);
        assert!(download(
            &paths,
            "pack",
            &prepared.summary.snapshot_id,
            &store,
            no_progress()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("integrity"));
        assert_eq!(list_local(&paths, "pack").await.unwrap().len(), 2);
        delete_remote("pack", &prepared.summary.snapshot_id, &store)
            .await
            .unwrap();
        assert!(list_remote("pack", &store).await.unwrap().is_empty());
        assert_eq!(
            std::fs::read(dir.join(&restored.file_name)).unwrap(),
            b"world data"
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn incomplete_and_untrusted_manifests_are_not_restorable() {
        let store = MemoryStore::default();
        store.objects.lock().unwrap().insert(
            format!("instances/pack/snapshots/{}/manifest.json", "a".repeat(64)),
            br#"{"format":"other"}"#.to_vec(),
        );
        assert!(list_remote("pack", &store).await.unwrap().is_empty());
        assert!(prefix("../pack", &"a".repeat(64)).is_err());
        assert!(prefix("pack", "../escape").is_err());
        assert!(file_name("..\\world.zip").is_err());
        let paths = fixture();
        std::fs::write(
            paths
                .instance_backups_dir("pack")
                .unwrap()
                .join("world.zip"),
            b"world",
        )
        .unwrap();
        let p = prepare(&paths, "pack", "world.zip", None).await.unwrap();
        store.fail.store(true, Ordering::SeqCst);
        assert!(upload(&p, &store, no_progress()).await.is_err());
        assert!(list_remote("pack", &store).await.unwrap().is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
