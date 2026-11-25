//! User-defined config profiles: named, versioned sets of in-place config edits.
//!
//! A profile changes values in an instance's `.minecraft` tree without
//! replacing whole files, so comments, ordering and sibling keys survive. A
//! failed apply restores every file it already wrote.

use crate::fs_utils::write_atomic;
use crate::minecraft_files::scoped;
use crate::paths::LauncherPaths;
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROFILES_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilesFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<ConfigProfile>,
}

fn default_version() -> u32 {
    PROFILES_VERSION
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigProfile {
    pub name: String,
    #[serde(default)]
    pub overrides: Vec<ConfigOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigOverride {
    /// Path relative to the instance's `.minecraft` directory.
    pub file: String,
    /// Key exactly as written in the file, without a type prefix or quotes.
    pub key: String,
    /// Replacement value, written verbatim after the separator.
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApplyReport {
    pub changed_files: usize,
    pub applied_overrides: usize,
    /// Overrides the pack no longer supports, so the rest of the profile still applies.
    pub skipped: Vec<String>,
}

impl ApplyReport {
    pub fn summary(&self, profile: &str) -> String {
        let mut message = format!(
            "Applied config profile {profile:?} ({} overrides, {} files)",
            self.applied_overrides, self.changed_files
        );
        if !self.skipped.is_empty() {
            message.push_str(&format!("; skipped: {}", self.skipped.join("; ")));
        }
        message
    }
}

pub async fn load_profiles(paths: &LauncherPaths) -> anyhow::Result<ProfilesFile> {
    let path = paths.config_profiles_path();
    match crate::profile_store::read_object(&path).await? {
        Some(value) => parse_profiles(value)
            .with_context(|| format!("Invalid config profiles in {}", path.display())),
        None => {
            let seeded = seed_profiles();
            save_profiles_unlocked(paths, &seeded).await?;
            Ok(seeded)
        }
    }
}

fn parse_profiles(value: serde_json::Value) -> anyhow::Result<ProfilesFile> {
    let mut file: ProfilesFile = serde_json::from_value(value)?;
    ensure!(
        file.version <= PROFILES_VERSION,
        "Config profiles were written by a newer launcher (version {})",
        file.version
    );
    file.version = PROFILES_VERSION;
    validate_profiles(&file)?;
    Ok(file)
}

pub async fn save_profiles(
    paths: &LauncherPaths,
    file: &ProfilesFile,
) -> anyhow::Result<ProfilesFile> {
    validate_profiles(file)?;
    let paths = paths.clone();
    let file = file.clone();
    let saved = file.clone();
    crate::profile_store::mutate(async move { save_profiles_unlocked(&paths, &file).await })
        .await?;
    Ok(saved)
}

async fn save_profiles_unlocked(
    paths: &LauncherPaths,
    file: &ProfilesFile,
) -> anyhow::Result<()> {
    validate_profiles(file)?;
    crate::fs_utils::write_json(&paths.config_profiles_path(), &serde_json::to_value(file)?).await
}

fn validate_profiles(file: &ProfilesFile) -> anyhow::Result<()> {
    for (index, profile) in file.profiles.iter().enumerate() {
        ensure!(
            !profile.name.trim().is_empty(),
            "Config profile {} has no name",
            index + 1
        );
        ensure!(
            !file.profiles[..index]
                .iter()
                .any(|other| other.name.eq_ignore_ascii_case(profile.name.trim())),
            "Config profile names must be unique: {}",
            profile.name
        );
        for rule in &profile.overrides {
            validate_rel(&rule.file)?;
            ensure!(
                !rule.key.trim().is_empty(),
                "Config profile {:?} has an override with an empty key",
                profile.name
            );
        }
    }
    Ok(())
}

/// Applies a profile to an installed instance and records it as the last
/// applied profile. A missing file or key is skipped so pack updates do not
/// make a profile unusable; malformed paths fail before anything is written.
pub async fn apply_profile(
    paths: &LauncherPaths,
    id: &str,
    name: &str,
) -> anyhow::Result<ApplyReport> {
    let file = load_profiles(paths).await?;
    let profile = file
        .profiles
        .into_iter()
        .find(|profile| profile.name == name)
        .with_context(|| format!("Config profile {name:?} was not found"))?;
    let lease = crate::instance_guard::reserve_instances(paths, &[id])?;
    ensure!(
        !crate::launch::is_already_running(paths, id).await?,
        "Stop the game before applying a config profile"
    );
    crate::minecraft_files::stop_instance_watches(paths, id).await?;
    let profile_paths = paths.clone();
    let profile_id = id.to_owned();
    let applied = crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            apply_inner(&profile_paths, &profile_id, &profile)
        })
        .await?
    })
    .await?;
    let mut settings = crate::settings::load_instance_settings(paths, id).await?;
    if settings.config_profile.as_deref() != Some(name) {
        settings.config_profile = Some(name.to_string());
        crate::settings::save_instance_settings(paths, id, &settings).await?;
    }
    Ok(applied)
}

