//! Port of `apps/launcher/electron/backend/running-game-pids.ts`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::fs_utils::write_json;
use crate::paths::{validate_instance_id, LauncherPaths};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningGamePid {
    pub pid: u32,
    #[serde(rename = "creation_id")]
    pub creation_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "gpui_session"
    )]
    pub launcher_session: Option<GameSession>,
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSession {
    pub capture_id: String,
    pub started_at_ms: u64,
    pub record_time: bool,
    pub stdout_offset: u64,
    pub stderr_offset: u64,
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
}

fn valid_pid(value: &serde_json::Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
}

fn valid_creation_id(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Mirrors `loadRunningGamePids`: malformed entries are skipped, and ids only
/// survive when already in sanitized form (`id === rawId` in TS).
pub async fn load_running_game_pids(
    paths: &LauncherPaths,
) -> Result<HashMap<String, RunningGamePid>, anyhow::Error> {
    let Some(value) = crate::profile_store::read_object(&paths.running_game_pids_path()).await?
    else {
        return Ok(HashMap::new());
    };
    let Some(obj) = value.as_object() else {
        return Ok(HashMap::new());
    };
    let mut pids = HashMap::new();
    for (raw_id, entry) in obj {
        let (Some(pid), Some(creation_id)) = (
            entry.get("pid").and_then(valid_pid),
            entry.get("creation_id").and_then(valid_creation_id),
        ) else {
            continue;
        };
        match validate_instance_id(raw_id) {
            Ok(id) if id == *raw_id => {
                let mut extras = entry.as_object().cloned().unwrap_or_default();
                let session = entry
                    .get("launcher_session")
                    .or_else(|| entry.get("gpui_session"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                if session.is_some() {
                    extras.remove("launcher_session");
                    extras.remove("gpui_session");
                }
                for key in ["pid", "creation_id"] {
                    extras.remove(key);
                }
                pids.insert(
                    id,
                    RunningGamePid {
                        pid,
                        creation_id,
                        launcher_session: session,
                        extra_fields: extras,
                    },
                );
            }
            _ => continue,
        }
    }
    Ok(pids)
}

/// Mirrors `saveRunningGamePids`: sorted by id, file removed when empty.
pub async fn save_running_game_pids(
    paths: &LauncherPaths,
    pids: &HashMap<String, RunningGamePid>,
) -> Result<(), anyhow::Error> {
    let paths = paths.clone();
    let pids = pids.clone();
    crate::profile_store::mutate(async move { save_unlocked(&paths, &pids).await }).await
}

pub(crate) async fn update<T: Send + 'static>(
    paths: &LauncherPaths,
    change: impl FnOnce(&mut HashMap<String, RunningGamePid>) -> T + Send + 'static,
) -> anyhow::Result<T> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let mut pids = load_running_game_pids(&paths).await?;
        let result = change(&mut pids);
        save_unlocked(&paths, &pids).await?;
        Ok(result)
    })
    .await
}

