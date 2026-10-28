//! Port of `electron/backend/game-launch.ts`: instance launch orchestration.
//!
//! Every `ctx.emitLog` becomes both a live event (the caller's emitter) and a
//! persisted console.log line (via [`ConsoleLogWriter`]), matching the TS
//! backend's combined behavior.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use industrialis_core::instance_settings::InstanceSettings;
use industrialis_core::launcher_settings::resolve_default_account_id;

use crate::accounts::{
    choose_launch_account, ensure_fresh_token, load_accounts, profile_identity, AccountData,
};
use crate::console_log::{register_queue, ConsoleLogWriter, LogWrite};
use crate::instances::load_instances;
use crate::java::java_gui_executable;
use crate::launch_config::{
    build_classpath, build_launch_config, expand_minecraft_arguments, instance_command_vars,
    run_shell_command, split_command_args, substitute_command_vars, sync_assets,
    write_launch_argfile, EmitLaunchLog,
};
use crate::pack_prep::{
    apply_persistent_custom_mods, apply_persistent_minecraft, flatten_nested_pack,
    prepare_instance_configs,
};
use crate::paths::{validate_instance_id, LauncherPaths};
use crate::process_manager::{self, is_process_alive, LogEmitter};
use crate::running_pids::{load_running_game_pids, GameSession, RunningGamePid};
use crate::settings::{load_instance_settings, load_launcher_settings};

/// One ordered writer worker per launch; flush is a barrier in that queue.
struct LogSink {
    id: String,
    emit: EmitLaunchLog,
    queue: Arc<Mutex<tokio::sync::mpsc::UnboundedSender<LogWrite>>>,
}
impl LogSink {
    fn new(paths: &LauncherPaths, id: &str, emit: EmitLaunchLog) -> anyhow::Result<Arc<Self>> {
        let path = paths.console_log_path(id)?;
        let target_path = path.clone();
        let writer = ConsoleLogWriter::new(move |_| target_path.clone());
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let target = id.to_string();
        tokio::spawn(async move {
            let mut error: Option<String> = None;
            while let Some(event) = receiver.recv().await {
                match event {
                    LogWrite::Line(stream, line) => {
                        if let Err(e) = writer.append(&target, &stream, &line).await {
                            error = Some(format!("{e:#}"));
                        }
                    }
                    LogWrite::Flush(sender) => {
                        let _ = sender.send(error.clone().map_or(Ok(()), Err));
                    }
                }
            }
        });
        let queue = Arc::new(Mutex::new(sender));
        register_queue(&path, &queue);
        Ok(Arc::new(Self {
            id: id.into(),
            emit,
            queue,
        }))
    }
    fn log(&self, stream: &str, line: &str) {
        // A mutex orders both live fan-out and queued persistence across readers.
        let queue = self.queue.lock().unwrap();
        (self.emit)(self.id.clone(), stream.to_string(), line.to_string());
        let _ = queue.send(LogWrite::Line(stream.into(), line.into()));
    }
    fn process_emit(self: &Arc<Self>) -> LogEmitter {
        let sink = Arc::clone(self);
        Arc::new(move |stream: &str, line: &str| sink.log(stream, line))
    }
    async fn finish(&self) -> anyhow::Result<()> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.queue.lock().unwrap().send(LogWrite::Flush(sender))?;
        receiver.await?.map_err(anyhow::Error::msg)
    }
}

pub struct LaunchAuth {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
}

async fn launch_auth(
    paths: &LauncherPaths,
    account: &AccountData,
) -> Result<LaunchAuth, anyhow::Error> {
    if account.account_type == "offline" {
        let (username, uuid) = profile_identity(account);
        if username.is_empty() || uuid.is_empty() {
            anyhow::bail!("Offline account is missing a username.");
        }
        return Ok(LaunchAuth {
            username,
            uuid,
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
        });
    }
    if account
        .minecraft_entitlement
        .as_ref()
        .map(|e| !e.can_play_minecraft)
        .unwrap_or(false)
    {
        anyhow::bail!("This Microsoft account does not own Minecraft Java Edition.");
    }
    let token = ensure_fresh_token(paths, account).await?;
    let (username, uuid) = profile_identity(account);
    if username.is_empty() || uuid.is_empty() {
        anyhow::bail!("This Microsoft account has no Minecraft profile yet. Set a username in the official launcher first.");
    }
    Ok(LaunchAuth {
        username,
        uuid,
        access_token: token,
        user_type: "msa".to_string(),
    })
}

