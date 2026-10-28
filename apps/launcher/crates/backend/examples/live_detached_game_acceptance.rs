//! Opt-in Linux virtual-desktop acceptance with an actual client and fresh launcher processes.
use anyhow::{ensure, Context};
use industrialis_backend::{
    accounts, console_log, launch, launch_config, lifecycle, paths::LauncherPaths, process_manager,
    profile_owner, running_pids, settings,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() -> anyhow::Result<()> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    ensure!(
        cfg!(target_os = "linux"),
        "This acceptance harness requires a Linux virtual desktop"
    );
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3 || args.len() == 4,
        "Usage: live_detached_game_acceptance <marked-profile> <instance-id> <java-path>"
    );
    let profile = PathBuf::from(&args[0]);
    ensure!(
        profile.is_absolute()
            && !std::fs::symlink_metadata(&profile)?
                .file_type()
                .is_symlink()
            && std::fs::read_to_string(profile.join(".gpui-live-acceptance"))?
                == "industrialis-expendable-live-acceptance-v1",
        "Use a marked expendable profile"
    );
    let paths = LauncherPaths::new(profile);
    let id = industrialis_backend::paths::validate_instance_id(&args[1])?;
    let instance = paths.instance_dir(&id)?;
    ensure!(
        !std::fs::symlink_metadata(&instance)?
            .file_type()
            .is_symlink(),
        "Instance must not be a symlink"
    );
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        ensure!(
            accounts::load_accounts(&paths)
                .await?
                .iter()
                .all(|a| a.account_type == "offline"),
            "This test must not use real account credentials"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    if args.len() == 3 {
        return orchestrate(&runtime, &paths, &args);
    }
    let _owner = match profile_owner::acquire(&paths, profile_owner::OwnerKind::Operation)? {
        profile_owner::Acquisition::Primary(owner) => owner,
        _ => anyhow::bail!("Acceptance profile is already in use"),
    };
    runtime.block_on(async {
        match args[3].as_str() {
            "start" => start(&paths, &id, &args[2]).await,
            "attach" => attach(&paths, &id).await,
            "recover" => recover(&paths, &id).await,
            _ => anyhow::bail!("Unknown acceptance phase"),
        }
    })
}

