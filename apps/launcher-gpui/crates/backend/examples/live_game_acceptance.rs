//! Opt-in actual client startup/stop in an expendable profile and virtual desktop.
use anyhow::ensure;
use industrialis_backend::{
    accounts, launch, launch_config, paths::LauncherPaths, profile_owner, settings,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() -> anyhow::Result<()> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "Usage: live_game_acceptance <expendable-profile> <instance-id> <java-path>"
    );
    let profile = PathBuf::from(&args[0]);
    ensure!(
        profile.is_absolute()
            && std::fs::read_to_string(profile.join(".gpui-live-acceptance"))
                .ok()
                .as_deref()
                == Some("industrialis-gpui-expendable-live-acceptance-v1"),
        "Use a marked expendable acceptance profile"
    );
    let paths = LauncherPaths::new(profile);
    let _owner = match profile_owner::acquire(&paths, profile_owner::OwnerKind::Operation)? {
        profile_owner::Acquisition::Primary(owner) => owner,
        _ => anyhow::bail!("Acceptance profile is already in use"),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run(paths, args[1].clone(), args[2].clone()))
}

async fn run(paths: LauncherPaths, id: String, java: String) -> anyhow::Result<()> {
    ensure!(
        accounts::load_accounts(&paths)
            .await?
            .iter()
            .all(|account| account.account_type == "offline"),
        "Live acceptance must not use real account credentials"
    );
    ensure!(
        !launch::running_instances(&paths)
            .await?
            .iter()
            .any(|(running, _, alive)| running == &id && *alive),
        "Acceptance game is already running"
    );
    let existing = accounts::load_accounts(&paths).await?;
    let account_id = match existing
        .iter()
        .find(|account| accounts::profile_identity(account).0 == "GpuiAcceptance")
    {
        Some(account) => accounts::account_to_info(account).id,
        None => {
            accounts::create_offline_account(&paths, "GpuiAcceptance")
                .await?
                .id
        }
    };
    let mut config = settings::load_instance_settings(&paths, &id).await?;
    config.override_account = true;
    config.account_id = Some(account_id);
    config.override_java_location = true;
    config.java_path = Some(java);
    config.override_memory = true;
    config.min_ram_mb = 1024;
    config.max_ram_mb = 6144;
    config.perm_gen_mb = 0;
    config.override_window = true;
    config.window_width = 854;
    config.window_height = 480;
    config.override_java_args = true;
    config.jvm_args = "-Dgpui.acceptance.first=true\n-Dgpui.acceptance.second=true".into();
    settings::save_instance_settings(&paths, &id, &config).await?;
    let ready = Arc::new(AtomicBool::new(false));
    let pid = Arc::new(AtomicU32::new(0));
    let ready_log = ready.clone();
    let emit: launch_config::EmitLaunchLog = Arc::new(move |_, stream, line| {
        // This message is emitted after Forge finishes loading the actual client mods.
        if line.contains("Forge Mod Loader has successfully loaded") {
            ready_log.store(true, Ordering::SeqCst);
        }
        println!("[{stream}] {line}");
    });
    let started = pid.clone();
    let started_callback = Arc::new(move |value| {
        started.store(value, Ordering::SeqCst);
    });
    let game_paths = paths.clone();
    let game_id = id.clone();
    let mut task = tokio::spawn(async move {
        launch::launch_instance_with_started(&game_paths, &game_id, emit, started_callback).await
    });
    let start = std::time::Instant::now();
    let limit = std::env::var("GPUI_GAME_ACCEPTANCE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(900);
    let reached;
    loop {
        tokio::select! {
            result = &mut task => { result??; anyhow::bail!("Client exited before the deliberate stop"); }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if ready.load(Ordering::SeqCst) { reached = true; break; }
                if start.elapsed().as_secs() >= limit { reached = false; break; }
            }
        }
    }
    if reached {
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    ensure!(
        pid.load(Ordering::SeqCst) != 0,
        "Game process did not start before the timeout"
    );
    launch::kill_instance(&paths, &id).await?;
    let stopped = tokio::time::timeout(Duration::from_secs(30), &mut task).await??;
    // Forced Stop deliberately yields a non-zero child status; other errors fail acceptance.
    if let Err(error) = stopped {
        ensure!(
            error.to_string().contains("game exited with code"),
            "Stop failed: {error:#}"
        );
    }
    let live = launch::running_instances(&paths).await?;
    ensure!(
        !live
            .iter()
            .any(|(running, _, alive)| running == &id && *alive),
        "Stopped game remains live"
    );
    let args = tokio::fs::read_to_string(paths.instance_dir(&id)?.join("launch.arg")).await?;
    ensure!(
        args.contains("-Dgpui.acceptance.first=true")
            && args.contains("-Dgpui.acceptance.second=true"),
        "Multiline arguments were lost"
    );
    let report = serde_json::json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"instance":id,
        "pid":pid.load(Ordering::SeqCst),"forge_mod_loading_complete":reached,"deliberate_stop":true,
        "elapsed_seconds":start.elapsed().as_secs(),"play_seconds":settings::load_instance_settings(&paths,&id).await?.total_play_seconds,
        "account":"expendable offline","desktop":"virtual desktop; no physical acceptance"});
    tokio::fs::write(
        paths.base().join(format!("game-acceptance-{id}.json")),
        serde_json::to_vec_pretty(&report)?,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        reached,
        "Timed out before Forge completed actual client mod loading"
    );
    Ok(())
}
