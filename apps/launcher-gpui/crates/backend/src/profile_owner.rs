//! Cross-runtime profile ownership. Electron implements the same small protocol
//! in profile-owner.mjs. Bind before reading/writing profile state; retain the
//! guard until backend shutdown. Kernel socket ownership needs no stale PID file.

use crate::paths::LauncherPaths;
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const PREFIX: &str = "industrialis-profile-v1";
const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerKind {
    Launcher,
    Operation,
}

pub enum Acquisition {
    Primary(ProfileOwner),
    /// A launcher was signaled (for Launcher requests), or a CLI operation owns it.
    Existing(OwnerKind),
}

pub struct ProfileOwner {
    activation: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ProfileOwner {
    pub fn activation_signal(&self) -> Arc<AtomicBool> {
        self.activation.clone()
    }
}

impl Drop for ProfileOwner {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn identity(paths: &LauncherPaths) -> anyhow::Result<(String, SocketAddr)> {
    std::fs::create_dir_all(paths.base())
        .context("Could not access the launcher data directory")?;
    let canonical = paths.base().canonicalize()?;
    let mut name = canonical
        .to_str()
        .context("Launcher data directory must be valid Unicode")?
        .to_string();
    if cfg!(windows) {
        name = name
            .strip_prefix(r"\\?\")
            .unwrap_or(&name)
            .replace('\\', "/")
            .to_lowercase();
        if name.starts_with("unc/") {
            name = format!("//{}", &name[4..]);
        }
    }
    let digest = Sha256::digest(name.as_bytes());
    let key = format!("{digest:x}");
    let port = 20000 + u16::from_be_bytes([digest[0], digest[1]]) % 20000;
    Ok((key, SocketAddr::from((Ipv4Addr::LOCALHOST, port))))
}

fn read_line(stream: &mut TcpStream) -> std::io::Result<String> {
    // Accepted Winsock sockets may inherit the listener's nonblocking mode.
    // The request deadline below requires blocking reads with a bounded timeout.
    stream.set_nonblocking(false)?;
    let deadline = Instant::now() + TIMEOUT;
    let mut line = Vec::new();
    let mut byte = [0];
    while line.len() < 128 {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Profile ownership request timed out",
                )
            })?;
        stream.set_read_timeout(Some(remaining))?;
        if stream.read(&mut byte)? == 0 {
            break;
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

pub fn acquire(paths: &LauncherPaths, kind: OwnerKind) -> anyhow::Result<Acquisition> {
    let (key, address) = identity(paths)?;
    let listener = match TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
            ) =>
        {
            let (owner, pid) = request_owner(address, &key, "probe")?;
            if kind == OwnerKind::Launcher && owner == OwnerKind::Launcher {
                allow_foreground(pid);
                // Permission handoff precedes the activation request, including
                // when the current owner is Electron rather than GPUI.
                return Ok(Acquisition::Existing(
                    request_owner(address, &key, "focus")?.0,
                ));
            }
            return Ok(Acquisition::Existing(owner));
        }
        Err(error) => return Err(error).context("Could not acquire launcher profile ownership"),
    };
    crate::legacy_launcher::ensure_safe_start(paths)?;
    listener.set_nonblocking(true)?;
    let activation = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = activation.clone();
    let stop = stopped.clone();
    let worker = thread::Builder::new()
        .name("industrialis-profile-owner".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(TIMEOUT));
                        let _ = stream.set_write_timeout(Some(TIMEOUT));
                        if let Ok(request) = read_line(&mut stream) {
                            let focus = request == format!("{PREFIX} {key} focus\n");
                            if focus || request == format!("{PREFIX} {key} probe\n") {
                                if focus && kind == OwnerKind::Launcher {
                                    signal.store(true, Ordering::Release);
                                }
                                let owner = if kind == OwnerKind::Launcher {
                                    "launcher"
                                } else {
                                    "operation"
                                };
                                let _ = stream.write_all(
                                    format!("{PREFIX} {key} {owner} {}\n", std::process::id())
                                        .as_bytes(),
                                );
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    // Retain the bound listener on transient accept failures.
                    Err(_) => thread::sleep(Duration::from_millis(20)),
                }
            }
        })?;
    Ok(Acquisition::Primary(ProfileOwner {
        activation,
        stopped,
        worker: Some(worker),
    }))
}

