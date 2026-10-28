//! Synthetic process fixture. Never accepts the default/user profile.
use industrialis_backend::{
    paths::LauncherPaths,
    process_manager,
    running_pids::{GameSession, RunningGamePid},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 3 || (args.len() == 2 && args[1] == "recover"),
        "Expected temporary profile, executable, script; or temporary profile, recover"
    );
    let paths = LauncherPaths::new(PathBuf::from(&args[0]));
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        if args.len() == 2 {
            industrialis_backend::launch::monitor_restored_game(
                &paths,
                "game",
                Arc::new(|_, _, _| {}),
            )
            .await?;
            println!("RECOVERED");
            return Ok(());
        }
        let directory = paths.instance_dir("game")?.join(".industrialis-capture");
        tokio::fs::create_dir_all(&directory).await?;
        industrialis_backend::fs_utils::write_json(
            &paths.settings_path("game")?,
            &serde_json::json!({"name":"Synthetic game","total_play_seconds":0}),
        )
        .await?;
        let capture_id = uuid::Uuid::new_v4().to_string();
        let directory = directory.join(&capture_id);
        let process = process_manager::spawn_brokered_game(
            &PathBuf::from(&args[1]),
            &["-e".into(), args[2].to_string_lossy().into()],
            paths.base(),
            &HashMap::new(),
            Arc::new(|_, _| {}),
            &directory,
            Some((
                if cfg!(windows) {
                    "echo hook>>hook.txt".into()
                } else {
                    "printf 'hook\n' >> hook.txt".into()
                },
                paths.base().into(),
            )),
        )
        .await?;
        let pid = process.pid;
        let session = GameSession {
            capture_id: capture_id.clone(),
            started_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
            record_time: true,
            stdout_offset: 0,
            stderr_offset: 0,
            extra_fields: Default::default(),
        };
        industrialis_backend::running_pids::save_running_game_pids(
            &paths,
            &HashMap::from([(
                "game".into(),
                RunningGamePid {
                    pid,
                    creation_id: process.creation_id.clone().unwrap(),
                    launcher_session: Some(session),
                    extra_fields: Default::default(),
                },
            )]),
        )
        .await?;
        drop(process);
        let monitor = tokio::spawn({
            let paths = paths.clone();
            async move {
                industrialis_backend::launch::monitor_restored_game(
                    &paths,
                    "game",
                    Arc::new(|_, _, _| {}),
                )
                .await
            }
        });
        println!("READY {capture_id} {pid}");
        let input = tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map(|_| line)
        })
        .await??;
        anyhow::ensure!(input.trim() == "close", "Expected close");
        industrialis_backend::lifecycle::for_profile(&paths)
            .shutdown(&paths)
            .await?;
        monitor.await??;
        println!("CLOSED");
        Ok::<_, anyhow::Error>(())
    })
}
