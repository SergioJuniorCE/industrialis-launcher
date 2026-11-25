# Launcher updates and releases

Launcher self-updates are separate from instance pack updates. Distribution
builds check GitHub asynchronously at startup. Settings → Updates offers the
channel choice, a manual check, status, retry, and the release page; the toolbar
shows an update action when a newer version is available or a check failed. The
status bar also reports the launcher update state (`Launcher up to date`, `Update
available`, checking, downloading, or failed), and selecting it opens the update
dialog. Settings → Updates and the update dialog include a changelog viewer with
recent published release notes. Updates install only after the user confirms
**Download and restart** in the launcher update dialog.

## Update channels

The launcher follows one of three release channels, chosen in Settings → Updates
and stored as `update_channel` in `launcher-settings.json` (`stable`, `nightly`
or `preview`). With no stored choice, the build follows the channel it was
compiled for: stable release builds follow stable, preview builds follow preview,
nightly builds follow nightly. Stable is the product default.

The 0.4.0 cutover was authorized on 2026-10-17 with the remaining provider,
platform and published-update checks accepted as documented follow-ups.
See [release notes](RELEASE_NOTES.md) and [cutover status](CUTOVER.md).

Each channel is identified by its tag prefix, so one binary can follow any of
them:

| Channel | Tag                                       | Published as             | Windows installer                          |
| ------- | ----------------------------------------- | ------------------------ | ------------------------------------------ |
| Stable  | `launcher-vX.Y.Z`                         | release, latest          | `IndustrialisLauncherSetup.exe`            |
| Nightly | `launcher-nightly-vX.Y.Z-YYYYMMDD-<sha7>` | prerelease, never latest | `IndustrialisLauncherNightlySetup-x64.exe` |
| Preview | `launcher-preview-vX.Y.Z`                 | prerelease, never latest | `IndustrialisLauncherPreviewSetup-x64.exe` |

The production release workflow is `.github/workflows/launcher-release.yml`.
Stable `launcher-vX.Y.Z` tags publish a regular GitHub Release and make it
latest; release bodies carry `<!-- industrialis-launcher-release-v1 -->` and the
updater rejects unmarked historical artifacts. Optional `launcher-preview-vX.Y.Z`
tags publish prereleases with `make_latest: false`. Existing profile data stays in
its original location.

The three-platform build matrix runs only for release tags and manual
`workflow_dispatch`; pull requests do not trigger builds. Tag pushes alone
publish releases.

Development builds omit the `packaged-updates` feature and perform no launcher
update network requests. Packaging enables the feature. Portable Windows builds
still check versions, but open the release page for manual replacement.

Selection is strict. Stable admits only `launcher-v` tags that are neither draft
nor prerelease and whose body carries the stable marker (or the legacy marker),
so Electron releases and GitHub prereleases never reach stable installs. Nightly
admits only `launcher-nightly-v` tags with a numeric version, an eight-digit UTC
build date and a lowercase commit; the extra `-v` keeps the tag outside the
`launcher-v*` release-workflow glob. A nightly install never falls back to stable:
with no nightly published it reports that and stays where it is. Preview keeps its
tags and asset name, and Settings offers it only for preview builds or installs
already on preview.

Changing the channel persists immediately, applies to the next check, and
discards an update already offered on the old channel, so a cached artifact is
never installed after a switch. Inside stable and preview the updater compares
semantic versions and needs a strictly newer release. Inside nightly it compares
the tag's build date, then `published_at`, then the commit, and never re-offers
the build that is already running; if a newer nightly was deleted, an older one
does not pull the install backwards. Builds that agree on date, publication second
and commit are the same build for ordering.

### Switching back to stable

