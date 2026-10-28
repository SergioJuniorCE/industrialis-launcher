//! Real offline client's auto-join handshake and window-close / injected abnormal exit.
//! The loopback probe disconnects before world/login success. Server-world mode
//! instead uses an isolated Forge server and waits for its unique in-world chat marker.
use anyhow::{ensure, Context};
use industrialis_backend::{
    accounts, console_log, launch, launch_config, paths::LauncherPaths, process_manager,
    profile_owner, settings,
};
use industrialis_core::launch_log::LogStream;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn main() -> anyhow::Result<()> {
    if industrialis_backend::game_broker::run_if_requested() {
        return Ok(());
    }
    ensure!(
        cfg!(target_os = "linux")
            && std::env::var("LAUNCHER_ISOLATED_VIRTUAL_DESKTOP").as_deref() == Ok("1"),
        "Requires the isolated Linux virtual desktop runner"
    );
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "Usage: live_connection_exit_acceptance <marked-profile> <instance> <java>"
    );
    let root = PathBuf::from(&args[0]);
    ensure!(
        root.is_absolute()
            && !std::fs::symlink_metadata(&root)?.file_type().is_symlink()
            && std::fs::read_to_string(root.join(".gpui-live-acceptance"))?
                == "industrialis-expendable-live-acceptance-v1",
        "Use a marked expendable profile"
    );
    let paths = LauncherPaths::new(root);
    let id = industrialis_backend::paths::validate_instance_id(&args[1])?;
    ensure!(
        !std::fs::symlink_metadata(paths.instance_dir(&id)?)?
            .file_type()
            .is_symlink(),
        "Instance must not be a symlink"
    );
    let _owner = match profile_owner::acquire(&paths, profile_owner::OwnerKind::Operation)? {
        profile_owner::Acquisition::Primary(owner) => owner,
        _ => anyhow::bail!("Acceptance profile is in use"),
    };
    tokio::runtime::Runtime::new()?.block_on(run(paths, id, args[2].clone()))
}