fn apply_inner(
    paths: &LauncherPaths,
    id: &str,
    profile: &ConfigProfile,
) -> anyhow::Result<ApplyReport> {
    let mut planned: Vec<PlannedFile> = Vec::new();
    let mut skipped = Vec::new();
    let mut applied_overrides = 0;
    for rule in &profile.overrides {
        validate_rel(&rule.file)?;
        ensure!(
            !rule.key.trim().is_empty(),
            "Config profile {:?} has an override with an empty key",
            profile.name
        );
        let primary = match scoped(paths, id, ".minecraft", &rule.file, false) {
            Ok(path) => path,
            Err(error) if is_not_found(&error) => {
                skipped.push(format!("{}: file not found", rule.file));
                continue;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Config path {:?} was rejected", rule.file))
            }
        };
        validate_destination(&primary)?;
        ensure_inside(paths, id, ".minecraft", &primary)?;
        if plan_edit(&mut planned, &primary, rule)? {
            applied_overrides += 1;
        } else {
            skipped.push(format!("{}: key {:?} not found", rule.file, rule.key));
        }
        // A persistent overlay is copied over .minecraft on every pack update,
        // so the same edit keeps it from undoing the profile later.
        let overlay = match scoped(paths, id, "persistent-minecraft", &rule.file, false) {
            Ok(path) => path,
            Err(error) if is_not_found(&error) => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Config path {:?} was rejected", rule.file))
            }
        };
        validate_destination(&overlay)?;
        ensure_inside(paths, id, "persistent-minecraft", &overlay)?;
        plan_edit(&mut planned, &overlay, rule)?;
    }
    let changes: Vec<Change> = planned
        .into_iter()
        .filter(|plan| plan.changed)
        .map(|plan| Change {
            old: plan.old,
            new: plan.text.into_bytes(),
            path: plan.path,
        })
        .collect();
    write_changes(&changes)?;
    Ok(ApplyReport {
        changed_files: changes.len(),
        applied_overrides,
        skipped,
    })
}

struct PlannedFile {
    path: PathBuf,
    old: Vec<u8>,
    text: String,
    changed: bool,
}

struct Change {
    path: PathBuf,
    old: Vec<u8>,
    new: Vec<u8>,
}

fn write_changes(changes: &[Change]) -> anyhow::Result<()> {
    for (index, change) in changes.iter().enumerate() {
        if let Err(error) = write_atomic(&change.path, &change.new) {
            let mut failures = Vec::new();
            for previous in changes[..index].iter().rev() {
                if let Err(e) = write_atomic(&previous.path, &previous.old) {
                    failures.push(format!("{}: {e:#}", previous.path.display()));
                }
            }
            if !failures.is_empty() {
                anyhow::bail!(
                    "Config profile failed ({error:#}); some files could not be restored: {}",
                    failures.join("; ")
                );
            }
            return Err(error).context("Config profile was not applied; original files were restored");
        }
    }
    Ok(())
}

fn validate_destination(path: &Path) -> anyhow::Result<()> {
    let meta = std::fs::metadata(path)?;
    ensure!(meta.is_file(), "{} is not a file", path.display());
    ensure!(
        !meta.permissions().readonly(),
        "{} is read-only",
        path.display()
    );
    ensure!(
        meta.len() <= MAX_CONFIG_BYTES,
        "{} exceeds the configuration limit",
        path.display()
    );
    Ok(())
}

