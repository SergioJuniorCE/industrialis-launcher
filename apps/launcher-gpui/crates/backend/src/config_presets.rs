//! Retained Electron preset service. Disabling removes overlays, keeping game files.
use crate::{fs_utils::write_atomic, minecraft_files::scoped, paths::LauncherPaths};
use anyhow::{ensure, Context};
use std::path::PathBuf;

const FILES: &[(&str, &[u8])] = &[
    (
        "config/salisarcana.cfg",
        include_bytes!("../assets/config/salisarcana.cfg"),
    ),
    (
        "config/salisarcana/addons.cfg",
        include_bytes!("../assets/config/salisarcana/addons.cfg"),
    ),
    (
        "config/salisarcana/bugfixes.cfg",
        include_bytes!("../assets/config/salisarcana/bugfixes.cfg"),
    ),
    (
        "config/salisarcana/commands.cfg",
        include_bytes!("../assets/config/salisarcana/commands.cfg"),
    ),
    (
        "config/salisarcana/enhancements.cfg",
        include_bytes!("../assets/config/salisarcana/enhancements.cfg"),
    ),
    (
        "config/salisarcana/mod_integrations.cfg",
        include_bytes!("../assets/config/salisarcana/mod_integrations.cfg"),
    ),
    (
        "config/salisarcana/thaumcraft_configuration.cfg",
        include_bytes!("../assets/config/salisarcana/thaumcraft_configuration.cfg"),
    ),
    (
        "config/Betterloadingscreen/betterloadingscreen.cfg",
        include_bytes!("../assets/config/Betterloadingscreen/betterloadingscreen.cfg"),
    ),
    (
        "config/RandomThings.cfg",
        include_bytes!("../assets/config/RandomThings.cfg"),
    ),
];
const GADOMANCY: &str = "config/gadomancy.cfg";

