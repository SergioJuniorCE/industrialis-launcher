//! Third-party content uses the original persistent overlay, with no migration.
use crate::{content_archive, paths::LauncherPaths};
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContentKind {
    Mod,
    ResourcePack,
}
impl ContentKind {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRecord {
    pub project: String,
    pub release: String,
    pub url: String,
    pub archive_sha256: String,
    #[serde(default)]
    pub pack_series: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
}

pub struct PendingContent {
    pub name: String,
    pub raw: Vec<u8>,
    pub source: SourceRecord,
}

pub(crate) const MAX_INSTALL_ITEMS: usize = 64;
pub(crate) const MAX_INSTALL_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct ContentEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub managed: bool,
    pub source: Option<SourceRecord>,
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn filename(kind: ContentKind, name: &str) -> anyhow::Result<()> {
    ensure!(
        !name.is_empty()
            && crate::minecraft_files::sanitize_rel_path(name)? == name
            && !name.contains('/'),
        "Invalid content filename"
    );
    let lower = name.to_lowercase();
    ensure!(
        lower.ends_with(".zip") || (kind == ContentKind::Mod && lower.ends_with(".jar")),
        "Choose a ZIP resource pack or JAR/ZIP mod"
    );
    Ok(())
}
fn receipt_rel(kind: ContentKind, name: &str) -> String {
    format!(".industrialis-content/{}/{}.json", kind.directory(), name)
}

pub async fn list(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
) -> anyhow::Result<Vec<ContentEntry>> {
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        let _lease = lease;
        let mut entries = BTreeMap::new();
        for root in [".minecraft", "persistent-minecraft"] {
            let dir =
                match crate::minecraft_files::scoped(&paths, &id, root, kind.directory(), false) {
                    Ok(p) => p,
                    Err(e) if missing(&e) => continue,
                    Err(e) => return Err(e),
                };
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = crate::minecraft_files::scoped(
                    &paths,
                    &id,
                    root,
                    &format!("{}/{name}", kind.directory()),
                    false,
                )?;
                let meta = std::fs::symlink_metadata(&path)?;
                if !meta.is_file() && !(kind == ContentKind::ResourcePack && meta.is_dir()) {
                    continue;
                }
                if meta.is_file() && filename(kind, &name).is_err() {
                    continue;
                }
                let managed = root == "persistent-minecraft" && meta.is_file();
                let source = if managed {
                    let receipt = crate::minecraft_files::scoped(
                        &paths,
                        &id,
                        root,
                        &receipt_rel(kind, &name),
                        false,
                    );
                    match receipt {
                        Ok(p) => serde_json::from_slice::<SourceRecord>(&std::fs::read(p)?).ok(),
                        Err(e) if missing(&e) => None,
                        Err(e) => return Err(e),
                    }
                } else {
                    None
                };
                entries.insert(
                    name.to_lowercase(),
                    ContentEntry {
                        filename: name,
                        size_bytes: meta.len(),
                        managed,
                        source,
                    },
                );
            }
        }
        Ok(entries.into_values().collect())
    })
    .await?
}
fn missing(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}

pub async fn import(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
    source: &Path,
) -> anyhow::Result<()> {
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?
        .to_owned();
    filename(kind, &name)?;
    let source = source.to_owned();
    let raw = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(source)?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() <= content_archive::MAX_ARCHIVE,
            "Content file exceeds the import limit"
        );
        let mut raw = Vec::new();
        file.take(content_archive::MAX_ARCHIVE + 1)
            .read_to_end(&mut raw)?;
        Ok::<_, anyhow::Error>(raw)
    })
    .await??;
    install_bytes(paths, id, kind, name, raw, None).await
}