The numeric version cannot decide a channel switch, because a nightly built after
the 0.5.0 release still reports 0.5.0. When the running build's channel differs
from the chosen one, the updater offers the newest build of the chosen channel
even when its version is equal to or older than the running one. The update
dialog says which case applies (same version number, downgrade, or newer) and the
install goes through the normal verified handoff. The reinstall ends the switch:
the new build reports the chosen channel, so the offer does not come back.
Downgrading is mechanically safe. NSIS never compares versions, renames the old
executable aside until the new one is extracted, restores it on failure and keeps
`install.json` unchanged, so the marker check still accepts the installation.
Instances, worlds, accounts and settings are never touched; launcher preferences
written by the newer build stay in `launcher-settings.json` as unknown fields.

### Nightly builds

`launcher-nightly.yml` builds only when a commit landed since the last successful
nightly, then publishes one GitHub prerelease with `prerelease: true` and
`make_latest: false`. A prerelease can never become latest on GitHub, and the
stable channel filters by tag regardless. Nightly identity lives in the tag and
the release name (`Nightly 0.5.0 · 2026-11-26 · <sha7>`); the artifact version
stays numeric because NSIS `VIProductVersion`, the Windows file version and the
updater's version comparison all need three integers.
`scripts/launcher-version.mjs` keeps rejecting prerelease and build suffixes.
The release tag is compiled into distribution builds
(`INDUSTRIALIS_BUILD_TAG`), which is how the running launcher reports its own
identity: Settings → Updates and About show `Stable release` or
`Nightly build · 2026-11-26 · <sha7>`, and untagged local builds show
`Local build`.

The publish job keeps the newest 30 nightly releases and deletes older nightly
releases with their tags, because the updater pages the release list in ten
requests of 100 and fails past that bound.

If a nightly will not start, install the stable build from the releases page; the
installer replaces the executable in place and leaves all launcher data alone.
Switching the channel back to Stable from a running nightly also works and offers
the newest stable build on the next check.

## Windows installation

The per-user NSIS installer installs `industrialis.exe` and `install.json`
in `%LOCALAPPDATA%\Programs\Industrialis Launcher` by default. Existing native
installations keep their registered install directory when upgraded; the retired
codename folder is replaced by the current default and removed. The marker
identifies an installation that the updater may replace; it is omitted from
portable ZIPs. Every channel installs into the same directory, shortcut and
uninstall entry, so a channel switch replaces the existing installation instead
of adding a second one.

`node scripts/build-launcher.mjs --replacement` also registers the retained
`industrialislauncher:` scheme. Default preview artifacts omit that registration.
Windows saves the previous handler and restores it on uninstall while the launcher still
owns the command; another application's later registration is kept. macOS embeds
CFBundleURLTypes, and the Linux desktop entry declares the scheme for installation
with `xdg-mime`. These registration paths require native installed acceptance on
each platform; normal preview builds do not change the protocol handler.

An in-app update from the retired `%LOCALAPPDATA%\Programs\Industrialis GPUI`
directory is recognized by the old marker there even though the installer moves
the files to the current directory. Running games do not block the update: the
detached broker can keep the old executable mapped, the installer tolerates the
locked copy, and the current launcher reattaches to the running game. The
retired folder can remain until that game exits.

The stable Windows installer extracts the new launcher executable, retaining an
existing launcher executable for rollback, and restarts the installed launcher.
A failed extraction restores the previous executable. The launcher installer
never imports, moves or recursively deletes the launcher profile.

For an installed Windows x64 build:

1. Finish file/account operations and settings saves. Running games stay open:
   they are supervised by a detached broker that the installer ignores.
2. Confirm the launcher update. The dialog blocks background input and closing
   the launcher during download/handoff. Busy state is checked again after the
   download, before installation.
3. Download the exact `IndustrialisLauncherPreviewSetup-x64.exe` preview asset, or
   `IndustrialisLauncherSetup.exe` for a marked stable release, from the matching
   tag in this repository. Require its GitHub SHA-256 digest, enforce the
   declared size and 512 MiB cap, verify the executable header, and remove
   partial/corrupt downloads. Missing checksums fail visibly.
4. Start the installer with the old launcher PID, the current install directory,
   and an exclusive temporary readiness path. Keep the launcher open unless
   the installer acknowledges readiness within ten seconds.