fn resolve_java(
    settings: &InstanceSettings,
    default_path: Option<&str>,
) -> Result<PathBuf, anyhow::Error> {
    if settings.override_java_location {
        if let Some(candidate) = settings
            .java_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let path = PathBuf::from(candidate);
            if !path.exists() {
                anyhow::bail!("configured Java not found: {candidate}");
            }
            return Ok(path);
        }
    }
    if let Some(candidate) = default_path.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(candidate);
        if !path.exists() {
            anyhow::bail!("default Java not found: {candidate}");
        }
        return Ok(path);
    }
    crate::java::java_path().ok_or_else(|| {
        anyhow::anyhow!("no Java configured or found — choose a default Java in launcher settings, set JAVA_HOME, or pick a Java in instance settings")
    })
}

fn resolve_launch_command(
    settings: &InstanceSettings,
    java: &str,
    args: Vec<String>,
    vars: &HashMap<String, String>,
) -> Result<(PathBuf, Vec<String>), anyhow::Error> {
    if !settings.override_commands || settings.wrapper_command.trim().is_empty() {
        return Ok((PathBuf::from(java), args));
    }
    let wrapper = substitute_command_vars(settings.wrapper_command.trim(), vars);
    let mut parts = split_command_args(&wrapper);
    if parts.is_empty() {
        anyhow::bail!("wrapper command is empty");
    }
    let executable = PathBuf::from(parts.remove(0));
    let mut full = parts;
    full.push(java.to_string());
    full.extend(args);
    Ok((executable, full))
}

fn append_server_arguments(
    settings: &InstanceSettings,
    args: &mut Vec<String>,
) -> anyhow::Result<()> {
    let address = settings.join_server_address.trim();
    if !settings.join_server_on_launch || address.is_empty() {
        return Ok(());
    }
    // Minecraft 1.7.10 takes the hostname and port separately. Passing host:port
    // as --server makes GuiConnecting resolve that entire string as a hostname.
    let (host, port) = if let Some(bracketed) = address.strip_prefix('[') {
        let (host, suffix) = bracketed.split_once(']').ok_or_else(|| {
            anyhow::anyhow!("Auto-join IPv6 address is missing its closing bracket")
        })?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| anyhow::anyhow!("Auto-join address contains an invalid IPv6 address"))?;
        let port =
            if suffix.is_empty() {
                None
            } else {
                Some(suffix.strip_prefix(':').ok_or_else(|| {
                    anyhow::anyhow!("Auto-join IPv6 address must use [address]:port")
                })?)
            };
        (host, port)
    } else if address.matches(':').count() > 1 {
        address
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| anyhow::anyhow!("Auto-join IPv6 address must use [address]:port"))?;
        (address, None)
    } else {
        address
            .rsplit_once(':')
            .map_or((address, None), |(host, port)| (host, Some(port)))
    };
    anyhow::ensure!(
        !host.is_empty()
            && !host.chars().any(|c| c.is_whitespace()
                || c.is_control()
                || matches!(c, '/' | '\\' | '[' | ']' | '?' | '#')),
        "Auto-join address must contain a hostname or IP address"
    );
    let port = port
        .map(|port| {
            port.parse::<u16>()
                .ok()
                .filter(|n| *n != 0 && port.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(|| {
                    anyhow::anyhow!("Auto-join server port must be a number from 1 to 65535")
                })
        })
        .transpose()?;
    // Forge rebuilds host:port and feeds it through Minecraft's ServerAddress
    // parser. Keep IPv6 bracketed there; otherwise the port becomes part of the
    // address and the parser falls back to 25565. Java also accepts brackets in
    // the direct-connection fallback's InetAddress.getByName call.
    let cli_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    args.extend(["--server".to_string(), cli_host]);
    if let Some(port) = port {
        args.extend(["--port".to_string(), port.to_string()]);
    }
    Ok(())
}