async fn run(paths: LauncherPaths, id: String, java: String) -> anyhow::Result<()> {
    let accounts = accounts::load_accounts(&paths).await?;
    ensure!(
        accounts.iter().all(|a| a.account_type == "offline"),
        "Must not use real credentials"
    );
    ensure!(
        launch::running_instances(&paths)
            .await?
            .iter()
            .all(|(_, _, alive)| !alive),
        "Game already running"
    );
    // A prior failed expendable run may have been removed with its container.
    // Reconcile that dead record before starting a new session, just as startup does.
    launch::monitor_restored_game(&paths, &id, Arc::new(|_, _, _| {})).await?;
    let account = accounts
        .iter()
        .find(|a| accounts::profile_identity(a).0 == "LauncherAcceptance")
        .context("Create the expendable offline acceptance account first")?;
    let server_world = std::env::var("LAUNCHER_GAME_EXIT_MODE").as_deref() == Ok("server-world");
    let ipv6_probe = std::env::var("LAUNCHER_AUTO_JOIN_IPV6").as_deref() == Ok("1");
    ensure!(
        !ipv6_probe || !server_world,
        "IPv6 mode is a loopback protocol probe, not a Forge world test"
    );
    let probe_host = if ipv6_probe { "::1" } else { "127.0.0.1" };
    let delayed_server =
        server_world && std::env::var("LAUNCHER_FORGE_LATENCY_CONTROL").as_deref() == Ok("1");
    let proxy_delay_ms = if delayed_server {
        std::env::var("LAUNCHER_FORGE_PROXY_DELAY_MS")
            .unwrap_or_else(|_| "200".into())
            .parse::<u64>()?
    } else {
        0
    };
    ensure!(
        proxy_delay_ms == 0 || proxy_delay_ms == 200,
        "Use the zero-delay baseline or 200 ms diagnostic control"
    );
    let server_host = if delayed_server {
        "launcher-acceptance-proxy"
    } else {
        "launcher-acceptance-server"
    };
    let world_marker = if server_world {
        let nonce =
            std::env::var("LAUNCHER_FORGE_WORLD_NONCE").context("Missing private-server nonce")?;
        ensure!(
            nonce.len() == 32 && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
            "Private-server nonce must be 32 hexadecimal characters"
        );
        Some(format!("LAUNCHER_WORLD_{nonce}"))
    } else {
        None
    };
    let listener = if server_world {
        None
    } else {
        Some(TcpListener::bind((probe_host, 0)).await?)
    };
    let port = match &listener {
        Some(listener) => listener.local_addr()?.port(),
        None => {
            if delayed_server {
                25571
            } else {
                25570
            }
        }
    };
    ensure!(port != 25565, "Probe must use a custom port");
    let mut config = settings::load_instance_settings(&paths, &id).await?;
    let before = config.total_play_seconds;
    config.override_account = true;
    config.account_id = Some(accounts::account_to_info(account).id);
    config.override_java_location = true;
    config.java_path = Some(java);
    config.override_memory = true;
    config.min_ram_mb = 1024;
    config.max_ram_mb = 6144;
    config.perm_gen_mb = 0;
    config.override_window = true;
    config.launch_maximized = false;
    config.window_width = 854;
    config.window_height = 480;
    config.override_java_args = true;
    config.jvm_args = "-Dgpui.acceptance.connection=true\n-Dgpui.acceptance.exit=true".into();
    config.override_env = false;
    config.override_commands = true;
    config.pre_launch_command.clear();
    config.wrapper_command.clear();
    let hook = format!("acceptance-exit-{}.txt", uuid::Uuid::new_v4().simple());
    config.post_exit_command = format!("printf 'post-exit\\n' >> {hook}");
    config.override_game_time = true;
    config.record_game_time = true;
    config.join_server_on_launch = true;
    config.join_server_address = if server_world {
        format!("{server_host}:{port}")
    } else if ipv6_probe {
        format!("[{probe_host}]:{port}")
    } else {
        format!("127.0.0.1:{port}")
    };
    settings::save_instance_settings(&paths, &id, &config).await?;
    let ready = Arc::new(AtomicBool::new(false));
    let connection_failed = Arc::new(AtomicBool::new(false));
    let pid = Arc::new(AtomicU32::new(0));
    let live_exits = Arc::new(AtomicU32::new(0));
    let world_ready = Arc::new(AtomicBool::new(false));
    let log_ready = ready.clone();
    let log_failed = connection_failed.clone();
    let log_exits = live_exits.clone();
    let log_world = world_ready.clone();
    let log_marker = world_marker.clone();
    let emit: launch_config::EmitLaunchLog = Arc::new(move |_, stream, line| {
        if stream == "system" && line.starts_with("Process exited with code") {
            log_exits.fetch_add(1, Ordering::SeqCst);
        }
        if line.contains("Forge Mod Loader has successfully loaded") {
            log_ready.store(true, Ordering::SeqCst);
        }
        if line.contains("Connecting to") || line.contains("Couldn't connect to server") {
            println!("[connection] {line}");
        }
        if line.contains("Couldn't connect to server")
            || (server_world && line.contains("NetworkDispatcher exception"))
        {
            log_failed.store(true, Ordering::SeqCst);
        }
        if log_marker
            .as_ref()
            .is_some_and(|marker| line.contains("[CHAT]") && line.contains(marker))
        {
            log_world.store(true, Ordering::SeqCst);
            println!("Private Forge server's in-world chat marker received");
        }
    });
    let started_pid = pid.clone();
    let mut task = tokio::spawn({
        let paths = paths.clone();
        let id = id.clone();
        async move {
            launch::launch_instance_with_started(
                &paths,
                &id,
                emit,
                Arc::new(move |p| {
                    started_pid.store(p, Ordering::SeqCst);
                }),
            )
            .await
        }
    });
    let start = std::time::Instant::now();
    let handshake = tokio::time::timeout(Duration::from_secs(900), async {
        tokio::select! {
            value = async {
                if let Some(listener) = listener {
                    probe(listener, probe_host, port).await
                } else {
                    while !world_ready.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Ok(serde_json::json!({"host":server_host,"port":port,"forge_world_chat":true,"latency_control_ms_per_direction":proxy_delay_ms}))
                }
            } => value,
            value = &mut task => { value??; anyhow::bail!("Client exited before auto-join"); },
            _ = async {
                while !connection_failed.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_millis(100)).await; }
            } => anyhow::bail!("Actual client reported a connection failure"),
            _ = async {
                while !ready.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_secs(1)).await; }
                println!("Forge ready; waiting for custom-port connection acceptance");
                tokio::time::sleep(Duration::from_secs(180)).await;
            } => anyhow::bail!("Actual client never reached the custom-port listener"),
        }
    }).await;
    let handshake = match handshake {
        Ok(Ok(value)) => value,
        other => {
            let _ = launch::kill_instance(&paths, &id).await;
            if !task.is_finished() {
                let _ = tokio::time::timeout(Duration::from_secs(30), &mut task).await;
            }
            return Err(anyhow::anyhow!("Auto-join failed: {other:?}"));
        }
    };
    ensure!(
        ready.load(Ordering::SeqCst),
        "Connection arrived before Forge completed"
    );
    let exit_delay = std::env::var("LAUNCHER_GAME_EXIT_DELAY_SECONDS")
        .ok()
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(3);
    ensure!(
        exit_delay <= 60,
        "Inspection delay is limited to 60 seconds"
    );
    println!(
        "Actual custom-port connection acceptance passed; triggering exit in {exit_delay} seconds"
    );
    tokio::time::sleep(Duration::from_secs(exit_delay)).await;
    let game_pid = pid.load(Ordering::SeqCst);
    let creation =
        process_manager::get_process_creation_id(game_pid).context("Missing live game identity")?;
    let abnormal = std::env::var("LAUNCHER_GAME_EXIT_MODE").as_deref() == Ok("abnormal");
    let exit_trigger = if abnormal {
        // Fault injection, not evidence of a naturally occurring Minecraft crash.
        ensure!(
            process_manager::matches_creation_id(game_pid, &creation),
            "Game PID changed"
        );
        std::process::Command::new("kill")
            .args(["-KILL", &game_pid.to_string()])
            .status()?
    } else {
        let helper =
            std::env::var("LAUNCHER_VIRTUAL_WINDOW_HELPER").context("Missing virtual-window helper")?;
        std::process::Command::new("python3")
            .arg(helper)
            .arg(game_pid.to_string())
            .status()?
    };
    if !exit_trigger.success() {
        let _ = launch::kill_instance(&paths, &id).await;
        let _ = tokio::time::timeout(Duration::from_secs(30), &mut task).await;
        anyhow::bail!("Exit trigger failed");
    }
    let result = tokio::time::timeout(Duration::from_secs(60), &mut task).await;
    let result = match result {
        Ok(value) => value?,
        Err(error) => {
            let _ = launch::kill_instance(&paths, &id).await;
            let _ = tokio::time::timeout(Duration::from_secs(30), &mut task).await;
            return Err(error.into());
        }
    };
    if abnormal {
        ensure!(
            result.is_err_and(|e| e.to_string().contains("game exited with code")),
            "Abnormal exit was not surfaced"
        );
    } else {
        result.context("Normal window close must exit successfully")?;
    }
    ensure!(
        launch::running_instances(&paths)
            .await?
            .iter()
            .all(|(_, _, alive)| !alive),
        "Exited game remains running"
    );
    let lines = console_log::read_console_log(&paths, &id, true).await?;
    // Persisted history compacts as the client logs. Older session exits can
    // disappear during this run, so a before/after history count is invalid.
    // Count this launch's emitted exits and its retained session separately.
    ensure!(
        live_exits.load(Ordering::SeqCst) == 1,
        "Current launch did not emit exactly one exit event"
    );
    let session_start = lines
        .iter()
        .rposition(|l| l.stream == LogStream::System && l.line == "──────── Launch ────────")
        .context("Current acceptance session exceeded the retained log window")?;
    let exits: Vec<_> = lines[session_start..]
        .iter()
        .filter(|l| l.stream == LogStream::System && l.line.starts_with("Process exited with code"))
        .collect();
    ensure!(
        exits.len() == 1,
        "Current launch's persisted exit event was lost or duplicated"
    );
    if !abnormal {
        ensure!(
            exits.last().unwrap().line == "Process exited with code 0",
            "Normal close returned a nonzero code"
        );
    }
    ensure!(
        tokio::fs::read_to_string(paths.instance_dir(&id)?.join(hook)).await? == "post-exit\n",
        "Post-exit hook did not run exactly once"
    );
    let after = settings::load_instance_settings(&paths, &id)
        .await?
        .total_play_seconds;
    ensure!(
        after > before && after - before <= start.elapsed().as_secs() + 2,
        "Play time was lost or overcredited"
    );
    ensure!(
        !launch::monitor_restored_game(&paths, &id, Arc::new(|_, _, _| {})).await?,
        "Finalized session recovered twice"
    );
    ensure!(
        settings::load_instance_settings(&paths, &id)
            .await?
            .total_play_seconds
            == after,
        "Repeat recovery changed time"
    );
    let mode = if abnormal {
        "injected-abnormal"
    } else {
        "normal-window-close"
    };
    let report = serde_json::json!({"platform":std::env::consts::OS,"instance":id,"forge_complete":true,
        "auto_join_custom_port":port,"handshake":handshake,"exit_mode":mode,"exit_log":exits.last().unwrap().line,
        "post_exit_exactly_once":true,"play_seconds_added":after-before,"repeat_recovery_unchanged":true,
        "elapsed_seconds":start.elapsed().as_secs(),"desktop":"isolated Xvfb","account":"expendable offline",
        "limits": if server_world { "Private offline Forge world entry verified by an in-world server chat marker; no physical controls or online authentication" }
            else { "Loopback login probe disconnects before authentication/world success; abnormal mode injects SIGKILL" }});
    let prefix = if server_world {
        "world-connection-exit"
    } else if ipv6_probe {
        "ipv6-connection-exit"
    } else {
        "connection-exit"
    };
    tokio::fs::write(
        paths.base().join(format!("{prefix}-{id}-{mode}.json")),
        serde_json::to_vec_pretty(&report)?,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn varint(stream: &mut TcpStream) -> anyhow::Result<usize> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let b = stream.read_u8().await?;
        value |= ((b & 127) as usize) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("Oversized VarInt")
}
fn take_varint(bytes: &mut &[u8]) -> anyhow::Result<usize> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let (&b, rest) = bytes.split_first().context("Truncated VarInt")?;
        *bytes = rest;
        value |= ((b & 127) as usize) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("Oversized VarInt")
}
fn take_string(bytes: &mut &[u8]) -> anyhow::Result<String> {
    let n = take_varint(bytes)?;
    ensure!(n <= bytes.len(), "Truncated string");
    let text = std::str::from_utf8(&bytes[..n])?.to_owned();
    *bytes = &bytes[n..];
    Ok(text)
}
async fn packet(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    let n = varint(stream).await?;
    ensure!(n <= 4096, "Oversized packet");
    let mut bytes = vec![0; n];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

fn put_varint(mut n: usize, bytes: &mut Vec<u8>) {
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        bytes.push(if n == 0 { b } else { b | 128 });
        if n == 0 {
            break;
        }
    }
}

