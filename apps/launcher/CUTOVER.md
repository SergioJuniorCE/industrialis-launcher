# Replacement acceptance and source removal

## Stable cutover authorized 2026-10-17

The owner explicitly chose **Replace Electron in stable**, accepting the remaining
checks listed in [release notes](RELEASE_NOTES.md). They remain follow-up work;
this decision does not turn unrun scenarios into passed tests. GPUI 0.4.0 owns
the production launcher tag workflow. Electron source/build dependencies are
removed; the original profile path and installer retirement safeguards remain.
The immutable last-Electron release is launcher-v0.3.0.

The functional port reads and writes the original profile in place. There is no
profile import, relocation, migration wizard or mandatory reconnect. The tracker
is [MIGRATION_TRACKER.md](MIGRATION_TRACKER.md); this file lists the remaining
release actions in their required order.

## Historical local evidence

The [latest native acceptance record](MIGRATION_TRACKER.md#2026-10-11-native-functional-acceptance-and-remaining-external-gates)
closes F03/F04/F05/F10/F22 after the complete installed Windows keyboard,
picker, editor and icon run passes. Together with earlier functional closures,
F15's [native copy cue verification](MIGRATION_TRACKER.md#2026-10-11-f15-copy-completion-cue-verification)
brings the total to 24 completed migration tickets and 15 remaining gates.
The remaining tickets depend on provider configuration,
original-account testing, physical/platform checks, release access or cutover approval.
Sound selection (F40) is a separate requested enhancement, not an Electron removal gate.

Windows has 233 passing Rust tests. Linux has 230, including 44 app tests with GPUI
interactions, in a read-only source container. Native Windows process tests cover cross-runtime
ownership, second launch, detached games, durable logs and exactly-once exit
accounting. A synthetic profile passes actual Electron safeStorage ↔ Rust
encryption/decryption without replacing its key on Windows DPAPI and a private
Linux GNOME Secret Service and KWallet 6 keyrings. The Linux native test caught a libsecret
unload crash; its library handles now live for the process lifetime. The isolated NSIS handoff test
passes install, readiness failure, PID waiting, update, restart and uninstall.

Actual Java 8/17+ clients also pass IPv6 loopback custom-port status/ping and
login-start, normal close and exactly-once accounting after preserving bracketed
IPv6 hosts for Forge's address parser. Windows preview/stable and Linux preview
artifacts are rebuilt after this fix; their provenance is in the latest tracker
record. The release workflow now verifies and boots the actual Linux portable
after packaging, with separate acceptance evidence. Local checks pass; the remote
workflow and physical/platform gates remain open.

The [2026-10-12 Files repair](MIGRATION_TRACKER.md#2026-10-12-files-contrast-and-native-code-editor-repair)
adds readable Sandstone surfaces, bounded complete file browsing, visible Easy
controls, syntax highlighting and a focused Find bar. Native dark/light paired
saves pass on an owned Linux Xvfb; the current Windows editor tests also pass.
This strengthens F01/F02 evidence without closing the remaining external gates.

The [hourly pack checks and drag feedback record](MIGRATION_TRACKER.md#2026-10-12-hourly-pack-checks-and-drag-insertion-feedback)
adds startup/hourly catalog polling, a context-menu manual check and a dashed
insertion slot matching the saved order. Dark/light native drag/menu checks pass;
the current full serial suites contain 44 app tests on each platform.

Actual GTNH 2.9.0-RC-2 Java 8 and Java 17+ archives install and resolve on Windows.
Copy, reinstall, replacement with 2.8.4 and update back to 2.9.0-RC-2 preserve
seeded player files, JourneyMap data, options, config overlays and unknown settings;
selected custom mods survive and unchecked mods are removed. On Debian 13 x64 in
an isolated virtual desktop, both real clients complete Forge mod loading and
respond to Stop, with persisted logs and play time. These use offline acceptance
accounts, not user credentials; world gameplay and physical input remain open.

Both actual Linux clients also survive two owning launcher-process closes,
recover the same game PID, then Stop from a fresh owner. Environment/wrapper,
pre/post hooks, JVM/RAM/window arguments and exactly-once captured logs/play time
pass. The final Java 8 runner exits successfully after the stricter complete-log
and single-exit checks. This covers backend process lifetime in Xvfb; physical
GPUI gestures and native Windows/macOS lifetime acceptance remain open. Both
actual Linux clients now also pass normal window close with code 0; Java 17+
passes injected SIGKILL failure handling. Each records one exit, one post-exit
hook and bounded play time, and repeated recovery leaves time unchanged. Both
clients reach a custom-port probe and complete status/ping/login-start. The
probe disconnects before world entry. Both clients also enter a fresh private
Forge server world and receive an actual in-world chat marker through an
explicit 200 ms-per-direction diagnostic latency control, then close normally.
Immediate local-server auto-join in the software-rendered fixture fails inside
Forge's startup handshake. A matched zero-delay run with the same proxy hostname
and port reproduces the failure; that timing-sensitive path remains open. Natural
Minecraft crashes, ordinary server acceptance and physical gameplay remain
separate scenarios.

The actual Linux preview portable archive is now built and verified on Debian
13 x64. Its extracted executable boots and renders a 1100x750 window on a new
isolated Xvfb/Openbox desktop, closes with code 0 and retains a seeded unknown
settings field. Executable/archive bytes, resources, licenses/notices, dynamic
libraries and manual-update guidance pass. This establishes packaged boot on
that fixture OS; physical desktops, Wayland, older-distribution compatibility
and production distribution acceptance remain open.

The portable also reads the normal Linux profile location with the data-root
override removed. A directly launched second instance and the actual installed
desktop/MIME handler through `xdg-open` each restore the same minimized owner to
foreground. Handler registration is confined to the disposable HOME; the checked
preview uses the explicit replacement-protocol flag. These results retain the
packaged executable's recorded provenance and do not certify newer source edits.

On 2026-10-08, actual Windows Microsoft device-code sign-in, playable license,
profile/skin resolution, live token refresh and persisted-account reload pass in
a newly authorized expendable profile. Existing original-account compatibility
and physical GPUI account interaction remain separate acceptance scenarios.
Google acceptance is pending the user's desktop OAuth client configuration.

The native Windows CLI also reads the original normal app-data profile without
changing its saved settings, accounts, groups or instance documents. All 11
checked metadata files retain their exact bytes; one account and two instances
are recognized without read errors. This verifies in-place parsing, without
refreshing original credentials or operating their native UI.

The saved, newly authorized Microsoft test account also passes real Java 17+
GTNH client loading and Stop on the isolated Linux desktop, without another
sign-in. Account persistence and play time pass; online-server authentication,
world gameplay, native account controls and pre-existing original accounts
remain outside this evidence.

The prepared stable build recognizes marked GPUI `launcher-v*` releases and uses
the Windows filename accepted by Electron's existing updater. Its installer
includes guarded Squirrel retirement. Synthetic tests cover running/overlap/data
guards, failure rollback and cleanup of only the obsolete protocol handler.
The actual public 0.3.0 Squirrel installation is also retired successfully inside
a disposable Windows Sandbox, preserving nine generated profile files before
first boot. Clean-VM boot exposed an external MSVC runtime dependency in the
previous Windows artifacts; packaging now requires static CRT and inspects the
actual PE imports. The rebuilt stable artifact passes native rendering, normal
close, protocol restoration/foreground activation of the same owner and uninstall,
with all post-boot profile files retained. The full host runner exits 0 and
cleans up its owned VM. `Local State` is a generated marker; original encryption-key
and live-account acceptance are separate. No host installation or
production distribution was changed. See
[the live acceptance runbook](LIVE_ACCEPTANCE.md) and [update channels](UPDATES.md).

`node scripts/rehearse-gpui-cutover.mjs` copies only GPUI sources/resources into an
isolated source snapshot with no Electron app. It compiles/tests the port, runs
the frozen old ownership protocol against native processes and renders a hidden
window. It never copies user profiles or replaces a running development EXE.
Snapshots, source hashes and results stay under GPUI's ignored `target` directory.

`node scripts/rehearse-gpui-workspace.mjs --verify` additionally creates a
source-only monorepo snapshot without Electron, removes legacy build aliases and
its zip override there, redirects the copied version command to GPUI, and prunes
the copied pnpm lockfile. It installs dependencies with scripts disabled, checks
lint/types/tests, builds server/dashboard/website and verifies packaging/notices.
The 2026-10-08 Windows run passes: 205 Rust tests, 27 server tests, 8 dashboard
tests and 4 packaging/notice tests. The snapshot has no Electron runtime/build
dependencies. Evidence is `target/workspace-removal-3HBHVu/result.json`.
This complements the native entrypoint/process/render rehearsal above; it does
not build a new production launcher, switch public distribution or remove the
original app in this checkout. `--online` allows missing public registry metadata
and packages; the default uses the local cache only. Corepack is located next to
Node, or supplied with `GPUI_REHEARSAL_COREPACK` pointing to its `dist/corepack.js`.

## Follow-up acceptance (accepted for 0.4.0)

1. **Existing accounts and providers (F16–F19, F25, F36).** On each supported OS,
   open an existing account and cloud configuration without reauthentication.
   Verify token refresh and usable credentials before testing fresh sign-in.
   With expendable test data, upload, interrupt/retry, restore and prune a Google
   backup; compare restored bytes and verify deletion/retention scope. Offline
   HTTP fixtures establish protocol behavior, not real provider acceptance.
2. **Actual packs and games (F23, F28, F36).** Install supported Java 8 and Java
   17+ packs in expendable instances, launch and stop them, verify args/RAM/server/
   environment/hooks, game exit/crash logs and play time. Exercise copy, update
   and reinstall while preserving player data, mods and config overlays. The
   automated real-pack and virtual-client subset above passes. Finish physical
   gameplay, native account launches, Forge server/world auto-join, natural crashes
   and actual game lifetime across physical launcher close/reopen on each target.
   Linux backend detached lifetime/hooks/arguments and saved Microsoft client
   loading now pass; they do not close the physical/platform gates.
3. **Native desktop acceptance (F20–F22, F31, F38).** Windows x64, macOS arm64
   and Linux x64 must pass packaged boot, focus, IME, keyboard navigation,
   dialogs, links/folders and second-launch activation. On macOS verify both
   main-window close (hide main only) and Dock/menu Quit (save/drain). Linux
   currently minimizes instead of hiding because the pinned adapter's hide API
   does nothing; record acceptance or implement an alternative. Screen-reader
   and physical window-drag behavior also need native acceptance.
4. **Installer replacement (F26, F33, F39).** The actual public 0.3.0 Squirrel
   replacement and installed protocol/boot/uninstall subset passes in Windows
   Sandbox. Finish running-user handoff, original live credentials, physical
   installation and older supported Windows acceptance. Remove executable
   entry points/shortcuts/startup behavior
   without deleting `%APPDATA%/industrialis-launcher`, `Local State`, accounts,
   cloud credentials or instances. Do not run both launchers against the profile.
   Cooperative owners are mutually exclusive; the older released binary guard
   is only best effort and cannot stop an old app starting later. The packager's
   `--replacement` flag registers `industrialislauncher:`; it does not uninstall
   the old launcher. The separate `--channel stable` build includes the guarded
   retirement helper: extract GPUI first, wait for normal launcher closure, invoke
   the original Squirrel uninstaller and verify old executables are gone. Its
   synthetic rollback tests do not establish real partially failed Squirrel
   recovery. Windows protocol launch, handler ownership and dead-handler cleanup
   pass in the isolated replacement; finish the other platform/published scenarios.
   Preserve any required old protocol
   handler until the actual replacement sequence is established.
5. **Published updates (F32–F33).** Establish signing/notarization policy; build
   and launch native artifacts on all targets. Run a real published GPUI N → N+1
   Windows update with digest verification, busy/close/save guards, failure
   recovery and preserved profile. Verify documented manual updates elsewhere.
   Publishing is a separate action; local packaging does not publish anything.
6. **Repository/distribution switch (F34).** Root launcher dev/build/installer/
   portable commands select GPUI; `pnpm dev` excludes Electron and explicit
   legacy aliases remain for reference checks. Switch production version scripts,
   release assets, website download targets and stable update policy together. Keep
   server/dashboard/website packages intact. The GPUI preview feed intentionally
   remains separate from Electron's current stable feed until this step.
7. **Remove Electron (F35).** Keep a last-Electron release/tag and a documented
   binary rollback path using the same untouched profile. Run the source-removal
   rehearsal, then remove `apps/launcher` and Electron-only scripts/dependencies,
   regenerate the monorepo lockfile and run root checks plus packaged smoke tests.
   Replace live reference links in GPUI docs/skills with GPUI-owned descriptions
   or fixed historical links. Removing source never removes user data.

Each scenario needs platform, OS/build version, artifact provenance, exact result
and failure/recovery evidence recorded in the tracker. Do not mark a native/live
gate complete based on a source build or a simulated pointer test.