/// Launch an instance and wait for exit. Mirrors `launchInstance`.
pub async fn launch_instance(
    paths: &LauncherPaths,
    raw_id: &str,
    emit: EmitLaunchLog,
) -> Result<(), anyhow::Error> {
    launch_instance_with_started(paths, raw_id, emit, Arc::new(|_| {})).await
}

/// The native UI receives process startup separately from unstructured logs.
pub async fn launch_instance_with_started(
    paths: &LauncherPaths,
    raw_id: &str,
    emit: EmitLaunchLog,
    on_started: Arc<dyn Fn(u32) + Send + Sync>,
) -> Result<(), anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let _lease = crate::instance_guard::reserve_instances(paths, &[&id])?;
    if is_already_running(paths, &id).await? {
        anyhow::bail!("Instance is already running");
    }
    let instance = paths.instance_dir(&id)?;
    if !crate::fs_utils::exists(&instance).await {
        anyhow::bail!("instance not installed");
    }
    flatten_nested_pack(&instance).await?;
    prepare_instance_configs(&instance, false).await?;
    apply_persistent_custom_mods(&instance).await?;
    apply_persistent_minecraft(&instance).await?;

    let sink = LogSink::new(paths, &id, emit)?;
    let result = launch_prepared(paths, id.clone(), instance, &sink, on_started).await;
    if let Err(error) = &result {
        sink.log("system", &format!("Launch failed: {error:#}"));
    }
    let flushed = sink.finish().await;
    result?;
    flushed
}

