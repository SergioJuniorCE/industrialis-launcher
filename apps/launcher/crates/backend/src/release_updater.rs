//! GitHub release checks and verified Windows installer handoff.
//! No Electron assets, arbitrary endpoints, or unverified executables are used.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use industrialis_core::launcher_settings::UpdateChannel;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

const REPOSITORY: &str = "SergioJuniorCE/industrialis-launcher";
pub const STABLE_TAG_PREFIX: &str = "launcher-v";
pub const PREVIEW_TAG_PREFIX: &str = "launcher-preview-v";
/// `launcher-nightly-v<major.minor.patch>-<YYYYMMDD>-<sha>`; the extra `-v`
/// keeps the tag outside the stable `launcher-v*` glob.
pub const NIGHTLY_TAG_PREFIX: &str = "launcher-nightly-v";
/// Release tag compiled into a distribution build by `build.rs`.
pub const BUILD_TAG_ENV: &str = "INDUSTRIALIS_BUILD_TAG";
pub const RELEASE_PAGE: &str = "https://github.com/SergioJuniorCE/industrialis-launcher/releases";
pub const STABLE_RELEASE_MARKER: &str = "<!-- industrialis-launcher-release-v1 -->";
const LEGACY_STABLE_RELEASE_MARKER: &str = "<!-- industrialis-gpui-release-v1 -->";
pub const INSTALL_MARKER: &str = "install.json";
const LEGACY_INSTALL_MARKER: &str = "gpui-install.json";
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

    fn asset_name(self, channel: UpdateChannel) -> Option<&'static str> {
        match self {
            Self::WindowsX64 => Some(match channel {
                UpdateChannel::Stable => "IndustrialisLauncherSetup.exe",
                UpdateChannel::Nightly => "IndustrialisLauncherNightlySetup-x64.exe",
                UpdateChannel::Preview => "IndustrialisLauncherPreviewSetup-x64.exe",
            }),
            Self::MacArm64 => Some("IndustrialisLauncher-macos-arm64.dmg"),
            Self::LinuxX64 => Some("IndustrialisLauncher-linux-x64.tar.gz"),
            Self::Unsupported => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    /// GitHub's UTC RFC 3339 timestamp; equal-length values sort chronologically.
    #[serde(default)]
    published_at: Option<String>,
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

/// Identity of a nightly build. Nightly tags carry a numeric version so the
/// installed product version stays valid for NSIS, and date plus commit so
/// builds order without semver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightlyTag {
    pub version: Version,
    pub date: u32,
    pub sha: String,
}

impl NightlyTag {
    pub fn parse(tag: &str) -> Option<Self> {
        let rest = tag.strip_prefix(NIGHTLY_TAG_PREFIX)?;
        let mut parts = rest.split('-');
        let version = Version::parse(parts.next()?).ok()?;
        if !version.pre.is_empty() || !version.build.is_empty() {
            return None;
        }
        let date = parts.next()?;
        if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let date: u32 = date.parse().ok()?;
        if !(1..=12).contains(&(date / 100 % 100)) || !(1..=31).contains(&(date % 100)) {
            return None;
        }
        let sha = parts.next()?;
        if parts.next().is_some()
            || !(7..=40).contains(&sha.len())
            || !sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return None;
        }
        Some(Self {
            version,
            date,
            sha: sha.to_string(),
        })
    }

    /// `YYYYMMDD` as `YYYY-MM-DD`.
    pub fn date_label(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}",
            self.date / 10_000,
            self.date / 100 % 100,
            self.date % 100
        )
    }
}

/// Channel this build follows when the person has not chosen one.
pub fn build_channel() -> UpdateChannel {
    if cfg!(feature = "stable-updates") {
        UpdateChannel::Stable
    } else {
        UpdateChannel::Preview
    }
}

/// The build this process is running: its numeric version and the release tag
/// the distribution workflow compiled into it.
#[derive(Debug, Clone)]
pub struct RunningBuild {
    pub version: Version,
    pub tag: Option<String>,
    nightly: Option<NightlyTag>,
}