5. The installer holds a handle to that PID and waits up to thirty seconds for
   exit before replacing the executable. It retains a rollback executable
   while extracting the new file, then restarts the installed launcher.

Installer arguments are passed directly, without a shell. NSIS's final `/D=`
argument is unquoted as its parser requires, including directories with spaces.
Failed downloads/handoffs clean their temporary directory. Successful handoffs
retain the temporary installer for the running installer process; it remains
in the OS temporary directory afterward.

The installer/uninstaller only manages its own application files, shortcut and
registry keys. The launcher does not recursively delete the install directory or
touch the launcher data directory, instances, saves, accounts, or preferences.

All artifacts include the project license and generated third-party
notices from the locked Cargo app graph. Supplied license/notice texts and declared
license expressions are retained; the report excludes unrelated workspace packages
and machine/profile paths. Native code-signing and distribution review remain
release gates.

Windows distribution builds link the MSVC CRT statically while retaining existing
compiler flags. Packaging and artifact verification inspect the actual x64 PE
ordinary and delay imports and reject external MSVC runtime DLLs. This requirement
also applies to `--skip-build`: a developer-machine executable that depends on
`VCRUNTIME140.dll` cannot be packaged accidentally. Development builds keep their
normal compiler settings. Clean-Windows acceptance runs without installing the
Visual C++ redistributable; OS libraries remain platform requirements.

## Artifacts and versioning

| Platform             | Artifact                                   | Launcher update installation |
| -------------------- | ------------------------------------------ | ---------------------------- |
| Windows x64 stable   | `IndustrialisLauncherSetup.exe`            | In-app installer handoff     |
| Windows x64 nightly  | `IndustrialisLauncherNightlySetup-x64.exe` | In-app installer handoff     |
| Windows x64 preview  | `IndustrialisLauncherPreviewSetup-x64.exe` | In-app installer handoff     |
| Windows x64 portable | `IndustrialisLauncher-windows-x64.zip`     | Manual release page          |
| macOS arm64          | `IndustrialisLauncher-macos-arm64.dmg`     | Manual release page          |
| Linux x64            | `IndustrialisLauncher-linux-x64.tar.gz`    | Manual release page          |

macOS uses an app bundle with an ad-hoc signature. Linux is a portable archive;
run `./industrialis` in the extracted directory. It requires a graphical desktop,
Vulkan drivers, Fontconfig and X11/Wayland libraries. Other CPU architectures
have no distribution artifacts yet.

Preview builds are not publisher-signed or Apple-notarized. SHA-256 verification
checks the bytes against the digest returned by GitHub; it is not a code-signing
certificate. Windows SmartScreen/macOS Gatekeeper behavior, signing and
notarization remain release-readiness work. Do not describe these previews as
signed production distributions.

Stamp versions using:

```powershell
node scripts/launcher-version.mjs 0.1.1
cargo update --workspace --offline --manifest-path apps/launcher/Cargo.toml
```

The script synchronizes the launcher package and app Cargo versions. About and
Windows executable resources use `CARGO_PKG_VERSION`; the installer uses that
same package version. Components must fit Windows's 16-bit version fields.
Versions stay numeric on every channel: nightly identity is carried by the
release tag and the `INDUSTRIALIS_BUILD_TAG` value the build workflow compiles
in, never by a prerelease suffix in the product version.

From the repository root:

```powershell
pnpm build:launcher:preview
pnpm build:launcher:preview:installer
pnpm build:launcher:preview:portable
```

Windows needs NSIS 3.11 or later. Use `MAKENSIS` or `--makensis <absolute path>`
with `node scripts/build-launcher.mjs` for a portable compiler. The build
uses `cargo build --release --locked --features packaged-updates`; outputs go
to `apps/launcher/dist`. `--skip-build --binary <path>` is only for packaging
smoke fixtures, not release builds. The packager supports native Windows x64,
macOS arm64 and Linux x64 hosts.

Build-only stable preparation:

```powershell
node scripts/build-launcher.mjs --channel stable --out apps/launcher/target/stable-review
```

