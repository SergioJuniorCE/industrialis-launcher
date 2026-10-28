# Launcher updates and releases

Launcher self-updates are separate from instance pack updates. Distribution
builds check GitHub asynchronously at startup. Settings → About offers a manual
check, status, retry, and the release page; the toolbar shows an update action
when a newer version is available or a check failed. The status bar also reports
the launcher update state (`Launcher up to date`, `Update available`, checking,
downloading, or failed), and selecting it opens the update dialog. Settings →
About and the update dialog include a changelog viewer with recent published
release notes. Updates install only after the user confirms **Download and
restart** in the launcher update dialog.

## Stable and preview channels

The 0.4.0 cutover was authorized on 2026-10-17 with the remaining provider,
platform and published-update checks accepted as documented follow-ups.
See [release notes](RELEASE_NOTES.md) and [cutover status](CUTOVER.md).

The production workflow is `.github/workflows/launcher-release.yml`.
Stable `launcher-vX.Y.Z` tags compile `stable-updates`, publish a regular GitHub
Release and make it latest. Stable release bodies carry
`<!-- industrialis-launcher-release-v1 -->`; the updater rejects unmarked Electron
artifacts. Windows uses `IndustrialisLauncherSetup.exe`, preserving the existing
website and old-updater download contract, with protocol registration and guarded
Squirrel retirement. Existing profile data stays in its original location.

Optional `launcher-preview-vX.Y.Z` tags publish prereleases with `make_latest: false`
and the preview update feature. They remain outside the stable feed.
Pull requests build and test all three platforms without publication;
workflow_dispatch also builds artifacts only. Tag pushes alone publish releases.

Development builds omit the `packaged-updates` feature and perform no launcher
update network requests. Packaging enables the feature. Portable Windows builds
still check versions, but open the release page for manual replacement.

## Windows installation

The per-user NSIS installer installs `industrialis.exe` and `install.json`
in `%LOCALAPPDATA%\Programs\Industrialis Launcher` by default. Existing native
installations keep their registered install directory when upgraded; the retired
codename folder is replaced by the current default and removed. The marker
identifies an installation that the updater may replace; it is omitted from
portable ZIPs. The preview uses a separate install directory, shortcut, and
uninstall entry. It does not remove the Electron installation or migrate
encrypted accounts.

`node scripts/build-launcher.mjs --replacement` also registers the retained
`industrialislauncher:` scheme. Default preview artifacts omit that registration.
Windows saves the previous handler and restores it on uninstall while the launcher still
owns the command; another application's later registration is kept. macOS embeds
CFBundleURLTypes, and the Linux desktop entry declares the scheme for installation
with `xdg-mime`. These registration paths need native installed acceptance before
cutover; normal preview builds do not change the current protocol handler.

The stable Windows installer first extracts the new launcher executable, retaining
an existing launcher executable for rollback. It then runs the bundled retirement
helper against the exact old `%LOCALAPPDATA%\IndustrialisLauncher` installation.
The helper refuses redirected paths, profile overlap or profile files inside
the install directory, waits for launchers to close normally, and invokes the
original `Update.exe --uninstall --silent`. It verifies the old executables are
gone, including Squirrel's root shortcut stub, and clears only a handler
targeting one of those removed files before the launcher
registers its own handler. The helper waits for normal closure before delegating
to Squirrel; legacy uninstall callbacks and cleanup still need installed acceptance.
A failed helper aborts
registration/restart and restores the previous launcher executable. A failed or
partially completed Squirrel uninstall requires review/retry; fixture tests do
not establish rollback of a real Squirrel installation. The original profile is
never imported, moved or recursively deleted by the launcher installer.

For an installed Windows x64 build:

1. Stop games and finish file/account operations and settings saves.
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

The preview installer/uninstaller only manages its own application files, shortcut
and registry keys. Stable installation additionally delegates old application
retirement to Squirrel after the guards above. The launcher does not recursively delete
the install directory or touch
the launcher data directory, instances, saves, accounts, or preferences.

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

| Platform             | Artifact                                | Launcher update installation |
| -------------------- | --------------------------------------- | ---------------------------- |
| Windows x64 stable   | `IndustrialisLauncherSetup.exe`         | In-app installer handoff     |
| Windows x64 portable | `IndustrialisLauncher-windows-x64.zip`  | Manual release page          |
| macOS arm64          | `IndustrialisLauncher-macos-arm64.dmg`  | Manual release page          |
| Linux x64            | `IndustrialisLauncher-linux-x64.tar.gz` | Manual release page          |

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