impl RunningBuild {
    pub fn new(version: &str, tag: Option<&str>) -> Result<Self> {
        let version = Version::parse(version).context("Invalid installed launcher version")?;
        let tag = tag
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string);
        let nightly = tag.as_deref().and_then(NightlyTag::parse);
        Ok(Self {
            version,
            tag,
            nightly,
        })
    }

    pub fn channel(&self) -> UpdateChannel {
        match self.tag.as_deref() {
            Some(tag) if tag.starts_with(NIGHTLY_TAG_PREFIX) => UpdateChannel::Nightly,
            Some(tag) if tag.starts_with(STABLE_TAG_PREFIX) => UpdateChannel::Stable,
            Some(tag) if tag.starts_with(PREVIEW_TAG_PREFIX) => UpdateChannel::Preview,
            _ => build_channel(),
        }
    }

    pub fn nightly(&self) -> Option<&NightlyTag> {
        self.nightly.as_ref()
    }

    /// Human-readable identity for Settings and the update dialog.
    pub fn label(&self) -> String {
        match (&self.nightly, self.tag.is_some()) {
            (Some(nightly), _) => format!("Nightly build · {} · {}", nightly.date_label(), nightly.sha),
            (None, true) => match self.channel() {
                UpdateChannel::Stable => "Stable release".into(),
                UpdateChannel::Preview => "Preview release".into(),
                UpdateChannel::Nightly => "Nightly build".into(),
            },
            (None, false) => "Local build".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub channel: UpdateChannel,
    /// The newest build in the selected channel is not newer than this one;
    /// installing it is what moves the installation onto that channel.
    pub channel_switch: bool,
    /// The offered build is numerically older than the running one.
    pub is_downgrade: bool,
    /// The offered build carries the same numeric version as the running one.
    pub same_version: bool,
    pub notes: String,
    pub release_url: String,
    pub tag: String,
    pub nightly: Option<NightlyTag>,
    asset: Option<Asset>,
}

#[derive(Debug, Clone)]
pub struct ReleaseNote {
    pub label: String,
    pub notes: String,
    pub release_url: String,
}

impl AvailableUpdate {
    pub fn has_windows_installer(&self) -> bool {
        self.asset.as_ref().is_some_and(|a| {
            Some(a.name.as_str()) == Platform::WindowsX64.asset_name(self.channel)
        })
    }

    /// `v0.5.0` or `Nightly 2026-11-25 · v0.5.0`.
    pub fn label(&self) -> String {
        match &self.nightly {
            Some(nightly) => format!("Nightly {} · v{}", nightly.date_label(), self.version),
            None => format!("v{}", self.version),
        }
    }

    /// Compact form for the footer status.
    pub fn summary(&self) -> String {
        match (&self.nightly, self.channel_switch) {
            (Some(nightly), _) => format!("nightly {}", nightly.date_label()),
            (None, true) => format!("{} v{}", self.channel.name(), self.version),
            (None, false) => format!("v{}", self.version),
        }
    }
}

#[derive(Debug, Clone)]
pub enum UpdateCheck {
    Current,
    /// Nothing has been published on the selected channel yet.
    NoReleases,
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
        .user_agent("Industrialis-Launcher-Updater")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()?)
}

async fn fetch_releases() -> Result<Vec<Release>> {
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
            return Ok(releases);
        }
    }
    bail!("Too many release pages; open the launcher releases page to check manually")
}

pub async fn check_for_update(
    build: &RunningBuild,
    channel: UpdateChannel,
    platform: Platform,
) -> Result<UpdateCheck> {
    let releases = fetch_releases().await?;
    select_update(&releases, build, channel, platform)
}

pub async fn recent_releases(channel: UpdateChannel, limit: usize) -> Result<Vec<ReleaseNote>> {
    let releases = fetch_releases().await?;
    Ok(select_changelog(&releases, channel, limit))
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

type Candidate<'a> = (&'a Release, Version, Option<NightlyTag>);

fn release_identity(tag: &str, channel: UpdateChannel) -> Option<(Version, Option<NightlyTag>)> {
    if channel == UpdateChannel::Nightly {
        return NightlyTag::parse(tag).map(|nightly| (nightly.version.clone(), Some(nightly)));
    }
    let prefix = match channel {
        UpdateChannel::Stable => STABLE_TAG_PREFIX,
        UpdateChannel::Preview => PREVIEW_TAG_PREFIX,
        UpdateChannel::Nightly => unreachable!(),
    };
    let version = Version::parse(tag.strip_prefix(prefix)?).ok()?;
    (version.pre.is_empty() && version.build.is_empty()).then_some((version, None))
}