fn plan_edit(
    planned: &mut Vec<PlannedFile>,
    path: &Path,
    rule: &ConfigOverride,
) -> anyhow::Result<bool> {
    let index = match planned.iter().position(|plan| plan.path == path) {
        Some(index) => index,
        None => {
            let old = std::fs::read(path)?;
            let text = String::from_utf8(old.clone())
                .with_context(|| format!("{} is not UTF-8", path.display()))?;
            planned.push(PlannedFile {
                path: path.to_path_buf(),
                old,
                text,
                changed: false,
            });
            planned.len() - 1
        }
    };
    let plan = &mut planned[index];
    let (next, replaced) = replace_key(&plan.text, &rule.key, &rule.value);
    if replaced == 0 {
        return Ok(false);
    }
    plan.text = next;
    plan.changed = true;
    Ok(true)
}

fn validate_rel(rel: &str) -> anyhow::Result<()> {
    ensure!(!rel.trim().is_empty(), "A config path is empty");
    ensure!(
        !rel.starts_with('/') && !rel.starts_with('\\'),
        "Config paths are relative to .minecraft: {rel}"
    );
    ensure!(
        !rel.contains('\\'),
        "Config paths use forward slashes: {rel}"
    );
    ensure!(
        !rel.contains(':'),
        "Config paths must stay inside .minecraft: {rel}"
    );
    for part in rel.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "Config paths must stay inside .minecraft: {rel}"
        );
    }
    Ok(())
}

fn ensure_inside(
    paths: &LauncherPaths,
    id: &str,
    root: &str,
    path: &Path,
) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(paths.instance_dir(id)?.join(root))?;
    let path = std::fs::canonicalize(path)?;
    ensure!(
        path.starts_with(&root),
        "Config paths must stay inside {root:?}"
    );
    Ok(())
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

/// Replaces every line whose key matches, keeping the line's indentation,
/// separator spacing, trailing whitespace and line ending.
fn replace_key(text: &str, key: &str, value: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len() + value.len());
    let mut replaced = 0;
    for line in text.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        let (body, carriage) = body
            .strip_suffix('\r')
            .map_or((body, ""), |body| (body, "\r"));
        match rewrite_line(body, key, value) {
            Some(line) => {
                out.push_str(&line);
                replaced += 1;
            }
            None => out.push_str(body),
        }
        out.push_str(carriage);
        out.push_str(newline);
    }
    (out, replaced)
}

fn rewrite_line(line: &str, key: &str, value: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }
    let separator = line
        .find('=')
        .or_else(|| line.find(':'))
        .filter(|index| *index > 0)?;
    let mut name = line[..separator].trim();
    if name.len() > 2 && name.as_bytes()[1] == b':' && name.as_bytes()[0].is_ascii_alphabetic() {
        name = name[2..].trim();
    }
    let name = name
        .strip_prefix('"')
        .and_then(|name| name.strip_suffix('"'))
        .unwrap_or(name);
    if name != key {
        return None;
    }
    let after = &line[separator + 1..];
    let leading = after.len() - after.trim_start_matches([' ', '\t']).len();
    let value_start = separator + 1 + leading;
    let old_value = &line[value_start..];
    let trailing = old_value.len() - old_value.trim_end_matches([' ', '\t']).len();
    let mut rewritten = String::with_capacity(line.len() + value.len());
    rewritten.push_str(&line[..value_start]);
    rewritten.push_str(value);
    rewritten.push_str(&old_value[old_value.len() - trailing..]);
    Some(rewritten)
}

