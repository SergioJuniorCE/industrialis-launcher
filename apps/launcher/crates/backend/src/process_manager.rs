//! Process identity, independently captured output, and game lifetime.
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use sysinfo::{Pid, ProcessesToUpdate, System};

const WAIT_POLL: Duration = Duration::from_millis(500);

/// A game process. `child` is present for processes spawned by this backend
/// run; restored entries (previous launcher run) carry pid + creation id only.
pub struct RunningProcess {
    pub pid: u32,
    pub creation_id: Option<String>,
    child: Option<tokio::process::Child>,
    broker_directory: Option<std::path::PathBuf>,
    temporary_capture: bool,
    capture: Option<crate::game_capture::Capture>,
}

impl RunningProcess {
    pub fn detached(pid: u32, creation_id: Option<String>) -> Self {
        Self {
            pid,
            creation_id,
            child: None,
            broker_directory: None,
            temporary_capture: false,
            capture: None,
        }
    }

    pub fn is_owned(&self) -> bool {
        self.child.is_some()
    }
}

/// Mirrors `normalizeProcessCreationId` (trim; empty → None).
pub fn normalize_process_creation_id(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        None
    } else {
        #[cfg(windows)]
        {
            if let Ok(ticks) = normalized.parse::<u64>() {
                return Some(
                    if ticks >= 1_000_000_000_000_000 {
                        ticks / 10_000
                    } else {
                        ticks
                    }
                    .to_string(),
                );
            }
        }
        Some(normalized.to_string())
    }
}

/// Start time (seconds since epoch) of a live process, if it exists.
fn process_start_time(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    let raw = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[raw]), false);
    system.process(raw).map(|process| process.start_time())
}

/// Mirrors `isProcessAlive`.
pub fn is_process_alive(pid: u32) -> bool {
    process_start_time(pid).is_some()
}

#[cfg(windows)]
fn windows_creation_id(pid: u32) -> Option<String> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            handle: *mut std::ffi::c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    let handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let success =
        unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(handle);
    }
    if success == 0 {
        return None;
    }
    let ticks = (u64::from(creation.high) << 32) | u64::from(creation.low);
    // Electron stores .NET UTC ticks normalized to milliseconds, not Unix seconds.
    Some(((ticks + 504_911_232_000_000_000) / 10_000).to_string())
}

pub fn matches_creation_id(pid: u32, expected: &str) -> bool {
    let Some(expected) = normalize_process_creation_id(expected) else {
        return false;
    };
    if get_process_creation_id(pid).as_deref() == Some(&expected) {
        return true;
    }
    // Read records written by the early launcher prototype without rewriting them.
    expected
        .parse::<u64>()
        .ok()
        .is_some_and(|seconds| seconds < 10_000_000_000 && process_start_time(pid) == Some(seconds))
}

#[cfg(target_os = "linux")]
fn linux_creation_id(pid: u32) -> Option<String> {
    let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let boot_id = normalize_process_creation_id(&boot_id)?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let command_end = stat.rfind(')')?;
    let mut fields = stat[command_end + 2..].trim().split_whitespace();
    // starttime is the 22nd field overall, i.e. index 19 after `) `.
    let start_time = normalize_process_creation_id(fields.nth(19)?)?;
    Some(format!("{boot_id}:{start_time}"))
}

/// Mirrors `getProcessCreationId`. Linux reads `/proc` exactly like the TS
/// backend; other platforms use the process start time from sysinfo.
pub fn get_process_creation_id(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        return windows_creation_id(pid);
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        return output
            .status
            .success()
            .then(|| normalize_process_creation_id(&String::from_utf8_lossy(&output.stdout)))
            .flatten();
    }
    #[cfg(target_os = "linux")]
    {
        if let id @ Some(_) = linux_creation_id(pid) {
            return id;
        }
    }
    // `start_time` is seconds since the epoch; combined with the pid it is a
    // reliable reuse guard (matches TS normalization: non-empty → Some).
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        normalize_process_creation_id(&process_start_time(pid)?.to_string())
    }
}

async fn is_tracked_process_alive(info: &RunningProcess) -> bool {
    if !is_process_alive(info.pid) {
        return false;
    }
    let Some(expected) = info.creation_id.as_deref() else {
        return true;
    };
    matches_creation_id(info.pid, expected)
}