pub(crate) async fn install_bytes(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
    name: String,
    raw: Vec<u8>,
    source: Option<SourceRecord>,
) -> anyhow::Result<()> {
    filename(kind, &name)?;
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing add-ons"
    );
    if let Some(series) = source.as_ref().and_then(|s| s.pack_series.as_ref()) {
        let settings = crate::settings::load_instance_settings(paths, id).await?;
        ensure!(
            crate::modernity::pack_series(&settings.pack_version)? == *series,
            "The instance pack version changed. Refresh Modernity releases before installing."
        );
    }
    let paths = paths.clone();
    let id = id.to_owned();
    // Validation/normalization does not hold the profile transaction lock.
    let source = source.unwrap_or_else(|| SourceRecord {
        project: "Local import".into(),
        release: "Local file".into(),
        url: name.clone(),
        archive_sha256: digest(&raw),
        pack_series: None,
        provider: None,
        project_id: None,
        version_id: None,
    });
    let raw = tokio::task::spawn_blocking(move || {
        content_archive::prepare(raw, kind == ContentKind::ResourcePack)
    })
    .await??;
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let rel = format!("{}/{name}", kind.directory());
            let persistent = crate::minecraft_files::scoped(&paths, &id, "persistent-minecraft", &rel, true)?;
            let active = crate::minecraft_files::scoped(&paths, &id, ".minecraft", &rel, true)?;
            // Never silently replace pack-owned files, user imports or other versions.
            for root in [persistent.parent().unwrap(), active.parent().unwrap()] {
                for entry in std::fs::read_dir(root)? {
                    let existing = entry?.file_name().to_string_lossy().into_owned();
                    let collision = if kind == ContentKind::Mod { crate::pack_files::mod_identity_from_filename(&existing) == crate::pack_files::mod_identity_from_filename(&name) } else { existing.eq_ignore_ascii_case(&name) };
                    ensure!(!collision, "{existing} is already installed. Remove it before installing another version.");
                }
            }
            let receipt = crate::minecraft_files::scoped(&paths, &id, "persistent-minecraft", &receipt_rel(kind, &name), true)?;
            let previous_receipt = match std::fs::read(&receipt) { Ok(raw) => Some(raw), Err(e) if e.kind() == std::io::ErrorKind::NotFound => None, Err(e) => return Err(e.into()) };
            let mut written = Vec::new();
            let result = (|| {
                for path in [&persistent, &active] { crate::fs_utils::write_atomic(path, &raw)?; written.push(path.clone()); }
                crate::fs_utils::write_atomic(&receipt, &serde_json::to_vec_pretty(&source)?)?;
                Ok::<_, anyhow::Error>(())
            })();
            if result.is_err() {
                for path in written.into_iter().rev() { std::fs::remove_file(path)?; }
                if let Some(old) = previous_receipt { crate::fs_utils::write_atomic(&receipt, &old)?; }
                else if receipt.exists() { std::fs::remove_file(receipt)?; }
            }
            result
        }).await?
    }).await
}

/// Validate and install several mods as one transaction. Every archive is
/// checked before the first write; any handled failure removes every file and
/// restores every previous receipt, so no partial install survives.
pub async fn install_batch(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
    items: Vec<PendingContent>,
) -> anyhow::Result<usize> {
    ensure!(!items.is_empty(), "No content to install");
    ensure!(
        items.len() <= MAX_INSTALL_ITEMS,
        "Too many files in one install"
    );
    let total = items
        .iter()
        .try_fold(0u64, |sum, item| sum.checked_add(item.raw.len() as u64))
        .context("Content install is too large")?;
    ensure!(
        total <= MAX_INSTALL_BYTES,
        "Content install exceeds the 512 MB limit"
    );
    let mut names = std::collections::HashSet::new();
    let mut identities = std::collections::HashSet::new();
    for item in &items {
        filename(kind, &item.name)?;
        ensure!(
            names.insert(item.name.to_ascii_lowercase()),
            "{} is selected more than once",
            item.name
        );
        if kind == ContentKind::Mod {
            ensure!(
                identities.insert(crate::pack_files::mod_identity_from_filename(&item.name)),
                "{} conflicts with another selected mod",
                item.name
            );
        }
    }
    let mut projects = std::collections::HashSet::new();
    for item in &items {
        if let Some(project) = &item.source.project_id {
            ensure!(
                projects.insert(project.clone()),
                "The same project is selected more than once"
            );
        }
    }
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing add-ons"
    );
    if items.iter().any(|item| item.source.pack_series.is_some()) {
        let settings = crate::settings::load_instance_settings(paths, id).await?;
        let series = crate::modernity::pack_series(&settings.pack_version)?;
        for item in &items {
            if let Some(expected) = &item.source.pack_series {
                ensure!(
                    series == *expected,
                    "The instance pack version changed. Refresh the content source before installing."
                );
            }
        }
    }
    let paths = paths.clone();
    let id = id.to_owned();
    let prepared = tokio::task::spawn_blocking(move || {
        items
            .into_iter()
            .map(|item| {
                let raw = content_archive::prepare(item.raw, kind == ContentKind::ResourcePack)?;
                Ok((item.name, raw, item.source))
            })
            .collect::<anyhow::Result<Vec<_>>>()
    })
    .await??;
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            install_prepared(&paths, &id, kind, prepared)
        })
        .await?
    })
    .await
}