fn seed_profiles() -> ProfilesFile {
    ProfilesFile {
        version: PROFILES_VERSION,
        profiles: vec![ConfigProfile {
            name: "GTNH quiet defaults".into(),
            overrides: vec![
                ConfigOverride {
                    file: "config/GregTech/Pollution.cfg".into(),
                    key: "Activate Pollution".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/GregTech/WorldGeneration.cfg".into(),
                    key: "generateUndergroundDirtGen".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/GregTech/WorldGeneration.cfg".into(),
                    key: "generateUndergroundGravelGen".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/rwg2.cfg".into(),
                    key: "Generate Caves".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/RWG.cfg".into(),
                    key: "Generate Caves".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "serverutilities/server/ranks.txt".into(),
                    key: "serverutilities.claims.max_chunks".into(),
                    value: "1000".into(),
                },
                ConfigOverride {
                    file: "serverutilities/serverutilities.cfg".into(),
                    key: "chunk_claiming".into(),
                    value: "true".into(),
                },
            ],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> LauncherPaths {
        LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-profiles-{}", uuid::Uuid::new_v4())),
        )
    }

    fn instance(paths: &LauncherPaths, id: &str) -> PathBuf {
        let dir = paths.instance_dir(id).unwrap().join(".minecraft");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn in_place_edit_preserves_comments_siblings_and_line_endings() {
        let text = "# Top comment\r\nworldgen {\r\n    general {\r\n        # keep me\r\n        B:generateUndergroundDirtGen=true\r\n        B:generateUndergroundGravelGen=true\r\n        I:other=7\r\n    }\r\n}\r\n";
        let (next, count) = replace_key(text, "generateUndergroundDirtGen", "false");
        assert_eq!(count, 1);
        assert!(next.contains("# Top comment\r\n"));
        assert!(next.contains("# keep me\r\n"));
        assert!(next.contains("B:generateUndergroundDirtGen=false\r\n"));
        assert!(next.contains("B:generateUndergroundGravelGen=true\r\n"));
        assert!(next.contains("I:other=7\r\n"));
        let (next, count) = replace_key(&next, "generateUndergroundGravelGen", "false");
        assert_eq!(count, 1);
        assert!(next.contains("B:generateUndergroundGravelGen=false\r\n"));
        assert!(next.contains("I:other=7\r\n"));
    }

    #[test]
    fn repeated_keys_are_all_replaced() {
        let text = "[player]\nserverutilities.claims.max_chunks: 100\n[vip]\nserverutilities.claims.max_chunks: 100\n";
        let (next, count) = replace_key(text, "serverutilities.claims.max_chunks", "1000");
        assert_eq!(count, 2);
        assert_eq!(
            next,
            "[player]\nserverutilities.claims.max_chunks: 1000\n[vip]\nserverutilities.claims.max_chunks: 1000\n"
        );
    }

    #[tokio::test]
    async fn profiles_seed_once_and_round_trip() {
        let paths = paths();
        let first = load_profiles(&paths).await.unwrap();
        assert_eq!(first.profiles.len(), 1);
        assert_eq!(first.profiles[0].name, "GTNH quiet defaults");
        let second = load_profiles(&paths).await.unwrap();
        assert_eq!(first, second);
        let mut edited = second.clone();
        edited.profiles[0].name = "Edited".into();
        save_profiles(&paths, &edited).await.unwrap();
        assert_eq!(load_profiles(&paths).await.unwrap(), edited);
    }

    #[tokio::test]
    async fn missing_key_and_file_are_skipped_without_touching_files() {
        let paths = paths();
        instance(&paths, "pack");
        let config = paths.instance_dir("pack").unwrap().join(".minecraft/config");
        std::fs::create_dir_all(&config).unwrap();
        let file = config.join("test.cfg");
        std::fs::write(&file, "B:known=true\n").unwrap();
        let mut profiles = load_profiles(&paths).await.unwrap();
        profiles.profiles[0] = ConfigProfile {
            name: "Partial".into(),
            overrides: vec![
                ConfigOverride {
                    file: "config/test.cfg".into(),
                    key: "known".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/test.cfg".into(),
                    key: "absent".into(),
                    value: "false".into(),
                },
                ConfigOverride {
                    file: "config/gone.cfg".into(),
                    key: "known".into(),
                    value: "false".into(),
                },
            ],
        };
        save_profiles(&paths, &profiles).await.unwrap();
        let report = apply_profile(&paths, "pack", "Partial").await.unwrap();
        assert_eq!(report.applied_overrides, 1);
        assert_eq!(report.changed_files, 1);
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "B:known=false\n"
        );
    }

    #[tokio::test]
    async fn paths_outside_minecraft_are_rejected_before_any_write() {
        let paths = paths();
        instance(&paths, "pack");
        let file = ProfilesFile {
            version: PROFILES_VERSION,
            profiles: vec![ConfigProfile {
                name: "Escape".into(),
                overrides: vec![ConfigOverride {
                    file: "../outside.cfg".into(),
                    key: "x".into(),
                    value: "1".into(),
                }],
            }],
        };
        crate::fs_utils::write_json(
            &paths.config_profiles_path(),
            &serde_json::to_value(&file).unwrap(),
        )
        .await
        .unwrap();
        let error = apply_profile(&paths, "pack", "Escape").await.unwrap_err();
        assert!(format!("{error:#}").contains("inside .minecraft"));
        assert!(!paths
            .instance_dir("pack")
            .unwrap()
            .join("outside.cfg")
            .exists());
    }

    #[tokio::test]
    async fn seeded_gtnh_profile_edits_every_pack_key() {
        let paths = paths();
        let root = instance(&paths, "pack");
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "config/GregTech/Pollution.cfg",
            "pollution {\n    # keep\n    B:\"Activate Pollution\"=true\n}\n",
        );
        write(
            "config/GregTech/WorldGeneration.cfg",
            "worldgen {\n    general {\n        B:generateUndergroundDirtGen=true\n        B:generateUndergroundGravelGen=true\n    }\n}\n",
        );
        write(
            "config/rwg2.cfg",
            "settings {\n    B:\"Generate Caves\"=true\n}\n",
        );
        write(
            "config/RWG.cfg",
            "settings {\n    B:\"Generate Caves\"=true\n}\n",
        );
        write(
            "serverutilities/server/ranks.txt",
            "[player]\nserverutilities.claims.max_chunks: 100\n[admin]\nserverutilities.claims.max_chunks: 1000\n",
        );
        write(
            "serverutilities/serverutilities.cfg",
            "general {\n    B:chunk_claiming=false\n}\n",
        );
        let overlay = paths.instance_dir("pack").unwrap().join("persistent-minecraft");
        let write_overlay = |rel: &str, text: &str| {
            let path = overlay.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write_overlay(
            "config/RWG.cfg",
            "settings {\n    B:\"Generate Caves\"=true\n}\n",
        );
        write_overlay(
            "serverutilities/serverutilities.cfg",
            "general {\n    B:chunk_claiming=false\n}\n",
        );
        let report = apply_profile(&paths, "pack", "GTNH quiet defaults")
            .await
            .unwrap();
        assert_eq!(report.applied_overrides, 7);
        assert_eq!(report.changed_files, 8);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap();
        assert!(read("config/GregTech/Pollution.cfg").contains("B:\"Activate Pollution\"=false"));
        assert!(read("config/GregTech/Pollution.cfg").contains("# keep"));
        let worldgen = read("config/GregTech/WorldGeneration.cfg");
        assert!(worldgen.contains("B:generateUndergroundDirtGen=false"));
        assert!(worldgen.contains("B:generateUndergroundGravelGen=false"));
        assert!(read("config/rwg2.cfg").contains("B:\"Generate Caves\"=false"));
        assert!(read("config/RWG.cfg").contains("B:\"Generate Caves\"=false"));
        let ranks = read("serverutilities/server/ranks.txt");
        assert_eq!(ranks.matches("serverutilities.claims.max_chunks: 1000").count(), 2);
        assert!(ranks.contains("serverutilities.claims.max_chunks: 1000"));
        assert!(read("serverutilities/serverutilities.cfg").contains("B:chunk_claiming=true"));
        let overlay_read = |rel: &str| std::fs::read_to_string(overlay.join(rel)).unwrap();
        assert!(overlay_read("config/RWG.cfg").contains("B:\"Generate Caves\"=false"));
        assert!(overlay_read("serverutilities/serverutilities.cfg")
            .contains("B:chunk_claiming=true"));
        let settings = crate::settings::load_instance_settings(&paths, "pack")
            .await
            .unwrap();
        assert_eq!(
            settings.config_profile.as_deref(),
            Some("GTNH quiet defaults")
        );
    }

    #[test]
    fn a_mid_apply_failure_restores_every_written_file() {
        let root = std::env::temp_dir().join(format!("industrialis-rollback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("first.cfg");
        let blocker = root.join("blocker");
        std::fs::write(&first, "old\n").unwrap();
        std::fs::write(&blocker, "not a directory\n").unwrap();
        let changes = vec![
            Change {
                path: first.clone(),
                old: b"old\n".to_vec(),
                new: b"new\n".to_vec(),
            },
            Change {
                path: blocker.join("second.cfg"),
                old: Vec::new(),
                new: b"new\n".to_vec(),
            },
        ];
        assert!(write_changes(&changes).is_err());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "old\n");
    }
}