async fn save_unlocked(
    paths: &LauncherPaths,
    pids: &HashMap<String, RunningGamePid>,
) -> anyhow::Result<()> {
    let mut entries: Vec<(&String, &RunningGamePid)> = pids
        .iter()
        .filter(|(id, p)| !id.is_empty() && p.pid > 0 && !p.creation_id.trim().is_empty())
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let path = paths.running_game_pids_path();
    let original = crate::profile_store::read_object(&path)
        .await?
        .unwrap_or_else(|| serde_json::json!({}));
    let previous = load_running_game_pids(paths).await?;
    let mut saved = original.as_object().unwrap().clone();
    for id in previous.keys() {
        if !pids.contains_key(id) {
            saved.remove(id);
        }
    }
    for (id, p) in entries {
        if previous.get(id) != Some(p) {
            saved.insert(id.clone(), serde_json::to_value(p)?);
        }
    }
    if saved.is_empty() {
        tokio::fs::remove_file(&path).await.or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })?;
        return Ok(());
    }
    if original.as_object() == Some(&saved) {
        return Ok(());
    }
    write_json(&path, &serde_json::Value::Object(saved)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_updates_keep_other_games_and_unknown_rows() {
        let paths = temp_paths("concurrent");
        write_json(&paths.running_game_pids_path(),&serde_json::json!({
            "legacy":{"pid":33,"creation_id":"  old  ","launcher_session":{"future":true},"extension":null},
            "unknown":{"pid":"unsupported","keep":true}
        })).await.unwrap();
        let before: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(paths.running_game_pids_path())
                .await
                .unwrap(),
        )
        .unwrap();
        let mut work = Vec::new();
        for n in 0..10 {
            let paths = paths.clone();
            work.push(tokio::spawn(async move {
                update(&paths, move |pids| {
                    pids.insert(
                        format!("game-{n}"),
                        RunningGamePid {
                            pid: n + 100,
                            creation_id: format!("c{n}"),
                            launcher_session: None,
                            extra_fields: Default::default(),
                        },
                    );
                })
                .await
                .unwrap();
            }));
        }
        for task in work {
            task.await.unwrap();
        }
        let after: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(paths.running_game_pids_path())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(after["legacy"], before["legacy"]);
        assert_eq!(after["unknown"], before["unknown"]);
        assert_eq!(load_running_game_pids(&paths).await.unwrap().len(), 11);
        tokio::fs::write(paths.running_game_pids_path(), "broken")
            .await
            .unwrap();
        assert!(update(&paths, |pids| pids.clear()).await.is_err());
        assert_eq!(
            tokio::fs::read(paths.running_game_pids_path())
                .await
                .unwrap(),
            b"broken"
        );
    }

    #[tokio::test]
    async fn retired_session_key_still_loads() {
        let paths = temp_paths("retired-session");
        tokio::fs::create_dir_all(paths.base()).await.unwrap();
        write_json(
            &paths.running_game_pids_path(),
            &serde_json::json!({
                "game": {
                    "pid": 10,
                    "creation_id": "c",
                    "gpui_session": {
                        "capture_id": "x",
                        "started_at_ms": 1,
                        "record_time": false,
                        "stdout_offset": 0,
                        "stderr_offset": 0
                    }
                }
            }),
        )
        .await
        .unwrap();
        let pids = load_running_game_pids(&paths).await.unwrap();
        assert!(pids["game"].launcher_session.is_some());
    }

    fn temp_paths(name: &str) -> LauncherPaths {
        let dir = std::env::temp_dir().join(format!(
            "industrialis-pids-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        LauncherPaths::new(dir)
    }

    #[tokio::test]
    async fn round_trips_sorted_and_skips_malformed() {
        let paths = temp_paths("roundtrip");
        let target = paths.running_game_pids_path();
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &target,
            serde_json::to_string(&serde_json::json!({
                "zeta": { "pid": 11, "creation_id": "c1" },
                "alpha": { "pid": 22, "creation_id": "c2" },
                "bad pid": { "pid": 0, "creation_id": "c3" },
                "bad creation": { "pid": 33, "creation_id": "  " },
                "needs sanitize/instance": { "pid": 44, "creation_id": "c4" },
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let loaded = load_running_game_pids(&paths).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["alpha"].pid, 22);

        save_running_game_pids(&paths, &loaded).await.unwrap();
        let raw = tokio::fs::read_to_string(&target).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let keys: Vec<&str> = parsed
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 5);
        assert_eq!(tokio::fs::read_to_string(&target).await.unwrap(), raw);
    }

    #[tokio::test]
    async fn removes_file_when_empty() {
        let paths = temp_paths("empty");
        let target = paths.running_game_pids_path();
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&target, "{}").await.unwrap();
        save_running_game_pids(&paths, &HashMap::new())
            .await
            .unwrap();
        assert!(!target.exists());
        assert!(load_running_game_pids(&paths).await.unwrap().is_empty());
    }
}