Stable builds compile the stable update channel and register
`industrialislauncher:`. `--replacement` enables the same registration for a
preview build.

The canonical `.github/workflows/launcher-release.yml` tests and builds all
three native platforms only for release tags and manual `workflow_dispatch`;
pull requests do not trigger builds. `launcher-v*` tags publish stable releases
and `launcher-preview-v*` tags publish optional isolated previews after every
build succeeds. `workflow_dispatch` builds CI artifacts without publishing a
release.
Root launcher development, packaging and version commands select the native
launcher. The immutable `launcher-v0.3.0` tag preserves the historical release.

Windows CI downloads the official NSIS 3.11 portable ZIP, verifies its pinned
SHA-256 before extraction and checks the compiler version. `MAKENSIS` selects
that same compiler for packaging and updater smoke checks. It does not depend on
the Chocolatey feed or install a global compiler.

## Verification

```powershell
# From apps/launcher:
cargo test --workspace --lib --bins --tests --jobs 1 --features industrialis-app/packaged-updates --locked
cargo run -p industrialis-cli --locked --features industrialis-backend/stable-updates -- check-launcher-update --current-version 0.3.0

# From the repository root:
node --test scripts/packaging.test.mjs scripts/notices.test.mjs scripts/windows-runtime.test.mjs
powershell -NoProfile -File scripts/verify-windows-artifacts.ps1
powershell -NoProfile -File scripts/test-windows-process-exit.ps1
powershell -NoProfile -File scripts/test-windows-updater.ps1 -MakeNSIS C:\Tools\NSIS\makensis.exe
powershell -NoProfile -File scripts/test-windows-migration.ps1 -MakeNSIS C:\Tools\NSIS\makensis.exe
```

Backend tests cover version/channel/platform selection, trusted URLs, mandatory
digests, valid installation markers, download progress, truncated/corrupt/invalid
executables, and cleanup. Native pointer/keyboard tests cover discovering failed
checks, modal input blocking, progress, and closing after failure at 800/1100 px
in light/dark modes. Busy guards cover instance operations, account work and
preferences; a running game does not block an update. The legacy migration
check accepts the marker in the retired install folder so an update from it can
still proceed.

The Windows smoke test compiles an isolated installer with registry/shortcut
writes disabled and registry adoption skipped, so a developer's installed
launcher is never migrated or cleaned by a fixture. It exercises fresh install,
the real Rust readiness handoff, rejection of a process that never acknowledges
readiness, replacement after a running old executable exits, restart into the
new executable, and uninstall while preserving an unrelated data file. The
legacy-migration fixture starts from a retired-folder marker and runs both with
and without the old executable mapped, verifying readiness, migration, target
preservation, restart, and cleanup. Both use fixture executables, not a real
GitHub release, and unique folders that are removed afterwards.

Stable-channel tests reject unmarked historical releases, GitHub prereleases and
the opposite channel.

The separate [Windows Sandbox acceptance](LIVE_ACCEPTANCE.md#installed-launcher-in-windows-sandbox)
uses the actual stable launcher artifact. Only generated profile data and owned
guest windows are used. This checks the installed lifecycle without replacing the
personal installation. Results and exact artifact hashes are recorded in the
[migration tracker](MIGRATION_TRACKER.md).

Accepted follow-ups after the 0.4.0 production cutover: an actual published N → N+1
launcher update, cross-platform artifact launch/install behavior,
signing/notarization, physical installation and older Windows compatibility. The
installed native lifecycle and same-owner protocol activation pass in Windows
Sandbox with generated profile files retained. Original live credentials remain
outside these fixtures. Profiles remain in the same location and require no import,
copy or format migration. No release is published merely by implementing or
testing this code.

Primary API references: [GitHub release assets and digests](https://docs.github.com/en/rest/releases/releases),
[NSIS installer arguments](https://nsis.sourceforge.io/Docs/Chapter3.html),
and [NSIS scripting reference](https://nsis.sourceforge.io/Docs/Chapter4.html).