fn request_owner(address: SocketAddr, key: &str, action: &str) -> anyhow::Result<(OwnerKind, u32)> {
    // Contending Windows launches can abort/reset a loopback connection while
    // the listener remains alive. Retry transport failures on the same endpoint;
    // never retry an invalid identity or rebind/claim a profile after a failure.
    for attempt in 0..3 {
        match request_owner_once(address, key, action) {
            Err(error)
                if attempt < 2
                    && error.downcast_ref::<std::io::Error>().is_some_and(|io| {
                        matches!(
                            io.kind(),
                            std::io::ErrorKind::ConnectionAborted
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionRefused
                                | std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::Interrupted
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::WouldBlock
                        )
                    }) =>
            {
                thread::sleep(Duration::from_millis(20))
            }
            result => return result,
        }
    }
    unreachable!("The final ownership attempt returns its result")
}

fn request_owner_once(
    address: SocketAddr,
    key: &str,
    action: &str,
) -> anyhow::Result<(OwnerKind, u32)> {
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).context(
        "The profile ownership endpoint is occupied or unavailable; no profile data was opened",
    )?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream
        .write_all(format!("{PREFIX} {key} {action}\n").as_bytes())
        .context("Could not send the profile ownership request")?;
    let response =
        read_line(&mut stream).context("Could not read the profile ownership response")?;
    let fields: Vec<_> = response.trim_end_matches('\n').split(' ').collect();
    if response.ends_with('\n') && fields.len() == 4 && fields[0] == PREFIX && fields[1] == key {
        let kind = match fields[2] {
            "launcher" => Some(OwnerKind::Launcher),
            "operation" => Some(OwnerKind::Operation),
            _ => None,
        };
        if let (Some(kind), Ok(pid)) = (kind, fields[3].parse::<u32>()) {
            if pid > 0 {
                return Ok((kind, pid));
            }
        }
    }
    anyhow::bail!(
        "The profile ownership endpoint did not identify this profile; no profile data was opened"
    )
}

