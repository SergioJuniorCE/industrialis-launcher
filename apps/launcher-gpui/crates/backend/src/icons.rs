//! Instance icons: default-icon install + path resolution. Ports the
//! launch-relevant subset of `electron/backend/instance-icons.ts`.
//!
//! The bundled GTNH logo is embedded at compile time from the Electron app's
//! icon assets so fresh installs get the same default icon.

use std::path::{Path, PathBuf};

use crate::fs_utils::exists;

const DEFAULT_ICON_FILENAME: &str = "instance-icon.png";
const SUPPORTED_ICON_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"];

const DEFAULT_ICON_BYTES: &[u8] = include_bytes!("../assets/gtnh-logo.png");

const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct IconEntry {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub built_in: bool,
}
fn extension(path: &Path) -> anyhow::Result<String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    anyhow::ensure!(
        SUPPORTED_ICON_EXTENSIONS.contains(&extension.as_str()),
        "Unsupported image type; use PNG, JPG, WebP, GIF, BMP, or ICO"
    );
    Ok(extension)
}
async fn read_image(path: &Path) -> anyhow::Result<(String, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let extension = extension(path)?;
    let file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    anyhow::ensure!(metadata.is_file(), "Image file not found");
    anyhow::ensure!(metadata.len() <= MAX_ICON_BYTES, "Image must be under 4 MB");
    let mut bytes = Vec::new();
    file.take(MAX_ICON_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_ICON_BYTES,
        "Image must be under 4 MB"
    );
    Ok((extension, bytes))
}
async fn ensure_library(paths: &crate::paths::LauncherPaths) -> anyhow::Result<()> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        tokio::fs::create_dir_all(paths.icons_dir()).await?;
        let target = paths.icons_dir().join("gtnh-logo.png");
        if !tokio::fs::try_exists(&target).await? {
            tokio::task::spawn_blocking(move || {
                crate::fs_utils::write_atomic(&target, DEFAULT_ICON_BYTES)
            })
            .await??;
        }
        Ok(())
    })
    .await
}
fn entry(path: PathBuf) -> IconEntry {
    let id = path.file_name().unwrap().to_string_lossy().into_owned();
    let built_in = id == "gtnh-logo.png";
    let label = if built_in {
        "GT New Horizons".into()
    } else {
        path.file_stem()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("custom-")
            .replace(['-', '_'], " ")
    };
    IconEntry {
        id,
        label,
        path,
        built_in,
    }
}
pub async fn list_icons(paths: &crate::paths::LauncherPaths) -> anyhow::Result<Vec<IconEntry>> {
    let _lease = crate::lifecycle::admit(paths)?;
    ensure_library(paths).await?;
    let mut files = tokio::fs::read_dir(paths.icons_dir()).await?;
    let mut icons = Vec::new();
    while let Some(file) = files.next_entry().await? {
        if file.file_type().await?.is_file() && extension(&file.path()).is_ok() {
            icons.push(entry(file.path()));
        }
    }
    icons.sort_by(|a, b| {
        b.built_in
            .cmp(&a.built_in)
            .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            .then(a.id.cmp(&b.id))
    });
    Ok(icons)
}
pub async fn open_icons_folder(paths: &crate::paths::LauncherPaths)->anyhow::Result<()> {
    let _lease=crate::lifecycle::admit(paths)?;ensure_library(paths).await?;
    let folder=paths.icons_dir();tokio::task::spawn_blocking(move ||open::that(folder)).await??;Ok(())
}
pub async fn import_icon(
    paths: &crate::paths::LauncherPaths,
    source: &Path,
) -> anyhow::Result<IconEntry> {
    use tokio::io::AsyncWriteExt;
    let _lease = crate::lifecycle::admit(paths)?;
    let (extension, bytes) = read_image(source).await?;
    ensure_library(paths).await?;
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "icon" } else { stem };
    for suffix in 1.. {
        let id = if suffix == 1 {
            format!("custom-{stem}.{extension}")
        } else {
            format!("custom-{stem}-{suffix}.{extension}")
        };
        let destination = paths.icons_dir().join(id);
        let mut file = match tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)
            .await
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        if let Err(error) = async {
            file.write_all(&bytes).await?;
            file.sync_all().await
        }
        .await
        {
            drop(file);
            let _ = tokio::fs::remove_file(&destination).await;
            return Err(error.into());
        }
        return Ok(entry(destination));
    }
    unreachable!()
}
pub async fn set_icon_from_library(
    paths: &crate::paths::LauncherPaths,
    id: &str,
    icon: &str,
) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    anyhow::ensure!(
        Path::new(icon).file_name().and_then(|s| s.to_str()) == Some(icon)
            && !icon.contains(['/', '\\']),
        "Invalid icon identity"
    );
    extension(Path::new(icon))?;
    ensure_library(paths).await?;
    set_instance_icon(paths, id, Some(paths.icons_dir().join(icon))).await
}
/// Stage old generated icons before committing metadata. A failed metadata
/// write restores every old file; an invalid import never clears the old icon.
pub async fn set_instance_icon(
    paths: &crate::paths::LauncherPaths,
    id: &str,
    source: Option<PathBuf>,
) -> anyhow::Result<()> {
    let id = crate::paths::validate_instance_id(id)?;
    let lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    anyhow::ensure!(
        crate::instances::known_instance_ids(paths)
            .await?
            .contains(&id),
        "Instance not found"
    );
    let image = match source {
        Some(source) => Some(read_image(&source).await?),
        None => None,
    };
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let _lease=lease;let instance=paths.instance_dir(&id)?;
        let mut settings=crate::settings::load_instance_settings(&paths,&id).await?;
        let backup=instance.join(format!(".industrialis-icon-{}",uuid::Uuid::new_v4()));
        tokio::fs::create_dir(&backup).await?;
        let mut files=tokio::fs::read_dir(&instance).await?;let mut moved=Vec::new();
        let filename=image.as_ref().map(|(extension,_)|format!("instance-icon-{}.{extension}",uuid::Uuid::new_v4()));
        let result=async {
            while let Some(file)=files.next_entry().await? {
                if file.file_name().to_string_lossy().starts_with("instance-icon") && file.file_type().await?.is_file() {
                    let name=file.file_name();tokio::fs::rename(file.path(),backup.join(&name)).await?;moved.push(name);
                }
            }
            if let (Some(filename),Some((_,bytes)))=(&filename,&image) {
                let target=instance.join(filename);let bytes=bytes.clone();tokio::task::spawn_blocking(move ||crate::fs_utils::write_atomic(&target,&bytes)).await??;
            }
            settings.custom_icon=filename.clone();
            crate::settings::save_instance_settings_unlocked(&paths,&id,&settings).await
        }.await;
        if let Err(error)=result {
            if let Some(filename)=&filename {let _=tokio::fs::remove_file(instance.join(filename)).await;}
            for name in moved {tokio::fs::rename(backup.join(&name),instance.join(&name)).await.map_err(|restore|anyhow::anyhow!("{error:#}; old icon retained in {} because restoring failed: {restore}",backup.display()))?;}
            let _=tokio::fs::remove_dir(&backup).await;return Err(error);
        }
        crate::fs_utils::remove_if_exists(&backup).await.map_err(|error|anyhow::anyhow!("Icon saved, but could not remove old icon backup {}: {error}",backup.display()))?;
        Ok(())
    }).await
}