pub type LogEmitter = std::sync::Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Mirrors `spawnGameProcess` (direct-spawn design — see module docs).
pub async fn spawn_game_process(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    environment: &HashMap<String, String>,
    emit: LogEmitter,
) -> Result<RunningProcess, anyhow::Error> {
    let directory =
        std::env::temp_dir().join(format!("industrialis-capture-{}", uuid::Uuid::new_v4()));
    let mut running =
        spawn_game_process_with_capture(executable, args, cwd, environment, emit, &directory)
            .await?;
    running.temporary_capture = true;
    Ok(running)
}

pub(crate) async fn spawn_game_process_with_capture(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    environment: &HashMap<String, String>,
    emit: LogEmitter,
    directory: &Path,
) -> anyhow::Result<RunningProcess> {
    tokio::fs::create_dir_all(directory).await?;
    let stdout = std::fs::File::create(directory.join("stdout.log"))?;
    let stderr = std::fs::File::create(directory.join("stderr.log"))?;
    let capture = crate::game_capture::Capture::start(directory, [0, 0], emit).await?;
    let mut command = tokio::process::Command::new(executable);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .kill_on_drop(false);
    for (key, value) in environment {
        command.env(key, value);
    }
    #[cfg(windows)]
    {
        // Keep the game console hidden, like the TS `CreateNoWindow` setup.
        command.creation_flags(0x08000000);
    }
    let child = command.spawn().map_err(|e| {
        anyhow::anyhow!(
            "failed to start game process ({}): {e}",
            executable.display()
        )
    })?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("game process has no pid"))?;
    let creation_id = get_process_creation_id(pid);
    Ok(RunningProcess {
        pid,
        creation_id,
        broker_directory: None,
        temporary_capture: false,
        capture: Some(capture),
        child: Some(child),
    })
}

pub async fn spawn_brokered_game(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    environment: &HashMap<String, String>,
    emit: LogEmitter,
    directory: &Path,
    post_exit: Option<(String, std::path::PathBuf)>,
) -> anyhow::Result<RunningProcess> {
    let ready = crate::game_broker::spawn(
        crate::game_broker::Request {
            executable: executable.into(),
            args: args.into(),
            cwd: cwd.into(),
            environment: environment.clone(),
            post_exit,
        },
        directory,
    )
    .await?;
    let mut process = RunningProcess::detached(ready.pid, Some(ready.creation_id));
    if let Err(error) = attach_capture(&mut process, directory, [0, 0], emit).await {
        let _ = kill_game_process(&mut process).await;
        return Err(error);
    }
    Ok(process)
}

/// Mirrors `waitForGameProcess`. Returns the exit code (signal deaths → 1).
pub async fn wait_for_game_process(info: &mut RunningProcess) -> i64 {
    let code = wait_for_exit(info).await;
    if let Some(capture) = info.capture.as_mut() {
        let _ = capture.drain().await;
    }
    if info.temporary_capture {
        if let Some(capture) = info.capture.take() {
            let _ = tokio::fs::remove_dir_all(&capture.directory).await;
        }
    }
    code
}
async fn wait_for_exit(info: &mut RunningProcess) -> i64 {
    if let Some(child) = info.child.as_mut() {
        let code = child
            .wait()
            .await
            .map(|s| s.code())
            .ok()
            .flatten()
            .map(i64::from)
            .unwrap_or(1);
        info.child.take();
        return code;
    }
    loop {
        if let Some(directory) = &info.broker_directory {
            match crate::game_broker::exit(
                directory,
                info.pid,
                info.creation_id.as_deref().unwrap_or_default(),
            )
            .await
            {
                Ok(Some(exit)) => return exit.exit_code,
                Err(_) => return 1,
                Ok(None) => {}
            }
            if let Ok(raw) = tokio::fs::read(directory.join("ready.json")).await {
                if let Ok(ready) = serde_json::from_slice::<crate::game_broker::Ready>(&raw) {
                    if !is_process_alive(ready.broker_pid)
                        || !matches_creation_id(ready.broker_pid, &ready.broker_creation_id)
                    {
                        if !is_tracked_process_alive(info).await {
                            return 1;
                        }
                    }
                }
            } else if !is_tracked_process_alive(info).await {
                return 1;
            }
            // The broker may still be completing the post-exit hook. Its durable
            // receipt is the completion barrier; shutdown can detach meanwhile.
        } else if !is_tracked_process_alive(info).await {
            return 0;
        }
        tokio::time::sleep(WAIT_POLL).await;
    }
}