async fn launch_prepared(
    paths: &LauncherPaths,
    id: String,
    instance: PathBuf,
    sink: &Arc<LogSink>,
    on_started: Arc<dyn Fn(u32) + Send + Sync>,
) -> anyhow::Result<()> {
    let log = |stream: &str, line: &str| sink.log(stream, line);

    let (settings, launcher_settings) = tokio::try_join!(
        load_instance_settings(paths, &id),
        load_launcher_settings(paths)
    )?;
    let java = resolve_java(&settings, launcher_settings.default_java_path.as_deref())?;
    let java_str = java.to_string_lossy().into_owned();
    let config = build_launch_config(&instance, &id, &sink.emit).await?;
    sync_assets(&config, &id, &sink.emit).await?;
    let classpath = build_classpath(&config.libraries);
    let vars = instance_command_vars(&id, &settings.name, &instance, &java_str);

    if settings.override_commands && !settings.pre_launch_command.trim().is_empty() {
        let env = if settings.override_env {
            settings.env_vars.clone()
        } else {
            HashMap::new()
        };
        run_shell_command(
            &substitute_command_vars(settings.pre_launch_command.trim(), &vars),
            &instance,
            &env,
        )
        .await?;
        log("system", "Pre-launch command finished");
    }

    let mut args = vec![
        format!(
            "-Xms{}M",
            if settings.override_memory {
                settings.min_ram_mb
            } else {
                4096
            }
        ),
        format!(
            "-Xmx{}M",
            if settings.override_memory {
                settings.max_ram_mb
            } else {
                6144
            }
        ),
    ];
    if settings.override_memory && settings.perm_gen_mb > 0 {
        args.push(format!("-XX:PermSize={}M", settings.perm_gen_mb));
        args.push(format!("-XX:MaxPermSize={}M", settings.perm_gen_mb));
    }
    if settings.override_java_args && !settings.jvm_args.trim().is_empty() {
        args.extend(split_command_args(&settings.jvm_args));
    }
    args.extend(["-cp".to_string(), classpath.clone()]);
    args.extend(config.jvm_args.clone());
    args.push(config.main_class.clone());
    args.extend(config.program_args.clone());
    if settings.override_window && !settings.launch_maximized {
        args.extend([
            "--width".to_string(),
            settings.window_width.to_string(),
            "--height".to_string(),
            settings.window_height.to_string(),
        ]);
    }
    append_server_arguments(&settings, &mut args)?;

    let accounts = load_accounts(paths).await?;
    let account = choose_launch_account(
        &settings,
        &accounts,
        resolve_default_account_id(&launcher_settings),
    )?;
    let auth = launch_auth(paths, &account).await?;
    if let Some(template) = config.minecraft_arguments_template.as_deref() {
        args.extend(expand_minecraft_arguments(
            template,
            &HashMap::from([
                ("auth_player_name".to_string(), auth.username.clone()),
                ("version_name".to_string(), config.minecraft_version.clone()),
                ("game_directory".to_string(), config.game_dir.clone()),
                ("assets_root".to_string(), config.assets_dir.clone()),
                (
                    "assets_index_name".to_string(),
                    config
                        .asset_index
                        .as_ref()
                        .map(|a| a.id.clone())
                        .unwrap_or_else(|| config.minecraft_version.clone()),
                ),
                ("auth_uuid".to_string(), auth.uuid.clone()),
                ("auth_access_token".to_string(), auth.access_token.clone()),
                ("user_properties".to_string(), "{}".to_string()),
                ("user_type".to_string(), auth.user_type.clone()),
            ]),
        ));
    } else {
        args.extend([
            "--username".to_string(),
            auth.username.clone(),
            "--version".to_string(),
            config.minecraft_version.clone(),
            "--gameDir".to_string(),
            config.game_dir.clone(),
            "--assetsDir".to_string(),
            config.assets_dir.clone(),
            "--accessToken".to_string(),
            auth.access_token.clone(),
            "--uuid".to_string(),
            auth.uuid.clone(),
            "--userType".to_string(),
            auth.user_type.clone(),
        ]);
    }

    log("system", "──────── Launch ────────");
    log("system", &format!("Java: {java_str}"));
    log("system", &format!("Main class: {}", config.main_class));
    log(
        "system",
        &format!(
            "Classpath: {} libraries ({} chars)",
            config.libraries.len(),
            classpath.len()
        ),
    );
    let argfile = instance.join("launch.arg");
    write_launch_argfile(&argfile, &args).await?;
    log(
        "system",
        &format!("Launch args saved to {}", argfile.display()),
    );

    let launch_java = java_gui_executable(&java);
    let (executable, final_args) =
        resolve_launch_command(&settings, &launch_java.to_string_lossy(), args, &vars)?;
    tokio::fs::create_dir_all(&config.game_dir).await?;
    let env = if settings.override_env {
        settings.env_vars.clone()
    } else {
        HashMap::new()
    };
    let mut session = GameSession {
        capture_id: uuid::Uuid::new_v4().to_string(),
        started_at_ms: now_ms(),
        record_time: !settings.override_game_time || settings.record_game_time,
        stdout_offset: 0,
        stderr_offset: 0,
        extra_fields: Default::default(),
    };
    let directory = crate::game_capture::session_directory(paths, &id, &session.capture_id)?;
    let mut running = process_manager::spawn_brokered_game(
        &executable,
        &final_args,
        std::path::Path::new(&config.game_dir),
        &env,
        sink.process_emit(),
        &directory,
        (settings.override_commands && !settings.post_exit_command.trim().is_empty()).then(|| {
            (
                substitute_command_vars(settings.post_exit_command.trim(), &vars),
                instance.clone(),
            )
        }),
    )
    .await?;
    if let Err(error) = remember_running(paths, &id, &running, session.clone()).await {
        let _ = process_manager::kill_game_process(&mut running).await;
        return Err(error);
    }
    on_started(running.pid);
    let lifecycle = crate::lifecycle::for_profile(paths);
    let (exit_code, offsets) = process_manager::wait_or_detach(&mut running, &lifecycle).await?;
    session.stdout_offset = offsets[0];
    session.stderr_offset = offsets[1];
    sink.finish().await?;
    let receipt = crate::game_broker::exit(
        &directory,
        running.pid,
        running.creation_id.as_deref().unwrap_or_default(),
    )
    .await?;
    crate::settings::record_game_time(
        paths,
        &id,
        &session,
        receipt.as_ref().map_or_else(now_ms, |r| r.ended_at_ms),
    )
    .await?;
    update_session(paths, &id, running.pid, session).await?;
    if exit_code.is_none() {
        return Ok(());
    }
    let exit_code = exit_code.unwrap();
    forget_running(paths, &id, running.pid).await?;
    log("system", &format!("Process exited with code {exit_code}"));

    crate::fs_utils::remove_if_exists(&directory).await?;
    // Measure only after the game and its post-exit hook have finished, before
    // the UI reloads the instance. A cache failure must not change its exit code.
    if let Err(error) = crate::install::refresh_size_cache(paths, &id).await {
        log(
            "system",
            &format!("Could not refresh instance disk usage: {error:#}"),
        );
    }
    if let Some(error) = receipt.and_then(|r| r.post_exit_error) {
        anyhow::bail!("{error}");
    }
    if exit_code != 0 {
        anyhow::bail!("game exited with code {exit_code}");
    }
    Ok(())
}