/// A release belongs to exactly one channel, decided by its tag. Stable also
/// requires the release marker so unrelated products with similar tags and
/// GitHub prereleases never reach stable installs.
fn channel_candidate(release: &Release, channel: UpdateChannel) -> Option<(Version, Option<NightlyTag>)> {
    if release.draft {
        return None;
    }
    if channel == UpdateChannel::Stable
        && (release.prerelease
            || !release.body.as_deref().is_some_and(|body| {
                body.contains(STABLE_RELEASE_MARKER) || body.contains(LEGACY_STABLE_RELEASE_MARKER)
            }))
    {
        return None;
    }
    release_identity(&release.tag_name, channel)
}

fn published_at(release: &Release) -> &str {
    release.published_at.as_deref().unwrap_or_default()
}

/// Nightly builds order by tag date, then publication time, then commit,
/// because their tags are not semver and two builds can share a date.
fn compare(channel: UpdateChannel, a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    match (channel, &a.2, &b.2) {
        (UpdateChannel::Nightly, Some(a_nightly), Some(b_nightly)) => {
            (a_nightly.date, published_at(a.0), a_nightly.sha.as_str()).cmp(&(
                b_nightly.date,
                published_at(b.0),
                b_nightly.sha.as_str(),
            ))
        }
        _ => a.1.cmp(&b.1),
    }
}

fn notes(body: Option<&str>) -> String {
    body.unwrap_or_default()
        .replace(STABLE_RELEASE_MARKER, "")
        .replace(LEGACY_STABLE_RELEASE_MARKER, "")
        .trim()
        .to_string()
}

fn release_url(tag: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/tag/{tag}")
}

fn select_update(
    releases: &[Release],
    build: &RunningBuild,
    channel: UpdateChannel,
    platform: Platform,
) -> Result<UpdateCheck> {
    let mut newest: Option<Candidate> = None;
    for release in releases {
        let Some((version, nightly)) = channel_candidate(release, channel) else {
            continue;
        };
        let candidate = (release, version, nightly);
        if newest
            .as_ref()
            .is_none_or(|best| compare(channel, &candidate, best).is_gt())
        {
            newest = Some(candidate);
        }
    }
    let Some((release, version, nightly)) = newest else {
        return Ok(UpdateCheck::NoReleases);
    };
    let channel_switch = build.channel() != channel;
    let offer = if channel_switch {
        true
    } else {
        match (channel, &nightly) {
            (UpdateChannel::Nightly, Some(candidate)) => match build.nightly() {
                Some(_) if build.tag.as_deref() == Some(release.tag_name.as_str()) => false,
                // A deleted newer nightly must not pull the install backwards.
                Some(running) => candidate.date >= running.date,
                None => true,
            },
            _ => version > build.version,
        }
    };
    if !offer {
        return Ok(UpdateCheck::Current);
    }
    let asset = platform
        .asset_name(channel)
        .and_then(|name| release.assets.iter().find(|a| a.name == name))
        .cloned();
    if let Some(asset) = &asset {
        validate_asset(asset, &release.tag_name, channel, platform)?;
    }
    Ok(UpdateCheck::Available(AvailableUpdate {
        version: version.to_string(),
        channel,
        channel_switch,
        is_downgrade: version < build.version,
        same_version: version == build.version,
        notes: notes(release.body.as_deref()),
        release_url: release_url(&release.tag_name),
        tag: release.tag_name.clone(),
        nightly,
        asset,
    }))
}

fn select_changelog(releases: &[Release], channel: UpdateChannel, limit: usize) -> Vec<ReleaseNote> {
    let mut candidates: Vec<Candidate> = releases
        .iter()
        .filter_map(|release| {
            let (version, nightly) = channel_candidate(release, channel)?;
            Some((release, version, nightly))
        })
        .collect();
    candidates.sort_by(|a, b| compare(channel, b, a));
    candidates
        .into_iter()
        .take(limit)
        .map(|(release, version, nightly)| ReleaseNote {
            label: match &nightly {
                Some(nightly) => format!("Nightly {} · v{}", nightly.date_label(), version),
                None => format!("v{version}"),
            },
            notes: notes(release.body.as_deref()),
            release_url: release_url(&release.tag_name),
        })
        .collect()
}