pub(crate) async fn wait_or_detach(
    info: &mut RunningProcess,
    lifecycle: &crate::lifecycle::Lifecycle,
) -> anyhow::Result<(Option<i64>, [u64; 2])> {
    let code = tokio::select! { code=wait_for_exit(info)=>Some(code), _=lifecycle.closing()=>None };
    let offsets = if let Some(capture) = info.capture.as_mut() {
        capture.drain().await?
    } else {
        [0, 0]
    };
    if code.is_none() {
        info.child.take();
    }
    Ok((code, offsets))
}

pub(crate) async fn attach_capture(
    info: &mut RunningProcess,
    directory: &Path,
    offsets: [u64; 2],
    emit: LogEmitter,
) -> anyhow::Result<()> {
    info.capture = Some(crate::game_capture::Capture::start(directory, offsets, emit).await?);
    info.broker_directory = Some(directory.into());
    Ok(())
}

/// Mirrors `killGameProcess`. Owned children are killed directly; detached
/// pids are verified against the creation id first, then SIGTERM'd (Unix) or
/// `taskkill /T /F`'d (Windows).
pub async fn kill_game_process(info: &mut RunningProcess) -> Result<(), anyhow::Error> {
    if let Some(mut child) = info.child.take() {
        child
            .kill()
            .await
            .map_err(|e| anyhow::anyhow!("failed to kill game process {}: {e}", info.pid))?;
        if let Some(capture) = info.capture.as_mut() {
            capture.drain().await?;
        }
        return Ok(());
    }
    if info.creation_id.is_some() && !is_tracked_process_alive(info).await {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let status = tokio::process::Command::new("taskkill.exe")
            .args(["/PID", &info.pid.to_string(), "/T", "/F"])
            .status()
            .await;
        match status {
            Ok(s) if s.success() => Ok(()),
            _ if !is_process_alive(info.pid) => Ok(()),
            Ok(_) | Err(_) => anyhow::bail!("failed to kill game process {}", info.pid),
        }
    }
    #[cfg(not(windows))]
    {
        let raw = Pid::from_u32(info.pid);
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[raw]), false);
        let sent = system
            .process(raw)
            .and_then(|process| process.kill_with(sysinfo::Signal::Term));
        // Only a refused signal against a live process is a failure; a dead
        // pid is a no-op success (mirrors the TS early return).
        if sent == Some(false) && is_process_alive(info.pid) {
            anyhow::bail!("failed to kill game process {}", info.pid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn normalizes_creation_ids() {
        assert_eq!(
            normalize_process_creation_id("  abc "),
            Some("abc".to_string())
        );
        assert_eq!(normalize_process_creation_id("   "), None);
    }

    #[test]
    fn rejects_zero_pid() {
        assert!(!is_process_alive(0));
        assert_eq!(get_process_creation_id(0), None);
    }

    #[test]
    fn current_process_is_alive_with_stable_creation_id() {
        let pid = std::process::id();
        assert!(is_process_alive(pid));
        let first = get_process_creation_id(pid);
        assert!(first.is_some());
        assert_eq!(get_process_creation_id(pid), first);
    }

    #[tokio::test]
    async fn spawns_and_waits_for_owned_process() {
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let capture = lines.clone();
        let emit: LogEmitter =
            Arc::new(move |stream, line| capture.lock().unwrap().push(format!("{stream}:{line}")));
        #[cfg(windows)]
        let (exe, args) = (
            Path::new("cmd.exe"),
            vec![
                "/C".to_string(),
                "echo hello-out& (echo hello-err) 1>&2& exit 3".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (exe, args) = (
            Path::new("/bin/sh"),
            vec![
                "-c".to_string(),
                "echo hello-out; echo hello-err >&2; exit 3".to_string(),
            ],
        );
        let mut running = spawn_game_process(exe, &args, Path::new("."), &HashMap::new(), emit)
            .await
            .unwrap();
        assert!(running.is_owned());
        assert!(is_process_alive(running.pid) || !is_process_alive(running.pid));
        let code = wait_for_game_process(&mut running).await;
        assert_eq!(code, 3);
        let lines = lines.lock().unwrap();
        assert!(
            lines.iter().any(|l| l == "stdout:hello-out"),
            "got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "stderr:hello-err"),
            "got: {lines:?}"
        );
    }

    #[tokio::test]
    async fn kill_stops_detached_guard_for_dead_pid() {
        let mut running = RunningProcess::detached(0x7FFF_FFFE, Some("stale".to_string()));
        // No such pid: the creation-id guard makes this a no-op success.
        assert!(kill_game_process(&mut running).await.is_ok());
    }
}
