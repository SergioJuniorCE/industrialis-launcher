//! Resolve instance-relative shell actions before invoking the platform opener.
use crate::{
    minecraft_files::{sanitize_rel_path, scoped},
    paths::LauncherPaths,
};
use std::path::PathBuf;

pub fn instance_folder(paths: &LauncherPaths, id: &str) -> anyhow::Result<PathBuf> {
    let path = paths.instance_dir(id)?;
    anyhow::ensure!(path.is_dir(), "Instance folder not found");
    Ok(path)
}
pub fn item_path(paths: &LauncherPaths, id: &str, rel: &str) -> anyhow::Result<PathBuf> {
    let rel = sanitize_rel_path(rel)?;
    let path = scoped(paths, id, ".minecraft", &rel, false)?;
    Ok(std::path::absolute(path)?)
}
pub async fn open_instance_folder(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let _lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        open::that(instance_folder(&paths, &id)?)?;
        Ok(())
    })
    .await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_and_item_targets_belong_to_the_clicked_instance() {
        let base = std::env::temp_dir().join(format!("file-actions-{}", uuid::Uuid::new_v4()));
        let paths = LauncherPaths::new(base);
        let folder = paths.instance_dir("clicked").unwrap();
        std::fs::create_dir_all(folder.join(".minecraft/serverutilities/server")).unwrap();
        std::fs::write(
            folder.join(".minecraft/serverutilities/server/ranks.txt"),
            "ranks",
        )
        .unwrap();
        assert_eq!(instance_folder(&paths, "clicked").unwrap(), folder);
        assert_ne!(
            instance_folder(&paths, "clicked").unwrap(),
            paths.instances_dir()
        );
        assert_eq!(
            item_path(&paths, "clicked", "serverutilities/server/ranks.txt").unwrap(),
            folder.join(".minecraft/serverutilities/server/ranks.txt")
        );
        assert!(item_path(&paths, "clicked", "../instance.json").is_err());
        assert!(instance_folder(&paths, "../clicked").is_err());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
