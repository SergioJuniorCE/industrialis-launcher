//! GPUI-only GitHub release checks and verified Windows installer handoff.
//! No Electron assets, arbitrary endpoints, or unverified executables are used.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

const REPOSITORY: &str = "SergioJuniorCE/industrialis-launcher";
#[cfg(not(feature = "stable-updates"))]
pub const TAG_PREFIX: &str = "launcher-gpui-v";
#[cfg(feature = "stable-updates")]
pub const TAG_PREFIX: &str = "launcher-v";
#[cfg(not(feature = "stable-updates"))]
pub const RELEASE_PAGE: &str =
    "https://github.com/SergioJuniorCE/industrialis-launcher/releases?q=launcher-gpui-v";
#[cfg(feature = "stable-updates")]
pub const RELEASE_PAGE: &str =
    "https://github.com/SergioJuniorCE/industrialis-launcher/releases?q=launcher-v";
pub const STABLE_RELEASE_MARKER: &str = "<!-- industrialis-gpui-release-v1 -->";
pub const INSTALL_MARKER: &str = "gpui-install.json";
const MAX_INSTALLER_BYTES: u64 = 512 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    MacArm64,
    LinuxX64,
    Unsupported,
}

impl Platform {
    pub fn current() -> Self {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Self::WindowsX64,
            ("macos", "aarch64") => Self::MacArm64,
            ("linux", "x86_64") => Self::LinuxX64,
            _ => Self::Unsupported,
        }
    }

    fn asset_name(self) -> Option<&'static str> {
        match self {
            Self::WindowsX64 => Some(if cfg!(feature = "stable-updates") {
                "IndustrialisLauncherSetup.exe"
            } else {
                "IndustrialisGPUISetup-x64.exe"
            }),
            Self::MacArm64 => Some("IndustrialisGPUI-macos-arm64.dmg"),
            Self::LinuxX64 => Some("IndustrialisGPUI-linux-x64.tar.gz"),
            Self::Unsupported => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    // GPUI releases are marked prerelease until cutover so Electron's
    // /releases/latest feed remains unchanged. Semver prerelease tags are ignored.
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
    pub release_url: String,
    tag: String,
    asset: Option<Asset>,
}

impl AvailableUpdate {
    pub fn has_windows_installer(&self) -> bool {
        self.asset
            .as_ref()
            .is_some_and(|a| a.name == Platform::WindowsX64.asset_name().unwrap())
    }
}

#[derive(Debug, Clone)]
pub enum UpdateCheck {
    Current,
    Available(AvailableUpdate),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

pub type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .user_agent("Industrialis-GPUI-Updater")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()?)
}

pub async fn check_for_update(current: &str, platform: Platform) -> Result<UpdateCheck> {
    let client = client()?;
    let mut releases = Vec::new();
    // The repository also publishes other products. Search beyond the first
    // page and fail visibly at the bound instead of reporting a false success.
    for page in 1..=10 {
        let response = client
            .get(format!(
                "https://api.github.com/repos/{REPOSITORY}/releases"
            ))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .query(&[("per_page", 100), ("page", page)])
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        let batch: Vec<Release> = serde_json::from_slice(&bounded_body(response).await?)?;
        let finished = batch.len() < 100;
        releases.extend(batch);
        if finished {
            return select_update(&releases, current, platform);
        }
    }
    bail!("Too many release pages; open the GPUI releases page to check manually")
}

async fn bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= MAX_METADATA_BYTES,
            "Release metadata is unexpectedly large"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn select_update(releases: &[Release], current: &str, platform: Platform) -> Result<UpdateCheck> {
    let current = Version::parse(current).context("Invalid installed launcher version")?;
    let newest = releases
        .iter()
        .filter_map(|release| {
            if cfg!(feature = "stable-updates")
                && (release.prerelease
                    || !release
                        .body
                        .as_deref()
                        .unwrap_or_default()
                        .contains(STABLE_RELEASE_MARKER))
            {
                return None;
            }
            let version = Version::parse(release.tag_name.strip_prefix(TAG_PREFIX)?).ok()?;
            (!release.draft
                && version.pre.is_empty()
                && version.build.is_empty()
                && version > current)
                .then_some((version, release))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b));
    let Some((version, release)) = newest else {
        return Ok(UpdateCheck::Current);
    };
    let asset = platform
        .asset_name()
        .and_then(|name| release.assets.iter().find(|a| a.name == name))
        .cloned();
    if let Some(asset) = &asset {
        validate_asset(asset, &release.tag_name, platform)?;
    }
    Ok(UpdateCheck::Available(AvailableUpdate {
        version: version.to_string(),
        notes: release
            .body
            .as_deref()
            .unwrap_or_default()
            .replace(STABLE_RELEASE_MARKER, "")
            .trim()
            .to_string(),
        release_url: format!(
            "https://github.com/{REPOSITORY}/releases/tag/{}",
            release.tag_name
        ),
        tag: release.tag_name.clone(),
        asset,
    }))
}