fn validate_asset(
    asset: &Asset,
    tag: &str,
    channel: UpdateChannel,
    platform: Platform,
) -> Result<()> {
    ensure!(
        release_identity(tag, channel).is_some(),
        "Not a {} launcher release",
        channel.name()
    );
    ensure!(
        Some(asset.name.as_str()) == platform.asset_name(channel),
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
    validate_asset(asset, &update.tag, update.channel, Platform::WindowsX64)?;
    let directory =
        std::env::temp_dir().join(format!("industrialis-launcher-update-{}", uuid::Uuid::new_v4()));
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
    let marker_path = [INSTALL_MARKER, LEGACY_INSTALL_MARKER]
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())?;
    let metadata = std::fs::metadata(&marker_path).ok()?;
    if metadata.len() > 1024 {
        return None;
    }
    let marker: InstallMarker = serde_json::from_slice(&std::fs::read(&marker_path).ok()?).ok()?;
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
                "Not a launcher installation directory"
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
    #[ignore = "Run scripts/test-windows-updater.ps1 with NSIS; uses isolated fixture executables"]
    async fn nsis_install_update_restart_and_uninstall() {
        use std::os::windows::process::CommandExt;
        let installer = PathBuf::from(
            std::env::var_os("LAUNCHER_NSIS_SMOKE_INSTALLER").expect("Smoke installer required"),
        );
        if std::env::var_os("LAUNCHER_NSIS_SMOKE_CHILD").is_some() {
            let failed_dir =
                std::env::temp_dir().join(format!("launcher-smoke-no-ack-{}", uuid::Uuid::new_v4()));
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
                std::env::temp_dir().join(format!("launcher-smoke-handoff-{}", uuid::Uuid::new_v4()));
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
            .env("LAUNCHER_NSIS_SMOKE_CHILD", "1")
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

    fn tagged_release(tag: &str, asset: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            draft: false,
            prerelease: false,
            published_at: Some("2026-11-25T09:00:00Z".into()),
            body: Some(format!("{STABLE_RELEASE_MARKER}\nRelease notes")),
            assets: vec![Asset {
                name: asset.into(),
                browser_download_url: format!(
                    "https://github.com/{REPOSITORY}/releases/download/{tag}/{asset}"
                ),
                size: 4,
                digest: Some(format!("sha256:{:x}", Sha256::digest(b"MZok"))),
            }],
        }
    }

    fn release(channel: UpdateChannel, version: &str, platform: Platform) -> Release {
        let tag = match channel {
            UpdateChannel::Stable => format!("{STABLE_TAG_PREFIX}{version}"),
            UpdateChannel::Preview => format!("{PREVIEW_TAG_PREFIX}{version}"),
            UpdateChannel::Nightly => unreachable!("nightly tags carry a date and a commit"),
        };
        let asset = platform.asset_name(channel).unwrap();
        tagged_release(&tag, asset)
    }

    fn nightly_release(version: &str, date: u32, sha: &str, platform: Platform) -> Release {
        let tag = format!("{NIGHTLY_TAG_PREFIX}{version}-{date}-{sha}");
        let asset = platform.asset_name(UpdateChannel::Nightly).unwrap();
        tagged_release(&tag, asset)
    }

    fn build(version: &str, channel: UpdateChannel) -> RunningBuild {
        let tag = match channel {
            UpdateChannel::Stable => format!("{STABLE_TAG_PREFIX}{version}"),
            UpdateChannel::Preview => format!("{PREVIEW_TAG_PREFIX}{version}"),
            UpdateChannel::Nightly => format!("{NIGHTLY_TAG_PREFIX}{version}-20261120-abcdef0"),
        };
        RunningBuild::new(version, Some(&tag)).unwrap()
    }

    #[test]
    fn stable_selects_only_marked_stable_releases() {
        let mut electron = release(UpdateChannel::Stable, "9.0.0", Platform::WindowsX64);
        electron.body = None;
        let mut draft = release(UpdateChannel::Stable, "8.0.0", Platform::WindowsX64);
        draft.draft = true;
        let mut flagged = release(UpdateChannel::Stable, "7.0.0", Platform::WindowsX64);
        flagged.prerelease = true;
        let mut unstable = release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64);
        unstable.tag_name = format!("{STABLE_TAG_PREFIX}0.2.0-beta.1");
        let releases = vec![
            electron,
            draft,
            flagged,
            unstable,
            release(UpdateChannel::Stable, "0.1.10", Platform::WindowsX64),
            release(UpdateChannel::Stable, "0.1.9", Platform::WindowsX64),
        ];
        let UpdateCheck::Available(update) = select_update(
            &releases,
            &build("0.1.8", UpdateChannel::Stable),
            UpdateChannel::Stable,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(update.version, "0.1.10");
        assert_eq!(update.channel, UpdateChannel::Stable);
        assert!(!update.channel_switch);
        assert!(update.has_windows_installer());
        assert_eq!(update.notes, "Release notes");
        for current in ["0.1.10", "1.0.0"] {
            assert!(matches!(
                select_update(
                    &releases,
                    &build(current, UpdateChannel::Stable),
                    UpdateChannel::Stable,
                    Platform::WindowsX64
                )
                .unwrap(),
                UpdateCheck::Current
            ));
        }
    }

    #[test]
    fn stable_ignores_newer_nightlies_and_previews() {
        let releases = vec![
            nightly_release("9.0.0", 20991231, "fffffff", Platform::WindowsX64),
            release(UpdateChannel::Preview, "8.0.0", Platform::WindowsX64),
            release(UpdateChannel::Stable, "0.3.0", Platform::WindowsX64),
        ];
        let UpdateCheck::Available(update) = select_update(
            &releases,
            &build("0.2.0", UpdateChannel::Stable),
            UpdateChannel::Stable,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(update.version, "0.3.0");
        assert_eq!(update.channel, UpdateChannel::Stable);
        assert!(!update.channel_switch);
        assert!(matches!(
            select_update(
                &releases,
                &build("0.3.0", UpdateChannel::Stable),
                UpdateChannel::Stable,
                Platform::WindowsX64
            )
            .unwrap(),
            UpdateCheck::Current
        ));
    }

    #[test]
    fn preview_channel_keeps_selecting_preview_assets() {
        let releases = vec![
            release(UpdateChannel::Preview, "0.2.0", Platform::WindowsX64),
            release(UpdateChannel::Stable, "0.9.0", Platform::WindowsX64),
        ];
        let UpdateCheck::Available(update) = select_update(
            &releases,
            &build("0.1.0", UpdateChannel::Preview),
            UpdateChannel::Preview,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(update.version, "0.2.0");
        assert_eq!(update.channel, UpdateChannel::Preview);
        assert!(update.tag.starts_with(PREVIEW_TAG_PREFIX));
        assert!(update.has_windows_installer());
    }

    #[test]
    fn nightly_channel_never_falls_back_to_stable() {
        let stable_only = vec![release(UpdateChannel::Stable, "9.0.0", Platform::WindowsX64)];
        assert!(matches!(
            select_update(
                &stable_only,
                &build("0.5.0", UpdateChannel::Nightly),
                UpdateChannel::Nightly,
                Platform::WindowsX64
            )
            .unwrap(),
            UpdateCheck::NoReleases
        ));
        let newest = nightly_release("0.5.0", 20261125, "abc1234", Platform::WindowsX64);
        let running = RunningBuild::new("0.5.0", Some(&newest.tag_name)).unwrap();
        assert!(matches!(
            select_update(&[newest], &running, UpdateChannel::Nightly, Platform::WindowsX64).unwrap(),
            UpdateCheck::Current
        ));
    }

    #[test]
    fn equal_version_channel_switch_is_offered_both_ways() {
        let stable = release(UpdateChannel::Stable, "0.5.0", Platform::WindowsX64);
        let UpdateCheck::Available(to_stable) = select_update(
            std::slice::from_ref(&stable),
            &build("0.5.0", UpdateChannel::Nightly),
            UpdateChannel::Stable,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(to_stable.version, "0.5.0");
        assert_eq!(to_stable.channel, UpdateChannel::Stable);
        assert!(to_stable.channel_switch);
        assert!(to_stable.same_version);
        assert!(!to_stable.is_downgrade);
        assert!(to_stable.has_windows_installer());

        let nightly = nightly_release("0.5.0", 20261125, "abc1234", Platform::WindowsX64);
        let UpdateCheck::Available(to_nightly) = select_update(
            std::slice::from_ref(&nightly),
            &build("0.5.0", UpdateChannel::Stable),
            UpdateChannel::Nightly,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(to_nightly.channel, UpdateChannel::Nightly);
        assert!(to_nightly.channel_switch);
        assert!(to_nightly.has_windows_installer());
        assert_eq!(to_nightly.label(), "Nightly 2026-11-25 · v0.5.0");

        assert!(matches!(
            select_update(
                &[stable],
                &build("0.5.0", UpdateChannel::Stable),
                UpdateChannel::Stable,
                Platform::WindowsX64
            )
            .unwrap(),
            UpdateCheck::Current
        ));
    }

    #[test]
    fn switching_back_to_stable_offers_an_older_release_as_a_downgrade() {
        let older = release(UpdateChannel::Stable, "0.4.9", Platform::WindowsX64);
        let UpdateCheck::Available(update) = select_update(
            &[older],
            &build("0.5.0", UpdateChannel::Nightly),
            UpdateChannel::Stable,
            Platform::WindowsX64,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(update.version, "0.4.9");
        assert!(update.channel_switch);
        assert!(update.is_downgrade);
        assert!(!update.same_version);
        assert_eq!(update.summary(), "stable v0.4.9");
    }

    #[test]
    fn nightly_orders_by_date_then_publication_then_commit() {
        let selected = |releases: &[Release]| {
            match select_update(
                releases,
                &build("0.4.0", UpdateChannel::Nightly),
                UpdateChannel::Nightly,
                Platform::WindowsX64,
            )
            .unwrap()
            {
                UpdateCheck::Available(update) => update.tag,
                other => panic!("expected a nightly update: {other:?}"),
            }
        };
        let older = nightly_release("0.5.0", 20261124, "fffffff", Platform::WindowsX64);
        let mut later = nightly_release("0.5.0", 20261125, "bbbbbbb", Platform::WindowsX64);
        later.published_at = Some("2026-11-25T12:00:00Z".into());
        let mut earlier = nightly_release("0.5.0", 20261125, "aaaaaaa", Platform::WindowsX64);
        earlier.published_at = Some("2026-11-25T09:00:00Z".into());
        assert_eq!(
            selected(&[earlier.clone(), older.clone(), later.clone()]),
            format!("{NIGHTLY_TAG_PREFIX}0.5.0-20261125-bbbbbbb")
        );
        earlier.published_at = None;
        later.published_at = None;
        assert_eq!(
            selected(&[earlier, later]),
            format!("{NIGHTLY_TAG_PREFIX}0.5.0-20261125-bbbbbbb")
        );
        assert_eq!(
            selected(&[
                older,
                nightly_release("0.5.0", 20261126, "0000000", Platform::WindowsX64)
            ]),
            format!("{NIGHTLY_TAG_PREFIX}0.5.0-20261126-0000000")
        );
    }

    #[test]
    fn nightly_tag_parsing_rejects_other_shapes() {
        let nightly = NightlyTag::parse("launcher-nightly-v0.5.7-20261125-abc1234").unwrap();
        assert_eq!(nightly.version.to_string(), "0.5.7");
        assert_eq!(nightly.date, 20261125);
        assert_eq!(nightly.date_label(), "2026-11-25");
        assert_eq!(nightly.sha, "abc1234");
        for tag in [
            "launcher-nightly-v0.5.7-20261125",
            "launcher-nightly-v0.5.7-beta-20261125-abc1234",
            "launcher-nightly-v0.5.7-2026112-abc1234",
            "launcher-nightly-v0.5.7-20261325-abc1234",
            "launcher-nightly-v0.5.7-20261125-ABC1234",
            "launcher-nightly-v0.5.7-20261125-abc",
            "launcher-nightly-v0.5.7-20261125-abc1234-extra",
            "launcher-nightly-0.5.7-20261125-abc1234",
            "launcher-nightly-v0.5.7",
            "launcher-v0.5.7",
            "launcher-preview-v0.5.7",
        ] {
            assert!(NightlyTag::parse(tag).is_none(), "{tag}");
        }
    }

    #[test]
    fn running_build_reports_its_channel_and_label() {
        assert_eq!(
            build("0.5.0", UpdateChannel::Stable).channel(),
            UpdateChannel::Stable
        );
        assert_eq!(
            build("0.5.0", UpdateChannel::Preview).channel(),
            UpdateChannel::Preview
        );
        assert_eq!(
            build("0.5.0", UpdateChannel::Nightly).channel(),
            UpdateChannel::Nightly
        );
        assert_eq!(
            build("0.5.0", UpdateChannel::Nightly).label(),
            "Nightly build · 2026-11-20 · abcdef0"
        );
        assert_eq!(
            build("0.5.0", UpdateChannel::Stable).label(),
            "Stable release"
        );
        let untagged = RunningBuild::new("0.5.0", None).unwrap();
        assert_eq!(untagged.channel(), build_channel());
        assert_eq!(untagged.label(), "Local build");
        assert!(RunningBuild::new("not-a-version", None).is_err());
    }

    #[test]
    fn changelog_follows_the_selected_channel_and_strips_marker() {
        let mut draft = release(UpdateChannel::Stable, "9.0.0", Platform::WindowsX64);
        draft.draft = true;
        let mut electron = release(UpdateChannel::Stable, "8.0.0", Platform::WindowsX64);
        electron.body = Some("Original Electron release".into());
        let mut legacy = release(UpdateChannel::Stable, "0.0.9", Platform::WindowsX64);
        legacy.body = Some(format!("{LEGACY_STABLE_RELEASE_MARKER}\nLegacy notes"));
        let mut unstable = release(UpdateChannel::Stable, "0.3.0", Platform::WindowsX64);
        unstable.tag_name = format!("{STABLE_TAG_PREFIX}0.3.0-beta.1");
        let nightly = nightly_release("0.5.0", 20261125, "abc1234", Platform::WindowsX64);
        let releases = vec![
            draft,
            electron,
            legacy,
            nightly,
            unstable,
            release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64),
            release(UpdateChannel::Stable, "0.1.0", Platform::WindowsX64),
        ];
        let notes = select_changelog(&releases, UpdateChannel::Stable, 3);
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].label, "v0.2.0");
        assert_eq!(notes[1].label, "v0.1.0");
        assert_eq!(notes[2].label, "v0.0.9");
        assert_eq!(notes[0].notes, "Release notes");
        assert_eq!(notes[2].notes, "Legacy notes");
        assert!(notes[0].release_url.ends_with("/launcher-v0.2.0"));
        let nightlies = select_changelog(&releases, UpdateChannel::Nightly, 3);
        assert_eq!(nightlies.len(), 1);
        assert_eq!(nightlies[0].label, "Nightly 2026-11-25 · v0.5.0");
        assert!(nightlies[0]
            .release_url
            .ends_with("/launcher-nightly-v0.5.0-20261125-abc1234"));
    }

    #[test]
    fn wrong_architecture_is_manual_and_untrusted_urls_or_missing_digests_fail() {
        let stable = build("0.1.0", UpdateChannel::Stable);
        let releases = vec![release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64)];
        let UpdateCheck::Available(update) =
            select_update(&releases, &stable, UpdateChannel::Stable, Platform::MacArm64).unwrap()
        else {
            panic!()
        };
        assert!(!update.has_windows_installer());
        let mut release = release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64);
        for url in ["http://github.com/x", "https://evil.example/setup.exe", "https://github.com/SergioJuniorCE/industrialis-launcher/releases/download/not-a-launcher-v0.2.0/IndustrialisLauncherSetup.exe"] {
            release.assets[0].browser_download_url = url.into();
            assert!(select_update(std::slice::from_ref(&release), &stable, UpdateChannel::Stable, Platform::WindowsX64).is_err());
        }
        let mut release = self::release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64);
        release.assets[0].digest = None;
        assert!(select_update(&[release], &stable, UpdateChannel::Stable, Platform::WindowsX64).is_err());
    }

    #[test]
    fn installer_mode_requires_valid_marker_and_exact_executable() {
        let dir = std::env::temp_dir().join(format!("launcher-marker-test-{}", uuid::Uuid::new_v4()));
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
        std::fs::remove_file(dir.join(INSTALL_MARKER)).unwrap();
        std::fs::write(
            dir.join(LEGACY_INSTALL_MARKER),
            r#"{"format":1,"kind":"nsis"}"#,
        )
        .unwrap();
        assert_eq!(
            installation_directory(&exe, Platform::WindowsX64),
            Some(dir.clone())
        );
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
        let dir = std::env::temp_dir().join(format!("launcher-download-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let prepared = PreparedInstaller {
            installer: dir.join("setup.exe"),
            directory: dir.clone(),
            handed_off: false,
        };
        let asset = release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64).assets.remove(0);
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
                std::env::temp_dir().join(format!("launcher-download-fail-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&dir).unwrap();
            let prepared = PreparedInstaller {
                installer: dir.join("setup.exe"),
                directory: dir.clone(),
                handed_off: false,
            };
            let mut asset = release(UpdateChannel::Stable, "0.2.0", Platform::WindowsX64).assets.remove(0);
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