async fn send_packet(stream: &mut TcpStream, payload: &[u8]) -> anyhow::Result<()> {
    let mut framed = Vec::new();
    put_varint(payload.len(), &mut framed);
    framed.extend_from_slice(payload);
    stream.write_all(&framed).await?;
    Ok(())
}

async fn probe(
    listener: TcpListener,
    expected_host: &str,
    port: u16,
) -> anyhow::Result<serde_json::Value> {
    for status_probes in 0..16 {
        let (mut stream, _) = listener.accept().await?;
        // The client can open TCP while texture work delays its Netty handshake.
        // The outer post-Forge deadline still bounds total connection acceptance.
        let result = tokio::time::timeout(Duration::from_secs(180), async {
        let bytes = packet(&mut stream).await?; let mut bytes = bytes.as_slice();
        ensure!(take_varint(&mut bytes)? == 0, "Not a handshake");
        let protocol = take_varint(&mut bytes)?;
        let host = take_string(&mut bytes)?;
        ensure!(bytes.len() >= 2, "Missing port");
        let received_port = u16::from_be_bytes([bytes[0], bytes[1]]); bytes = &bytes[2..];
        let state = take_varint(&mut bytes)?;
        ensure!(protocol == 5 && host.strip_suffix("\0FML\0").unwrap_or(&host) == expected_host && received_port == port && (state == 1 || state == 2) && bytes.is_empty(),
            "Incorrect Minecraft 1.7.10 login handshake: protocol={protocol}, host={host}, port={received_port}, state={state}");
        if state == 1 {
            ensure!(packet(&mut stream).await? == [0], "Invalid status request");
            let status = serde_json::to_vec(&serde_json::json!({
                "version":{"name":"1.7.10","protocol":5},"players":{"max":1,"online":0,"sample":[]},
                "description":{"text":"Launcher isolated connection probe"},"modinfo":{"type":"FML","modList":[]}
            }))?;
            let mut payload = vec![0];
            put_varint(status.len(), &mut payload); payload.extend_from_slice(&status);
            send_packet(&mut stream, &payload).await?;
            let ping = packet(&mut stream).await?;
            ensure!(ping.len() == 9 && ping[0] == 1, "Invalid status ping");
            send_packet(&mut stream, &ping).await?;
            stream.shutdown().await?;
            println!("Actual client custom-port status query and ping passed");
            return Ok(None);
        }
        let login = packet(&mut stream).await?; let mut login = login.as_slice();
        ensure!(take_varint(&mut login)? == 0 && take_string(&mut login)? == "LauncherAcceptance" && login.is_empty(), "Unexpected login start");
        let message = br#"{"text":"Launcher isolated connection probe complete"}"#;
        let mut disconnect = vec![0]; put_varint(message.len(), &mut disconnect);
        disconnect.extend_from_slice(message); send_packet(&mut stream, &disconnect).await?; stream.shutdown().await?;
        Ok(Some(serde_json::json!({"protocol":protocol,"host":host,"port":received_port,"login_start":true,"status_probes":status_probes})))
    }).await??;
        if let Some(report) = result {
            return Ok(report);
        }
    }
    anyhow::bail!("Too many status probes without a login handshake")
}