fn validate_asset(asset: &Asset, tag: &str, platform: Platform) -> Result<()> {
    let version = Version::parse(tag.strip_prefix(TAG_PREFIX).context("Not a GPUI release")?)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "Not a stable GPUI version"
    );
    ensure!(
        Some(asset.name.as_str()) == platform.asset_name(),
        "Wrong platform update asset"
    );
    let expected = format!(
        "https://github.com/{REPOSITORY}/releases/download/{tag}/{}",
        asset.name
    );
    ensure!(
        asset.browser_download_url == expected,
        "Launcher update URL is not trusted"
    );
    ensure!(
        asset.size > 2 && asset.size <= MAX_INSTALLER_BYTES,
        "Invalid update asset size"
    );
    expected_digest(asset)?;
    Ok(())
}

fn expected_digest(asset: &Asset) -> Result<String> {
    let digest = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("Release asset has no SHA-256 checksum; open the release page or retry later")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid release SHA-256 checksum"
    );
    Ok(digest.to_ascii_lowercase())
}

/// An exclusive temporary directory removes partial files on errors/cancellation.
pub struct PreparedInstaller {
    directory: PathBuf,
    installer: PathBuf,
    handed_off: bool,
}

impl Drop for PreparedInstaller {
    fn drop(&mut self) {
        if !self.handed_off {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

pub async fn download_installer(
    update: &AvailableUpdate,
    progress: ProgressCallback,
) -> Result<PreparedInstaller> {
    let asset = update
        .asset
        .as_ref()
        .context("No Windows installer in this release")?;
    validate_asset(asset, &update.tag, Platform::WindowsX64)?;
    let directory =
        std::env::temp_dir().join(format!("industrialis-gpui-update-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir(&directory).await?;
    let prepared = PreparedInstaller {
        installer: directory.join(&asset.name),
        directory,
        handed_off: false,
    };
    let response = client()?
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?;
    write_verified_installer(response, &prepared.installer, asset, progress).await?;
    Ok(prepared)
}

async fn write_verified_installer(
    mut response: reqwest::Response,
    destination: &Path,
    asset: &Asset,
    progress: ProgressCallback,
) -> Result<()> {
    let expected = expected_digest(asset)?;
    ensure!(
        asset.size > 2 && asset.size <= MAX_INSTALLER_BYTES,
        "Invalid installer size"
    );
    if let Some(length) = response.content_length() {
        ensure!(
            length == asset.size,
            "Installer download size does not match the release"
        );
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .await?;
    let mut hash = Sha256::new();
    let mut header: Vec<u8> = Vec::new();
    let mut downloaded = 0;
    progress(DownloadProgress {
        downloaded,
        total: asset.size,
    });
    while let Some(chunk) = response.chunk().await? {
        downloaded += chunk.len() as u64;
        ensure!(
            downloaded <= asset.size,
            "Installer download is larger than expected"
        );
        if header.len() < 2 {
            header.extend(chunk.iter().take(2 - header.len()).copied());
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
        progress(DownloadProgress {
            downloaded,
            total: asset.size,
        });
    }
    ensure!(downloaded == asset.size, "Installer download is incomplete");
    ensure!(header == b"MZ", "Update asset is not a Windows executable");
    ensure!(
        format!("{:x}", hash.finalize()) == expected,
        "Installer failed SHA-256 verification"
    );
    file.sync_all().await?;
    Ok(())
}

#[derive(Deserialize)]
struct InstallMarker {
    format: u32,
    kind: String,
}

/// Portable/development builds never overwrite themselves or create an install.
pub fn installation_directory(executable: &Path, platform: Platform) -> Option<PathBuf> {
    if platform != Platform::WindowsX64 || executable.file_name()? != "industrialis.exe" {
        return None;
    }
    let directory = executable.parent()?;
    let metadata = std::fs::metadata(directory.join(INSTALL_MARKER)).ok()?;
    if metadata.len() > 1024 {
        return None;
    }
    let marker: InstallMarker =
        serde_json::from_slice(&std::fs::read(directory.join(INSTALL_MARKER)).ok()?).ok()?;
    (marker.format == 1 && marker.kind == "nsis").then(|| directory.to_path_buf())
}

impl PreparedInstaller {
    /// Launch the packaged NSIS installer. It waits for this PID to exit before
    /// replacing the executable, then restarts it. The app quits only on success.
    pub async fn hand_off(mut self, directory: &Path) -> Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let executable = std::env::current_exe()?;
            ensure!(
                installation_directory(&executable, Platform::current()).as_deref()
                    == Some(directory),
                "Not a GPUI installation directory"
            );
            ensure!(
                !directory.to_string_lossy().contains(['\r', '\n', '"']),
                "Invalid installation directory"
            );
            let mut command = std::process::Command::new(&self.installer);
            command.arg(format!("/UPDATEPID={}", std::process::id()));
            let ready = self.directory.join("installer-ready");
            command.arg(format!("/READYFILE={}", ready.display()));
            // NSIS /D must be last and unquoted; Rust's normal argument quoting
            // conflicts with its parser. raw_arg preserves spaces verbatim.
            command.raw_arg(format!("/D={}", directory.display()));
            command.creation_flags(0x00000008 | 0x00000200); // detached, new process group
            let mut child = command
                .spawn()
                .context("Could not start the launcher installer")?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                if tokio::fs::read(&ready).await.ok().as_deref() == Some(b"ready") {
                    break;
                }
                if let Some(status) = child.try_wait()? {
                    bail!("Launcher installer exited before handoff: {status}");
                }
                if tokio::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "Launcher installer did not confirm readiness; the launcher was kept open"
                    );
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            self.handed_off = true;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            bail!("Automatic launcher installation is only supported on Windows")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "Run scripts/test-gpui-windows-updater.ps1 with NSIS; uses isolated fixture executables"]
    async fn nsis_install_update_restart_and_uninstall() {
        use std::os::windows::process::CommandExt;
        let installer = PathBuf::from(
            std::env::var_os("GPUI_NSIS_SMOKE_INSTALLER").expect("Smoke installer required"),
        );
        if std::env::var_os("GPUI_NSIS_SMOKE_CHILD").is_some() {
            let failed_dir =
                std::env::temp_dir().join(format!("gpui-smoke-no-ack-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&failed_dir).unwrap();
            let failed = PreparedInstaller {
                installer: failed_dir.join("not-an-installer.exe"),
                directory: failed_dir.clone(),
                handed_off: false,
            };
            std::fs::copy(
                installer.parent().unwrap().join("new-launcher.exe"),
                &failed.installer,
            )
            .unwrap();
            let executable = std::env::current_exe().unwrap();
            assert!(
                failed.hand_off(executable.parent().unwrap()).await.is_err(),
                "No readiness acknowledgement must keep the launcher alive"
            );
            assert!(!failed_dir.exists());
            let dir =
                std::env::temp_dir().join(format!("gpui-smoke-handoff-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let prepared = PreparedInstaller {
                installer: dir.join("setup.exe"),
                directory: dir,
                handed_off: false,
            };
            std::fs::copy(installer, &prepared.installer).unwrap();
            let executable = std::env::current_exe().unwrap();
            prepared
                .hand_off(executable.parent().unwrap())
                .await
                .unwrap();
            return; // NSIS must wait for this test process to exit before replacing it.
        }
        let directory = installer
            .parent()
            .unwrap()
            .join("Installed launcher with spaces");
        assert!(!directory.exists(), "Smoke fixture must start empty");
        let mut initial = std::process::Command::new(&installer);
        initial
            .arg("/S")
            .raw_arg(format!("/D={}", directory.display()))
            .creation_flags(0x08000000);
        assert!(initial.status().unwrap().success());
        let executable = directory.join("industrialis.exe");
        assert_eq!(
            installation_directory(&executable, Platform::WindowsX64),
            Some(directory.clone())
        );
        let expected_new = std::fs::read(&executable).unwrap();
        // Simulate a running old version using this backend test binary. Its
        // child test invokes the real readiness/PID handoff implementation.
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        std::fs::write(directory.join("keep-user-data.txt"), b"preserve").unwrap();
        let result = std::process::Command::new(&executable)
            .args([
                "--ignored",
                "--exact",
                "release_updater::tests::nsis_install_update_restart_and_uninstall",
                "--nocapture",
            ])
            .env("GPUI_NSIS_SMOKE_CHILD", "1")
            .creation_flags(0x08000000)
            .status()
            .unwrap();
        assert!(result.success());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while !directory.join("restarted-version.txt").exists() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "Installer did not restart the updated executable"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(std::fs::read(&executable).unwrap(), expected_new);
        assert_eq!(
            std::fs::read_to_string(directory.join("restarted-version.txt")).unwrap(),
            "0.1.1"
        );
        assert!(!directory.join("industrialis.exe.previous").exists());
        let mut uninstall = std::process::Command::new(directory.join("Uninstall.exe"));
        uninstall.arg("/S").creation_flags(0x08000000);
        assert!(uninstall.status().unwrap().success());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while executable.exists() || directory.join(INSTALL_MARKER).exists() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "Uninstall did not finish"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(
            std::fs::read(directory.join("keep-user-data.txt")).unwrap(),
            b"preserve"
        );
    }

    fn release(version: &str, platform: Platform) -> Release {
        let tag = format!("{TAG_PREFIX}{version}");
        let name = platform.asset_name().unwrap();
        Release {
            tag_name: tag.clone(),
            draft: false,
            prerelease: false,
            body: Some(format!("{STABLE_RELEASE_MARKER}\nRelease notes")),
            assets: vec![Asset {
                name: name.into(),
                browser_download_url: format!(
                    "https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"
                ),
                size: 4,
                digest: Some(format!("sha256:{:x}", Sha256::digest(b"MZok"))),
            }],
        }
    }

    #[test]
    fn selects_newest_gpui_semver_ignoring_electron_drafts_and_unstable_tags() {
        let mut electron = release("9.0.0", Platform::WindowsX64);
        electron.tag_name = "launcher-v9.0.0".into();
        electron.body = None;
        let mut draft = release("8.0.0", Platform::WindowsX64);
        draft.draft = true;
        let releases = vec![
            electron,
            draft,
            release("0.2.0-beta.1", Platform::WindowsX64),
            release("0.1.10", Platform::WindowsX64),
            release("0.1.9", Platform::WindowsX64),
        ];
        let UpdateCheck::Available(update) =
            select_update(&releases, "0.1.8", Platform::WindowsX64).unwrap()
        else {
            panic!()
        };
        assert_eq!(update.version, "0.1.10");
        assert!(update.has_windows_installer());
        assert!(matches!(
            select_update(&releases, "0.1.10", Platform::WindowsX64).unwrap(),
            UpdateCheck::Current
        ));
        assert!(matches!(
            select_update(&releases, "1.0.0", Platform::WindowsX64).unwrap(),
            UpdateCheck::Current
        ));
    }

    #[test]
    fn wrong_architecture_is_manual_and_untrusted_urls_or_missing_digests_fail() {
        let releases = vec![release("0.2.0", Platform::WindowsX64)];
        let UpdateCheck::Available(update) =
            select_update(&releases, "0.1.0", Platform::MacArm64).unwrap()
        else {
            panic!()
        };
        assert!(!update.has_windows_installer());
        let mut release = release("0.2.0", Platform::WindowsX64);
        for url in ["http://github.com/x", "https://evil.example/setup.exe", "https://github.com/SergioJuniorCE/industrialis-launcher/releases/download/not-a-launcher-v0.2.0/IndustrialisLauncherSetup.exe"] {
            release.assets[0].browser_download_url = url.into();
            assert!(select_update(std::slice::from_ref(&release), "0.1.0", Platform::WindowsX64).is_err());
        }
        let mut release = self::release("0.2.0", Platform::WindowsX64);
        release.assets[0].digest = None;
        assert!(select_update(&[release], "0.1.0", Platform::WindowsX64).is_err());
    }

    #[test]
    fn release_channels_stay_separate_and_stable_never_reinstalls_electron() {
        let mut other_channel = release("9.0.0", Platform::WindowsX64);
        other_channel.tag_name = format!(
            "{}9.0.0",
            if cfg!(feature = "stable-updates") {
                "launcher-gpui-v"
            } else {
                "launcher-v"
            }
        );
        assert!(matches!(
            select_update(&[other_channel], "0.1.0", Platform::WindowsX64).unwrap(),
            UpdateCheck::Current
        ));
        if cfg!(feature = "stable-updates") {
            let mut electron = release("9.0.0", Platform::WindowsX64);
            electron.body = Some("Original Electron release".into());
            let mut preview = release("8.0.0", Platform::WindowsX64);
            preview.prerelease = true;
            assert!(matches!(
                select_update(&[electron, preview], "0.1.0", Platform::WindowsX64).unwrap(),
                UpdateCheck::Current
            ));
        }
        let UpdateCheck::Available(update) = select_update(
            &[release("0.2.0", Platform::WindowsX64)],
            "0.1.0",
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(update.notes, "Release notes");
    }

    #[test]
    fn installer_mode_requires_valid_marker_and_exact_executable() {
        let dir = std::env::temp_dir().join(format!("gpui-marker-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let exe = dir.join("industrialis.exe");
        assert!(installation_directory(&exe, Platform::WindowsX64).is_none());
        std::fs::write(dir.join(INSTALL_MARKER), r#"{"format":1,"kind":"nsis"}"#).unwrap();
        assert_eq!(
            installation_directory(&exe, Platform::WindowsX64),
            Some(dir.clone())
        );
        assert!(installation_directory(&exe, Platform::LinuxX64).is_none());
        assert!(installation_directory(&dir.join("other.exe"), Platform::WindowsX64).is_none());
        std::fs::write(dir.join(INSTALL_MARKER), r#"{"format":2,"kind":"nsis"}"#).unwrap();
        assert!(installation_directory(&exe, Platform::WindowsX64).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    async fn response(body: &[u8], advertised: usize) -> reqwest::Response {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_vec();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {advertised}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        });
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn verified_download_reports_progress_and_keeps_only_complete_executable() {
        let dir = std::env::temp_dir().join(format!("gpui-download-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let prepared = PreparedInstaller {
            installer: dir.join("setup.exe"),
            directory: dir.clone(),
            handed_off: false,
        };
        let asset = release("0.2.0", Platform::WindowsX64).assets.remove(0);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let emit = seen.clone();
        write_verified_installer(
            response(b"MZok", 4).await,
            &prepared.installer,
            &asset,
            Arc::new(move |p| emit.lock().unwrap().push(p)),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&prepared.installer).unwrap(), b"MZok");
        assert_eq!(seen.lock().unwrap().last().unwrap().downloaded, 4);
        drop(prepared);
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn corrupt_incomplete_and_non_executable_downloads_are_rejected_and_cleaned() {
        for (body, length, size, digest) in [
            (&b"MZno"[..], 4, 4, Sha256::digest(b"MZok")),
            (&b"MZ"[..], 4, 4, Sha256::digest(b"MZok")),
            (&b"HTML"[..], 4, 4, Sha256::digest(b"HTML")),
            (&b"MZok"[..], 4, 3, Sha256::digest(b"MZok")),
        ] {
            let dir =
                std::env::temp_dir().join(format!("gpui-download-fail-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let prepared = PreparedInstaller {
                installer: dir.join("setup.exe"),
                directory: dir.clone(),
                handed_off: false,
            };
            let mut asset = release("0.2.0", Platform::WindowsX64).assets.remove(0);
            asset.size = size;
            asset.digest = Some(format!("sha256:{digest:x}"));
            assert!(write_verified_installer(
                response(body, length).await,
                &prepared.installer,
                &asset,
                Arc::new(|_| {})
            )
            .await
            .is_err());
            drop(prepared);
            assert!(!dir.exists());
        }
    }
}
