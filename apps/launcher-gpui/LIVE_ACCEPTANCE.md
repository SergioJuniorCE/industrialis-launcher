# Live replacement acceptance

Use [MIGRATION_TRACKER.md](MIGRATION_TRACKER.md) for status and stable feature IDs.
These scenarios supplement the synthetic regression suite. A successful backend
scenario does not establish physical GPUI, provider or installed-app acceptance.
Keep Electron until [CUTOVER.md](CUTOVER.md) is satisfied.

F15's [native copy sound check](MIGRATION_TRACKER.md#2026-10-11-f15-copy-completion-cue-verification)
passes on the owned Linux Xvfb: one real bell request for a successful native
copy, none for the actual rejected copy, preserved copied bytes and two normal
app closes. The receipt and inspected failed-process screenshot distinguish
this from a mocked notification. Physical speaker playback remains a platform
check; F40 tracks the requested sound selector as a separate enhancement.

## Native third-party content

The 2026-10-12 [Modernity acceptance record](MIGRATION_TRACKER.md#2026-10-12-modernity-release-assets-and-third-party-content)
covers local ZIP import/removal and GitHub release downloads. The full Rust
suites pass with 242 Windows and 239 Linux tests; the final resource UI check
also passes separately. The headless online scenario installs, restores and
removes the main pack, Outlined Ores and MyCTMLib from the actual upstream release.

The native runner uses a fresh temporary profile on an owned Docker/Xvfb display,
with the real independently named acceptance app. It clicks the main pack,
optional texture pack and optional mod, verifies source receipts and paired
active/persistent files, checks unrelated metadata, visits the original Mods
manager and closes normally. The release controls use distinct per-asset IDs;
a regression test clicks same-labelled controls and verifies correct dispatch.
Alternative MyCTMLib versions are blocked by mod identity and displayed as
"Other version installed" when appropriate.

The runner and commands are documented in [THIRD_PARTY_CONTENT.md](THIRD_PARTY_CONTENT.md#verification).
Evidence is under `target/live-acceptance/content-20261012/`, including
`native-verification.json`, `verification.json`, source hashes, logs and screenshots.
Activation/load order remains in Minecraft. Actual in-game rendering and physical
platform acceptance are not established by these fixture checks.

## Native Files Explorer

The 2026-10-13 [Explorer record](MIGRATION_TRACKER.md#2026-10-13-files-explorer-tree)
adds an expandable, lazy file tree matching the supplied reference. All 47 app
tests pass on both Windows and Linux, including nested draft preservation,
keyboard navigation, instance isolation, scrolling, Easy/Advanced edits and pop-out.
The existing full workspace baseline is recorded separately.

The owned editor runner also supports the Explorer scenario:

```sh
GPUI_ISOLATED_EDITOR_CONTAINER=1 sh scripts/test-gpui-linux-editor.sh \
  /target/debug/examples/profile_ownership_smoke /output explorer
```

It uses temporary profiles and a private Docker/Xvfb in both themes and a compact
window. Receipts and inspected screenshots are under
`target/live-acceptance/explorer-20261013/`. No original profile or physical
desktop is used; physical IME/screen-reader acceptance remains an external gate.

## Native Files editor

The [Files design and acceptance record](EDITOR_DESIGN.md) covers the 2026-10-12
contrast, scrolling, Easy-mode discoverability and highlighted-editor repair.
Build the actual app under its independent acceptance filename:

```sh
cargo build -p industrialis-app --example profile_ownership_smoke --locked --offline --jobs 1
GPUI_ISOLATED_EDITOR_CONTAINER=1 sh scripts/test-gpui-linux-editor.sh \
  /target/debug/examples/profile_ownership_smoke /output
```

Run inside an owned disposable Linux container with Xvfb, Openbox, D-Bus, Mesa,
Python and XTest. Mount the repository and executable read-only, with only the
generated output directory writable; disable networking. The marker is mandatory
before creating a desktop, and every input action verifies the app window's PID.
Fresh generated profiles use Sandstone dark and light at 1600x900. The runner
opens a Forge file, records highlighted code and Find, types a query, closes Find,
uses Easy mode to change a boolean and verifies exact game/overlay bytes with
comments, lists and Unicode preserved. Both processes must close with exit 0.
Failed full runs remove the previous passing receipt.

`target/live-acceptance/editor-20261012/` contains screenshots, native verification,
source/container provenance and full Rust-suite logs. GPUI regressions additionally
reach the final file in a 120-file directory and verify bounded clickable rows,
Advanced saves, conflict/pop-out guards and search focus. This does not replace
physical font/IME or other-platform acceptance.

## Native library insertion preview and pack-check menu

Inside an owned disposable Linux container, with the repository and built app
mounted read-only and only a generated output mount writable:

```sh
GPUI_ISOLATED_LIBRARY_CONTAINER=1 sh scripts/test-gpui-linux-library.sh \
  /target/debug/examples/profile_ownership_smoke /output
```

The runner requires its explicit ownership marker before desktop setup, creates
private Xvfb/D-Bus and generated profiles, and verifies the app window's PID before
input. In each Sandstone mode it moves C before A, captures the dashed landing
slot, verifies unchanged disk order during hover, releases and checks C/A/B in
the original group schema with extension fields retained. It opens the native
instance context menu and dispatches the manual pack check. Networking is disabled;
the expected failure reports the generated C instance. Both apps close with exit 0.
`target/live-acceptance/library-20261012/` holds receipts, screenshots, test logs
and provenance. Hourly timing is verified by a GPUI fake-clock regression rather
than waiting an hour on a desktop; physical/platform checks remain separate.

## Automated real-pack maintenance

From `apps/launcher-gpui`, build the opt-in harness with one compiler job:

```powershell
cargo build -p industrialis-backend --example live_pack_acceptance --locked --jobs 1
```

Run `target/debug/examples/live_pack_acceptance.exe` with three arguments:
an absolute **new expendable profile directory**, `2.9.0-RC-2`, and `2.8.4`.
The harness creates its own explicit marker. An existing directory must already
carry that exact marker; never add it to a user profile. Public pack downloads
and installed files require several GB. Never use the original data directory.

The harness installs both Java variants, resolves launch libraries, seeds player
files/config overlays/unknown settings/custom mods, copies the instance,
reinstalls, replaces with the previous pack and updates back. It checks exact
preserved bytes and selected/unselected custom-mod behavior at each step.
`pack-acceptance.json` records the versions and outcome. It deliberately does
not sign in, start a world or use personal account credentials.

## Automated actual client loading and Stop

`live_game_acceptance <marked-profile> <instance-id> <java-path>` starts the actual
client through the launcher backend. All accounts must be offline, and the
profile must be expendable and marked. It records the exact spawned PID, waits
for Forge's successful mod-loading message, invokes Stop and checks that the
game is no longer running. Logs and play time are persisted normally; the report
is `game-acceptance-<instance-id>.json`. It replaces the acceptance instance's
Java/account/RAM/window/JVM settings and never accesses the user's account store.
The default deadline is 900 seconds; `GPUI_GAME_ACCEPTANCE_SECONDS` overrides it.

On Linux, run from the repo root:

```sh
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveModern /usr/bin/java
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveLegacy /absolute/java8/bin/java
```

The script creates and cleans an owned temporary virtual desktop, D-Bus session
and XDG directories. Install Xvfb/Xauth/D-Bus, Mesa OpenGL/EGL, OpenAL/ALSA and
`xrandr` first. The test run used Debian 13 x64, Java 21.0.12.1 and verified
Adoptium Java 8u504. The two JVM flags entered on separate lines must remain
separate arguments. Use a separate expendable profile for game loading while
maintenance runs; profile ownership intentionally rejects concurrent writers.

The 2026-10-07 runs completed actual 2.9.0-RC-2 Java 17+ loading in 434 seconds
and Java 8 loading in 564 seconds, then stopped successfully. These results
cover client mod loading and process handling, **not world gameplay or input**.

### Actual detached-client lifetime, hooks and recovery

Use the same marked **offline** profile and installed Java variants:

```sh
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveModern /usr/bin/java --detached
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveLegacy /absolute/java8/bin/java --detached
```

This selects `live_detached_game_acceptance`. Fresh owning launcher processes
perform launch, live recovery/close, then Stop/final recovery. The actual GTNH
PID and creation identity must survive both closes. A generated shell wrapper
records environment/JVM/RAM/window arguments and adds uniquely marked heartbeat
lines to the capture stream while the launcher is closed. Final recovery checks
complete heartbeat replay without duplicates, one exit log, exact receipt-based
play time and exactly one pre/post hook. Repeating recovery must leave finalized
logs/time unchanged. Hook files and `detached-game-acceptance-<id>.json` belong
to the expendable instance/profile. This establishes backend process lifetime
on a virtual desktop, not physical GPUI gestures or world gameplay. Keep the
runner and binaries unchanged while acceptance processes are running.

The 2026-10-08 Linux Java 17+ run passes in 266 seconds. The final Java 8u504
run passes in 311 seconds, including the stricter complete-log/single-exit checks.
Both record actual client survival, same-process recovery, Stop, hooks and exact
play-time reconciliation. See the tracker for the reports and platform limits.

## Automated custom-port connection and client exits

With the same marked, offline acceptance profile and installed pack used above:

```sh
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveModern /usr/bin/java --normal-exit
sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveModern /usr/bin/java --abnormal-exit
```

For a real IPv6 loopback connection, prefix normal/abnormal mode with
`GPUI_AUTO_JOIN_IPV6=1`. The fixture binds `::1` on an unused custom port and
checks the actual host and port in the Minecraft handshake. IPv6 mode rejects
server-world mode and writes separate `ipv6-connection-exit-*.json` reports.
No public server or production network route is involved.

The lightweight `scripts/GpuiMinecraftServerAddress.java` probe can additionally
run against the installed 1.7.10 client JAR, without launching Minecraft. Compile
it with `javac --release 8 -d <probe-output>`, add the output and client JAR to the
classpath, and invoke `gpui.acceptance.GpuiMinecraftServerAddress '[::1]' 25570`.
The raw `::1` comparison fails because Forge joins host and port before parsing;
the bracketed control preserves both the intended address and custom port.
Use a network-disabled disposable fixture for these parser comparisons.

The `live_connection_exit_acceptance` example requires Linux/Xvfb, Python 3 and
libX11 (plus libXRes/XRes 1.2 for older LWJGL 2 windows), the exact expendable-profile marker and the generated `GpuiAcceptance`
offline account. It rejects real account credentials, linked profile/instance
roots and an already-running game. Its loopback listener selects an unused
custom port, checks the actual Minecraft 1.7.10/Forge login handshake and offline
login-start packet, then deliberately disconnects before login/world success.
Forge's loaded message precedes remaining texture/menu work; the runner allows
three minutes afterward and fails immediately on an actual connection error.

Normal mode sends `WM_DELETE_WINDOW` to the exact game's X11 window using
`gpui-virtual-game-window.py`. That helper requires the isolated-desktop flag,
matches `_NET_WM_PID` or obtains a local owner's PID from XRes on a visible window,
and refuses ambiguous/missing ownership. Older LWJGL 2 windows omit the PID
property; their X-server resource ownership is queried directly. Hidden SDL
rendering windows are excluded. It does not
operate the launcher's physical desktop or invoke Stop. A pass requires exit
code 0. Abnormal mode instead injects `SIGKILL` into the verified game process;
it tests nonzero exit handling on a real loaded client, not a naturally occurring
Minecraft crash. This avoids depending on the host VM's core-dump capture service.
Disable core dumps in disposable containers (`--ulimit core=0`).
Both modes check one final exit log, exactly one post-exit hook, retained play
time, removal of running state and unchanged repeated recovery. Exit counts use
the current live launch and its persisted session, since older sessions can be
discarded by normal log compaction. A client may open the probe socket before
finishing texture/Netty initialization; each connection shares the three-minute
post-Forge acceptance deadline rather than imposing a short additional deadline.
Reports are
`connection-exit-<id>-normal-window-close.json` and
`connection-exit-<id>-injected-abnormal.json` in the ignored acceptance profile.
Keep source scripts and binaries unchanged while their acceptance run is active.
These checks establish auto-join routing and exit accounting, not online-server
authentication, a Forge server connection, world gameplay or native GPUI input.

The visible-window helper regression runs separately without a game:

```sh
GPUI_ISOLATED_VIRTUAL_DESKTOP=1 xvfb-run -a python3 scripts/gpui-virtual-game-window.test.py
```

It checks the isolated-desktop guard, rejection of multiple visible owned
windows, delivery to the visible exact-PID window when hidden auxiliary windows
share that PID, and property-free XRes ownership with unrelated-PID rejection.
`--inspect` on the helper prints window IDs, resolved owner PIDs and mapping state
without sending a close event.

### Private Forge server world entry

`scripts/test-gpui-forge-server.sh` is an entrypoint for a fresh, disposable GTNH
2.9.0-RC-2 Docker server. It requires the explicit
`GPUI_EXPENDABLE_FORGE_SERVER=gtnh-2.9.0-rc2-acceptance-v1` flag, an already configured
EULA and no existing world. It never accepts an agreement or deletes a world.
The current fixture uses cached image
`sha256:4e44a10826a4fd72b2d8a123eaf3af67796762cb549b3cbc29bdbff8747094ff`.

Run that server with an anonymous `/app/server` volume, interactive stdin, the
script mounted read-only, a private **internal** Docker network, alias
`gpui-acceptance-server`, `--ulimit core=0`, and **no published ports**. Its
generated configuration permits only one offline player, uses port 25570 and a
fresh `gpui-acceptance-world`. The fixture uses a 4 GiB Java heap and view distance
2 to bound resource use. Wait for the server's `Done` message before starting the
client container on the same network.

```sh
nonce=$(tr -d '-' < /proc/sys/kernel/random/uuid)
GPUI_FORGE_WORLD_NONCE="$nonce" \
  sh scripts/test-gpui-live-game.sh /absolute/expendable/profile LiveModern /usr/bin/java --server-world
```

After the private server reports the generated `GpuiAcceptance` player joining,
send `say GPUI_WORLD_<same-nonce>` through that server's own console stdin. The
client must receive the exact marker in an actual `[CHAT]` line. This establishes
Forge connection and in-world traffic; login-start alone cannot pass. The runner
then closes the visible exact-PID game window and applies the same code-0,
logs/hook/play-time/recovery checks. Its outcome-only report is
`world-connection-exit-<id>-normal-window-close.json` in the ignored fixture profile.
Stop the generated server through its console, then remove only the network
created for this run. This scenario does not verify online authentication,
physical gameplay controls or any existing user's world.

For timing diagnosis, `gpui-isolated-tcp-delay.py` can run as a separate marked
container on that same internal network, with no published ports and alias
`gpui-acceptance-proxy`. It requires
`GPUI_EXPENDABLE_FORGE_PROXY=gtnh-acceptance-latency-v1`; its endpoints are fixed to
the generated server on 25570 and its own listener on 25571. Setting
`GPUI_FORGE_LATENCY_CONTROL=1` on the world runner selects that route. The report
explicitly records the configured forwarding delay (zero or 200 ms per direction). This is
a diagnostic control for a timing-sensitive Forge startup failure, never a
production workaround or an ordinary direct-server pass.
For a matched zero-delay baseline, set `GPUI_FORGE_PROXY_DELAY_MS=0` on both the
proxy and the runner while retaining `GPUI_FORGE_LATENCY_CONTROL=1`. Stop the
owned proxy after the prior game exits, then restart it before changing delay;
keep source scripts and binaries unchanged during each actual process run.

## Actual Linux portable artifact acceptance

On an isolated Linux fixture with Xvfb, Openbox, Python 3, libX11/XRes and software
Vulkan drivers, use the actual tarball and its compiled input:

```sh
sh scripts/test-gpui-linux-portable.sh \
  /absolute/artifacts/IndustrialisGPUI-linux-x64.tar.gz \
  /absolute/build/release/industrialis /absolute/artifacts
```

Set `GPUI_PACKAGE_CHANNEL=stable` for a stable artifact; the default is preview.
Set `GPUI_PACKAGE_REPLACEMENT=1` for an explicitly packaged preview using
`--replacement`; this validates the opt-in protocol registration separately from
the release channel.
The check requires protocol metadata to match the selected channel and reports
the actual Linux distribution and architecture. It supports Python 3.10 and
newer; `python3 scripts/gpui-linux-portable-smoke.test.py` verifies extraction of
the expected files and rejection of links, duplicate files and unexpected paths
before anything is extracted. The release workflow runs these checks and boots
the packaged Linux executable after packaging. Screenshots/logs/outcomes upload
as separate acceptance evidence, not public release assets.

The runner always creates its own virtual desktop and generated temporary profile.
It checks exact regular-file archive contents, executable bytes/architecture/mode,
resource/license/notices equality, dynamic dependencies and preview/manual-update
guidance. It removes `INDUSTRIALIS_DATA_DIR` and seeds the normal owned
`XDG_CONFIG_HOME/industrialis-launcher` path. It boots the extracted executable, identifies its unique visible owned
window, verifies rendered content, captures `portable-render.png`, closes it via
the native window event, and verifies a seeded unknown settings field survives.
Direct second launch must restore the same minimized window to foreground. A
replacement package must also install its actual desktop file/MIME association
in the temporary HOME and pass `xdg-open industrialislauncher://owned-acceptance`
with the same owner restored to foreground. The host desktop association is never
used or changed; `xdg-utils` is a fixture prerequisite.
`verification.json` is written only after all checks pass; native output remains
in `portable-native.log`. This establishes packaged boot/render/close and local
profile persistence on the fixture OS, not physical input, Wayland, provider
credentials or compatibility with other Linux distributions. Use the production
CI distribution baseline for release artifacts; a newer Debian acceptance build
can require a newer glibc than an older user's Linux distribution.

### Native Linux credential interoperability

This test uses an actual Electron 43.3.0 safeStorage envelope under the original
app name and normal Linux app-data path, a generated value and a new native GNOME
keyring. It checks Electron-to-Rust decryption, Rust-to-Electron decryption and
unchanged native key bytes without printing them. It does not authorize Google,
touch a user's keyring or certify macOS Keychain/KWallet.

```sh
cargo build --manifest-path apps/launcher-gpui/Cargo.toml --workspace \
  --example credential_compat --locked --jobs 1 --features industrialis-app/packaged-updates
docker build -f apps/launcher-gpui/packaging/acceptance/linux-credentials.Dockerfile \
  -t industrialis-gpui-credential-acceptance:local apps/launcher-gpui/packaging/acceptance
mkdir -p apps/launcher-gpui/target/credential-acceptance
docker run --rm --network none \
  --mount "type=bind,source=$PWD,target=/repo,readonly" \
  --mount "type=bind,source=$PWD/apps/launcher-gpui/target,target=/target,readonly" \
  --mount "type=bind,source=$PWD/apps/launcher-gpui/target/credential-acceptance,target=/receipt" \
  -e GPUI_ISOLATED_CREDENTIAL_CONTAINER=1 -w /repo \
  industrialis-gpui-credential-acceptance:local \
  sh scripts/test-gpui-linux-credentials.sh /opt/electron-fixture/electron \
  /target/debug/examples/credential_compat /receipt/verification.json
```

The shell runner refuses a non-Docker environment. Its HOME, runtime directory,
D-Bus, Xvfb and unlocked generated keyring exist only in the disposable container.
The checked-in image verifies Electron's ZIP against its release checksums.
The release workflow builds/runs this regression and retains its non-secret
receipt. Remote CI still needs execution. On 2026-10-09, the original native loop
reproduced a second-lookup SIGSEGV in libsecret/GLib; retaining those libraries for
the process lifetime fixed it. The end-to-end checked-in recipe passes locally
on Debian 13, using the existing Debian fixture as `BASE_IMAGE` after Docker Hub
returned HTTP 504 twice. This validates the recipe and native interoperability,
without claiming a clean pull of its default minimal base image.

## Native/provider acceptance still required

### Opt-in Windows provider harness

The user may authorize real provider accounts in a **new** expendable profile
marked `.gpui-live-accounts` with contents
`industrialis-gpui-expendable-live-accounts-v1`. Never mark a normal user profile.
Reports contain outcomes only; never display or commit the saved account/credential
files. These tests do not import or move existing user accounts.

```powershell
# From apps/launcher-gpui:
cargo build -p industrialis-backend --example live_provider_acceptance --locked --jobs 1
# First authorize Microsoft using industrialis-cli --data-dir <profile> ms-login.
target/debug/examples/live_provider_acceptance.exe <absolute-marked-profile> microsoft
# Configure the public Google Desktop app client ID, then authorize in the browser.
target/debug/examples/live_provider_acceptance.exe <absolute-marked-profile> google
# Optional actual client loading with the already-authorized test account:
target/debug/examples/live_provider_acceptance.exe <absolute-marked-profile> microsoft-game <instance-id> <java-path>
```

Microsoft mode checks the actual license/profile/skin and normal refresh pipeline,
then compares the reloaded game token in memory. Google mode reloads the encrypted
connection in a new provider, interrupts a generated ~10 MiB upload after Google
acknowledges a partial chunk, retries through the durable queue after manager
restart, compares download bytes, tests retention with another generated archive,
and deletes only those cloud test files. It retains local generated archives and
may leave empty folders under its unique test instance; it never prunes another
instance. These are opt-in network/provider scenarios, not ordinary CI tests.

`microsoft-game` requires the account-profile marker above and an installed
expendable pack in that profile. It selects the saved authorized account, uses
the normal refresh/launch path, waits for actual Forge loading, invokes Stop,
and verifies persisted account/play time. It prints no authenticated game output
or account identifiers. Its outcome-only report is
`microsoft-game-<id>-acceptance.json`. It needs no new sign-in while the saved
test authorization remains usable. In an isolated Linux fixture, use:

```sh
sh scripts/test-gpui-live-game.sh /absolute/expendable/account-profile AuthModern /usr/bin/java --microsoft
```

Never use a normal user profile or copy its credentials into a fixture. Tests
may copy only generated pack contents into the already-marked test profile;
they do not import user accounts or provide a user-data migration workflow.

For first-time Google configuration, enable Google Drive API in your selected
Cloud project, complete Google Auth platform branding/audience setup, and create
a **Desktop app** client. Add the authorizing account as a test user if the app is
in testing. The public client ID may be configured in the acceptance profile or
via `INDUSTRIALIS_GOOGLE_DRIVE_CLIENT_ID`. The loopback PKCE flow uses the system
browser; do not paste account passwords, authorization codes or tokens into chat.
See [Google's installed-app OAuth guide](https://developers.google.com/identity/protocols/oauth2/native-app).

The 2026-10-08 Windows Microsoft scenario passes fresh device authorization,
playable-license/profile/skin resolution, live refresh and persisted reload.
Google is still pending client configuration and real authorization. Neither
fresh test authorization proves pre-existing original-account compatibility.

The saved Microsoft test account also completes actual Java 17+ client loading
and Stop in the isolated Linux desktop in 217 seconds without another sign-in;
the account and 211 added play seconds persist. This does not prove an online
server accepted that session or establish world gameplay/native input.

Record each supported Windows x64, Linux x64 and macOS arm64 artifact separately:

| Scenario                  | Acceptance evidence                                                                                                                                                                                                                                                                                                                         |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Existing profile/accounts | Open the normal original profile with the old launcher closed. Confirm paths, instances, groups/icons/settings and existing Microsoft/Google credentials work without reconnecting. Verify refresh before a separate fresh sign-in scenario. Never import or relocate user data.                                                            |
| Microsoft                 | With an authorized test account, complete device login, license/profile/skin resolution, refresh and visible cancellation/error handling. Launch a real instance using that account.                                                                                                                                                        |
| Google backups            | Use expendable backup contents. Link the authorized account, upload, interrupt/retry, download/restore and compare exact bytes. Test retention/delete scope against expendable backups only.                                                                                                                                                |
| Actual gameplay           | Create/open an expendable world in both supported Java variants. Check window/RAM/JVM arguments, configured environment and pre/post commands. Use a disposable server for auto-join. Check exit/crash logs and play time.                                                                                                                  |
| Detached game             | Leave a real game running across launcher close and reopening. Confirm one game process, recovered logs/PID/Stop and exactly-once exit/play-time accounting.                                                                                                                                                                                |
| Native input/desktop      | Verify focus, keyboard navigation, clipboard/IME, native pickers, external editor/folders/links, second-launch foreground activation and physical dragging. Check macOS main close versus Dock/menu Quit. Record the accepted Linux minimize behavior.                                                                                      |
| Windows replacement       | On an authorized expendable old Squirrel installation, test successful stable installation and failure/retry. Confirm old app/shortcuts/startup entries are retired, the original profile remains usable in place and protocol launch/uninstall owns/restores the correct handler. Do not use a running personal installation as a fixture. |
| Published updates         | After signing policy and publication are authorized, install actual version N and update to published N+1. Exercise digest/size/header validation, busy/save/close guards, failed handoff and profile preservation. Confirm manual update behavior on macOS/Linux/portable packages.                                                        |

For every record include date, OS/build/architecture, artifact version and SHA-256,
scenario, result, recovery checks and remaining limits. Do not include tokens,
account payloads or private profile contents in evidence. Append results under
the stable Fxx IDs in the tracker; partial evidence keeps the item unchecked.

## Local installer preparation

### Actual Squirrel replacement in Windows Sandbox

`scripts/test-gpui-windows-squirrel-sandbox.ps1` prepares a disposable VM fixture.
Use the unchanged public `launcher-v0.3.0` Windows installer and a stable GPUI
artifact directory containing its `verification.json`. From the repository root:

```powershell
# Verify the actual guest wait function with disposable native children only.
powershell -NoProfile -File scripts/test-gpui-windows-process-exit.ps1
# Preparation only; no installer executes on the host.
powershell -NoProfile -File scripts/test-gpui-windows-squirrel-sandbox.ps1 `
  -LegacyInstaller <downloaded-0.3.0-setup.exe> -StableArtifactDirectory <verified-stable-directory>
# Add -Run to execute in an already-enabled Windows Sandbox.
# Add -RunningLegacy to keep the actual released Electron fixture window open
# while the GPUI installer waits; the guest closes only that fixture normally.
# Add -NativeUi for guest-only accessibility, keyboard, native pickers,
# folder/default-editor dispatch and independent editor-window scenarios.
```

The runner never enables host features or reboots the host. It refuses an existing
Sandbox session. Only two checksum-verified installers, the guarded guest scripts,
a fixture image and a
unique identity marker are mapped read-only; one fresh evidence directory is
mapped writable. Networking, clipboard, audio/video input and printer redirection
are disabled, with software graphics. No repository or user profile is mapped.
These controls use Microsoft's [Windows Sandbox configuration format](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file).

The guest verifies its disposable identity before installation or shutdown. It
installs the actual old release, closes only its expendable fixture processes,
seeds generated accounts/settings/instance/player data in the normal app-data
location, then runs the actual stable GPUI installer. Assertions cover old
executables/shortcuts/uninstall registration, new executable checksum, exact
pre-boot profile preservation, protocol ownership, native GPUI boot/normal close,
unknown-field persistence, actual protocol launch restoring/foregrounding the same
minimized guest owner, GPUI uninstall and post-uninstall profile preservation.
Evidence stays under `target/live-acceptance/squirrel-sandbox-*/output`. The host
checks the receipt nonce and closes only that fixture's identified Sandbox client
after guest shutdown. Neither installer runs on the physical host.

The 2026-10-09 static-CRT stable artifact passes this complete runner with exit 0
on Windows build 10.0.26100.0 without the MSVC redistributable. Nine generated
profile files survive, the installed library discovers the fixture instance,
native rendering and same-owner protocol foreground activation pass, and normal
close/uninstall retain the profile. Inspect the saved native-window PNG as well
as the receipt. Exact provenance and remaining limits are in the latest
[tracker record](MIGRATION_TRACKER.md#2026-10-09-rebuilt-clean-windows-replacement-acceptance).

The process-wait regression imports only the guest's two wait/log functions via
the PowerShell AST; it cannot execute the installer/profile/shutdown body. It
checks redirected children with exit codes 0 and 7. The wait function retains
the native process handle because Windows PowerShell can otherwise return a blank
exit code for redirected output; see the [PowerShell issue and resolution](https://github.com/PowerShell/PowerShell/issues/5421).
This VM test does not establish existing real credentials, physical input,
published update handoff, an actual user's running installation or older Windows compatibility.
`-RunningLegacy` verifies the controlled released-app window handoff, with no
original account: preservation begins after its normal close because the old app
legitimately rewrites some metadata on its own boot/close. `-NativeUi` interacts
only with verified guest-owned window/dialog PIDs. Native filename controls are
addressed by both control ID and Edit class because the common dialog also gives
the parent combo controls the same ID. The external editor is a generated native
guest form registered only for the fixture's `.cfg` files; its atomic save must
reach both the game file and original overlay. No host desktop APIs are invoked.
When Electron does not create `Local State`, the fixture seeds a marker and
records `legacyCreatedLocalState: false`; that result cannot certify encryption-key
interoperability or live refresh of an original account.

Windows distribution builds enable static CRT linkage; both packaging and the
artifact verifier inspect actual x64 PE imports, including delay imports, and
reject external MSVC runtime DLLs. The Sandbox runner requires this verification
in the artifact receipt. This avoids silently relying on developer-machine
`VCRUNTIME140.dll` installation. See [Rust's CRT linkage documentation](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes)
and [Microsoft's PE import format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).

Ordinary previews never retire Electron. `--replacement` registers the protocol
only. `--channel stable` additionally compiles stable update selection and embeds
the guarded retirement helper; building it does not run the helper. The helper's
standalone default is inspection only. Avoid `-Apply` on a personal installation
until the installed acceptance scenario is authorized and launchers are closed.

```powershell
node --test scripts/gpui-packaging.test.mjs scripts/gpui-notices.test.mjs
powershell -NoProfile -File scripts/test-gpui-windows-retirement.ps1 -MakeNSIS C:\Tools\NSIS\makensis.exe
```

The retirement test uses synthetic install files, a uniquely owned registry test
key and native NSIS failure/rollback fixtures. It never invokes the actual old
installation's uninstaller. The production release workflow, public downloads
and source deletion remain gated even after these tests pass.

## Latest native interaction boundary

The complete 2026-10-11 `-RunningLegacy -NativeUi` run passes both guest assertions
and host cleanup. Use `rebuilt-stable-20261011-native-focus-fixed`; its artifact
receipt and exact hashes are in the [latest tracker record](MIGRATION_TRACKER.md#2026-10-11-native-functional-acceptance-and-remaining-external-gates).
The guest receipt, native trees, editor receipt, screenshot and progress log are
`target/live-acceptance/squirrel-sandbox-0faf50f8f6ee4c1ea6f49f01061b631f/output/`.

Eleven generated files survive the old window's normal close/replacement before
GPUI boot. Same-owner registered protocol foreground restoration, installed
rendering, normal close and uninstall pass; fifteen post-test files survive
uninstall and the dead legacy handler is not restored. The screenshot was inspected.

Native picker Cancel, two-file mod import/removal/folder dispatch, twelve distinct
menu actions, icon import/select/reset/library-folder dispatch, OS editor atomic
save/mirroring, repeated editor-window focus and independent close all pass.
Another external save after override removal does not recreate it. Two generated
offline accounts verify creation/removal/default persistence; all preferences,
Java Cancel, theme Enter/Space, Tab/Shift+Tab and modal/group actions pass. Shared
field names, editable values and keyboard-focus identity are observable in UIA.

The harness scopes duplicate labels to the active dialog and waits for confirmed
native focus before keys. Host-safe reflection validates its UIA static methods,
and the actual guest atomic replacement expression is tested without desktop APIs.
Earlier failed fixtures and their specific corrections remain recorded in the
tracker; they are not counted as full passes.

F03/F04/F05/F10/F22 are now complete. Physical IME/screen-reader and other-platform
checks remain in F20/F38. UIA reports disabled Create as enabled; the native blank
activation guard nevertheless preserves the accounts file. This announcement
limitation remains explicit. Original credentials, older Windows and a published
update are outside the generated-profile VM evidence.

## Native Linux credential stores

The checked-in `packaging/acceptance/linux-credentials.Dockerfile` now builds from
its default Debian trixie-slim base and verifies the official Electron 43.3.0 ZIP.
Both `scripts/test-gpui-linux-credentials.sh` and `scripts/test-gpui-linux-kwallet.sh`
pass with actual GNOME Secret Service and KWallet 6 on this fresh image. Each
requires an owned Docker session, creates private HOME/runtime/config/data/temp
paths, D-Bus and Xvfb, and runs with networking disabled and source/binaries read-only.
No host HOME or session bus is mounted. The KWallet runner checks its native
dialog's PID before creating the generated encrypted wallet.

Both Electron/Rust directions and unchanged native keys pass. Receipts are
`target/live-acceptance/linux-kwallet-20261011/clean-recipe-verification.json` and
`gnome-clean-recipe-verification.json`. CI includes both checks and retains their
receipts; remote CI is still unrun. macOS Keychain, older KWallet/platform fixtures,
original user wallets and live Google authorization remain separate gates.

### 2026-10-14 Open VSX themes (virtual UI and public registry)

The Open VSX enhancement passes 50 core, 144 backend and 51 GPUI app tests on
Windows. Its opt-in network GPUI test passes separately, clicking Install, Apply
and Search against Open VSX in a generated profile. The public backend smoke
imports Dracula 2.25.1 with registry checksum verification, retaining two variants
and 85 syntax rules each, then saves/reloads 51,139 bytes of theme data and removes
the profile. Saved light/dark selection, updates, custom copies, extension-field
preservation and editor theme changes with undo are covered by the app regressions.
Logs and source/log hashes are in
`target/live-acceptance/openvsx-20261014/receipt.json`. These are virtual GPUI windows;
rendered platform-desktop acceptance has not been run for this enhancement. No
physical Windows desktop input, user profile, extension execution, retained VSIX
or release is part of this verification. See [Open VSX themes](OPEN_VSX_THEMES.md).