fn install_prepared(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
    prepared: Vec<(String, Vec<u8>, SourceRecord)>,
) -> anyhow::Result<usize> {
    let mut targets = Vec::with_capacity(prepared.len());
    for (name, _, _) in &prepared {
        let rel = format!("{}/{name}", kind.directory());
        targets.push((
            crate::minecraft_files::scoped(paths, id, "persistent-minecraft", &rel, true)?,
            crate::minecraft_files::scoped(paths, id, ".minecraft", &rel, true)?,
            crate::minecraft_files::scoped(
                paths,
                id,
                "persistent-minecraft",
                &receipt_rel(kind, name),
                true,
            )?,
        ));
    }
    let mut directories = std::collections::HashSet::new();
    for (persistent, active, _) in &targets {
        if let Some(parent) = persistent.parent() {
            directories.insert(parent.to_path_buf());
        }
        if let Some(parent) = active.parent() {
            directories.insert(parent.to_path_buf());
        }
    }
    for directory in directories {
        for entry in std::fs::read_dir(directory)? {
            let existing = entry?.file_name().to_string_lossy().into_owned();
            for (name, _, _) in &prepared {
                let collision = if kind == ContentKind::Mod {
                    crate::pack_files::mod_identity_from_filename(&existing)
                        == crate::pack_files::mod_identity_from_filename(name)
                } else {
                    existing.eq_ignore_ascii_case(name)
                };
                ensure!(
                    !collision,
                    "{existing} is already installed. Remove it before installing another version."
                );
            }
        }
    }
    let mut previous = Vec::with_capacity(targets.len());
    for (_, _, receipt) in &targets {
        let old = match std::fs::read(receipt) {
            Ok(raw) => Some(raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        previous.push((receipt.clone(), old));
    }
    let mut written = Vec::new();
    let result = (|| -> anyhow::Result<()> {
        for ((_, raw, source), (persistent, active, receipt)) in prepared.iter().zip(&targets) {
            crate::fs_utils::write_atomic(persistent, raw)?;
            written.push(persistent.clone());
            crate::fs_utils::write_atomic(active, raw)?;
            written.push(active.clone());
            crate::fs_utils::write_atomic(receipt, &serde_json::to_vec_pretty(source)?)?;
            written.push(receipt.clone());
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in written.into_iter().rev() {
            let _ = std::fs::remove_file(path);
        }
        let mut rollback = None;
        for (path, old) in previous {
            let restored = match old {
                Some(raw) => crate::fs_utils::write_atomic(&path, &raw),
                None => match std::fs::remove_file(&path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                },
            };
            if let Err(e) = restored {
                rollback.get_or_insert(e);
            }
        }
        return Err(match rollback {
            Some(e) => error.context(format!("Rollback also failed: {e:#}")),
            None => error,
        });
    }
    Ok(prepared.len())
}

pub async fn remove(
    paths: &LauncherPaths,
    id: &str,
    kind: ContentKind,
    name: &str,
) -> anyhow::Result<()> {
    filename(kind, name)?;
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing add-ons"
    );
    let paths = paths.clone();
    let id = id.to_owned();
    let name = name.to_owned();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let rel = format!("{}/{name}", kind.directory());
            let persistent = crate::minecraft_files::scoped(&paths, &id, "persistent-minecraft", &rel, false)?;
            let original = std::fs::read(&persistent)?;
            let mut targets = vec![persistent];
            match crate::minecraft_files::scoped(&paths, &id, ".minecraft", &rel, false) {
                Ok(active) => { ensure!(std::fs::read(&active)? == original, "The active file was changed outside the launcher; both copies have been kept"); targets.push(active); },
                Err(e) if missing(&e) => {}, Err(e) => return Err(e),
            }
            match crate::minecraft_files::scoped(&paths, &id, "persistent-minecraft", &receipt_rel(kind, &name), false) {
                Ok(p) => targets.push(p), Err(e) if missing(&e) => {}, Err(e) => return Err(e),
            }
            for p in &targets { ensure!(std::fs::metadata(p)?.is_file() && !std::fs::metadata(p)?.permissions().readonly(), "Content is read-only; its files have been kept"); }
            let mut staged = Vec::new();
            for target in targets {
                let pending = target.with_file_name(format!(".industrialis-content-{}.tmp", uuid::Uuid::new_v4()));
                if let Err(e) = std::fs::rename(&target, &pending) { for (p, tmp) in staged.into_iter().rev() { std::fs::rename(tmp, p)?; } return Err(e.into()); }
                staged.push((target, pending));
            }
            for (_, p) in staged { std::fs::remove_file(p)?; }
            Ok(())
        }).await?
    }).await
}