pub(crate) async fn is_already_running(
    paths: &LauncherPaths,
    id: &str,
) -> Result<bool, anyhow::Error> {
    let pids = load_running_game_pids(paths).await?;
    let Some(entry) = pids.get(id) else {
        return Ok(false);
    };
    if !is_process_alive(entry.pid) {
        return Ok(false);
    }
    Ok(process_manager::matches_creation_id(
        entry.pid,
        &entry.creation_id,
    ))
}

async fn remember_running(
    paths: &LauncherPaths,
    id: &str,
    running: &process_manager::RunningProcess,
    session: GameSession,
) -> Result<(), anyhow::Error> {
    let id = id.to_string();
    let entry = RunningGamePid {
        pid: running.pid,
        creation_id: running.creation_id.clone().unwrap_or_default(),
        launcher_session: Some(session),
        extra_fields: Default::default(),
    };
    // TS only persists entries with a creation id; without one the record
    // cannot be verified after restart.
    if entry.creation_id.is_empty() {
        anyhow::bail!("Could not identify the game process safely");
    }
    crate::running_pids::update(paths, move |pids| {
        pids.insert(id, entry);
    })
    .await
}

async fn forget_running(paths: &LauncherPaths, id: &str, pid: u32) -> Result<(), anyhow::Error> {
    let id = id.to_string();
    crate::running_pids::update(paths, move |pids| {
        if pids.get(&id).is_some_and(|p| p.pid == pid) {
            pids.remove(&id);
        }
    })
    .await
}