Build-only stable preparation, after reviewing the retirement behavior:

```powershell
node scripts/build-launcher.mjs --channel stable --out apps/launcher/target/stable-review
```

Do not install that stable artifact over an existing installation until the actual
old-installer acceptance scenario is ready. Merely building it does not retire any
application. `--replacement` alone continues to register only the protocol.

The canonical `.github/workflows/launcher-release.yml` tests and builds all
three native platforms. `launcher-v*` tags publish stable releases and
`launcher-preview-v*` tags publish optional isolated previews after every build
succeeds. `workflow_dispatch` builds CI artifacts without publishing a release.
Root launcher development, packaging and version commands select the native launcher;
Electron source, aliases and build dependencies have been removed. The last
Electron source and binary remain available under `launcher-v0.3.0` for rollback.

Windows CI downloads the official NSIS 3.11 portable ZIP, verifies its pinned
SHA-256 before extraction and checks the compiler version. `MAKENSIS` selects
that same compiler for packaging and updater/retirement smoke checks. It does
not depend on the Chocolatey feed or install a global compiler.

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
powershell -NoProfile -File scripts/test-windows-retirement.ps1 -MakeNSIS C:\Tools\NSIS\makensis.exe
```

Backend tests cover version/channel/platform selection, trusted URLs, mandatory
digests, valid installation markers, download progress, truncated/corrupt/invalid
executables, and cleanup. Native pointer/keyboard tests cover discovering failed
checks, modal input blocking, progress, and closing after failure at 800/1100 px
in light/dark modes. Busy guards cover games, instance operations and preferences.

The Windows smoke test compiles an isolated installer with registry/shortcut
writes disabled. It exercises fresh install, the real Rust readiness handoff,
rejection of a process that never acknowledges readiness, replacement after a
running old executable exits, restart into the new executable, and uninstall
while preserving an unrelated data file. It uses fixture executables, not a real
GitHub release, and does not touch the user's launcher installation/profile.

Retirement fixtures additionally cover profile overlap/data guards, running
launchers, unsuccessful/incomplete uninstall, native NSIS extraction rollback,
and registry cleanup in a unique test key. They invoke a synthetic uninstall
callback, never the user's installed Squirrel updater. Stable-channel tests
reject unmarked Electron releases, GitHub prereleases and the opposite channel.

The separate [Windows Sandbox acceptance](LIVE_ACCEPTANCE.md#actual-squirrel-replacement-in-windows-sandbox)
uses the unchanged public Electron 0.3.0 installer and the actual stable launcher
artifact. Only generated profile data and owned guest windows are used. This
checks real Squirrel retirement and installed lifecycle without replacing the
personal installation. Results and exact artifact hashes are recorded in the
[migration tracker](MIGRATION_TRACKER.md).

Accepted follow-ups after the 0.4.0 production cutover: an actual published N → N+1 launcher update,
cross-platform artifact launch/install behavior, signing/notarization, physical
replacement, running-user handoff and older Windows compatibility. The actual
Electron 0.3.0 replacement, installed native lifecycle and same-owner protocol
activation pass in an earlier Windows Sandbox with nine generated profile files
retained. The subsequent controlled running-Electron window handoff waits for
normal close and preserves eleven post-close fixture files. The current 2026-10-11
artifact passes the complete extended native UI runner, normal close and uninstall,
with all fifteen post-test files retained and owned VM cleanup complete. Exact
provenance is in the latest tracker record. Original live credentials remain outside
these fixtures. Profiles remain in the same location and
require no import, copy or format migration. No release is published merely by
implementing or testing this code.

Primary API references: [GitHub release assets and digests](https://docs.github.com/en/rest/releases/releases),
[NSIS installer arguments](https://nsis.sourceforge.io/Docs/Chapter3.html),
and [NSIS scripting reference](https://nsis.sourceforge.io/Docs/Chapter4.html).
The retained Squirrel options are defined in its
[command-line parser](https://github.com/Squirrel/Squirrel.Windows/blob/develop/src/Update/StartupOption.cs).
