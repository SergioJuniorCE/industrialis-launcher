//! Headless child of the same executable. Owns the game wait, output files and
//! exit receipt independently of the window; never writes profile metadata.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const FLAG: &str = "--industrialis-game-broker";
#[derive(Serialize, Deserialize)]
pub(crate) struct Request {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub environment: HashMap<String, String>,
    pub post_exit: Option<(String, PathBuf)>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Ready {
    pub pid: u32,
    pub creation_id: String,
    pub broker_pid: u32,
    pub broker_creation_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Exit {
    pub pid: u32,
    pub creation_id: String,
    pub exit_code: i64,
    pub ended_at_ms: u64,
    pub post_exit_error: Option<String>,
}
fn save<T: Serialize>(directory: &Path, name: &str, value: &T) -> anyhow::Result<()> {
    crate::fs_utils::write_atomic(&directory.join(name), &serde_json::to_vec(value)?)
}
fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    let _ = command;
}
fn run(request_path: &Path) -> anyhow::Result<()> {
    let directory = request_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing capture directory"))?;
    let bytes = std::fs::read(request_path)?;
    // Arguments may contain an access token. Remove the request before spawning
    // and never include its contents in diagnostics.
    std::fs::remove_file(request_path)?;
    let request: Request = serde_json::from_slice(&bytes)?;
    let mut command = Command::new(&request.executable);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .envs(&request.environment)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(directory.join("stdout.log"))?)
        .stderr(std::fs::File::create(directory.join("stderr.log"))?);
    hidden(&mut command);
    let mut child = command.spawn()?;
    let pid = child.id();
    let Some(creation_id) = crate::process_manager::get_process_creation_id(pid) else {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("Could not identify game process");
    };
    if let Err(error) = save(
        directory,
        "ready.json",
        &Ready {
            pid,
            creation_id: creation_id.clone(),
            broker_pid: std::process::id(),
            broker_creation_id: crate::process_manager::get_process_creation_id(std::process::id())
                .unwrap_or_default(),
        },
    ) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let exit_code = child.wait()?.code().map(i64::from).unwrap_or(1);
    let ended_at_ms = crate::launch::now_ms();
    let post_exit_error = request.post_exit.and_then(|(text, cwd)| {
        #[cfg(windows)]
        let mut hook = {
            let mut c = Command::new("cmd.exe");
            c.args(["/D", "/S", "/C", &text]);
            c
        };
        #[cfg(not(windows))]
        let mut hook = {
            let mut c = Command::new("/bin/sh");
            c.args(["-c", &text]);
            c
        };
        hook.current_dir(cwd)
            .envs(&request.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hidden(&mut hook);
        match hook.status() {
            Ok(status) if status.success() => None,
            Ok(status) => Some(format!("Post-exit command failed ({status})")),
            Err(error) => Some(format!("Could not run post-exit command: {error}")),
        }
    });
    save(
        directory,
        "exit.json",
        &Exit {
            pid,
            creation_id,
            exit_code,
            ended_at_ms,
            post_exit_error,
        },
    )
}
/// Call before ownership, CLI parsing or GPUI initialization in every executable.
pub fn run_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(FLAG)) {
        return false;
    }
    let Some(request) = args.next().map(PathBuf::from) else {
        return true;
    };
    if let Err(error) = run(&request) {
        if let Some(directory) = request.parent() {
            let _ = save(
                directory,
                "startup-error.json",
                &format!("Game broker failed: {error}"),
            );
        }
    }
    true
}
pub(crate) async fn spawn(request: Request, directory: &Path) -> anyhow::Result<Ready> {
    tokio::fs::create_dir_all(directory).await?;
    let request_path = directory.join("request.json");
    crate::fs_utils::write_json(&request_path, &serde_json::to_value(request)?).await?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .arg(FLAG)
        .arg(&request_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    #[cfg(windows)]
    command.creation_flags(0x09000000); // NO_WINDOW | BREAKAWAY_FROM_JOB
    let mut broker = match command.spawn() {
        Ok(child) => Some(child),
        #[cfg(windows)]
        Err(_) => {
            if let Err(error) = spawn_outside_job(&request_path, directory).await {
                let _ = tokio::fs::remove_file(&request_path).await;
                return Err(error);
            }
            None
        }
        #[cfg(not(windows))]
        Err(error) => {
            let _ = tokio::fs::remove_file(&request_path).await;
            return Err(error.into());
        }
    };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    // Do not cancel this startup handshake during a normal close. The admitted
    // launch finishes startup and persists the PID before shutdown can detach it.
    loop {
        if let Ok(raw) = tokio::fs::read(directory.join("ready.json")).await {
            return Ok(serde_json::from_slice(&raw)?);
        }
        if let Ok(raw) = tokio::fs::read(directory.join("startup-error.json")).await {
            anyhow::bail!("{}", serde_json::from_slice::<String>(&raw)?);
        }
        if broker
            .as_mut()
            .map(|child| child.try_wait())
            .transpose()?
            .flatten()
            .is_some()
            || tokio::time::Instant::now() >= deadline
        {
            let _ = tokio::fs::remove_file(request_path).await;
            anyhow::bail!("Game broker exited before game startup");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
pub(crate) async fn exit(
    directory: &Path,
    pid: u32,
    creation_id: &str,
) -> anyhow::Result<Option<Exit>> {
    let bytes = match tokio::fs::read(directory.join("exit.json")).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let exit: Exit = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        exit.pid == pid && exit.creation_id == creation_id,
        "Game exit record does not match its process"
    );
    Ok(Some(exit))
}

#[cfg(windows)]
async fn spawn_outside_job(request: &Path, directory: &Path) -> anyhow::Result<()> {
    // WMI creates the broker through its service, outside the inherited job.
    // All dynamic values are JSON data, never interpolated PowerShell code.
    fn quote(value: &std::ffi::OsStr) -> String {
        let mut output = String::from("\"");
        let mut slashes = 0;
        for ch in value.to_string_lossy().chars() {
            if ch == '\\' {
                slashes += 1;
                continue;
            }
            if ch == '"' {
                output.push_str(&"\\".repeat(slashes * 2 + 1));
            } else {
                output.push_str(&"\\".repeat(slashes));
            }
            output.push(ch);
            slashes = 0;
        }
        output.push_str(&"\\".repeat(slashes * 2));
        output.push('"');
        output
    }
    let payload_path = directory.join("broker-start.json");
    let line = format!(
        "{} {} {}",
        quote(std::env::current_exe()?.as_os_str()),
        FLAG,
        quote(request.as_os_str())
    );
    let environment: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    crate::fs_utils::write_json(
        &payload_path,
        &serde_json::json!({"command":line,"environment":environment}),
    )
    .await?;
    let script = r#"$ErrorActionPreference='Stop'
$p=Get-Content -LiteralPath $env:INDUSTRIALIS_BROKER_START -Raw | ConvertFrom-Json
$s=New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow=[uint16]0; EnvironmentVariables=[string[]]$p.environment }
$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine=[string]$p.command; ProcessStartupInformation=$s }
if ([uint32]$r.ReturnValue -ne 0) { throw 'Could not start independent game broker' }
"#;
    let mut command = tokio::process::Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("INDUSTRIALIS_BROKER_START", &payload_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x08000000);
    let result = tokio::time::timeout(std::time::Duration::from_secs(20), command.status()).await;
    let _ = tokio::fs::remove_file(&payload_path).await;
    anyhow::ensure!(
        matches!(result,Ok(Ok(status)) if status.success()),
        "Could not start independent Windows game broker"
    );
    Ok(())
}