/// Mirrors `installDefaultInstanceIcon`: writes the bundled logo into the
/// instance and returns the file name for `settings.custom_icon`.
pub async fn install_default_instance_icon(instance: &Path) -> Result<String, anyhow::Error> {
    tokio::fs::create_dir_all(instance).await?;
    tokio::fs::write(instance.join(DEFAULT_ICON_FILENAME), DEFAULT_ICON_BYTES).await?;
    Ok(DEFAULT_ICON_FILENAME.to_string())
}

/// Mirrors `resolveIconPath`: explicit `custom_icon` first, then any
/// `instance-icon.*` file with a supported extension.
pub async fn resolve_icon_path(instance: &Path, custom_icon: Option<&str>) -> Option<PathBuf> {
    if let Some(custom) = custom_icon.filter(|c| !c.is_empty()) {
        let candidate = instance.join(custom);
        if exists(&candidate).await {
            return Some(candidate);
        }
    }
    let mut entries = tokio::fs::read_dir(instance).await.ok()?;
    while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("instance-icon") {
            continue;
        }
        let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
        if SUPPORTED_ICON_EXTENSIONS.contains(&ext.as_str()) {
            return Some(entry.path());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn library_import_selection_reset_and_failed_metadata_save_preserve_user_data() {
        let root = std::env::temp_dir().join(format!(
            "industrialis-icon-library-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = crate::paths::LauncherPaths::new(root.clone());
        let instance = paths.instance_dir("game").unwrap();
        tokio::fs::create_dir_all(&instance).await.unwrap();
        tokio::fs::write(instance.join("mmc-pack.json"), "{}")
            .await
            .unwrap();
        crate::fs_utils::write_json(
            &paths.settings_path("game").unwrap(),
            &serde_json::json!({"name":"Keep","extra":true}),
        )
        .await
        .unwrap();
        let icons = list_icons(&paths).await.unwrap();
        assert_eq!(icons[0].id, "gtnh-logo.png");
        assert!(icons[0].built_in);
        let source = root.join("my-image.png");
        tokio::fs::write(&source, DEFAULT_ICON_BYTES).await.unwrap();
        let first = import_icon(&paths, &source).await.unwrap();
        let second = import_icon(&paths, &source).await.unwrap();
        assert_eq!(first.id, "custom-my-image.png");
        assert_eq!(second.id, "custom-my-image-2.png");
        set_icon_from_library(&paths, "game", &first.id)
            .await
            .unwrap();
        let settings = crate::settings::load_instance_settings(&paths, "game")
            .await
            .unwrap();
        let original = settings.custom_icon.unwrap();
        assert_eq!(
            tokio::fs::read(instance.join(&original)).await.unwrap(),
            DEFAULT_ICON_BYTES
        );
        let target = paths.settings_path("game").unwrap();
        let permissions = std::fs::metadata(&target).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&target, readonly).unwrap();
        assert!(set_icon_from_library(&paths, "game", "gtnh-logo.png")
            .await
            .is_err());
        assert!(instance.join(&original).exists());
        assert_eq!(
            crate::settings::load_instance_settings(&paths, "game")
                .await
                .unwrap()
                .custom_icon
                .as_deref(),
            Some(original.as_str())
        );
        std::fs::set_permissions(&target, permissions).unwrap();
        assert!(set_icon_from_library(&paths, "game", "../gtnh-logo.png")
            .await
            .is_err());
        let large = root.join("large.png");
        tokio::fs::write(&large, vec![0; MAX_ICON_BYTES as usize + 1])
            .await
            .unwrap();
        assert!(set_instance_icon(&paths, "game", Some(large))
            .await
            .is_err());
        assert!(instance.join(&original).exists());
        set_instance_icon(&paths, "game", None).await.unwrap();
        assert!(!instance.join(&original).exists());
        let settings = crate::settings::load_instance_settings(&paths, "game")
            .await
            .unwrap();
        assert!(settings.custom_icon.is_none());
        assert_eq!(settings.name, "Keep");
        assert_eq!(settings.extra_fields["extra"], true);
        assert_eq!(list_icons(&paths).await.unwrap().len(), 3);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn installs_and_resolves_default_icon() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-icons-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let instance = dir.join("inst");
        let filename = install_default_instance_icon(&instance).await.unwrap();
        assert_eq!(filename, "instance-icon.png");
        let resolved = resolve_icon_path(&instance, Some(&filename)).await.unwrap();
        assert_eq!(resolved, instance.join("instance-icon.png"));
        assert!(resolve_icon_path(&instance, None).await.is_some());
        assert!(resolve_icon_path(&instance, Some("missing.png"))
            .await
            .is_some());
    }
}