fn allow_foreground(_pid: u32) {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn AllowSetForegroundWindow(pid: u32) -> i32;
        }
        // The OS may decline foreground permission; restoration/attention still
        // belongs to the owner. Never use ASFW_ANY or manipulate another window.
        unsafe {
            AllowSetForegroundWindow(_pid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Profile(LauncherPaths);
    impl Profile {
        fn new() -> Self {
            Self(LauncherPaths::new(std::env::temp_dir().join(format!(
                "industrialis-owner-{}",
                uuid::Uuid::new_v4()
            ))))
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.base());
        }
    }
    fn primary(paths: &LauncherPaths, kind: OwnerKind) -> ProfileOwner {
        match acquire(paths, kind).unwrap() {
            Acquisition::Primary(owner) => owner,
            _ => panic!("Expected primary owner"),
        }
    }

    #[test]
    fn bounded_request_reader_waits_for_fragmented_nonblocking_connections() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut accepted, _) = listener.accept().unwrap();
        // Winsock accepted sockets can retain the nonblocking listener's mode.
        // Set it explicitly so the regression also exercises that contract on Unix.
        accepted.set_nonblocking(true).unwrap();
        client.write_all(b"fragmented request").unwrap();
        let ready = Arc::new(std::sync::Barrier::new(2));
        let reader_ready = ready.clone();
        let reader = thread::spawn(move || {
            reader_ready.wait();
            read_line(&mut accepted)
        });
        ready.wait();
        thread::sleep(Duration::from_millis(100));
        let sent = client.write_all(b"\n");
        assert_eq!(reader.join().unwrap().unwrap(), "fragmented request\n");
        sent.unwrap();
    }

    #[test]
    fn second_launch_signals_owner_but_write_commands_do_not() {
        let profile = Profile::new();
        let owner = primary(&profile.0, OwnerKind::Launcher);
        let signal = owner.activation_signal();
        assert!(matches!(
            acquire(&profile.0, OwnerKind::Operation).unwrap(),
            Acquisition::Existing(OwnerKind::Launcher)
        ));
        assert!(!signal.load(Ordering::Acquire));
        assert!(matches!(
            acquire(&profile.0, OwnerKind::Launcher).unwrap(),
            Acquisition::Existing(OwnerKind::Launcher)
        ));
        assert!(signal.swap(false, Ordering::AcqRel));
        drop(owner);
        let _new = primary(&profile.0, OwnerKind::Launcher);
    }

    #[test]
    fn operations_block_launchers_and_other_writers_without_focus() {
        let profile = Profile::new();
        let owner = primary(&profile.0, OwnerKind::Operation);
        for kind in [OwnerKind::Launcher, OwnerKind::Operation] {
            assert!(matches!(
                acquire(&profile.0, kind).unwrap(),
                Acquisition::Existing(OwnerKind::Operation)
            ));
        }
        assert!(!owner.activation_signal().load(Ordering::Acquire));
    }

    #[test]
    fn simultaneous_starts_have_one_owner_and_directory_aliases_share_it() {
        let profile = Profile::new();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let paths = profile.0.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    acquire(&paths, OwnerKind::Launcher).unwrap()
                })
            })
            .collect();
        let owners: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(
            owners
                .iter()
                .filter(|v| matches!(v, Acquisition::Primary(_)))
                .count(),
            1
        );
        let alias = LauncherPaths::new(profile.0.base().join("."));
        assert!(matches!(
            acquire(&alias, OwnerKind::Operation).unwrap(),
            Acquisition::Existing(OwnerKind::Launcher)
        ));
        assert!(
            std::fs::read_dir(profile.0.base())
                .unwrap()
                .next()
                .is_none(),
            "Ownership creates no profile documents"
        );
    }

    #[test]
    fn unrecognized_endpoint_fails_closed_without_opening_profile_documents() {
        let profile = Profile::new();
        let (_, address) = identity(&profile.0).unwrap();
        let listener = TcpListener::bind(address).unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            let _ = read_line(&mut stream);
            stream.write_all(b"wrong-profile\n").unwrap();
        });
        assert!(acquire(&profile.0, OwnerKind::Launcher).is_err());
        peer.join().unwrap();
        assert!(std::fs::read_dir(profile.0.base())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn a_reset_during_owner_probe_retries_without_claiming_the_profile() {
        let profile = Profile::new();
        let (key, address) = identity(&profile.0).unwrap();
        let listener = TcpListener::bind(address).unwrap();
        let peer = thread::spawn(move || {
            // Drop an accepted connection with its request still unread. This
            // produces a real TCP reset, matching the Windows contention failure.
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            assert!(stream.peek(&mut [0]).unwrap() > 0);
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut requests = Vec::new();
            while requests.len() < 2 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let action = ["probe", "focus"][requests.len()];
                        let request = read_line(&mut stream).unwrap();
                        assert_eq!(request, format!("{PREFIX} {key} {action}\n"));
                        requests.push(request);
                        stream
                            .write_all(
                                format!("{PREFIX} {key} launcher {}\n", std::process::id())
                                    .as_bytes(),
                            )
                            .unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("Probe fixture failed: {error}"),
                }
            }
            requests.len()
        });
        let result = acquire(&profile.0, OwnerKind::Launcher);
        let requests = peer.join().unwrap();
        assert!(
            result.is_ok(),
            "Reset aborted activation: {:?}",
            result.err()
        );
        assert!(matches!(
            result.unwrap(),
            Acquisition::Existing(OwnerKind::Launcher)
        ));
        assert_eq!(requests, 2);
        assert!(std::fs::read_dir(profile.0.base())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn repeated_transport_resets_still_fail_closed_without_profile_writes() {
        let profile = Profile::new();
        let (_, address) = identity(&profile.0).unwrap();
        let listener = TcpListener::bind(address).unwrap();
        listener.set_nonblocking(true).unwrap();
        let peer = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut accepted = 0;
            while accepted < 3 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream.set_read_timeout(Some(TIMEOUT)).unwrap();
                        assert!(stream.peek(&mut [0]).unwrap() > 0);
                        drop(stream);
                        accepted += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("Reset fixture failed: {error}"),
                }
            }
            accepted
        });
        let result = acquire(&profile.0, OwnerKind::Launcher);
        assert_eq!(peer.join().unwrap(), 3);
        assert!(result.is_err());
        assert!(std::fs::read_dir(profile.0.base())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn invalid_requests_do_not_activate_and_idle_clients_cannot_hold_shutdown() {
        let profile = Profile::new();
        let owner = primary(&profile.0, OwnerKind::Launcher);
        let signal = owner.activation_signal();
        let (_, address) = identity(&profile.0).unwrap();
        let mut invalid = TcpStream::connect(address).unwrap();
        invalid.write_all(b"wrong-profile focus\n").unwrap();
        invalid.set_read_timeout(Some(TIMEOUT)).unwrap();
        match read_line(&mut invalid) {
            Ok(reply) => assert!(
                reply.is_empty(),
                "Invalid request received a valid owner reply"
            ),
            Err(error) => assert!(
                matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ),
                "Invalid peer must be closed, not time out: {error}"
            ),
        }
        assert!(!signal.load(Ordering::Acquire));
        let _idle = TcpStream::connect(address).unwrap();
        let start = Instant::now();
        drop(owner);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