fn validate(preset: &str) -> anyhow::Result<()> {
    ensure!(preset == "aditya", "Unknown config preset: {preset}");
    Ok(())
}
fn original(path: &std::path::Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            ensure!(meta.is_file(), "{} is not a file", path.display());
            ensure!(
                !meta.permissions().readonly(),
                "{} is read-only",
                path.display()
            );
            ensure!(
                meta.len() <= 32 * 1024 * 1024,
                "{} exceeds the configuration limit",
                path.display()
            );
            Ok(Some(std::fs::read(path)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
struct Change {
    path: PathBuf,
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
}
fn replace(path: &std::path::Path, bytes: Option<&[u8]>) -> anyhow::Result<()> {
    match bytes {
        Some(bytes) => write_atomic(path, bytes),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        },
    }
}
fn apply(paths: &LauncherPaths, id: &str, enabled: bool) -> anyhow::Result<()> {
    // Validate and capture every destination before touching any file. Symlinks
    // and read-only destinations reject the whole operation.
    let mut contents: Vec<(&str, Vec<u8>)> = FILES.iter().map(|(p, b)| (*p, b.to_vec())).collect();
    if enabled {
        let game = scoped(paths, id, ".minecraft", GADOMANCY, true)?;
        if let Some(raw) = original(&game)? {
            let text = String::from_utf8(raw).context("Gadomancy config is not UTF-8")?;
            contents.push((
                GADOMANCY,
                text.replace("B:ancientStoneRecipes=false", "B:ancientStoneRecipes=true")
                    .into_bytes(),
            ));
        }
    } else {
        contents.push((GADOMANCY, Vec::new()));
    }
    let mut changes = Vec::new();
    for (rel, bytes) in contents {
        for root in if enabled {
            &["persistent-minecraft", ".minecraft"][..]
        } else {
            &["persistent-minecraft"][..]
        } {
            let path = scoped(paths, id, root, rel, true)?;
            let old = original(&path)?;
            changes.push(Change {
                path,
                old,
                new: enabled.then(|| bytes.clone()),
            });
        }
    }
    for (index, change) in changes.iter().enumerate() {
        if let Err(error) = replace(&change.path, change.new.as_deref()) {
            let mut failures = Vec::new();
            for previous in changes[..index].iter().rev() {
                if let Err(e) = replace(&previous.path, previous.old.as_deref()) {
                    failures.push(format!("{}: {e:#}", previous.path.display()));
                }
            }
            if !failures.is_empty() {
                anyhow::bail!(
                    "Preset failed ({error:#}); some files could not be restored: {}",
                    failures.join("; ")
                );
            }
            return Err(error).context("Preset was not applied; original files were restored");
        }
    }
    Ok(())
}

pub async fn apply_config_preset(
    paths: &LauncherPaths,
    id: &str,
    preset: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    validate(preset)?;
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before changing a config preset"
    );
    crate::minecraft_files::stop_instance_watches(paths, id).await?;
    let paths = paths.clone();
    let id = id.to_owned();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            apply(&paths, &id, enabled)
        })
        .await?
    })
    .await
}
pub async fn get_config_preset_status(
    paths: &LauncherPaths,
    id: &str,
    preset: &str,
) -> anyhow::Result<bool> {
    validate(preset)?;
    let lease = crate::lifecycle::admit(paths)?;
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        let _lease = lease;
        for rel in FILES
            .iter()
            .map(|(rel, _)| *rel)
            .chain(std::iter::once(GADOMANCY))
        {
            let path = match scoped(&paths, &id, "persistent-minecraft", rel, false) {
                Ok(path) => path,
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    continue
                }
                Err(e) => return Err(e),
            };
            match std::fs::metadata(&path) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(false)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn retained_preset_updates_game_and_overlays_and_disables_without_removing_game() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-preset-{}", uuid::Uuid::new_v4())),
        );
        let game = paths.instance_dir("pack").unwrap().join(".minecraft");
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::write(
            game.join(GADOMANCY),
            "# Keep me\nB:ancientStoneRecipes=false\n",
        )
        .unwrap();
        assert!(!get_config_preset_status(&paths, "pack", "aditya")
            .await
            .unwrap());
        apply_config_preset(&paths, "pack", "aditya", true)
            .await
            .unwrap();
        let overlay = paths
            .instance_dir("pack")
            .unwrap()
            .join("persistent-minecraft");
        for (rel, bytes) in FILES {
            assert_eq!(std::fs::read(game.join(rel)).unwrap(), *bytes);
            assert_eq!(std::fs::read(overlay.join(rel)).unwrap(), *bytes);
        }
        let patched = "# Keep me\nB:ancientStoneRecipes=true\n";
        assert_eq!(
            std::fs::read_to_string(game.join(GADOMANCY)).unwrap(),
            patched
        );
        assert_eq!(
            std::fs::read_to_string(overlay.join(GADOMANCY)).unwrap(),
            patched
        );
        assert!(get_config_preset_status(&paths, "pack", "aditya")
            .await
            .unwrap());
        apply_config_preset(&paths, "pack", "aditya", false)
            .await
            .unwrap();
        assert!(!get_config_preset_status(&paths, "pack", "aditya")
            .await
            .unwrap());
        assert_eq!(
            std::fs::read_to_string(game.join(GADOMANCY)).unwrap(),
            patched
        );
        assert!(apply_config_preset(&paths, "pack", "unknown", true)
            .await
            .is_err());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn protected_destination_rejects_the_entire_preset() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-preset-{}", uuid::Uuid::new_v4())),
        );
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/config");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("salisarcana.cfg"), "original").unwrap();
        let blocked = game.join("RandomThings.cfg");
        std::fs::write(&blocked, "protected").unwrap();
        let mut permissions = std::fs::metadata(&blocked).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&blocked, permissions).unwrap();
        assert!(apply_config_preset(&paths, "pack", "aditya", true)
            .await
            .is_err());
        assert_eq!(
            std::fs::read_to_string(game.join("salisarcana.cfg")).unwrap(),
            "original"
        );
        assert!(!get_config_preset_status(&paths, "pack", "aditya")
            .await
            .unwrap());
        let mut permissions = std::fs::metadata(&blocked).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&blocked, permissions).unwrap();
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