fn orchestrate(
    runtime: &tokio::runtime::Runtime,
    paths: &LauncherPaths,
    args: &[String],
) -> anyhow::Result<()> {
    runtime.block_on(async {
        ensure!(
            launch::running_instances(paths)
                .await?
                .iter()
                .all(|(_, _, alive)| !alive),
            "Acceptance profile already has a running game"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    let started = std::time::Instant::now();
    for phase in ["start", "attach", "recover"] {
        println!("Actual-client detached acceptance: {phase}");
        let status = Command::new(std::env::current_exe()?)
            .args(args)
            .arg(phase)
            .status()?;
        if !status.success() {
            // Stop only the game in this offline, marked acceptance instance.
            if let profile_owner::Acquisition::Primary(_owner) =
                profile_owner::acquire(paths, profile_owner::OwnerKind::Operation)?
            {
                let _ = runtime.block_on(launch::kill_instance(paths, &args[1]));
            }
            anyhow::bail!("Acceptance phase {phase} failed ({status})");
        }
        if phase != "recover" {
            let report: Value = serde_json::from_slice(&std::fs::read(
                paths
                    .base()
                    .join(format!("detached-start-{}.json", args[1])),
            )?)?;
            let pid = report["pid"].as_u64().context("Missing game PID")? as u32;
            let creation = report["creation_id"]
                .as_str()
                .context("Missing process identity")?;
            ensure!(
                process_manager::matches_creation_id(pid, creation),
                "Actual client did not survive launcher exit"
            );
            // No launcher process exists during this interval; the broker still captures output.
            std::thread::sleep(Duration::from_secs(5));
            ensure!(
                process_manager::matches_creation_id(pid, creation),
                "Client exited while launcher was closed"
            );
        }
    }
    let file = paths
        .base()
        .join(format!("detached-game-acceptance-{}.json", args[1]));
    let mut report: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    report["elapsed_seconds"] = started.elapsed().as_secs().into();
    report["launcher_process_restart"] = true.into();
    std::fs::write(&file, serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn start(paths: &LauncherPaths, id: &str, java: &str) -> anyhow::Result<()> {
    ensure!(
        !launch::running_instances(paths)
            .await?
            .iter()
            .any(|(_, _, alive)| *alive),
        "Game already running"
    );
    let instance = paths.instance_dir(id)?;
    let account = accounts::load_accounts(paths).await?.into_iter().next();
    let account = match account {
        Some(a) => accounts::account_to_info(&a).id,
        None => {
            accounts::create_offline_account(paths, "LauncherAcceptance")
                .await?
                .id
        }
    };
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let mut config = settings::load_instance_settings(paths, id).await?;
    let initial_seconds = config.total_play_seconds;
    let initial_exit_lines = console_log::read_console_log(paths, id, true)
        .await?
        .iter()
        .filter(|line| line.line.starts_with("Process exited with code"))
        .count();
    config.override_account = true;
    config.account_id = Some(account);
    config.override_java_location = true;
    config.java_path = Some(java.into());
    config.override_memory = true;
    config.min_ram_mb = 1024;
    config.max_ram_mb = 6144;
    config.perm_gen_mb = 0;
    config.override_window = true;
    config.window_width = 854;
    config.window_height = 480;
    config.launch_maximized = false;
    config.override_java_args = true;
    config.jvm_args = "-Dgpui.acceptance.first=true\n-Dgpui.acceptance.second=true".into();
    config.join_server_on_launch = false;
    config.override_game_time = true;
    config.record_game_time = true;
    config.override_env = true;
    config
        .env_vars
        .insert("LAUNCHER_ACCEPTANCE_MARKER".into(), nonce.clone());
    config.override_commands = true;
    config.pre_launch_command =
        "printf '%s\\n' \"$LAUNCHER_ACCEPTANCE_MARKER\" '$INST_ID' > acceptance-pre.txt".into();
    config.post_exit_command =
        "printf '%s\\n' \"$LAUNCHER_ACCEPTANCE_MARKER\" '$INST_ID' >> acceptance-post.txt".into();
    config.wrapper_command = "sh \"$INST_DIR/acceptance-wrapper.sh\"".into();
    tokio::fs::write(instance.join("acceptance-wrapper.sh"), format!(
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$LAUNCHER_ACCEPTANCE_MARKER\" > ../acceptance-env.txt\nprintf '%s\\n' \"$@\" > ../acceptance-args.txt\ngame=$$\n(while kill -0 \"$game\" 2>/dev/null; do printf 'LAUNCHER_ACCEPTANCE_TICK_{nonce} %s\\n' \"$(date +%s%N)\"; sleep 1; done) &\nexec \"$@\"\n"
    )).await?;
    // Unique marker makes repeated runs independent without deleting previous evidence.
    tokio::fs::write(instance.join("acceptance-post.txt"), b"").await?;
    settings::save_instance_settings(paths, id, &config).await?;
    let ready = Arc::new(AtomicBool::new(false));
    let observed = ready.clone();
    let emit: launch_config::EmitLaunchLog = Arc::new(move |_, _, line| {
        if line.contains("Forge Mod Loader has successfully loaded") {
            observed.store(true, Ordering::SeqCst);
        }
    });
    let game_paths = paths.clone();
    let game_id = id.to_string();
    let mut task =
        tokio::spawn(async move { launch::launch_instance(&game_paths, &game_id, emit).await });
    let begin = std::time::Instant::now();
    let deadline = std::env::var("LAUNCHER_GAME_ACCEPTANCE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(900);
    loop {
        tokio::select! {
            result = &mut task => { result??; anyhow::bail!("Client exited before detach"); }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if ready.load(Ordering::SeqCst) { break; }
                if begin.elapsed().as_secs() >= deadline {
                    let _ = launch::kill_instance(paths, id).await;
                    anyhow::bail!("Timed out before actual Forge loading completed");
                }
                if begin.elapsed().as_secs() % 30 == 0 { println!("Waiting for Forge loading: {} seconds", begin.elapsed().as_secs()); }
            }
        }
    }
    println!("Actual Forge loading complete; draining and closing first launcher process");
    let entry = running_pids::load_running_game_pids(paths)
        .await?
        .remove(id)
        .context("Missing game session")?;
    let session = entry
        .launcher_session
        .as_ref()
        .context("Missing native capture session")?;
    tokio::time::timeout(
        Duration::from_secs(30),
        lifecycle::for_profile(paths).shutdown(paths),
    )
    .await??;
    tokio::time::timeout(Duration::from_secs(30), task).await???;
    ensure!(
        process_manager::matches_creation_id(entry.pid, &entry.creation_id),
        "Shutdown killed the client"
    );
    let saved = running_pids::load_running_game_pids(paths)
        .await?
        .remove(id)
        .context("Detach lost PID")?;
    let saved_session = saved.launcher_session.context("Detach lost session")?;
    let report = json!({"pid":entry.pid,"creation_id":entry.creation_id,"capture_id":session.capture_id,
        "started_at_ms":session.started_at_ms,"initial_play_seconds":initial_seconds,"nonce":nonce,
        "stdout_offset":saved_session.stdout_offset,"initial_exit_lines":initial_exit_lines,
        "checkpoint_play_seconds":settings::load_instance_settings(paths,id).await?.total_play_seconds});
    tokio::fs::write(
        paths.base().join(format!("detached-start-{id}.json")),
        serde_json::to_vec_pretty(&report)?,
    )
    .await?;
    Ok(())
}

async fn attach(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let entry = running_pids::load_running_game_pids(paths)
        .await?
        .remove(id)
        .context("Reopen lost session")?;
    ensure!(
        process_manager::matches_creation_id(entry.pid, &entry.creation_id),
        "Reopen lost actual client"
    );
    let monitor_paths = paths.clone();
    let monitor_id = id.to_string();
    let monitor = tokio::spawn(async move {
        launch::monitor_restored_game(&monitor_paths, &monitor_id, Arc::new(|_, _, _| {})).await
    });
    tokio::time::sleep(Duration::from_secs(2)).await;
    ensure!(
        !monitor.is_finished(),
        "Live recovery exited before deliberate close"
    );
    tokio::time::timeout(
        Duration::from_secs(30),
        lifecycle::for_profile(paths).shutdown(paths),
    )
    .await??;
    ensure!(
        !tokio::time::timeout(Duration::from_secs(30), monitor).await???,
        "Live recovery unexpectedly finalized exit"
    );
    ensure!(
        process_manager::matches_creation_id(entry.pid, &entry.creation_id),
        "Second close killed actual client"
    );
    Ok(())
}

async fn recover(paths: &LauncherPaths, id: &str) -> anyhow::Result<()> {
    let start: Value = serde_json::from_slice(
        &tokio::fs::read(paths.base().join(format!("detached-start-{id}.json"))).await?,
    )?;
    let entry = running_pids::load_running_game_pids(paths)
        .await?
        .remove(id)
        .context("Reopen lost session")?;
    ensure!(
        entry.pid as u64 == start["pid"].as_u64().unwrap()
            && entry.creation_id == start["creation_id"].as_str().unwrap()
            && process_manager::matches_creation_id(entry.pid, &entry.creation_id),
        "Reopened a different game process"
    );
    let session = entry
        .launcher_session
        .as_ref()
        .context("Missing capture session")?;
    let directory = paths
        .instance_dir(id)?
        .join(".industrialis-capture")
        .join(&session.capture_id);
    let raw = tokio::fs::read(directory.join("stdout.log")).await?;
    let offset = start["stdout_offset"]
        .as_u64()
        .context("Missing checkpoint offset")? as usize;
    let nonce = start["nonce"].as_str().unwrap();
    let tick = format!("LAUNCHER_ACCEPTANCE_TICK_{nonce}");
    ensure!(
        raw.get(offset..)
            .is_some_and(|bytes| String::from_utf8_lossy(bytes).contains(&tick)),
        "No output captured while launcher was closed"
    );
    ensure!(
        launch::kill_instance(paths, id).await?,
        "Reopened Stop did not find the actual client"
    );
    // Observe the broker receipt before attaching the final dead-session monitor.
    // This avoids racing the monitor's normal capture-directory cleanup.
    let receipt = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(bytes) = tokio::fs::read(directory.join("exit.json")).await {
                return serde_json::from_slice::<Value>(&bytes);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    ensure!(
        tokio::time::timeout(
            Duration::from_secs(30),
            launch::monitor_restored_game(paths, id, Arc::new(|_, _, _| {}))
        )
        .await??,
        "Reopened monitor did not finalize exit"
    );
    ensure!(
        receipt["post_exit_error"].is_null(),
        "Post-exit command failed"
    );
    ensure!(
        !process_manager::matches_creation_id(entry.pid, &entry.creation_id),
        "Stopped client remains alive"
    );
    let instance = paths.instance_dir(id)?;
    let hooks = format!("{nonce}\n{id}\n");
    ensure!(
        tokio::fs::read_to_string(instance.join("acceptance-pre.txt")).await? == hooks,
        "Pre-launch environment/substitution mismatch"
    );
    ensure!(
        tokio::fs::read_to_string(instance.join("acceptance-post.txt")).await? == hooks,
        "Post-exit hook did not run exactly once"
    );
    ensure!(
        tokio::fs::read_to_string(instance.join("acceptance-env.txt"))
            .await?
            .trim()
            == nonce,
        "Wrapper environment lost"
    );
    let args = tokio::fs::read_to_string(instance.join("acceptance-args.txt")).await?;
    let args: Vec<_> = args.lines().collect();
    for expected in [
        "-Xms1024M",
        "-Xmx6144M",
        "-Dgpui.acceptance.first=true",
        "-Dgpui.acceptance.second=true",
    ] {
        ensure!(
            args.contains(&expected),
            "Actual JVM argument missing: {expected}"
        );
    }
    for pair in [["--width", "854"], ["--height", "480"]] {
        ensure!(
            args.windows(2).any(|window| window == pair),
            "Actual window arguments missing"
        );
    }
    let total = settings::load_instance_settings(paths, id)
        .await?
        .total_play_seconds;
    let expected = start["initial_play_seconds"].as_u64().unwrap()
        + receipt["ended_at_ms"]
            .as_u64()
            .unwrap()
            .saturating_sub(start["started_at_ms"].as_u64().unwrap())
            / 1000;
    ensure!(
        total == expected,
        "Detached/reopened play time was not credited exactly once"
    );
    let log = console_log::read_console_log(paths, id, true).await?;
    let logged_ticks: Vec<_> = log
        .iter()
        .filter(|line| line.line.starts_with(&tick))
        .map(|line| &line.line)
        .collect();
    let unique: std::collections::HashSet<_> = logged_ticks.iter().copied().collect();
    ensure!(
        !unique.is_empty() && unique.len() == logged_ticks.len(),
        "Captured lines were lost or replayed twice"
    );
    ensure!(
        String::from_utf8_lossy(&raw)
            .lines()
            .filter(|line| line.starts_with(&tick))
            .all(|line| unique.contains(&line.to_string())),
        "Some captured output was not restored into the console log"
    );
    let exits = log
        .iter()
        .filter(|line| line.line.starts_with("Process exited with code"))
        .count();
    ensure!(
        exits as u64 == start["initial_exit_lines"].as_u64().unwrap() + 1,
        "Actual game exit was not logged exactly once"
    );
    ensure!(
        !launch::monitor_restored_game(paths, id, Arc::new(|_, _, _| {})).await?,
        "Exit was finalized twice"
    );
    ensure!(
        settings::load_instance_settings(paths, id)
            .await?
            .total_play_seconds
            == total
            && console_log::read_console_log(paths, id, true)
                .await?
                .iter()
                .filter(|line| line.line.starts_with("Process exited with code"))
                .count()
                == exits,
        "Second recovery changed finalized time or exit logs"
    );
    let report = json!({"date":chrono::Utc::now().to_rfc3339(),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"instance":id,
        "forge_mod_loading_complete":true,"survived_launcher_exit":true,"same_process_recovered":true,
        "output_captured_while_closed":true,"log_replay_exactly_once":true,"reopened_stop":true,
        "pre_launch_hook":true,"post_exit_hook_exactly_once":true,"environment_and_wrapper":true,
        "multiline_jvm_ram_window_arguments":true,"play_time_exactly_once":true,"play_seconds_added":total-start["initial_play_seconds"].as_u64().unwrap(),
        "account":"expendable offline","desktop":"virtual; no physical input or world gameplay"});
    tokio::fs::write(
        paths
            .base()
            .join(format!("detached-game-acceptance-{id}.json")),
        serde_json::to_vec_pretty(&report)?,
    )
    .await?;
    Ok(())
}