async fn update_session(
    paths: &LauncherPaths,
    id: &str,
    pid: u32,
    session: GameSession,
) -> anyhow::Result<()> {
    let id = id.to_string();
    crate::running_pids::update(paths, move |pids| {
        if let Some(entry) = pids.get_mut(&id).filter(|p| p.pid == pid) {
            entry.launcher_session = Some(session);
        }
    })
    .await
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Resume only the exact persisted process. Dead native sessions are recovered
/// too, including output produced while the launcher was closed.
pub async fn monitor_restored_game(
    paths: &LauncherPaths,
    id: &str,
    emit: EmitLaunchLog,
) -> anyhow::Result<bool> {
    let id = validate_instance_id(id)?;
    let _lease = crate::lifecycle::admit(paths)?;
    recover_game(paths, &id, emit).await
}

async fn recover_game(
    paths: &LauncherPaths,
    id: &str,
    emit: EmitLaunchLog,
) -> anyhow::Result<bool> {
    let id = validate_instance_id(id)?;
    let Some(entry) = load_running_game_pids(paths).await?.remove(&id) else {
        return Ok(false);
    };
    let mut process =
        process_manager::RunningProcess::detached(entry.pid, Some(entry.creation_id.clone()));
    let sink = LogSink::new(paths, &id, emit)?;
    let directory = entry
        .launcher_session
        .as_ref()
        .map(|s| crate::game_capture::session_directory(paths, &id, &s.capture_id))
        .transpose()?;
    if let (Some(directory), Some(session)) = (&directory, &entry.launcher_session) {
        process_manager::attach_capture(
            &mut process,
            directory,
            [session.stdout_offset, session.stderr_offset],
            sink.process_emit(),
        )
        .await?;
    }
    let (exit_code, offsets) =
        process_manager::wait_or_detach(&mut process, &crate::lifecycle::for_profile(paths))
            .await?;
    sink.finish().await?;
    if let Some(mut session) = entry.launcher_session {
        session.stdout_offset = offsets[0];
        session.stderr_offset = offsets[1];
        let receipt =
            crate::game_broker::exit(directory.as_ref().unwrap(), entry.pid, &entry.creation_id)
                .await?;
        // A missing exit receipt after a crash cannot tell us when a dead game
        // stopped. Do not credit the entire time since its last launch.
        if let Some(receipt) = &receipt {
            crate::settings::record_game_time(paths, &id, &session, receipt.ended_at_ms).await?;
            if let Some(error) = &receipt.post_exit_error {
                sink.log("system", error);
            }
        } else if exit_code.is_none() {
            crate::settings::record_game_time(paths, &id, &session, now_ms()).await?;
        }
        update_session(paths, &id, entry.pid, session).await?;
        if exit_code.is_none() {
            return Ok(false);
        }
        sink.log(
            "system",
            &receipt.map_or_else(
                || "Game stopped; exit code unavailable".into(),
                |r| format!("Process exited with code {}", r.exit_code),
            ),
        );
    } else if exit_code.is_none() {
        return Ok(false);
    } else {
        sink.log(
            "system",
            "Previously running game stopped; exit code unavailable",
        );
    }
    sink.finish().await?;
    forget_running(paths, &id, entry.pid).await?;
    if let Some(directory) = directory {
        crate::fs_utils::remove_if_exists(&directory).await?;
    }
    if let Err(error) = crate::install::refresh_size_cache(paths, &id).await {
        sink.log(
            "system",
            &format!("Could not refresh instance disk usage: {error:#}"),
        );
    }
    Ok(true)
}

pub(crate) async fn flush_capture_sessions(paths: &LauncherPaths) -> anyhow::Result<()> {
    for id in load_running_game_pids(paths).await?.keys() {
        recover_game(paths, id, Arc::new(|_, _, _| {})).await?;
    }
    Ok(())
}

/// Kill a running instance. Mirrors `killInstance`.
pub async fn kill_instance(paths: &LauncherPaths, raw_id: &str) -> Result<bool, anyhow::Error> {
    let id = validate_instance_id(raw_id)?;
    let pids = load_running_game_pids(paths).await?;
    let Some(entry) = pids.get(&id) else {
        return Ok(false);
    };
    let mut running =
        process_manager::RunningProcess::detached(entry.pid, Some(entry.creation_id.clone()));
    process_manager::kill_game_process(&mut running).await?;
    // Owned/restored monitors remove the record after draining logs and time.
    // With no monitor (CLI stop), retain a dead record for startup recovery.
    Ok(true)
}

/// Live running-instance snapshot for `ps`-style listings.
pub async fn running_instances(
    paths: &LauncherPaths,
) -> Result<Vec<(String, u32, bool)>, anyhow::Error> {
    let known: std::collections::HashSet<String> = load_instances(paths)
        .await?
        .into_iter()
        .map(|i| i.id)
        .collect();
    let pids = load_running_game_pids(paths).await?;
    let mut out = Vec::new();
    for (id, entry) in &pids {
        let alive = is_process_alive(entry.pid)
            && process_manager::matches_creation_id(entry.pid, &entry.creation_id);
        out.push((id.clone(), entry.pid, alive && known.contains(id)));
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod server_argument_tests {
    use super::*;

    #[test]
    fn auto_join_custom_port_matches_minecraft_cli() {
        let settings = InstanceSettings {
            join_server_on_launch: true,
            join_server_address: " 127.0.0.1:25570 ".into(),
            ..Default::default()
        };
        let mut args = vec!["net.minecraft.launchwrapper.Launch".into()];
        append_server_arguments(&settings, &mut args).unwrap();
        assert_eq!(
            args,
            [
                "net.minecraft.launchwrapper.Launch",
                "--server",
                "127.0.0.1",
                "--port",
                "25570"
            ]
        );
    }

    #[test]
    fn auto_join_accepts_default_port_and_ipv6_addresses() {
        for (address, expected) in [
            (" play.example.com ", vec!["--server", "play.example.com"]),
            (
                "play.example.com:25566",
                vec!["--server", "play.example.com", "--port", "25566"],
            ),
            ("[::1]:25567", vec!["--server", "[::1]", "--port", "25567"]),
            ("[2001:db8::1]", vec!["--server", "[2001:db8::1]"]),
            ("2001:db8::1", vec!["--server", "[2001:db8::1]"]),
            (
                "[::ffff:127.0.0.1]:25570",
                vec!["--server", "[::ffff:127.0.0.1]", "--port", "25570"],
            ),
        ] {
            let settings = InstanceSettings {
                join_server_on_launch: true,
                join_server_address: address.into(),
                ..Default::default()
            };
            let mut args = Vec::new();
            append_server_arguments(&settings, &mut args).unwrap();
            assert_eq!(args, expected, "{address}");
        }
    }

    #[test]
    fn auto_join_disabled_or_empty_does_not_add_arguments() {
        for (enabled, address) in [(false, "invalid:port"), (true, "  ")] {
            let settings = InstanceSettings {
                join_server_on_launch: enabled,
                join_server_address: address.into(),
                ..Default::default()
            };
            let mut args = vec!["existing".into()];
            append_server_arguments(&settings, &mut args).unwrap();
            assert_eq!(args, ["existing"]);
        }
    }

    #[test]
    fn auto_join_rejects_invalid_addresses_without_mutating_arguments() {
        for address in [
            ":25565",
            "host:",
            "host:0",
            "host:65536",
            "host:port",
            "host:+25565",
            "host:12:34",
            "[::1",
            "[host]:25565",
            "[::1]extra",
            "[::1]:",
            "https://host",
            "a b",
            "host/path",
            "host\0",
        ] {
            let settings = InstanceSettings {
                join_server_on_launch: true,
                join_server_address: address.into(),
                ..Default::default()
            };
            let mut args = vec!["existing".into()];
            assert!(
                append_server_arguments(&settings, &mut args).is_err(),
                "{address:?}"
            );
            assert_eq!(args, ["existing"]);
        }
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;
    #[tokio::test]
    async fn recovers_completed_game_logs_time_and_preserves_instance_edits() {
        let root =
            std::env::temp_dir().join(format!("industrialis-recovery-{}", uuid::Uuid::new_v4()));
        let paths = LauncherPaths::new(root.clone());
        let session = GameSession {
            capture_id: uuid::Uuid::new_v4().to_string(),
            started_at_ms: 1_000,
            record_time: true,
            stdout_offset: 5,
            stderr_offset: 0,
            extra_fields: Default::default(),
        };
        let directory =
            crate::game_capture::session_directory(&paths, "game", &session.capture_id).unwrap();
        tokio::fs::create_dir_all(&directory).await.unwrap();
        crate::fs_utils::write_json(&paths.settings_path("game").unwrap(), &serde_json::json!({
            "name":"User rename", "total_play_seconds":100,"extension":{"kept":true},"max_ram_mb":8192,"cached_size_bytes":7
        })).await.unwrap();
        tokio::fs::write(
            paths.instance_dir("game").unwrap().join("world-data.bin"),
            vec![1; 8192],
        )
        .await
        .unwrap();
        tokio::fs::write(directory.join("stdout.log"), "old\r\nnew\nfinal")
            .await
            .unwrap();
        tokio::fs::write(directory.join("stderr.log"), "error\n")
            .await
            .unwrap();
        crate::fs_utils::write_json(&directory.join("exit.json"), &serde_json::json!({
            "pid":2147483646u32,"creation_id":"fixture","exit_code":7,"ended_at_ms":11_000,"post_exit_error":null
        })).await.unwrap();
        crate::running_pids::update(&paths, {
            let session = session.clone();
            move |pids| {
                pids.insert(
                    "game".into(),
                    RunningGamePid {
                        pid: 2147483646,
                        creation_id: "fixture".into(),
                        launcher_session: Some(session),
                        extra_fields: Default::default(),
                    },
                );
            }
        })
        .await
        .unwrap();
        // A prior detach checkpoint must not be credited twice on recovery.
        crate::settings::record_game_time(&paths, "game", &session, 6_000)
            .await
            .unwrap();
        assert!(
            monitor_restored_game(&paths, "game", Arc::new(|_, _, _| {}))
                .await
                .unwrap()
        );
        let saved = load_instance_settings(&paths, "game").await.unwrap();
        assert_eq!(saved.total_play_seconds, 110);
        assert_eq!(saved.name, "User rename");
        assert_eq!(saved.max_ram_mb, 8192);
        assert!(
            saved.cached_size_bytes >= 8192,
            "a crash exit refreshes disk usage too"
        );
        assert_eq!(saved.extra_fields["extension"]["kept"], true);
        assert!(load_running_game_pids(&paths).await.unwrap().is_empty());
        assert!(!directory.exists());
        let logs = crate::console_log::read_console_log(&paths, "game", true)
            .await
            .unwrap();
        let lines: Vec<_> = logs.iter().map(|l| l.line.as_str()).collect();
        assert!(!lines.contains(&"old"));
        assert!(lines.contains(&"new"));
        assert!(lines.contains(&"final"));
        assert!(lines.contains(&"error"));
        assert!(lines.contains(&"Process exited with code 7"));
        assert!(
            !monitor_restored_game(&paths, "game", Arc::new(|_, _, _| {}))
                .await
                .unwrap()
        );
        assert_eq!(
            load_instance_settings(&paths, "game")
                .await
                .unwrap()
                .total_play_seconds,
            110
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn restores_live_game_then_detaches_during_graceful_shutdown() {
        let root =
            std::env::temp_dir().join(format!("industrialis-live-detach-{}", uuid::Uuid::new_v4()));
        let paths = LauncherPaths::new(root.clone());
        let session = GameSession {
            capture_id: uuid::Uuid::new_v4().to_string(),
            started_at_ms: now_ms(),
            record_time: false,
            stdout_offset: 0,
            stderr_offset: 0,
            extra_fields: Default::default(),
        };
        let directory =
            crate::game_capture::session_directory(&paths, "game", &session.capture_id).unwrap();
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("stdout.log"), "hello\n")
            .await
            .unwrap();
        tokio::fs::write(directory.join("stderr.log"), "")
            .await
            .unwrap();
        let pid = std::process::id();
        let creation_id = process_manager::get_process_creation_id(pid).unwrap();
        crate::running_pids::update(&paths, move |pids| {
            pids.insert(
                "game".into(),
                RunningGamePid {
                    pid,
                    creation_id,
                    launcher_session: Some(session),
                    extra_fields: Default::default(),
                },
            );
        })
        .await
        .unwrap();
        let task = tokio::spawn({
            let paths = paths.clone();
            async move { monitor_restored_game(&paths, "game", Arc::new(|_, _, _| {})).await }
        });
        // Wait for the admitted monitor without depending on a scheduling race.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        crate::lifecycle::for_profile(&paths)
            .shutdown(&paths)
            .await
            .unwrap();
        assert!(!task.await.unwrap().unwrap());
        assert!(process_manager::is_process_alive(pid));
        let entry = load_running_game_pids(&paths)
            .await
            .unwrap()
            .remove("game")
            .unwrap();
        assert_eq!(entry.launcher_session.unwrap().stdout_offset, 6);
        assert_eq!(
            load_instance_settings(&paths, "game")
                .await
                .unwrap()
                .cached_size_bytes,
            0,
            "detaching from a live game must not trigger a size scan"
        );
        assert!(directory.exists());
        let logs = crate::console_log::read_console_log(&paths, "game", true)
            .await
            .unwrap();
        assert_eq!(logs.iter().filter(|l| l.line == "hello").count(), 1);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn live_and_persisted_logs_remain_ordered_and_read_waits_for_writes() {
        let dir =
            std::env::temp_dir().join(format!("industrialis-log-order-{}", uuid::Uuid::new_v4()));
        let paths = LauncherPaths::new(dir.clone());
        let events = Arc::new(Mutex::new(Vec::new()));
        let live = events.clone();
        let sink = LogSink::new(
            &paths,
            "game",
            Arc::new(move |_, _, line| live.lock().unwrap().push(line)),
        )
        .unwrap();
        let expected = (0..1000).map(|n| n.to_string()).collect::<Vec<_>>();
        for line in &expected {
            sink.log("stdout", line);
        }
        // The reader sends a barrier through the active queue, without callers
        // needing access to the launch's LogSink or its worker.
        let persisted = crate::console_log::read_console_log(&paths, "game", true)
            .await
            .unwrap();
        assert_eq!(*events.lock().unwrap(), expected);
        assert_eq!(
            persisted.iter().map(|e| e.line.clone()).collect::<Vec<_>>(),
            *events.lock().unwrap()
        );
        sink.finish().await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