pub async fn open_folder(paths: &LauncherPaths, id: &str, kind: ContentKind) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        let p = crate::minecraft_files::scoped(
            &paths,
            &id,
            ".minecraft",
            &format!("{}/.probe", kind.directory()),
            true,
        )?;
        open::that(p.parent().unwrap())?;
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> LauncherPaths {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-content-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(paths.instance_dir("pack").unwrap().join(".minecraft")).unwrap();
        paths
    }
    fn pack() -> Vec<u8> {
        content_archive::fixture(&[
            ("pack.mcmeta", b"{\"pack\":{\"pack_format\":1}}"),
            ("LICENSE.txt", b"credit"),
            ("assets/test/texture.png", b"texture"),
        ])
    }
    #[tokio::test]
    async fn import_restore_collision_and_removal_preserve_existing_packs() {
        let paths = profile();
        let instance = paths.instance_dir("pack").unwrap();
        let source = paths.base().join("custom.zip");
        std::fs::write(&source, pack()).unwrap();
        let active = instance.join(".minecraft/resourcepacks/custom.zip");
        import(&paths, "pack", ContentKind::ResourcePack, &source)
            .await
            .unwrap();
        assert!(
            list(&paths, "pack", ContentKind::ResourcePack)
                .await
                .unwrap()[0]
                .managed
        );
        let installed = std::fs::read(&active).unwrap();
        assert!(import(&paths, "pack", ContentKind::ResourcePack, &source)
            .await
            .is_err());
        assert_eq!(std::fs::read(&active).unwrap(), installed);
        std::fs::remove_file(&active).unwrap();
        crate::pack_prep::apply_persistent_minecraft(&instance)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&active).unwrap(), installed);
        std::fs::write(
            instance.join(".minecraft/resourcepacks/pack-owned.zip"),
            pack(),
        )
        .unwrap();
        assert!(
            remove(&paths, "pack", ContentKind::ResourcePack, "pack-owned.zip")
                .await
                .is_err()
        );
        std::fs::write(&active, b"external change").unwrap();
        assert!(
            remove(&paths, "pack", ContentKind::ResourcePack, "custom.zip")
                .await
                .is_err()
        );
        assert!(instance
            .join("persistent-minecraft/resourcepacks/custom.zip")
            .exists());
        std::fs::write(&active, &installed).unwrap();
        remove(&paths, "pack", ContentKind::ResourcePack, "custom.zip")
            .await
            .unwrap();
        assert!(!active.exists());
        assert!(!instance
            .join("persistent-minecraft/resourcepacks/custom.zip")
            .exists());
        assert!(instance
            .join(".minecraft/resourcepacks/pack-owned.zip")
            .exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn mod_download_receipt_removes_through_original_manager_and_conflicts_are_blocked() {
        let paths = profile();
        let instance = paths.instance_dir("pack").unwrap();
        let raw = content_archive::fixture(&[("mcmod.info", b"[]")]);
        let source = SourceRecord {
            project: "test".into(),
            release: "test-1".into(),
            url: "test".into(),
            archive_sha256: digest(&raw),
            pack_series: None,
            provider: Some("modrinth".into()),
            project_id: Some("GTNHMod".into()),
            version_id: Some("abc12345".into()),
        };
        install_bytes(
            &paths,
            "pack",
            ContentKind::Mod,
            "MyCTMLib-1.4.2.jar".into(),
            raw.clone(),
            Some(source),
        )
        .await
        .unwrap();
        assert!(list(&paths, "pack", ContentKind::Mod).await.unwrap()[0]
            .source
            .is_some());
        assert!(install_bytes(
            &paths,
            "pack",
            ContentKind::Mod,
            "MyCTMLib-1.4.1.jar".into(),
            raw.clone(),
            None
        )
        .await
        .is_err());
        crate::custom_mods::remove(&paths, "pack", "myctmlib")
            .await
            .unwrap();
        assert!(!instance
            .join("persistent-minecraft/.industrialis-content/mods/MyCTMLib-1.4.2.jar.json")
            .exists());
        install_bytes(
            &paths,
            "pack",
            ContentKind::Mod,
            "MyCTMLib-1.4.2.jar".into(),
            raw,
            None,
        )
        .await
        .unwrap();
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn busy_invalid_archive_and_changed_pack_version_leave_profile_untouched() {
        let paths = profile();
        let instance = paths.instance_dir("pack").unwrap();
        let lease = crate::instance_guard::reserve_instances(&paths, &["pack"]).unwrap();
        assert!(install_bytes(
            &paths,
            "pack",
            ContentKind::ResourcePack,
            "x.zip".into(),
            pack(),
            None
        )
        .await
        .is_err());
        drop(lease);
        assert!(install_bytes(
            &paths,
            "pack",
            ContentKind::ResourcePack,
            "x.zip".into(),
            b"bad".to_vec(),
            None
        )
        .await
        .is_err());
        let source = SourceRecord {
            project: "test".into(),
            release: "test".into(),
            url: "test".into(),
            archive_sha256: "test".into(),
            pack_series: Some("2.9".into()),
            provider: None,
            project_id: None,
            version_id: None,
        };
        assert!(install_bytes(
            &paths,
            "pack",
            ContentKind::ResourcePack,
            "x.zip".into(),
            pack(),
            Some(source)
        )
        .await
        .is_err());
        assert!(!instance
            .join("persistent-minecraft/resourcepacks/x.zip")
            .exists());
        assert!(!instance.join(".minecraft/resourcepacks/x.zip").exists());
        let receipt =
            instance.join("persistent-minecraft/.industrialis-content/resourcepacks/x.zip.json");
        std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        std::fs::write(&receipt, b"old receipt").unwrap();
        let mut permissions = std::fs::metadata(&receipt).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&receipt, permissions).unwrap();
        assert!(install_bytes(
            &paths,
            "pack",
            ContentKind::ResourcePack,
            "x.zip".into(),
            pack(),
            None
        )
        .await
        .is_err());
        assert!(!instance
            .join("persistent-minecraft/resourcepacks/x.zip")
            .exists());
        assert!(!instance.join(".minecraft/resourcepacks/x.zip").exists());
        assert_eq!(std::fs::read(&receipt).unwrap(), b"old receipt");
        let mut permissions = std::fs::metadata(&receipt).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&receipt, permissions).unwrap();
        install_bytes(
            &paths,
            "pack",
            ContentKind::ResourcePack,
            "x.zip".into(),
            pack(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            list(&paths, "pack", ContentKind::ResourcePack)
                .await
                .unwrap()[0]
                .source
                .as_ref()
                .unwrap()
                .project,
            "Local import"
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    fn jar(name: &str) -> PendingContent {
        let raw = content_archive::fixture(&[("mcmod.info", b"[]")]);
        PendingContent {
            name: name.into(),
            source: SourceRecord {
                project: format!("https://modrinth.com/mod/{name}"),
                release: "1.0".into(),
                url: format!("https://cdn.modrinth.com/{name}"),
                archive_sha256: digest(&raw),
                pack_series: None,
                provider: Some("modrinth".into()),
                project_id: Some(name.trim_end_matches(".jar").into()),
                version_id: Some(format!("{name}-version")),
            },
            raw,
        }
    }

    #[tokio::test]
    async fn batch_install_writes_every_file_and_receipt_or_nothing() {
        let paths = profile();
        let instance = paths.instance_dir("pack").unwrap();
        let count = install_batch(
            &paths,
            "pack",
            ContentKind::Mod,
            vec![jar("First-1.0.jar"), jar("Second-1.0.jar")],
        )
        .await
        .unwrap();
        assert_eq!(count, 2);
        for name in ["First-1.0.jar", "Second-1.0.jar"] {
            for root in [".minecraft", "persistent-minecraft"] {
                assert!(instance.join(root).join("mods").join(name).is_file());
            }
            let receipt = instance
                .join("persistent-minecraft/.industrialis-content/mods")
                .join(format!("{name}.json"));
            let source: SourceRecord =
                serde_json::from_slice(&std::fs::read(receipt).unwrap()).unwrap();
            assert_eq!(source.provider.as_deref(), Some("modrinth"));
            assert!(source.project_id.is_some() && source.version_id.is_some());
        }
        assert!(install_batch(
            &paths,
            "pack",
            ContentKind::Mod,
            vec![jar("First-2.0.jar"), jar("Third-1.0.jar")],
        )
        .await
        .is_err());
        assert!(!instance
            .join(".minecraft/mods/Third-1.0.jar")
            .exists());
        assert!(install_batch(
            &paths,
            "pack",
            ContentKind::Mod,
            vec![jar("Fourth-1.0.jar"), jar("Fourth-2.0.jar")],
        )
        .await
        .is_err());
        assert!(!instance.join(".minecraft/mods/Fourth-1.0.jar").exists());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn batch_install_rolls_back_written_files_when_a_late_write_fails() {
        let paths = profile();
        let instance = paths.instance_dir("pack").unwrap();
        let receipt = instance.join(
            "persistent-minecraft/.industrialis-content/mods/Second-1.0.jar.json",
        );
        std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        std::fs::write(&receipt, b"old receipt").unwrap();
        let mut permissions = std::fs::metadata(&receipt).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&receipt, permissions).unwrap();
        let result = install_batch(
            &paths,
            "pack",
            ContentKind::Mod,
            vec![jar("First-1.0.jar"), jar("Second-1.0.jar")],
        )
        .await;
        let mut permissions = std::fs::metadata(&receipt).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&receipt, permissions).unwrap();
        assert!(result.is_err());
        for name in ["First-1.0.jar", "Second-1.0.jar"] {
            assert!(!instance.join(".minecraft/mods").join(name).exists());
            assert!(!instance
                .join("persistent-minecraft/mods")
                .join(name)
                .exists());
        }
        assert_eq!(std::fs::read(&receipt).unwrap(), b"old receipt");
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
