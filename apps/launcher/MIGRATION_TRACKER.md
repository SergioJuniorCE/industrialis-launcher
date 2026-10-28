# Launcher launcher replacement tracker

## Stable cutover authorized 2026-10-17

The owner explicitly chose **Replace Electron in stable**, accepting the remaining
checks listed in [release notes](RELEASE_NOTES.md). They remain follow-up work;
this decision does not turn unrun scenarios into passed tests. Launcher 0.4.0 owns
the production launcher tag workflow. Electron source/build dependencies are
removed; the original profile path and installer retirement safeguards remain.
The immutable last-Electron release is launcher-v0.3.0.

The native [0.4.0 stable release](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.4.0)
was published and its public downloads/updater feed verified on 2026-10-19.
See [published cutover evidence](CUTOVER.md#public-stable-release-verified-2026-10-19).

Audit date: 2026-10-06. The goal was to replace the Electron launcher without
losing existing workflows or user data. `apps/launcher` now contains the native
application; the former Electron source has been removed.

Latest local baseline: 2026-10-19, 289 Windows Rust tests passed (65 app,
164 backend, 7 profile persistence, 53 core; two external opt-in tests ignored).
The latest editor/layout checks include native Windows Sandbox interaction.
The last full workspace baseline (2026-10-12) has 242 Windows / 239 Linux tests.
Pack status checks run at startup and hourly, with manual checks in the instance menu.
Dragging shows a stable rounded dashed insertion slot and saves the previewed order.
Files now have bounded scrolling, discoverable Easy/Advanced modes and a highlighted
native editor with a visible Find bar; isolated dark/light editing and paired saves pass. Actual
Java 8/17+ GTNH installation, maintenance preservation and virtual-desktop client
loading/Stop, saved Microsoft client launch and Linux detached recovery now pass.
Actual Electron 0.3.0 replacement now passes in Windows Sandbox, including installed
Launcher boot/protocol/uninstall and generated-profile preservation. Google, physical
desktops, running-user/published updates and cross-platform cutover remain open.
See the latest records below and
[LIVE_ACCEPTANCE.md](LIVE_ACCEPTANCE.md) for reproducible scenarios.

This is the functional source of truth for the migration. Pixel alignment,
spacing, decorative effects, and screenshot matching belong in
[UI_PARITY.md](UI_PARITY.md) and can wait. Editing saved themes and choosing
instance icons are features, so their missing controls remain here.

Functional tickets can close on recorded UI-to-disk and failure-recovery evidence;
the remaining native platform, provider and published-update gates stay in their
dedicated tickets. Physical desktop automation remains disabled at the user's
request. Native interaction checks use only disposable Windows Sandbox or an
owned Linux Xvfb desktop.

## How to maintain this file

Keep the stable `Fxx` IDs when adding implementation notes or linking issues.
Check an item only after its acceptance condition is verified; record the date,
platform, command or scenario, and result underneath it. A Rust service without
a reachable Launcher workflow is **Backend only**. **Partial** means some of the
workflow exists. **Verify** means implementation exists but replacement-level
evidence is missing. **Missing** means no equivalent implementation was found.
**Decision** means the old code exposes a capability whose current UI does not
use it; decide explicitly whether to retain it.

`P0` blocks safe cutover or deleting Electron. `P1` is existing functional parity
and must be completed or explicitly retired before deletion. `P2` is a smaller
interaction gap; it still needs a recorded disposition. Priorities describe
cutover risk, not mandatory implementation order. There is no reliable completion
percentage: command counts do not measure the size or safety of a workflow.

## Audit basis and current evidence

Reviewed the Electron app's main/preload lifecycle, React views and session/store,
all **69 backend command cases**, backend services, settings/account/group JSON,
file overlays, process/log handling, root build scripts, and release workflow.
Compared these with the Rust core/backend/CLI and the launcher app's actual call sites.
The command inventory below covers every case in Electron's `LauncherBackend`.
It is an equivalence map, not a requirement to reproduce Electron IPC in Rust.

On Windows, `cargo test --workspace --locked` passed during this audit:
**14 app + 72 backend + 45 core = 131 tests**, zero failures. The tests include
offline staged install/update/reinstall, copy/delete guards, account persistence,
logs, process helpers, and input logic. They do not prove real Microsoft/Google
authorization, a downloaded GTNH launch, installed-app updates, or other OSes.
This audit did not rerun native interaction or external-service scenarios.

### Existing implementation to build on

| Area               | Implemented coverage                                                                                                                                             | Limits tracked below                                             |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Native shell       | Toolbar, Instances/Processes/Accounts/preferences, Windows drag and window buttons, saved startup size/maximize                                                  | Lifecycle/platform behavior: F31, F38                            |
| Accounts           | Offline creation, Microsoft device flow, default selection/removal, per-instance resolution, skins/license messages, token refresh service                       | Live authorization and migrated credentials: F25, F36            |
| Instance lifecycle | Install, copy, update preview, selected custom mods to keep, reinstall, opt-in copy before replacement, delete, operation reservations                           | Catalog/UI gaps, cancellation, shutdown, live validation         |
| Instance settings  | Display-name editing; window, console, play time, account, auto-join, Java/memory/JVM, commands and environment overrides; serialized per-instance patches       | Backup controls: F19; input: F20-F22; launcher saves: F29        |
| Launch             | JVM/library/assets preparation, accounts, command substitution, wrapper/environment, PID reporting, stop, owned-game exit handling and time accounting           | Relaunch recovery and detached lifetime: F28; real launches: F36 |
| Logs/processes     | Persisted console, bounded visible log window, live tail following, full/crash copy, guarded clear; operation history, progress/logs and dismissal               | Restored-game monitoring: F28; cancellation: F14                 |
| Library            | Existing groups/order/custom icons load; double-click launch, right-click/actions, collapse and split persistence                                                | Management APIs/UI and refresh: F06-F13                          |
| Preferences        | Default Java detection/browse/selection, built-in/custom theme editing, dark/light switch, startup window settings, global/per-instance backup policy and health | Native theme/input acceptance: F20-F24; live backups: F16-F19    |

## Remaining work

### Files, configuration, and custom mods

Reference: [file editor](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/components/InstanceMinecraftEditor.tsx),
[file backend](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/minecraft-files.ts),
[custom mods](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/components/CustomModsPanel.tsx).
Launcher now exposes Info, Settings, Logs, Files and Mods for an instance.

- [x] **F01 — P1 · Done · Minecraft file browser and file services.**
      Implemented: Files tab, nested directory listing, UTF-8 editor, 2 MB limit, excluded/binary/link/path guards, persistent status/list/removal. Backend tests cover reads, saves, conflicts, rollback and removal; Launcher pointer test navigates and saves against the same profile.
      Repaired 2026-10-12: bounded independent scrolling and fixed-height rows expose every returned entry; the regression opens and saves file 120. Wide painted/clickable row bounds pass. See the editor acceptance record below.
      **Accept:** navigate nested
      `.minecraft` directories, read supported files, reject excluded/invalid paths
      and unsupported/oversized files, and show actionable errors without modifying
      unrelated files.
- [x] **F02 — P1 · Done · Integrated configuration editor.**
      Verified 2026-10-09 on Windows/Linux: the launcher workflow edits Easy boolean/integer/list values, saves both copies, edits Advanced text and reopens the saved file from disk. Conflict and close guards pass. Actual GTNH copy/reinstall/update retains seeded paired config overlays. Native input/platform checks remain in F20/F38.
      Repaired 2026-10-12: persistent Easy/Advanced tabs, syntax highlighting, readable editor surfaces and a visible focused Find bar. Earlier checks did not cover large lists or the missing search presentation; new regressions and isolated native dark/light checks cover both. Physical/platform acceptance remains F20/F38.

      **Accept:** edit a scalar and list in Easy mode, edit raw text, save both game
                                                          file and overlay, restart, then update the pack and verify the override survives.

- [x] **F03 — P1 · Done · Open a file in the OS editor.**
      Verified 2026-10-11: the installed Windows app dispatches the default editor, observes its actual atomic save and mirrors the bytes into the original overlay. Native override removal stops mirroring and does not recreate the removed override after another external edit. Shutdown/maintenance guards pass in backend tests; recorded actual GTNH pack maintenance preserves paired config overrides. Physical and other-platform editor behavior remains F38.
      **Accept:** an
      external edit survives pack replacement, and watcher cleanup does not recreate
      an intentionally removed override.
- [x] **F04 — P1 · Done · Separate editor windows.**
      Verified 2026-10-11: the installed app opens a native editor window, repeated file opening foregrounds that same window without a duplicate, and normal editor close leaves the launcher and saved override intact. Launcher multi-window paired saves, independent close/reuse, shared drafts and launch-time detached-draft saves pass. Physical and other-platform lifecycle checks remain F31/F38.

      **Accept:** pop out a file, focus the existing matching window on repeat,
                                                          synchronize edits, and close either window without losing saved data.

- [x] **F05 — P1 · Done · Custom-mod management.**
      Verified 2026-10-11: actual installed native picker Cancel and two-file .jar/.zip import pass byte checks in game/overlay copies; confirmed removal retains the other custom mod and pack-owned file. The OS folder action opens the original persistent mods directory. Launcher/backend disk reload, failed removal and running/busy guards pass; real GTNH copy/update/reinstall preservation is recorded below.
      **Accept:** pick `.jar`
      or `.zip`, list identity/name/size, remove it, open the mods folder, and verify
      additions/removals across restart, update and reinstall. Update-preview keep
      checkboxes alone do not cover this workflow. An online marketplace is not part
      of the existing feature set.

### Library and instance management

Reference: [App.tsx](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/App.tsx),
[groups service](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/groups.ts),
[icon service](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/instance-icons.ts).

- [x] **F06 — P1 · Done · Change/create instance groups.**
      Implemented: create/change/clear group dialogs using the original instgroups.json. Backend management and framework keyboard group-creation tests pass.
      **Accept:** choose an existing group, type a new
      one, move an existing instance, and clear its group without changing its folder.
- [x] **F07 — P1 · Done · Rename/delete groups.**
      Implemented: rename/delete group, custom Ungrouped name, collision validation and delete confirmation. Tests retain unrelated metadata and instance files.
      **Accept:** changes persist, deleting a group moves members to Ungrouped and
      preserves all instance files, and custom Ungrouped naming still loads.
- [x] **F08 — P1 · Done · Reorder instances.**
      Verified 2026-10-09 on Windows/Linux: native launcher drag persists and reloads order; explicit Move earlier/later controls use shared keyboard activation. A lifecycle regression reloads order after copy, cross-group move and delete while preserving unrelated groups, extension fields and player files.
      Improved 2026-10-12: rounded dashed insertion slots show exact before/after placement. Stable hover, rightward append, empty-group moves, Escape/outside cancellation and persisted order pass. Native dark/light previews and actual drop/save were inspected. Newly discovered ungrouped members now sort deterministically before the first edit, preserving neighbors' visual order.
      **Accept:**
      reorder within a group, restart, and preserve order through copy/move/delete;
      choose drag or an equally reachable keyboard-capable ordering control.
- [x] **F09 — P1 · Done · Shared group persistence.**
      Implemented: instgroups.json is authoritative for membership/order/collapse/Ungrouped name. Existing data is read and edited in place; no migration is required. launcher-workspace.json stores only the split preference. Latest-document transactions preserve extensions and reject stale snapshots.
      **Accept:** define the final authoritative schema, read and edit existing data in place,
      retain groups/order/collapse/name on restart, and verify no stale snapshots
      overwrite changes. Keep the split-layout preference separate from memberships.
- [x] **F10 — P1 · Done · Instance icon management.**
      Verified 2026-10-11: installed native picker Cancel/import, library selection, metadata persistence, reset and the OS library-folder action pass. Reset retains the reusable imported image. Launcher tests reuse it on another instance and reload from disk; backend rollback/path/size guards and recorded real GTNH copy/update/reinstall icon preservation pass.
      **Accept:** import supported images, reuse
      an icon on another instance, clear it, reopen the library folder, and preserve
      icons through copy/update/reinstall. This is functionality, independent of
      cosmetic alignment.
- [x] **F11 — P1 · Done · Complete install version picker.**
      Verified: complete searchable catalogs and All/Stable/Beta filters serve install and maintenance. Launcher tests exercise filtering, unavailable variants and selections beyond the former eight-row limit. Error/reload controls remain reachable. Actual supported Java 8/17+ archives install in expendable instances.

      **Accept:** select any supported catalog version, search/filter releases, see
                                                          available Java variants, reject unavailable variants before installation, and
                                                          retry a failed catalog fetch. The maintenance picker already has a full list
                                                          and archive-availability checks; do not replace that with the limited picker.
                                                          Actual 2.9.0-RC-2 Java 8/17+ and 2.8.4 Java 17+ archive installation passes in expendable instances.

- [x] **F12 — P1 · Done · Pack update status.**
      Updated 2026-10-12: one shared catalog checks at startup and every hour. Manual checks live in the instance context menu; ordinary library/detail check buttons are removed. Unknown/current status is informational; available updates open the matching maintenance workflow. The root owns the polling task and duplicate starts/in-flight requests are guarded. Fake-clock tests verify the hourly boundaries, repeated ticks and cancellation; actual native menu dispatch passes in dark/light. Failed checks retain cached versions and cannot turn unknown status into Up to date. Real catalogs and maintenance were exercised in the earlier GTNH acceptance fixtures.
      **Accept:** known current/old/unknown versions
      show correct status and the update action opens the appropriate workflow;
      catalog failure does not imply an instance is current.
- [x] **F13 — P1 · Done · Refresh instance sizes.**
      Verified 2026-10-09 on Windows/Linux: adding/changing an external file updates visible and saved sizes through the launcher action, with pending state and busy-instance refusal. The queued-write regression caught and fixed a stale metadata overwrite; refresh now patches only cached_size_bytes in the latest document and preserves concurrent settings/extension edits.
      **Accept:** external game/mod/file changes appear after size refresh, with a
      pending state and no stale overwrite of settings or maintenance operations.
      Respect the current running-instance guard rather than forcing a size write.
- [x] **F14 — P1 · Done · Delete cancellation and progress.**
      Implemented: File-by-file progress, Cancel deletion, explicit cancelled outcome, protected discovery/settings during partial cancellation and reservation-safe final cleanup are implemented. Cancellation does not restore deleted files. Synthetic cancellation preserves remaining files/discovery/group metadata, then a retry completes deletion.
      **Accept:** incremental
      progress, cancel action and canceled outcome match the original contract;
      cancellation truthfully reports that already deleted files are not restored,
      leaves group/settings state consistent, and releases operation reservations.
- [x] **F15 — P2 · Done · Copy completion sound.**
      Verified 2026-10-11: the actual Linux Launcher Copy workflow emits exactly one
      native XKB bell request after successful copy finalization and zero for a
      rejected copy. Windows/Linux regressions cover actual copy bytes, failure,
      repeated progress/results, stale retries and copy-before-maintenance.
      Both workflows now use the same generation-checked completion transition.
      Physical speaker output and Windows/macOS/Wayland playback remain F38;
      the cue follows OS sound settings. See the [F15 evidence](#2026-10-11-f15-copy-completion-cue-verification).
      **Accept:** one cue on success, none on failure or
      repeated progress events, or record an explicit decision to retire this cue.

### Cloud backups

Reference: [backup UI](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/components/BackupsPanel.tsx),
[backup service](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/backups.ts),
[automatic manager](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/backup-manager.ts),
[Google Drive](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/google-drive.ts).
Backup services, provider and durable manager now have reachable Launcher workflows. Live Google and native credential-store acceptance remain open.

- [ ] **F16 — P1 · Verify · Google Drive provider and authorization.**
      Implemented: OAuth PKCE loopback authorization, encrypted refresh-token reuse, paginated Drive folder operations and resumable uploads are implemented. Actual Electron 43.3.0/Rust Windows DPAPI, isolated Linux GNOME Secret Service and native KWallet 6 interoperability pass in both directions with the original generated key unchanged. The native Linux regression caught and fixed libsecret unloading between calls. macOS Keychain support compiles; CBC vectors match independent Node crypto. Live Google and Keychain acceptance remain open; older KWallet versions are not certified by the KWallet 6 fixture.

      **Accept:** connect,
                                                          restart, refresh, disconnect and handle denied/expired authorization without
                                                          exposing credentials or falsely displaying Connected.

- [ ] **F17 — P1 · Verify · Manual backup management.**
      Implemented: Local/cloud lists, manifest-last verified uploads, verified collision-safe restores into the existing backups folder, confirmed cloud deletion, folder actions and transfer errors/progress are reachable in Launcher. Synthetic integrity/collision/failure tests pass. Live Drive transfer acceptance remains open.

      **Accept:** upload/download a real archive byte-for-byte, reject a corrupt
                                                          remote artifact, handle an interrupted upload and delete only the selected
                                                          snapshot. Electron's Restore action downloads an archive into the backups
                                                          directory; it does not unpack or replace a running world's saves.

- [x] **F18 — P1 · Done · Automatic backup manager.**
      Implemented: Automatic settling/deduplication, original backup-state.json queue, retry/backoff/restart recovery, manual retry/scan, deletion tombstones and cloud-only retention are implemented. At most two instances upload concurrently; same-instance retention is serialized. Settling, bounded concurrency, durable delivery and empty-progress completion regressions pass.

      **Accept:** new completed archives upload automatically, partial files do not,
                                                          failures survive restart and can retry, deleted snapshots do not reappear, and
                                                          retention never deletes local archives.

- [x] **F19 — P1 · Done · Backup controls and health.**
      Verified: global/per-instance policy, inheritance and retention edits round-trip through native launcher fields into the original settings files without losing extensions. The panel reads existing archives; manager tests cover queued/failed/retried/healthy progress. Retry/scan, transfer progress and the local OAuth setup guide are reachable. Actual Google authorization/transfers remain independently gated by F16/F17.

      **Accept:** enable/disable one instance, inherit/override retention, observe
                                                          queued/failed/healthy states and transfer progress, retry, and open the guide.
                                                          Planned provider cards in Electron are not implementations: SFTP or other
                                                          providers need not be invented to complete this port.

### Input and settings features

Reference: [Launcher input](crates/app/src/input.rs),
[Java picker](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/components/JavaInstallationPicker.tsx),
[theme editor](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/src/components/ThemeEditor.tsx).

- [ ] **F20 — P1 · Verify · Native text editing.**
      Implemented: pinned gpui-base 0.7.1 native InputState/TextareaState used by account, path, group and settings fields; raw printable-key handlers removed. Launcher tests cover UTF-16 composition with emoji, selection, clipboard and undo/redo. Physical IME/dead-key/caret/drag checks remain.
      **Accept:** native composition/dead keys,
      Unicode editing, caret placement, selection, copy/cut/paste and undo/redo work
      in account names, paths and settings without misrouting shortcuts.
- [x] **F21 — P1 · Done · Multiline JVM arguments.**
      Verified 2026-10-09 on Windows/Linux: native multiline field editing/paste saves newlines and quoted spaces into the existing setting; command/environment fields retain values and extensions. Launch parsing preserves quoted arguments across lines. Actual Java 8/17+ game fixtures verify custom JVM/environment/wrapper/pre/post arguments at launch; physical input remains in F20.
      **Accept:** enter/paste/edit multiline JVM arguments, preserve newlines
      and quoting, and launch with the resulting values. Also verify single-line
      command quoting and environment key/value serialization; these controls
      already exist and do not require a new multiline workflow for parity.
- [x] **F22 — P1 · Done · Keyboard and accessible control activation.**
      Verified 2026-10-11 in the installed Windows Sandbox app: native selection semantics, named editable value replacement, account creation/removal/default persistence, all preference sections, theme Enter/Space save, Tab/Shift+Tab, modal Escape/Cancel/Save and distinct group actions pass. Native fields share the editing engine's focus identity. UIA reports disabled Create as enabled; blank activation is nevertheless blocked without changing accounts. That announcement limitation is recorded explicitly, with physical composition/screen-reader and other-platform checks in F20/F38.
      **Accept:** navigate and activate tabs, dialogs,
      accounts, settings and instance actions using the keyboard; focus/Enter/Escape
      behave predictably and controls expose supported names/state. Record platform
      accessibility limitations rather than treating a visible label as proof.
- [x] **F23 — P2 · Done · Java chooser search/sorting.**
      Verified: runtime metadata parsing and numeric sorting/search pass; a launcher workflow selects runtimes and reloads both global and per-instance precedence from disk. Actual Java 8/17+ game launches are recorded. Native executable-picker/platform coverage remains in F38.
      **Accept:** find and
      select from many installations, see the actual runtime metadata, and preserve
      global/per-instance precedence. Compatibility enforcement is a separate scope
      decision in F39; a saved `skip_java_compat` checkbox does not prove enforcement.
- [x] **F24 — P1 · Done · Custom theme management.**
      Verified: an Electron-shaped custom preset loads, edits/saves all supported tokens and overrides, reloads from disk and can be selected again. Launcher pointer reset/delete/fallback preserves unknown root/token/override fields. Platform rendering remains in F38 and visual matching in UI_PARITY.

      **Accept:** load an Electron custom preset, edit/save it and its overrides,
                                                          restart and select it again, then reset/delete safely. Visual matching can wait;
                                                          preserve user-authored settings now (F25).

### Persistence, process lifetime, and desktop behavior

Reference: [Electron main](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/main.ts),
[backend initialization/disposal](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/index.ts),
[Launcher boot](crates/app/src/main.rs),
[Launcher root](crates/app/src/views.rs),
[Rust settings](crates/backend/src/settings.rs).

- [ ] **F25 — P0 · Partial · Drop-in existing-profile compatibility.**
      Launcher/instance settings preserve unknown fields; all twenty theme tokens
      round-trip (Electron's declared override interface actually has ten fields).
      Current/legacy accounts load independently, retain extension fields and keep
      malformed/unsupported rows for recovery. Group saves retain extensions and
      empty groups. Discovery is read-only; corrupt documents block mutation.
      Launcher uses the same data root and filenames in place; no user copy/import or
      migration is required. Settings update only changed fields; unchanged legacy
      accounts keep their raw shape and ordering. Synthetic fixture tests cover these
      paths and untouched icons/logs/PIDs/overlays/mods/cache/backup bytes.
      Windows read-only verification loaded the original profile at the shared root:
      one account, two instances, no read issues, with all 11 checked metadata
      documents unchanged byte for byte. Windows Electron/Rust cloud credential
      interoperability and Linux synthetic persistence pass. The actual Linux
      portable also uses the normal profile path; a private native GNOME keyring
      passes Electron/Rust interoperability. **Remaining:** live original-account
      refresh, macOS paths/permissions/Keychain and live F16
      authorization. Native KWallet 6 now passes in an owned Linux virtual desktop;
      older KWallet versions remain platform-specific checks. See [profile persistence](PERSISTENCE.md). **Accept:** opening and
      saving unrelated preferences cannot erase old values; malformed/unreadable
      data remains recoverable and healthy items still load.
- [ ] **F26 — P0 · Partial · Single-instance/data ownership.** Launcher and Electron
      now acquire the same per-profile process ownership before backend startup;
      writer CLI commands participate, while read-only inspection remains available.
      A second launcher signals the existing window; queued startup activation,
      operation-owner rejection, path aliases, crash release and wrong-endpoint
      refusal are implemented. Real Rust/Node process tests cover cross-runtime
      exclusion and activation, five simultaneous launches and forced-exit release.
      Actual installed Windows and extracted Linux native windows each restore the
      same minimized owner to foreground through second-launch/protocol handoff.
      The real Windows replacement installer waits for an open released Electron
      fixture to close before retiring it and starting Launcher. **Remaining:** macOS,
      Wayland/physical platform checks and production installed-version cutover.
      Older released binaries do not participate:
      the upgrade path must stop them before replacement startup (F33). No profile
      copy or user migration is required. **Accept:** a second installed launch
      focuses the first; cooperating processes cannot write/maintain one profile
      concurrently, and old-install upgrade cannot leave an uncooperative writer.
      Protocol and limits: [profile persistence](PERSISTENCE.md#save-ownership).
- [ ] **F27 — P0 · Verify · Graceful shutdown coordinator.**
      Implemented: editor save guards, external watcher flush, admitted maintenance,
      profile transactions, game capture and backup transfers drain before ownership
      is released. New work is rejected during close. A macOS termination delegate
      cancels Dock/menu Quit until the shared coordinator succeeds, avoiding Launcher's
      200 ms final-cleanup timeout. Synthetic drain/failed-save/quit-gate tests pass;
      real provider shutdown and native macOS termination remain open.
      **Accept:** close during copy/update/reinstall, file saving, token refresh,
      provider transfers and running games without abandoning admitted work or
      erasing edits; errors stay recoverable and game processes remain alive.
- [ ] **F28 — P0 · Verify · Detached games and relaunch recovery.**
      Implemented: an independent game broker captures durable logs and exact
      process identity; relaunch resumes monitoring and reconciles one exit receipt
      and play-time update. Running games survive launcher exit. Three native
      Windows synthetic-child scenarios pass. Actual GTNH Java 8/17+ on Linux
      survives two owning launcher-process closes and recovers logs/Stop/hooks/time
      without another game process. Both actual Linux clients also pass normal
      window close/code 0 and exactly-once exit/hooks/time; Java 17+ passes injected
      SIGKILL failure accounting. Physical Launcher lifecycle, natural Minecraft
      crashes and native Windows/macOS lifetime acceptance remain open.
      **Accept:** launch, close/reopen the launcher, recover logs/PID/stop state,
      handle crash and normal exit, and record play time once without killing a
      reused PID or another instance.
- [x] **F29 — P0 · Done · Reliable settings saves.**
      Implemented: serialized latest-document patches, coalesced preference saves,
      pending instance edits, conflict/error retention and explicit retry/discard
      close guards preserve unknown fields and raw unchanged values.
      Synthetic concurrent/failed/read-only/cancelled-caller and Launcher save-retry
      tests pass on Windows/Linux; settings are reloaded from disk in the workflow tests. Original-profile and native lifecycle gates remain in F25/F27/F31/F38.
      **Accept:** rapid edits persist the latest values; delayed saves cannot
      overwrite newer state; close waits for edits, and failed saves retain a
      recoverable draft rather than silently discarding it.
- [x] **F30 — P0 · Done · Recoverable startup and refresh.**
      Implemented: startup/refresh warnings have a persistent recovery panel with
      Retry profile reads, independent of transient activity status. Valid previous
      snapshots survive errors. Maintenance uses the shared generation-checked
      reader instead of dropping discovery errors or applying late snapshots.
      Launcher corrupt-document/last-known-state/repair/retry regression passes.
      **Accept:** surface errors, retain valid last-known data, reject stale loads,
      and prevent fallback state from overwriting recoverable data.
- [ ] **F31 — P1 · Verify · Window lifecycle.**
      Implemented: saved dimensions/maximize, Windows native hide/restore, second
      launch and URL/reopen activation. macOS main-window close hides that window,
      retaining the app and other editors; Dock/menu Quit uses F27. Linux uses
      minimize because the pinned adapter's hide API does nothing. The actual
      Linux preview portable passes virtual-desktop boot/render and normal close,
      preserving a seeded unknown settings field. Physical
      Windows/macOS checks and the Linux behavior decision remain open.
      **Accept:** verify minimize/hide/restore, foreground after second launch,
      main/editor close and supported OS lifecycle; record platform changes.
      Network, startup and slow Java work must keep the window responsive.

### Updates, distribution, and deleting the original app

Reference: [release updater](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/release-updater.ts),
[installer downloader](https://github.com/SergioJuniorCE/industrialis-launcher/blob/launcher-v0.3.0/apps/launcher/electron/backend/launcher-updater.ts),
[release workflow](../../.github/workflows/launcher-release.yml),
[root commands](../../package.json),
[build wrapper](../../scripts/run-build-launcher.mjs).

- [ ] **F32 — P0 · Verify · Launcher self-update.** Implemented launcher-only
      paginated GitHub checks/semver comparison, startup checks in distribution
      builds, toolbar and About actions/dialog/retry, required SHA-256/size/header
      verification, progress and failed-download cleanup, busy/modal/close guards,
      Windows readiness/PID handoff and restart. Portable/macOS/Linux use the release
      page. Local backend/Launcher tests and isolated native NSIS install/update/restart/
      uninstall smoke test pass; real published N → N+1 remains unverified. See
      [updates documentation](UPDATES.md). **Accept:** packaged
      version N detects N+1, selects the right trusted asset, handles failure and busy
      state, updates/restarts on Windows and opens the documented release path on
      other OSes. A pack update is not a launcher update.
- [ ] **F33 — P0 · Partial · Launcher packaging and release pipeline.** Dedicated
      `launcher-preview-v*` preview workflow and native packager now produce Windows
      installer/portable ZIP, macOS arm64 app/DMG and Linux x64 archive; previews stay
      outside Electron's stable update feed. Windows resources carry icon/version
      metadata, and CI includes backend/core/artifact contracts, Launcher interactions and native Windows
      handoff smoke tests. Local Windows fixture installer lifecycle passes. An
      actual Debian 13 Linux preview portable passes archive/input/notices/resource
      verification and virtual-desktop boot/render/close. Remote CI, macOS and
      physical Linux launches, signing/notarization, production install/uninstall
      and running-user replacement remain unverified.
      The stable build mode now emits the old updater's required Windows filename
      and embeds guarded Squirrel retirement; synthetic running/data/overlap,
      protocol-cleanup and native installer rollback tests pass. Actual public
      Electron 0.3.0 retirement, installed Launcher rendering, same-owner protocol
      activation and uninstall pass in Windows Sandbox without an external MSVC
      runtime. With the real old window open, replacement waits for its normal
      fixture close; all eleven post-close metadata/player fixture files survive
      before Launcher boot. The old app legitimately updates two metadata documents
      during its own boot/close, so preservation uses the post-close baseline.
      Production
      activation and other supported-platform acceptance remain gated.
      **Accept:** native artifacts install/run/uninstall on supported targets, carry
      icons/version metadata and required resources, document runtime requirements
      and signing/notarization policy, and publish the assets expected by downloads
      and F32. Verify upgrade from the old Windows installer and preserve user data.
- [x] **F34 — P0 · Done · Repository/distribution cutover.** Stable replacement
      was authorized on 2026-10-17 with the recorded acceptance gaps retained as
      follow-ups. Electron source/build wrappers and workspace dependencies are
      removed. Root commands and version stamping target Launcher; launcher-v* tags
      now select the stable native workflow and the existing Windows download
      filename. Historical source references point at launcher-v0.3.0. PR #65
      merged as 5a864376e2acd521535a0ca31e985bab4d3ba9d8; launcher-v0.4.0
      published successfully on 2026-10-19 after all three platform jobs passed.
      All four public artifacts match their GitHub SHA256 digests, and the existing
      latest Windows installer URL returns the same bytes. The public stable feed
      offers 0.4.0 to 0.3.0 and reports 0.4.0 up to date. See the
      [published cutover evidence](CUTOVER.md#public-stable-release-verified-2026-10-19).
      Server/dashboard/website packages are preserved. Accepted provider/platform
      follow-ups and the published native N-to-N+1 cycle remain separate checks.
      **Accept:** the native production workflow and download links publish the
      synchronized version, Electron-only dependencies are absent, and root checks
      plus packaged smoke tests pass.
- [x] **F35 — P0 · Done · Own all runtime/build resources.** Brand artwork,
      launcher ICO/ICNS and GTNH fallback artwork now live under launcher-owned assets.
      Both compile-time Electron asset dependencies were removed. The nine Aditya
      preset files and full Lucide artwork attribution are also launcher-owned;
      distribution bundles project/dependency/artwork notices. An isolated source
      snapshot without an Electron directory compiled the actual app and rendered
      the real launcher benchmark successfully; Windows release packaging uses only
      launcher-owned assets. **Accept:**
      relocate required assets/config presets/notices into launcher-owned or shared paths,
      audit build/packaging/skills references, and build/run from a checkout with
      `apps/launcher` absent. Historical source attribution may remain as text;
      runtime and build dependencies may not.
- [ ] **F36 — P0 · Verify · Live accounts/install/game workflows.** Services
      exist. Actual GTNH 2.9.0-RC-2 Java 8/17+ installation/config resolution and
      copy/reinstall/2.8.4 replacement/update preservation pass on Windows.
      Both real clients complete Forge mod loading and Stop on Debian 13 x64
      with offline accounts in a virtual desktop, retaining logs and play time.
      Fresh Microsoft sign-in/profile/license/skin/refresh/persistence pass on
      Windows. The saved authorized test account launches an actual Java 17+
      client on Linux without another sign-in. Actual offline Java 8/17+ detached
      reopening, environment/wrapper/pre/post hooks, JVM/RAM/window arguments and
      exactly-once logs/time pass on Linux. Both actual clients pass custom-port
      status/ping/login-start and normal close; Java 17+ passes injected abrupt
      exit. Both also enter a private Forge world and receive its in-world chat
      receipt with an explicit 200 ms-per-direction diagnostic latency control.
      Actual IPv6 loopback custom-port status/ping/login-start and normal close
      now also pass for both clients after retaining IPv6 brackets for Forge.
      Immediate local-server auto-join fails inside the shipped Forge startup
      handshake in the software-rendered fixture and remains unresolved.
      Existing-original-account compatibility, Google, physical gameplay/input,
      ordinary Forge server auto-join and native account/
      platform checks remain open.
      **Accept:** real Microsoft login/profile/license/token refresh and errors;
      real catalog/download/install of supported Java 8 and Java 17+ packs; successful
      Minecraft startup, arguments/memory/account/server/environment/commands;
      stop/crash/logs/time handling; copy/update/reinstall preserving player data,
      overlays and selected mods. Use expendable instances and record actual versions.
- [ ] **F37 — P0 · Verify · Automated functional regression coverage.** Windows
      has 223 passing Rust tests; Linux has 220 including Launcher interactions. Native
      process, ownership, credential interoperability, installer and packaging
      contracts pass locally. CI now includes regression and installer checks;
      remote execution and native macOS/provider acceptance remain open.
      **Accept:** profile compatibility, file/config/mod management, groups/icons,
      backup/provider failures, shutdown/recovery, save races, cancellation and updater
      contracts have coverage; CI runs the Rust workspace. Tests should exercise
      observable behavior/failure recovery, not only mirror rendering code.
- [ ] **F38 — P0 · Verify · Supported-platform packaged smoke tests.** Current
      local evidence includes Windows artifacts, Linux native app compilation,
      both platforms' Launcher tests and hidden native rendering, plus actual Linux
      preview portable boot/render/close with a fresh profile and preserved unknown
      settings. macOS and full
      installed desktop/provider/game scenarios remain open. **Accept:** Windows x64,
      Linux x64 and macOS arm64 artifacts boot, accept keyboard/native dialogs,
      resolve data/resource paths, open external links/folders, launch/stop Java,
      recover running games, close safely and upgrade without losing profiles.
      Include OS/architecture/version and artifact provenance in each test record.

### Explicit scope decisions

- [ ] **F39 — P1 · Partial · Retained legacy contracts and protocol.**
      Retain the Aditya preset as backend and CLI preset-status / apply-preset
      services: all nine original bundled configs are embedded from launcher-owned
      resources; enable writes game/overlays and patches existing Gadomancy;
      disable removes only overlays. Unknown presets, running/busy instances,
      links and protected destinations reject safely. Retention tests pass.
      Overlay listing remains an intentionally backend-only contract (there was
      no original UI caller). Group moves and direct icon selection have equivalent
      services and reachable Launcher workflows.
      Keep industrialislauncher: activation; there was no frontend subscriber
      for OAuth deep links, so no new OAuth consumer is invented. Installed scheme
      registration is implemented behind the packager's --replacement flag on
      Windows/macOS/Linux. Installed Windows handler ownership, restoration of the
      same minimized owner to foreground and dead-handler cleanup on uninstall
      pass in the actual replacement Sandbox. Linux desktop/MIME registration and actual
      `xdg-open` activation also restore the same minimized owner in the owned
      virtual desktop. macOS and published replacement remain open.
      This flag does not uninstall Squirrel/Electron.
      The prepared stable-channel installer additionally embeds guarded retirement;
      actual Squirrel retirement also passes in the generated-profile Sandbox,
      alongside synthetic failure/rollback/handler-cleanup scenarios.
      Keep skip_java_compat data/control without inventing a launch check that
      neither original backend had.
      **Accept:** retained services/resources and installed protocol survive
      Electron deletion, with intentionally unused UI contracts documented.

## Post-migration improvements

These are requested enhancements, not missing Electron parity or removal gates.

- [ ] **F40 — P2 · Planned · Select a copy completion sound.**
      Requested 2026-10-11. Launcher currently uses the OS system bell and has no
      launcher-specific sound selector.
      **Accept:** choose and preview an alternative completion cue, persist the
      choice in the shared launcher settings across restarts, and retain the
      system bell as the default. A missing/unplayable selection must not fail a
      copy. Keep F15's one-cue-on-success and no-cue-on-failure/repeated-progress
      behavior for standalone and copy-before-maintenance workflows.

- [x] **F41 - P2 - Done - Modernity and third-party content foundation.**
      Requested and verified 2026-10-12. The instance Add-ons tab retains Mods and
      adds Resource packs with local ZIP import, inventory and managed removal.
      The Modernity browser resolves the newest matching 2.8/2.9 weekly release,
      including every checksum-bearing main/UI/texture pack and MyCTMLib JAR.
      Downloads verify exact size/SHA-256, guard stopped instances and collisions,
      and use the original persistent overlay. Optional items are selected
      individually. Existing pack-owned files and duplicate mod identities are
      protected. Shared artifact types, validation, provenance and installation
      form the provider extension point.
      **Accept:** reachable main/optional pack and mod installation, invalid-file
      failure, safe removal and real maintenance preservation in disposable
      profiles. Evidence and remaining general-marketplace work:
      [third-party content](THIRD_PARTY_CONTENT.md). This is a requested enhancement,
      separate from the 39 original parity tickets and external cutover gates.

- [x] **F42 - P2 - Done - Open VSX color themes.**
      Requested 2026-10-14. Appearance searches the public Themes catalog and
      imports checksummed static color-theme packages. Each variant is a Saved
      theme with native launcher colors and the file editor's TextMate syntax
      colors. Apply selects its brightness; saved variants work offline.
      Explicit package updates retain stable IDs and unknown saved-theme fields.
      JSONC includes and XML TextMate themes are supported; extension code and
      VSIX packages are not retained. Existing settings remain authoritative.
      **Accept:** live search and Install/Apply controls, light/dark selection,
      update/reload/clone preservation and editor theme switching without losing
      text/undo, all using disposable profiles. See
      [Open VSX themes](OPEN_VSX_THEMES.md). This enhancement is separate from
      the original 39 parity tickets and external Electron cutover gates.

- [x] **F43 - P2 - Done - Prism Launcher import.**
      Requested 2026-10-19. A one-time first-run prompt offers to copy instances
      from Prism Launcher; the same flow is permanent from the toolbar Add menu
      and the library background menu. GTNH instances are pre-selected, while
      other instances can be selected with a frozen/updates-disabled warning.
      Discovery checks the per-OS Prism data directories and honors a custom
      `InstanceDir`, with a typed-folder fallback. Copies only: the source is
      never moved, registered, or written. Name collisions receive a monotonic
      suffix. Metadata maps display name, icon, Java path, RAM, JVM arguments,
      and Prism groups (empty groups skipped); accounts are not copied. Imported
      GTNH copies stay frozen until a GTNH version is assigned, either before
      import or later from Info > GTNH version; assigning one enables update
      checks, updates, and reinstall. Other imported instances stay frozen with
      those actions unavailable. Imports run as cancellable Processes operations
      with per-instance failure isolation, and failed imports can be retried
      from the process card. The one-time choice is stored in
      `launcher-settings.json`; dismissing with Escape does not record it.
      **Accept:** first-run and menu entry points, GTNH/non-GTNH selection,
      copy-only source preservation, marker persistence, frozen gating and
      version assignment, using disposable Prism-shaped fixtures. Windows Launcher
      and backend fixtures pass; actual Prism profiles and macOS/Linux/Flatpak
      discovery remain platform verification. This enhancement is separate from
      the original parity tickets and external cutover gates.

## Electron backend contract coverage

`I` = equivalent service and reachable app workflow exist, still subject to
verification gates. `P` = partial. `B` = backend only. `M` = missing.
`D` = scope decision. Commands sharing an equivalent capability share a row.
Native window/desktop hooks outside this switch are covered by F03-F04,
F20-F22, F26-F28, F31 and F39.

| Electron commands                                                                                                                                                 | Coverage | Tracking / equivalent Rust area                             |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------- |
| `get_versions`                                                                                                                                                    | I        | `pack_files`; install picker F11, status F12, live F36      |
| `get_instances`                                                                                                                                                   | I        | `instances`; errors/migration F25, F30                      |
| `refresh_instance_sizes`                                                                                                                                          | I        | `install::refresh_sizes`; F13                               |
| `get_instance_groups`                                                                                                                                             | I        | Original instgroups.json; F06-F09                           |
| `set_instance_group`                                                                                                                                              | I        | Group management UI + groups service; F06                   |
| `rename_group`, `delete_group`                                                                                                                                    | I        | F07                                                         |
| `move_instance_in_group`                                                                                                                                          | I        | F08, F39                                                    |
| `set_group_instance_order`                                                                                                                                        | I        | F08                                                         |
| `set_group_collapsed`                                                                                                                                             | I        | Original group collapse persistence; F09                    |
| `delete_instance`                                                                                                                                                 | I        | `instance_ops`; F14, F27                                    |
| `cancel_delete_instance`                                                                                                                                          | I        | F14                                                         |
| `copy_instance`                                                                                                                                                   | I        | `instance_ops` + actions; F15, F27, F36                     |
| `open_instance_folder`                                                                                                                                            | I        | Native folder action; F38                                   |
| `open_minecraft_file_external`                                                                                                                                    | I        | F03                                                         |
| `open_mods_folder`                                                                                                                                                | I        | F05                                                         |
| `open_backups_folder`                                                                                                                                             | I        | F17                                                         |
| `get_google_drive_status`, `configure_google_drive`, `connect_google_drive`, `disconnect_google_drive`                                                            | I        | F16                                                         |
| `get_backup_dashboard`, `scan_backups`, `retry_backup`                                                                                                            | I        | F18-F19                                                     |
| `list_local_backups`, `list_cloud_backups`, `upload_backup`, `download_backup`, `delete_backup`                                                                   | I        | F17-F18                                                     |
| `save_settings`, `get_settings`                                                                                                                                   | I        | `settings` + instance editor; F19-F21, F25, F30             |
| `download_install`                                                                                                                                                | I        | `install` + actions; F11, F27, F36                          |
| `preview_update_mods`, `update_instance`, `reinstall_instance`                                                                                                    | I        | `install` + maintenance dialog; F27, F36                    |
| `list_minecraft_entries`, `read_minecraft_file`, `write_minecraft_file`, `delete_persistent_file`                                                                 | I        | F01-F02                                                     |
| `list_persistent_files`                                                                                                                                           | B        | Retained overlay-list service; no original UI caller, F39   |
| `apply_config_preset`, `get_config_preset_status`                                                                                                                 | B        | `config_presets` + CLI; no original UI caller, retained F39 |
| `list_custom_mods`, `add_custom_mod`, `remove_custom_mod`                                                                                                         | I        | `custom_mods` + Mods tab; F05                               |
| `browse_custom_mod`                                                                                                                                               | I        | Native picker; F05                                          |
| `browse_instance_icon_file`, `list_instance_icons`, `import_instance_icon`, `set_instance_icon_from_library`, `open_instance_icons_folder`, `clear_instance_icon` | I        | F10                                                         |
| `set_instance_icon`                                                                                                                                               | I        | F10, F39                                                    |
| `detect_java`, `browse_java_executable`, `test_java`                                                                                                              | I        | `java` + preferences/instance controls; F23, F36            |
| `launch_instance`, `kill_instance`                                                                                                                                | I        | `launch` + root; lifetime/recovery F28, live F36            |
| `exit_launcher`                                                                                                                                                   | I        | Shared graceful close path; disposal gates F27, F31         |
| `get_instance_console_log`, `clear_instance_console_log`                                                                                                          | I        | `console_log` + console panel; F28, F36                     |
| `get_accounts`, `add_offline_account`, `remove_account`, `start_microsoft_login`                                                                                  | I        | `accounts` + accounts view; F25, F36                        |
| `get_launcher_settings`, `save_launcher_settings`                                                                                                                 | I        | `settings` + preferences; F24-F25, F29-F30                  |
| `check_launcher_update`, `install_launcher_update`                                                                                                                | I        | `release_updater`, Launcher toolbar/About/dialog; F32-F34   |

## Verification records

| Date       | Evidence                                                                                          | Result / boundary                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ---------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-10-06 | Windows: `cargo test --workspace --locked` in `apps/launcher`                                     | 131 passed; offline/local baseline, not live providers or packaged cross-platform tests                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| 2026-10-06 | Source comparison of both apps, all 69 command cases, desktop hooks, scripts and release workflow | Functional inventory established; unchecked items remain open                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 2026-10-06 | Windows hidden-window `window_move_bench`, real Instances/Info layout at 1600 by 900              | Unoptimized Launcher: median 21.0 ms / p95 25.3 ms; dev Launcher optimized: median 4.4 ms / p95 6.0 ms. Fixes measured layout bottleneck in F31; excludes GPU presentation and actual drag cadence. Physical drag verification stopped when the user stopped UI automation.                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 2026-10-06 | Native window materials: workspace tests, actual DWM attribute checks, isolated visible profile   | 136 tests passed. Frosted/Mica/Opaque switch correctly in both themes, including native dark tint and clearing the material. Disabled Direct Composition falls back to opaque. Final CPU layout p95: opaque 5.8 ms, frosted 7.0 ms, Mica 5.7 ms. Appearance controls and frosted rendering inspected; user changes persisted in the isolated profile. No actual drag-FPS measurement or older-OS hardware test.                                                                                                                                                                                                                                                                                 |
| 2026-10-06 | Top-bar hover regression: Launcher simulated pointer events and actual painted colors             | Reproduced unchanged Add color; adding its stable ID fixed Add while Processes still failed. All eleven toolbar controls now change color on entry and restore it on exit, across light/dark, Monochrome/Industrialis, and 800/1100px widths. All 137 workspace tests pass. This validates Launcher event dispatch/rendering, without a physical OS-pointer capture.                                                                                                                                                                                                                                                                                                                            |
| 2026-10-06 | Launcher launcher updates and distribution                                                        | 144 Rust tests passed (18 app, 79 backend, 47 core), plus two Node packaging/version tests. Launcher pointer/keyboard regression caught and fixed modal click-through. Explicit native NSIS smoke test passed fresh install, failed readiness, old-PID exit/replacement/restart and uninstall preserving a data file, with registry/shortcuts disabled. Live GitHub checker excludes Electron releases. Optimized Windows app, installer and portable ZIP built; executable metadata is 0.1.0; isolated release boot stayed responsive. No GitHub release published; real published N → N+1, macOS/Linux artifacts, signing/notarization and old-installer/profile migration remain unverified. |
| 2026-10-06 | Standalone Launcher resource ownership (F35)                                                      | Copied only Launcher crates/Cargo files to an isolated source snapshot, with no Electron directory. Actual app compiled and hidden native launcher benchmark rendered owned artwork successfully (CPU p95 8.0 ms / 16.67 ms budget). Removes the two compile-time Electron asset dependencies; excludes other monorepo cutover work and physical drag cadence.                                                                                                                                                                                                                                                                                                                                  |

| 2026-10-06 | Profile preservation and reliable saves (F25/F29, save guards F27 and main refresh F30) | 156 workspace Rust tests passed (19 app, 84 backend, 48 core, five profile integrations); actual app build passed. Copied Electron-shaped fixture preserves current/legacy account extensions, all theme tokens, groups and untouched opaque files. Windows readers/replacement/read-only/sharing failures and caller cancellation tested. Launcher pointer/native-close simulation validates failed latest edits, retry, discard and close guards. Metadata-rehearsal wrapper/guarded Rust test also passed with a synthetic profile and unchanged source hashes. Real profile inventory is 71,017 files / about 13 GB; credential-bearing metadata copy was rejected by automatic approval review pending specific payload/destination authorization, so no real files were copied. macOS/Linux, physical crash/power-loss and full maintenance shutdown remain open. |

| 2026-10-07 | Drop-in profile compatibility requirement and in-place read verification | Same Electron data directory and filenames confirmed. Settings patch only changed fields; no-op saves preserve bytes. Untouched legacy/current account rows retain shape, optional nulls and order. Removed the proposed real-profile copy script and ignored rehearsal; users require no migration/import/copy. Read-only CLI opened the original Windows profile (one account, two instances, zero read issues), with no writes or copies. All 158 workspace tests pass (19 app, 84 backend, 48 core, seven profile integrations; one separate NSIS test ignored); actual app and CLI builds pass. Cloud credentials, cross-process ownership, other OS runtime checks and full functional parity remain open. |

| 2026-10-07 | Shared process ownership and second-launch handoff (F26) | Launcher, updated Electron and writer CLI commands share per-profile ownership before backend startup; read-only CLI inspection remains available. Five real Rust/Node process tests passed: cross-runtime write exclusion, actual native launcher secondary-launch signaling, CLI-owner rejection, five simultaneous launches with one primary, forced-exit release and unrelated-endpoint refusal. All 163 workspace Rust tests passed (19 app, 89 backend, 48 core, seven profile integrations; separate NSIS test ignored). Actual app source linked as a separate smoke executable because Windows locked the running development EXE; CLI and headless ownership fixture built. Electron typecheck, main bundle and scoped lint passed. Tests used synthetic temporary profiles only. Physical minimized-window foreground behavior, macOS/Linux runtime validation and stopping older uncooperative installed binaries remain open. |

Append scenario records here as work lands. Include failures and limitations;
do not mark a whole category complete after verifying one happy path.

## Gates for deleting Electron

Do the deletion in a separate change after these gates have evidence:

1. Every F01-F39 item is completed or has an explicit accepted retirement/change
   recorded here; no unresolved P0 item remains. Aesthetics can remain in UI_PARITY.
2. Launcher opens the original profile in place with the same paths and supported file
   formats. Settings, accounts, player data and backup metadata remain usable with
   no user migration, import, copy or reauthentication requirement. Read-only checks
   may inspect the live profile; write/recovery tests use hermetic synthetic fixtures.
3. Real install/launch/stop/copy/update/reinstall/files/mods/backup workflows work
   through the launcher interface, with error/retry/cancel paths verified.
4. Shutdown, second launch, concurrent operations and restored-game scenarios
   cannot corrupt data or leave the UI reporting stale running state.
5. Versioned Launcher artifacts and their update/upgrade path are verified on every
   supported OS/architecture; new installs and old-install upgrades preserve data.
6. Root commands, version stamping, release CI and public download paths target
   Launcher. Required resources and legal notices no longer depend on Electron paths.
7. In an isolated checkout, remove `apps/launcher`, prune Electron-only
   dependencies, and run builds/tests plus packaged smoke checks. This proves the
   directory is removable; a successful build while it exists does not.
8. Before the deletion, move any remaining reference behavior, fixtures or
   resource instructions needed by docs/framework skills into durable launcher-owned docs.
   Update this tracker's live links; historical provenance can use a fixed commit.
9. Preserve a release/tag of the last Electron version and document the profile
   backup and rollback path for the first Launcher release. Never delete user data
   when removing source code or uninstalling the old binary.

Suggested implementation sequence: protect profile saves/lifetime (F25-F30),
finish files/mods and library workflows (F01-F14), build backups (F16-F19), finish
input/settings (F20-F24), then distribution/cutover and full live/platform
verification (F32-F38). F35's asset ownership work is complete; distribution
implementation now needs the remaining real release/platform verification.

2026-10-07 process lifetime evidence: 167 workspace Rust tests passed (19 app,
93 backend, 48 core, seven profile integrations; one installer test remains
explicitly ignored). Three native Windows child-process scenarios pass in
`node --test scripts/game-broker.test.mjs`. Tests use synthetic profiles
and games only. The actual app entrypoint builds under the smoke example so the
running development executable is not replaced.

### 2026-10-07 native editor and library evidence

Windows workspace tests passed: 17 app, 98 backend, 7 profile integration, and 49 core tests (171 total, one Windows installer test ignored). A subsequent Launcher file workflow test also passed: pointer navigation, Easy boolean edit, paired save, external-change conflict with retained draft, close protection and native editor-window reuse. Native input tests cover UTF-16 composition, emoji boundaries, clipboard and undo/redo. These are synthetic-profile/headless Launcher checks, not physical OS-editor, IME, Google authorization, real GTNH or cross-platform acceptance.

### 2026-10-07 continued functional port

Linux Rust 1.96 container: 105 backend, seven profile integration and 49 core tests passed (161 total). Source was mounted read-only; no user profile or desktop credential store was mounted. Rust 1.93 failed the sysinfo 1.95 minimum before compilation. Windows Launcher controls: 21 tests passed after native click conversion; stronger multi-window regressions are under validation. A broad default-parallel workspace build exceeded the Windows paging-file limit; final checks use one compiler job. These records do not close native/provider/installed cutover gates.

### 2026-10-07 functional regression closure

Windows: 23 Launcher, 116 backend, seven profile integration and 49 core tests pass (195 total; one opt-in NSIS test excluded). Linux isolated Rust 1.96 container: 114 backend, seven profile integration and 49 core tests pass (170 total). Source was mounted read-only with no user profile/credential-store access. Added real HTTP partial-upload/offset-query/file-seek recovery, native theme reset/delete, recovery retry, independent editor windows and detached-draft launch saving. These are local synthetic/provider-protocol tests, not live Google/Microsoft/GTNH or macOS packaged acceptance. The macOS native quit bridge still needs native compilation and runtime verification.

### 2026-10-07 source removal, ownership stress and current artifacts

The source-only snapshot `target/source-removal-sXmans` contains no Electron app.
All 197 Rust tests pass (23 app, 118 backend, seven profile, 49 core; one separate
NSIS test excluded). The actual app entrypoint was forced through rustc from the
snapshot; all eight native ownership/game-lifetime scenarios pass against the
frozen Electron protocol fixture. Hidden native render: median 6.1 ms, p95 8.0 ms,
150 samples and all six backdrop switch checks pass. This excludes physical
drag cadence and live providers. Original-profile files were neither copied nor
written. Source hashes and results are retained in the ignored snapshot.

Repeating concurrent starts exposed Windows socket abort 10053 with an active
primary; the original one-shot secondary request exited. A real-reset regression
failed before the fix (10054). Bounded retries on transport failures now pass
both reset recovery and persistent-failure rejection, while untrusted identities
remain rejected immediately. The original stress loop passes 30 rounds / 150
native processes. All 12 current-source Node contracts pass, including actual
Electron-to-Rust credential interoperability. Updated Linux backend/core/profile
suite: 116 + 49 + seven = 172 passing tests.

Optimized Windows 0.1.0 installer and portable ZIP were rebuilt with the fix.
Artifact verification checks exact executable/license bytes, version metadata,
the four-file ZIP inventory, dependency notices and complete Lucide attribution.
Installer SHA-256: `4e200e585173774621af2d79a9e872d74493df7193f31d7322e8fda39074a40a`.
Portable SHA-256: `a5969c7207dff0fcf360c1171599665f0a4aec7ed5c6e980022ab24942cada5d`.
The compiled replacement build can register the original protocol; it was not
installed or published. Current isolated NSIS install/update/restart/uninstall
smoke passes with registry/shortcuts disabled. Native installed protocol handling,
old Squirrel retirement and live published updates remain open.

The real public GTNH catalog was fetched through the native CLI: 40 entries,
including 2.9.0-RC-2. No pack was downloaded or game launched in this check.
The ordered remaining release work is in [CUTOVER.md](CUTOVER.md).

Root launcher dev/build/release/installer/portable commands now select Launcher.
Turbo dry-run confirms Launcher is scheduled and Electron is excluded from `pnpm dev`;
the server/dashboard/website remain scheduled. Explicit legacy aliases remain.
The native task preserves profile/Java overrides and uses Cargo's own cache;
Turbo native build/test caching is disabled to avoid a cached success without an
executable. No application was started by these dry-runs. Production release
workflow, version scripts and public downloads remain an explicit F34 gate.

### 2026-10-07 native Linux application and rendering

Debian 13 x64 container, Rust 1.96, Xvfb/Openbox/D-Bus and Mesa llvmpipe: all 195
workspace Rust tests pass (23 Launcher, 116 backend, seven profile, 49 core). The
normal native app and renderer example compile. Source is mounted read-only;
no user profile or credential stores are mounted. Native hidden rendering passes
150 samples and all six material/fallback switches: median 8.6 ms / p95 13.9 ms
against 16.7 ms. Windows after the benchmark fix: median 6.3 ms / p95 8.5 ms.
These are CPU layout measurements, not physical dragging/compositor FPS.

The initial Linux benchmark stalled despite completing its frames. The pinned
platform invokes setup before calloop.run, which resets an earlier stop signal.
A tagged probe confirmed all frames ran before shutdown stalled; deferring quit
until the loop starts fixes it. Temporary traces were removed. A bounded native
render smoke and full Linux Launcher interactions are now included in preview CI;
the cloud smoke uses a generous budget because runner/software-GPU performance
does not establish desktop FPS. Remote CI has not been run/published here.
Native physical input/dialogs, original Unix credential stores, macOS runtime,
real provider/game workflows, old installer retirement and published updates
remain explicit acceptance gates.

The durable `scripts/test-linux-render.sh` also passed in the same isolated
desktop with a strict 16.67 ms budget: median 5.4 ms / p95 7.5 ms, 150 samples,
six fallback/material checks, exit zero. CI's default is a generous functional
budget; these timing differences do not establish physical drag or GPU FPS.

### 2026-10-07 actual pack lifecycle and client loading

Windows 11 Pro 10.0.26200 x64, Rust 1.96: `live_pack_acceptance` installed actual GTNH
2.9.0-RC-2 Java 17+ and Java 8 archives, resolving 92 and 46 libraries respectively.
Copy, reinstall, replacement with 2.8.4 Java 17+ and update back to 2.9.0-RC-2
all pass. Seeded save/JourneyMap/options/config-overlay bytes and unknown settings
survive each step; selected custom mods survive and unchecked mods are removed.
The marked expendable profile/report is
`target/live-acceptance/profile-20261007/pack-acceptance.json`. No original profile
files or credentials were copied or written.

Real packs exposed four product defects, each reproduced before the fix:
reference-only MultiMC components lacked metadata; legacy LWJGL/JInput classifier
JARs were not extracted; newline-separated JVM flags became one argument; and
Windows atomic replacement failed beyond MAX_PATH. Missing official components
now use bounded identity-checked Prism metadata with an atomic cache, bundled
patches remain authoritative, native downloads/extraction are bounded and guarded,
whitespace splits unquoted JVM arguments, and Windows atomic replacement uses
canonical extended-length paths. Launch-config/native/path regressions pass.

Debian 13 x64, Xvfb/D-Bus/Mesa software desktop: both actual 2.9.0-RC-2 clients
complete Forge mod loading and Stop successfully. Java 21.0.12.1: 434 seconds;
Adoptium Java 8u504 (archive size/SHA-256 verified): 564 seconds. Actual broker
PIDs, multiline arguments, stopped state, durable logs and play-time increments
were checked. Reports are under `target/live-acceptance/game-profile/` as
`game-acceptance-LiveModern.json` and `game-acceptance-LiveLegacy.json`.
Missing XDG desktop directories, EGL and xrandr initially prevented container
client loading; these were fixed in the test environment, not worked around in
the launcher. The durable opt-in `scripts/test-live-game.sh` uses an owned
virtual desktop. The accounts are expendable offline identities. World gameplay,
physical input, Microsoft/Google, native Windows/macOS client use, server/hooks
and an actual game spanning launcher close/reopen remain open F36/F38 gates.

### 2026-10-07 stable cutover preparation and regression results

Windows: 205 Rust tests pass (23 Launcher, 126 backend, seven profile, 49 core; one
separate opt-in NSIS test excluded). Linux: 202 pass (23 Launcher, 123 backend, seven
profile, 49 core). The Unix source is mounted read-only with no user credentials.
A full-suite ownership test exposed a valid connection reset on invalid requests;
the assertion now accepts EOF/reset/abort while still requiring no activation.
The source-only rehearsal `target/source-removal-ixGzrH` contains no Electron app:
all Windows tests and eight real ownership/game-broker process scenarios pass,
the actual main entrypoint recompiles, and hidden layout passes 150 frames/six
material switches (median 4.5 ms, p95 6.0 ms). This is not physical desktop FPS.

The prepared stable build uses `launcher-v*`, requires a non-prerelease Launcher
release-body marker and rejects unmarked Electron artifacts. Windows emits
`IndustrialisLauncherSetup.exe`, retaining the old updater's trusted filename.
Ordinary preview tags/feed remain separate. Four Node packaging/license contracts
and six stable-channel selection tests on each of Windows/Linux pass. The production workflow,
website downloads and source deletion are unchanged pending acceptance.

Stable installation embeds guarded Squirrel retirement after new-executable
extraction. Tests pass detection, profile overlap/data, running-launcher,
failed/incomplete-uninstall, native NSIS rollback and obsolete-protocol cleanup.
The read-only installed layout exposed a root Squirrel shortcut stub in addition
to versioned executables. A red/green regression now requires both to be removed
and rejects an orphan stub after partial retirement.
The protocol test reproduced the invalid `(default)` deletion; the fix deletes
the actual empty-name registry value. Only a uniquely owned test key was used;
unrelated handlers/values survive. The installed legacy helper was inspected
read-only, never applied. Actual Squirrel retirement and installed protocol launch
remain open. `--replacement` alone still registers only the protocol.

Optimized Windows preview 0.1.0 rebuilt successfully; artifact verification checks
version/resources, exact portable entries, executable/license bytes and complete
dependency/artwork notices. SHA-256: executable
`e135080bf677d7d62b26fa605d92ec8699d67394d0567dcc7b9e6155dbe8ff07`, installer
`8976d62f97bf90498e7991766ebf505aa53aba989557f2f5e9c33b7569e56a87`, ZIP
`d84a1b871ed2e0b5d8220a96d8b919fe1d84d23d56891213b9190b869eba5526`.
Artifacts/evidence remain ignored under `target/live-acceptance/rebuilt-preview`.
The separately compiled stable 0.1.0 package also passes version/portable/notices
verification. Archive inspection without executing the installer confirms exact
current retirement-helper, executable and license bytes plus complete notices.
Stable SHA-256: executable
`30c88be93424d0f0140d7ad96f8949eb804beffa31c8ab61fd8a80ea49851c41`, installer
`97c1892d6e09e114ea9803ad76dd7e3984905b061709e1dc8c5bbd8f140b063b`, ZIP
`7765855ff137b9a38b9a13b094b3e941746687914683006467ba2113f0836c45`.
These build-only artifacts are under `target/live-acceptance/rebuilt-stable`;
they were not installed or published, and 0.1.0 is not the cutover version.
The separate opt-in native NSIS install/readiness/PID/update/restart/uninstall
test passes against the current script with production registry/shortcuts disabled.
The current rebuilt credential fixture also passes actual Electron safeStorage
decryption/encryption in both directions without changing the synthetic Local
State key. Together with the source-rehearsal ownership/broker scenarios and
packaging/notice contracts, 13 Node scenarios pass. Original credentials were
never read by these tests.
No user launcher was closed/replaced, no original profile was migrated, and no
commit, push, tag or release was published. Reproducible remaining native/provider
scenarios are in [LIVE_ACCEPTANCE.md](LIVE_ACCEPTANCE.md).

### 2026-10-08 live Microsoft acceptance on Windows

Windows 11 Pro 10.0.26200 x64: the user completed actual Microsoft device-code
authorization in a new, marked expendable acceptance profile. The real Xbox /
Minecraft pipeline resolves a playable license, profile and skin. The opt-in
`live_provider_acceptance` example then performs the normal live token refresh
chain without editing expiry fields and verifies that the same account and its
refreshed game token reload from disk. All checks pass; its redacted report is
`target/live-accounts-a1bd2e31b4764076b45ae2f2930987f7/microsoft-acceptance.json`.
No original profile data or credentials were copied, reconnected or modified.
Fresh authorization does not establish existing-original-account compatibility,
physical Launcher control acceptance, actual authenticated gameplay or other OSes.

The user offered Google acceptance too. No public desktop client or saved Google
connection is configured in the normal Windows profile; only these presence
flags were inspected, without decrypting credentials. The user deferred Google
client creation. The provider harness is compiled and ready to test
generated backup data under a unique instance ID; Google results remain pending.

### 2026-10-08 monorepo source-removal and integration checks

`node scripts/rehearse-workspace.mjs --verify --online` created an isolated
256-file source snapshot without `apps/launcher` or its production workflow.
Only the snapshot loses legacy aliases/zip override, redirects version stamping
to Launcher and regenerates its pnpm lockfile. The copied dependency graph has no
Electron runtime/build packages; `electron-to-chromium` remains a legitimate web
browser-data dependency. Installing the snapshot with scripts disabled succeeds.
Lint/types, 205 Windows Rust tests, 27 server tests, 8 dashboard tests, production
server/dashboard/website builds and 4 packaging/notice tests all pass. Evidence:
`target/workspace-removal-3HBHVu/result.json` and its source inventory.
The real checkout's pnpm lock/workspace files, distribution and original app remain
unchanged. Existing entrypoint/process/render removal evidence still applies.

Integration validation also passed against the real checkout: lint (after removing
an unused migration-test import), all package type checks, 27 server / 8 dashboard
tests and all three web/server builds. The original Electron reference suite
passes 153 tests with one skipped when run serially using
`corepack pnpm --filter @industrialis/launcher exec vitest run --testTimeout 20000 --maxWorkers 1`.
Concurrent runs exposed intermittent five-second Windows process timeouts and a
capture/cleanup failure; the passing bounded serial run is the recorded result,
not a claim that the old concurrent suite is stable. A stale local pnpm 11 shim
prevented the literal root Turbo command; pinned Corepack package commands and
the isolated workspace run verified the equivalent tasks. The website build's
generated type-reference change was removed. No user launcher was stopped.

### 2026-10-08 actual authenticated client loading

The already-authorized expendable Microsoft account was reused without another
sign-in. Only generated GTNH pack contents were copied into an `AuthModern`
instance in that marked test profile; no original profile/account was copied.
On Debian 13 x64 with Java 21.0.12.1 and an isolated virtual desktop, the normal
backend account selection/refresh/launch path completed real 2.9.0-RC-2 Forge
loading in 217 seconds. Stop succeeded, the same Microsoft account remained
saved, and 211 seconds of play time were added. Authenticated game output and
credentials were never printed by the harness. Redacted evidence:
`target/live-accounts-a1bd2e31b4764076b45ae2f2930987f7/microsoft-game-AuthModern-acceptance.json`.
This proves client launch using the saved authorized test account, not joining
an online server, playing a world, physical Launcher interaction or compatibility
with pre-existing original accounts. Google remains deferred by the user.

### 2026-10-08 actual detached GTNH acceptance on Linux

In Debian 13 x64/Xvfb, the actual 2.9.0-RC-2 Java 17+ client (Java 21.0.12.1)
passes detached lifetime/recovery in 266 seconds, adding 261 play seconds.
The final Java 8u504 run passes the final runner and stronger complete-log/
single-exit assertions in 311 seconds, adding 308 play seconds. Both retain the
same exact game PID/creation identity across two owning launcher processes
closing, then Stop from a fresh owner and reconcile the broker receipt. No second
game is spawned. Unique generated heartbeat output captured while closed replays
without duplication; environment/wrapper, multiline JVM/RAM/window arguments,
pre-launch and exactly-once post-exit hooks pass. Repeating recovery leaves
finalized time/logs unchanged. Reports:
`target/live-acceptance/game-profile/detached-game-acceptance-LiveModern.json`
and `detached-game-acceptance-LiveLegacy.json`.

An earlier Java 8 harness run passed, but editing its running shell wrapper caused
a post-harness parse error; it is not the final passing command. The unchanged
final runner/harness above completed with exit 0. Generated heartbeat output is
a capture assertion, not a claim that Minecraft itself emitted those lines.
These are marked expendable offline profiles and backend/virtual-desktop checks.
They do not establish physical launcher gestures, world gameplay, online-server
authentication, actual game normal/crash exits or native Windows/macOS lifetime.
All owned acceptance containers completed and were removed; user launchers and
original data were never stopped, replaced, moved or reconnected.

### 2026-10-08 custom ports, bounded ownership reads and original-profile inspection

The real 2.9.0-RC-2 Java 17+ client reproduced an auto-join bug: a saved
`127.0.0.1:43381` address became the hostname while Minecraft separately used
port 25565. The production argument regression also failed before the fix.
Launch now splits explicit ports into Minecraft's `--server` / `--port`
arguments, accepts hostname/default-port and bracketed/bare IPv6 forms, and
reports malformed addresses before spawning Java. The saved address/schema
stays unchanged. Four argument regressions pass. Corrected actual clients reach
the custom-port probe and complete its status/ping and login-start packets;
exit results are recorded separately once complete.

The Windows full suite exposed reset-fixture failures on nonblocking accepted
sockets. A deterministic fragmented-request test returned immediate `WouldBlock`;
the previously compiled real Rust owner also reset a fragmented Node activation
request. The ownership reader now explicitly uses blocking I/O with its existing
500 ms deadline. Reset fixtures retain unread-byte semantics using blocking
peek. All eight ownership unit scenarios and six rebuilt actual Rust/Node
ownership scenarios pass, including fragmented activation, concurrent starts,
write exclusion and unrelated endpoint rejection. No ownership protocol or data
format changed. Final Windows application/backend/core/profile tests pass 210
tests with the separate opt-in NSIS test ignored; Linux passes 207. Rebuilt broker
processes and packaging/notices pass seven further Node scenarios, and lint passes.

The rebuilt native CLI's explicitly read-only `verify-profile` also reads the
normal original Windows data path while preserving all 11 checked root/group/
instance metadata documents byte for byte. It recognizes one account and two
instances with no unrecognized account rows or instance read errors. The
outcome-only evidence is `target/live-acceptance/original-profile-read-only-20261008.json`.
No original credentials were displayed, refreshed, reconnected, copied or moved.
This establishes in-place parsing, not live original-account token refresh,
original native control acceptance or provider-store interoperability.

### 2026-10-08 refreshed Windows distribution artifacts

Both local 0.1.0 preview and stable-channel installers and portable archives
were rebuilt after the custom-port and ownership-read fixes. Each completed
`verify-windows-artifacts.ps1` before a different channel could replace the
release build input. Verification checks the portable executable against that
channel's compiled bytes, executable/installer versions, archive contents and
complete dependency/artwork notices. Reports are retained under
`target/live-acceptance/rebuilt-preview-20261008-final/verification.json` and
`target/live-acceptance/rebuilt-stable-20261008-final/verification.json`.

Preview SHA-256: executable
`43de93d0d05a59d8b64e9d25033a31ca375aac5124f112cc5b49a92716b2071e`,
installer `4649dbc23fcaa500c5bab30d27952236379b4bca21132a40a362aaee92d2e99f`,
portable `6285242587c45a75ed3584ca1e75d48a8951e30eba393763c51b4ddc1475f667`.
Stable SHA-256: executable
`362f0679161fe6da72b1ae054cf33b60a49b5cd26e95791bd823e62a65777a43`,
installer `57524ad41891be0d4c639d11ee2af636e123f9e0cab1e76419567c426d25e759`,
portable `12ab160688aff94ebf7e67247fc0e38ed8ecde8f252e12b886789aae1c621810`.
These supersede the older local artifact hashes above. They are unsigned local
acceptance builds; 0.1.0 is not the production replacement version, which must
exceed the last Electron release. No installation, old-app retirement, release,
public download switch or publication occurred. Both running user launchers
remain on their untouched debug executables.

### 2026-10-08 actual custom-port client exits

The unchanged final runners pass on Debian 13 x64 / isolated Xvfb with the
marked offline profile and actual GTNH 2.9.0-RC-2 clients:

| Client and exit              | Elapsed seconds | Play seconds added | Final exit code |
| ---------------------------- | --------------: | -----------------: | --------------: |
| Java 17+ normal window close |             254 |                251 |               0 |
| Java 17+ injected SIGKILL    |             223 |                219 |               1 |
| Java 8 normal window close   |             368 |                364 |               0 |

Every case verifies real custom-port status/ping and login-start, completed
Forge loading, a single current-session exit emission and persisted exit,
one post-exit hook, no live running record, bounded time accounting and an
unchanged second recovery. Evidence is the three `connection-exit-*.json`
reports under `target/live-acceptance/game-profile`. SIGKILL is explicit fault
injection, not a naturally occurring Minecraft crash. The loopback probe
intentionally disconnects before authenticated login/world success.

Two acceptance-helper issues were reproduced and corrected without changing
production log retention or window behavior. Log compaction invalidates counts
of all historical exits, so the harness now counts the current live launch and
its retained session. Texture/Netty initialization can delay packets after a
socket opens; the probe uses the existing bounded post-Forge deadline. The
Java 8 LWJGL 2 window lacks `_NET_WM_PID`; XRes 1.2 now supplies the server's
local-client PID before a visible owned window can receive `WM_DELETE_WINDOW`.
A real Xvfb regression fails before this fallback and passes afterward,
including unrelated-PID rejection, missing-isolation rejection, ambiguous
visible-window rejection and hidden auxiliary windows. The test image adds
only the XRes dependency; physical user desktops are never selected.

A fresh cached GTNH server fixture also reaches `Done`, responds to its own
console and rejects reuse of its generated world before any file edits. Its
Docker network is internal, no ports are published, and its anonymous server
volume contains only generated acceptance data. Its image already has the EULA
configured; the fixture never accepts an agreement or deletes an existing world.
Real Forge world-entry results and their timing limit are recorded below.

### 2026-10-08 actual Linux preview portable build and native boot

The actual application compiles in optimized `packaged-updates` mode with locked,
offline Cargo dependencies in a read-only source container. Node's real native
packager produces the 0.1.0 preview tarball. The memory-limited first release
build completes successfully; no user launcher executable is replaced.

`test-linux-portable.sh` verifies the tarball has exactly the six expected
files, no links/unexpected paths, an executable x64 ELF identical to its compiled
input, the matching icon/license, complete dependency/artwork notices, manual
update guidance and no preview ownership of the old protocol. Dynamic libraries
resolve. The extracted executable then boots on a newly created Xvfb/Openbox
virtual desktop with networking disabled and its own generated profile. The
1100x750 window belongs to that actual process; its captured content has 153
sampled colors, closes through the native window event with code 0, and retains
a seeded unknown settings field after shutdown. The render was inspected.

Evidence lives in `target/live-acceptance/rebuilt-linux-preview-20261008-final/`:
`verification.json`, `portable-render.png`, the empty native stderr/stdout log,
and `IndustrialisLauncher-linux-x64.tar.gz`. SHA-256: executable
`44494f51f97c4c1779ffa7468fed6a293dbcd41464a39ffba7ddae32b988c227`;
archive `54f4b5907f0211d8e1d20f1f605b705f55ac073abcb3399443cc47bcde4428ba`.
This is a Debian 13 x64 acceptance build, not a published artifact or proof of
compatibility with older Linux distributions, Wayland, physical input/dialogs,
existing credentials or native macOS. The offline pack-check error is visible
without preventing initial render or close. New Launcher shell runners explicitly
retain LF endings through Windows checkouts.

### 2026-10-09 private Forge world entry and startup timing control

Both actual GTNH 2.9.0-RC-2 clients enter the newly generated private Forge world,
receive a unique server-console chat marker in the client, and close through their
owned native window with code 0. These runs explicitly use a disposable TCP proxy
with 200 ms of configured forwarding delay per direction:

| Client                              | Elapsed seconds | Play seconds added | Final exit code |
| ----------------------------------- | --------------: | -----------------: | --------------: |
| Java 17+ private-world normal close |             281 |                278 |               0 |
| Java 8 private-world normal close   |             329 |                326 |               0 |

Each passes exactly-once live and persisted exit reporting, one post-exit hook,
bounded play time, cleared running state and unchanged repeat recovery. Reports
are `target/live-acceptance/game-profile/world-connection-LiveModern-latency200.json`
and `world-connection-LiveLegacy-latency200.json` in that same directory.
This proves actual offline world entry on this fixture, not online authorization,
physical gameplay or an ordinary direct-server pass.

Without the added delay, both direct-server clients fail inside the shipped
Forge startup handshake. A matched Java 17+ control using the same proxy hostname
and port with zero configured delay reproduces the null `playClientBlock` failure
in `FMLClientHandler.waitForPlayClient`. The route change alone therefore does not
explain the successful runs. The outcome-only record is
`target/live-acceptance/forge-startup-timing-20261008.json`.

[Forge's 1.7.10 client handler](https://github.com/MinecraftForge/FML/blob/1.7.10/src/main/java/cpw/mods/fml/client/FMLClientHandler.java)
creates its play-client latch after opening the connection screen; its startup
fallback can also open that screen without initializing the latch. The installed
Forge bytecode matches these paths. The observed failure and matched delay
control support a timing-sensitive game-runtime failure in this software-rendered
fixture; they do not establish behavior on native desktops or all servers.
F36 remains open for ordinary/native server auto-join. No production proxy or
game-JAR patch was added.

The disposable server stopped, both proxy containers exited, and the verified
empty, acceptance-labeled internal network was removed. No ports were published,
user worlds or credentials were used, or physical windows were operated. Both
existing Windows launcher processes remain running on their unchanged debug
executables.

### 2026-10-09 IPv6 auto-join and packaged Linux CI acceptance

A small probe against the actual installed Minecraft 1.7.10 address parser
reproduces another launcher argument bug: Forge composes `host:port` from the
CLI arguments, so the previously emitted raw `::1` produces host `::1:25570`
and default port 25565. Keeping the CLI hostname bracketed yields host `::1`
and custom port 25570. Java 8 and 21 accept that bracketed hostname through
their direct-resolution fallback too; IPv4-mapped IPv6 also passes. The probe
uses `scripts/MinecraftServerAddress.java` in its own Java package, respecting
the client JAR's signature checks. No game JAR is changed.

The backend retains IPv6 brackets while continuing to pass host and port as
separate arguments. A production argument regression fails before this change
and passes afterward. Saved addresses and profile schemas stay unchanged.
Full Windows and Linux Rust suites pass 210 and 207 tests respectively; the
Windows opt-in NSIS test remains separately ignored. Four packaging/notices
contracts, workflow YAML/format checks and repository lint pass.

Actual clients then pass through a network-disabled Docker fixture whose only
listener is the generated IPv6 loopback custom port:

| Client   | Received IPv6 host / port | Elapsed seconds | Play seconds added | Exit code |
| -------- | ------------------------- | --------------: | -----------------: | --------: |
| Java 17+ | `::1` / 46051             |             314 |                310 |         0 |
| Java 8   | `::1` / 42797             |             577 |                572 |         0 |

Both verify actual status/ping and login-start, completed Forge loading, native
owned-window close, one exit/hook/time update and unchanged repeat recovery.
These timings include software rendering and concurrent build load; they are
not a physical startup-performance gate. Reports are
`target/live-acceptance/game-profile/ipv6-connection-exit-LiveModern-normal-window-close.json`
and the corresponding `LiveLegacy` report. The probe disconnects before world
authentication; the previously recorded direct Forge world-startup timing
failure remains open and is separate from this fixed endpoint bug.

The release workflow now validates and boots its actual packaged Linux archive,
not just an example renderer. The helper reports the real distribution/architecture
and selected preview/stable channel. Exact known regular files are copied after
validating all members, removing dependency on newer Python tar extraction APIs;
links, duplicate files and unexpected paths reject before extraction. Two tests
cover a valid archive and five malformed member scenarios on Python 3.10 and
3.13. The already-built artifact passes the updated real native boot check.
CI keeps screenshots/logs/outcomes in a separate acceptance artifact and publishes
only distribution file types. Remote workflow execution remains unverified.

Fresh post-fix Windows preview/stable installer/portable builds and the Linux
preview build all complete and pass artifact verification. Linux also boots,
renders 1100x750 with 153 sampled colors, closes normally and preserves a seeded
unknown setting. Evidence is under these ignored directories:

- `target/live-acceptance/rebuilt-preview-20261009-ipv6/verification.json`
- `target/live-acceptance/rebuilt-stable-20261009-ipv6/verification.json`
- `target/live-acceptance/rebuilt-linux-preview-20261009-ipv6/verification.json`

Preview Windows SHA-256: executable
`ca9b226c9e94923b87e05d278445807a88158bde32b9f1028c5fc11510765e71`,
installer `e286ad602c3b171fcb7f250f803015f8234082360a543acd37f88853692c06aa`,
portable `9419c32e7816b35d89b06c9ad3b657a795fe503e6500154b57f233994ae68e5b`.
Stable Windows: executable
`03bbc9aa22b8682dcc62d22c15d82067d031d7033258d6d4510bfec1f2d1b5ac`,
installer `2f12df3af534b870a98a2f96563f86dcdbc86375b8bfc0b6aa53815d32764677`,
portable `8819aaa6bdb48fcdae128edae1a3bf0e95f3cbf3d69d37cac372bf5bcd9f79c9`.
Linux preview: executable
`c45494f1529fc6b0e9b4f7c40ccd786ed4f5634fb374db427a94248e8aaa46e9`,
archive `713ecf73a0671a36a554b21497cb2fa6df61edee71bf64b7beb42977dab44850`.
These supersede the earlier local artifacts for the production IPv6 fix; they
remain unsigned 0.1.0 acceptance builds, not production releases.

### 2026-10-09 actual Squirrel retirement and clean-Windows startup regression

A fresh Windows Sandbox with build 10.0.26100.0 installs the unchanged public
Electron `launcher-v0.3.0` installer, SHA-256
`5b6a7b5ed7b94510124f3867b1c1adb30d5a40e4f817ab89d3427743f8a8c746`.
The actual local stable Launcher installer successfully delegates removal to that
installation's real Squirrel updater: both executable entry points, both
shortcuts and its uninstall registration are removed. All eight generated
profile files remain byte-for-byte unchanged before Launcher first boot, including
`Local State`; the new protocol owns Launcher. The guest records whether Electron
created `Local State` or the fixture seeded a generated marker instead.
No host installation or real user profile was mapped, opened or removed.

Installed Launcher boot then reproduces native exit `0xC0000135`,
[Windows' missing-DLL status](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55).
The compiled executable imports `VCRUNTIME140.dll`. This is a distribution
regression missed by developer-machine build/tests, not an accepted installed
boot. The previous Windows artifacts above do not pass the new runtime guard.
`windows-runtime.mjs` inspects actual x64 PE ordinary/delay imports and
rejects external MSVC runtimes before packaging and in artifact verification.
Windows packaging now enables static CRT through encoded compiler flags, retaining
existing flags. Six packaging/notices/runtime tests pass; the actual old executable
fails the guard. The subsequent static-CRT rebuild passes clean-VM acceptance
below, closing this isolated installed Windows subset of F33/F39.

The guest harness also reproduced a Windows PowerShell process-wait bug: redirected
children can report blank exit codes without retaining their native handles. Its
actual wait function now passes a host-safe regression for exit 0 and rejection
of exit 7. That regression imports only two function definitions through the
PowerShell AST, never the guest installer/profile/shutdown body. Original failed
VM receipts remain preserved; an earlier VM exited without a receipt and is not
counted as acceptance. The reusable Sandbox runner verifies unique guest identity,
maps only checksum-verified fixture inputs and an evidence directory, disables
network/clipboard/input redirection, and cleans up only its identified client.

### 2026-10-09 rebuilt clean-Windows replacement acceptance

The final complete host command exits 0 with guest `passed: true`:

```powershell
& scripts/test-windows-squirrel-sandbox.ps1 `
  -LegacyInstaller apps/launcher/target/live-acceptance/public-electron-0.3.0/IndustrialisLauncherSetup.exe `
  -StableArtifactDirectory apps/launcher/target/live-acceptance/rebuilt-stable-20261009-static-runtime -Run
```

Evidence is `target/live-acceptance/squirrel-sandbox-316020654051469eb7294aff32fdeb31/output/`:
`result.json`, `progress.log`, installer/native logs and `launcher-native-render.png`.
The screenshot was inspected: Launcher renders the existing generated instance,
icons and navigation. Network checks fail visibly as expected with VM networking
disabled. Build 10.0.26100.0 has no system `VCRUNTIME140.dll` installed.

The actual public Electron installer exits 0. Its fixture closes normally.
The stable Launcher installer invokes the actual Squirrel updater and exits 0;
both old executable entry points, both shortcuts and the uninstall registration
are removed. Nine generated profile files remain byte-identical before first
boot. The installed Launcher executable matches the verified artifact checksum,
creates a real `Zed::Window` and renders 576 sampled colors. Shell activation of
the registered protocol restores the same minimized owner to foreground without
a duplicate process. Normal window close exits 0. Launcher uninstall exits 0, leaves
all nine post-boot profile files intact and does not restore the dead legacy
handler. The owned VM/client is gone; the already-running personal Launcher process
is untouched.

`legacyCreatedLocalState` is false: `Local State` is a generated marker in this
fixture. This proves file preservation, not an original encryption-key or real
account refresh. The account is generated/offline. Physical input, running-user
handoff, older Windows, signing and a published N-to-N+1 update remain open.
An earlier static-build guest passed but its host command failed a cleanup-path
guard; that guard now checks the identified Store-package executable path. One
intervening VM exited during setup without a receipt and is not an acceptance
pass. The final run above passes both guest assertions and host cleanup.

Static-CRT Windows artifacts supersede all earlier Windows hashes above. Both
receipts verify version/resources, exact portable contents, compiled executable
bytes, licenses/notices and absence of external MSVC ordinary/delay imports:

| Channel | Artifact     | SHA-256                                                            |
| ------- | ------------ | ------------------------------------------------------------------ |
| Stable  | Executable   | `809631bf62d08d4bc2cbd16f6c7b22cabdfc7afbf119816e0098f83c05b31437` |
| Stable  | Installer    | `b5e79954710861e298bdc44e59531d82f2df8afc7e8b3b0c928c0641a7ee0c7b` |
| Stable  | Portable ZIP | `01b9b7b38bd3b9374d0ffe09e17311593a73e82470994fb02c88207b2ef0fffc` |
| Preview | Executable   | `924bfce8bab3b304a5e10e6c988a2ffb9d95361e86614394330fddaac0311c6d` |
| Preview | Installer    | `f831dee5f74d306e1884bfd27b704e5b7f76880388a7716f1ec9f651ca9047e3` |
| Preview | Portable ZIP | `f89882bae11c0b7e5a4a6f5200eb9c0e6e28f8899683474725ec7a58b19cedec` |

Receipts live in `target/live-acceptance/rebuilt-{stable,preview}-20261009-static-runtime/verification.json`.
These remain unsigned local 0.1.0 acceptance artifacts; production must exceed
Electron 0.3.0. The current Linux preview artifact remains the IPv6 rebuild
recorded above. Six Node packaging/notices/runtime tests, the native process-wait
regression, script syntax, lint and JavaScript/workflow formatting checks pass. No production
installation, feed, tag, publication or Electron source removal occurred.

### 2026-10-09 functional ticket closure and native Linux credentials

Functional evidence closes F01, F02, F06-F09, F11-F14, F18, F19, F21,
F23, F24, F29 and F30. F35 was already complete. Each closure retains its
acceptance condition above; platform/provider gates stay in their own tickets.
These are verified workflows, not a completion percentage for production cutover.

The full Rust workspace passes 218 tests on Windows and 215 on Debian 13 x64:
29 app interactions, 133/130 backend tests, seven profile-persistence tests and
49 core tests. The Windows-only opt-in NSIS integration is separately ignored
by the ordinary suite and has its own native installer evidence. Commands from
`apps/launcher` are:

```text
cargo test --workspace --lib --bins --tests --locked --jobs 1 --features industrialis-app/packaged-updates
cargo test -p industrialis-app --bin industrialis --locked --jobs 1 --features stable-updates
```

Linux uses the workspace command with `--offline` in the owned container,
read-only source, a separate Linux target and the cached registry. The protected
personal debug launcher is neither rebuilt nor stopped.

| Tickets     | Verified scenario and boundary                                                                                                                                                                                                                                                                                                      |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F01-F02     | Launcher pointer/field workflow opens nested config, edits Easy boolean/integer/list values and Advanced text, saves both game and overlay, then reloads from disk. Backend tests reject excluded/binary/oversized/link paths and preserve conflicts. Actual GTNH maintenance fixtures retain paired config overrides.              |
| F06-F09     | Launcher group creation/assignment/clear/rename collision/delete, pointer drag, collapse/layout save races and disk reload pass. Backend ordering survives copy, cross-group move and delete without changing other groups or extension fields.                                                                                     |
| F11-F14     | Full catalog search and unavailable Java variants are guarded; update status handles known/latest/unknown versions. Size refresh has pending/busy/external-change coverage and patches only cached size against the latest profile. Cancelled deletion reports partial removal, retains surviving metadata and can finish on retry. |
| F18-F19     | The durable automatic queue verifies settling, retries, bounded concurrency, restart, tombstones and cloud-only retention. Launcher global/instance policy and existing archive controls save to the original files without losing extensions. Drive authorization and actual remote transfers still need F16/F17.                  |
| F21/F23/F24 | Multiline/quoted JVM, command and environment fields round-trip through native inputs; real Java fixtures check launch arguments. Runtime search/numeric sorting and global/instance selection persist. Custom theme edits/reset/delete/reload retain extensions. Physical input/rendering remains F20/F38.                         |
| F29-F30     | Coalesced latest-document saves, admitted transactions, failed-save retry/discard and close guards pass. Startup/refresh retains last-known data and visible read errors until a successful repair/retry. This does not claim physical power-loss acceptance.                                                                       |

The native Linux credential loop initially reproduces SIGSEGV on the second
libsecret lookup. Debugger frames identify `g_intern_static_string` and
`secret_service_get_type`: unloading libsecret/GLib after a call leaves registered
native type state referring to unmapped library data. Keeping successfully loaded
libraries for the process lifetime fixes the actual repeated-call loop; failures
are not cached, so installing a missing provider can still be retried.

`node --test scripts/credential-compat.test.mjs` passes with actual Electron
43.3.0 safeStorage and Rust in both directions on Windows DPAPI. The checked-in
Linux Docker recipe and guarded `scripts/test-linux-credentials.sh` pass
the same loop through a private GNOME Secret Service, with the original generated
key unchanged and no plaintext/key/token output. Receipts are
`target/live-acceptance/linux-secret-service-20261009/verification.json` and
`recipe-verification.json`. The recipe's end-to-end build uses a cached Debian 13
base override after two Docker Hub authentication timeouts; this does not certify
a fresh pull of its minimal default base. The final credential run has networking
disabled and does not mount the host HOME or session bus. CI now runs this check
and uploads its nonsecret receipt, but remote CI has not run. Native KWallet,
Keychain and live Google remain open in F16/F25/F37/F38.

### 2026-10-09 native control regression and pending final guest run

The owned Windows Sandbox run against
`target/live-acceptance/rebuilt-stable-20261009-native-menu-fixed` verifies the
actual released Electron window handoff, Squirrel retirement, eleven post-close
fixture files preserved, clean installed Launcher boot, same-owner registered protocol
activation, native tile selection and keyboard tabs. Actual picker cancellation,
two-file mod import with byte checks, confirmed removal and the OS mods-folder
action pass. Its overall command exits 1 at the menu assertion; it is not a full
acceptance pass. Receipt:
`target/live-acceptance/squirrel-sandbox-d6680fd41d7544b088f3a81b709a6708/output/result.json`.

The old context-menu implementation reused one ID for all twelve actions; the
app now supplies distinct action IDs. The new guest assertion incorrectly
required globally unique names despite the footer also exposing Delete instance.
It now requires exactly one newly exposed button per action and saves the menu
tree even on failure. That assertion correction still needs its native rerun.

The control audit additionally fixes preference navigation IDs/names, account
star/removal IDs/names and default toggle state, group header action IDs/names,
environment-row removal names and keyboard activation for theme/process cards.
Caption controls share the unstyled button primitive, keeping one hover rule.
The final account/group changes pass all 29 Windows app tests. The matching Linux
app rerun starts, but its terminal session becomes unavailable before its final
result can be collected; it is not counted as another pass. The preceding full
Linux workspace run passes 215 tests before these final account/group edits.
The guest script now checks two generated offline accounts, all preference
sections, Java-picker Cancel, theme Enter/Space persistence, Tab/Shift+Tab,
modal Escape/Enter and distinct group controls. Host-safe AST parsing and
interop compilation pass before the final account/group scenario additions;
they invoke no host desktop APIs. The final script additions still need parsing
and native execution.

`rebuilt-stable-20261009-keyboard-final` finishes packaging before the final
account/group edits and is not verified or run as the latest acceptance artifact.
The intended `rebuilt-stable-20261009-native-controls-final` rebuild is not
executed: automatic approval review fails due to its usage limit, explicitly
without an unsafe-action determination. F03/F04/F05/F10/F22 remain unchecked
until the current artifact and complete guest scenarios pass. Native editor/icon
checks are implemented but their final end-to-end evidence is still pending.
No original profile, personal window, production installation or release feed
is changed by this work.

### 2026-10-09 native input semantics and external-editor regression

The approval service recovered and the current stable build completed. Two
subsequent native runs against the preceding controls artifact exposed separate
issues. The OS editor did start at the correct fixture pathname; PowerShell 5.1
converted the null backup argument in `File.Replace` to an invalid empty path.
The generated editor now passes `[NullString]::Value`. The host-safe regression
extracts the actual replacement expression from the guest script, reproduces the
failure before the fix, and verifies replacement bytes after it. It executes no
desktop APIs. The failed guest receipts remain in
`squirrel-sandbox-64da2fea40ab43558cfe93944859a797/output` and
`squirrel-sandbox-fbefb6971fcd4cbb92e8f1ffe961e79a/output` under the acceptance target.

The latter guest's actual account accessibility tree also had no Edit node.
The shared native field now supplies a named TextInput/MultilineTextInput frame,
placeholder/value semantics, focus delegation and accessible value replacement
through the editing engine. Replacement uses UTF-16 ranges, emits form changes
and retains undo history; it does not use model synchronization as editing.
The new regression verifies these observable behaviors in single-line,
multiline and code-editor modes. No dependency or saved-profile format changes.

The final full suites pass **219 Windows tests** with `stable-updates` and
**216 Linux tests** with `industrialis-app/packaged-updates`: 30 app, 133/130
backend, seven profile and 49 core tests. Linux uses the owned offline container;
its log and source-hash receipt are
`target/live-acceptance/linux-native-input-20261009/`. The Windows-only opt-in
NSIS test remains ignored by the ordinary suite and has separate native evidence.
The external-editor AST regression and packaging verification pass as well.

The current unsigned local 0.1.0 stable artifact is
`target/live-acceptance/rebuilt-stable-20261009-native-input-fixed/`:

| Artifact     | SHA-256                                                            |
| ------------ | ------------------------------------------------------------------ |
| Executable   | `66fcd627e9bfc37bdb33bdfaf5568e5f7ddc14fc96ef83c17b1bc2806fcbb668` |
| Installer    | `e9365968c585c9d90f6ae4fb908451a0ec2cee4de873d79d70a57e3350c8847b` |
| Portable ZIP | `770a3797f6f614eb6d2179ea2500f295fd3da531cc3ba78da1d37184b4172acb` |

Its receipt verifies resources, version, exact portable entries, notices and no
external MSVC runtime imports. A complete current-artifact native run is required
before closing F03/F04/F05/F10/F22. Earlier failed runs establish their recorded
partial scenarios only. The protected personal debug launcher remains running.

### 2026-10-11 native functional acceptance and remaining external gates

The complete host and guest run exits **0**, with `passed: true`, on Windows
10.0.26100.0. The saved screenshot was inspected. Command:

```powershell
& scripts/test-windows-squirrel-sandbox.ps1 `
  -LegacyInstaller apps/launcher/target/live-acceptance/public-electron-0.3.0/IndustrialisLauncherSetup.exe `
  -StableArtifactDirectory apps/launcher/target/live-acceptance/rebuilt-stable-20261011-native-focus-fixed `
  -RunningLegacy -NativeUi -Run
```

Receipt, native trees, editor receipt, screenshot and progress log are in
`target/live-acceptance/squirrel-sandbox-0faf50f8f6ee4c1ea6f49f01061b631f/output/`.
Replacement waits for the actual released Electron window's normal close, then
retires both executable entries, shortcuts and uninstall registration. All eleven
post-close baseline files survive before Launcher boot. Same-owner protocol restores
the minimized window to foreground. Native interactions, rendering with 641
sampled colors, normal close and uninstall pass. All fifteen post-test profile
files survive uninstall; the dead Electron protocol is not restored. Networking
and host input/clipboard redirection are disabled. The owned VM/client is gone.

| Closed tickets | Native and complementary lifecycle evidence                                                                                                                                                                                                                                         |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F03            | Actual OS editor atomic save reaches game and overlay. Override removal stops mirroring; another external write does not recreate it. Backend maintenance/drain tests and recorded GTNH paired-overlay preservation cover replacement.                                              |
| F04            | Popout creates one real editor; repeated opening foregrounds it without a duplicate. Normal editor close retains the root and saved data. Shared-draft/paired-save/independent-close Launcher tests pass.                                                                           |
| F05            | Native picker Cancel, two quoted .jar/.zip imports with exact hashes, confirmed removal preserving other custom/pack mods, and actual Explorer folder dispatch pass. Disk reload, busy/running/failure guards and real maintenance preservation have separate recorded regressions. |
| F10            | Native image picker Cancel/import/select/reset and Explorer library-folder dispatch pass. Reset retains the reusable image. Launcher two-instance reuse/disk reload, backend rollback and real pack maintenance preserve icons.                                                     |
| F22            | SelectionItem semantics, named Edit/value replacement, blank-create guard, account create/remove/default actions, preferences/Java Cancel, theme Enter/Space, Tab/Shift+Tab, modal Escape/Cancel/Save and keyboard group creation/rename/delete/collapse pass.                      |

The semantic field frame now shares its engine's focus identity; only the engine
contributes a tab stop. This fixes the native Shift+Tab naming failure observed in
`squirrel-sandbox-417d788321784f7bb9e368ec5f21ccc1`. The harness now scopes modal
actions to the dialog and waits for the exact native focused element before keys.
Its UIA static methods are verified by host-safe reflection without invoking
desktop APIs. Failed fixtures `96f8...`, `6af6...` and `5f5a...` retain the background
Save collision, asynchronous focus and wrong UIA comparison-class diagnostics;
none is counted as a complete pass. The last fixes are harness-only, so the
artifact/source stays unchanged across those reruns.

UIA reports blank disabled Create as enabled, although activation cannot modify
accounts. Selection and text-value semantics are verified. Disabled announcements,
physical composition and screen-reader behavior remain recorded platform limits.
`Local State` is a generated marker, not original-account/key acceptance.

Current unsigned local 0.1.0 stable artifact receipt:
`target/live-acceptance/rebuilt-stable-20261011-native-focus-fixed/verification.json`.
Version/resources/notices/exact ZIP contents and no external MSVC runtime imports
pass. Production still needs a version newer than Electron 0.3.0.

| Artifact     | SHA-256                                                            |
| ------------ | ------------------------------------------------------------------ |
| Executable   | `0af8d1e70e26436e72bdc83bcb6a1be24b6f09c29482eee51696ccb3bce0a75e` |
| Installer    | `f6132735023c793dcde2b9a83067617b2f0b534ddda1aab7cca7c7e382fd5443` |
| Portable ZIP | `4ad1303d6b1f9934011c7fb503a5de58493fe328c77369aae2f1ca1208953aae` |

Both full Rust suites pass after the focus fix: **219 Windows / 216 Linux**,
including 30 app tests. Linux log/source hashes are in
`target/live-acceptance/linux-native-focus-20261011/`. Windows DPAPI compatibility,
the actual guest atomic-save expression, UIA reflection, repository lint and
focused JavaScript/workflow formatting pass.

Native **KWallet 6** also passes without user intervention. A fresh owned
Xvfb/Openbox desktop creates an encrypted generated wallet, then actual Electron
43.3.0 and Rust exchange encrypted data both ways without replacing its Chromium
key. `scripts/test-linux-kwallet.sh` requires the explicit container marker
and Docker identity, creates fresh HOME/runtime/config/data/temp directories and
a private D-Bus session, checks the owned dialog PID and never prints keys or
plaintext. The Node fixture requires `kwallet6`/`v11`, rejecting basic-text fallback.
CI runs GNOME and KWallet and retains separate receipts; remote CI is unrun.

The checked-in credential recipe builds from its actual default
`debian:trixie-slim` base with a checksum-verified official Electron ZIP; no cached
base override is needed this time. Both guarded runners pass on that fresh image
with `--network none`, read-only source/binaries and no host HOME/session bus.
Receipts are `target/live-acceptance/linux-kwallet-20261011/clean-recipe-verification.json`
and `gnome-clean-recipe-verification.json`. This certifies KWallet 6 in that fixture,
not macOS Keychain, older KWallet versions or a user's original wallet.

**23 tickets are complete.** The sixteen unchecked tickets retain external
dependencies; none is silently marked done to permit Electron removal:

| Tickets     | Remaining dependency                                                                                                                                                                        |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F15         | A person must confirm the audible copy cue or explicitly retire it.                                                                                                                         |
| F16/F17     | Google desktop OAuth client, sign-in and actual Drive transfers; macOS credential access.                                                                                                   |
| F20         | Physical IME/dead-key/caret/selection and shortcut behavior.                                                                                                                                |
| F25         | Original-account refresh and macOS path/permissions/Keychain; older native stores need platform-specific checks.                                                                            |
| F26/F27/F28 | macOS/Wayland/native lifecycle, actual provider drain, original running-user ownership and ordinary game/crash recovery.                                                                    |
| F31         | Physical Windows/macOS lifecycle and explicit Linux minimize-versus-hide disposition.                                                                                                       |
| F32/F33/F34 | Published N-to-N+1 update, signing/notarization, supported platforms and production feed/download/source cutover after acceptance.                                                          |
| F36         | Original-account/Google and ordinary physical gameplay. Timing-sensitive zero-delay Forge startup remains unresolved; ordinary/native validation is needed before changing launch behavior. |
| F37/F38/F39 | Remote CI, native macOS/physical installed scenarios and published protocol/replacement acceptance.                                                                                         |

No personal installation/profile, production feed, public download, version, tag
or Electron source is changed. Their cutover depends on these gates.

### 2026-10-11 F15 copy completion cue verification

F15 is now functionally complete. Both standalone copies and the copy phase of
copy-before-update/reinstall call the same `finish_process` notification path.
`Processes::finish` reports whether the current generation actually transitions
out of Running. Only a successful Copy transition schedules the window's native
system bell. Duplicate terminal events, a stale prior attempt, a dismissed run,
failure, other operations and progress-only `done` events cannot trigger it.
A later maintenance failure does not retract or replay the completed copy's cue.
Missing/closed windows and unavailable system audio do not fail the copy.

The four `completion_sound_tests` pass on Windows and Linux. They include the
actual filesystem copy worker, byte/metadata finalization before notification,
an existing-destination worker failure, stale retries and the two maintenance
workflows. Full current suites pass **223 Windows / 220 Linux** (34 app,
133/130 backend, seven profile integration and 49 core; one opt-in NSIS test
ignored on Windows):

```text
cargo test --workspace --lib --bins --tests --locked --jobs 1 --features industrialis-app/stable-updates
cargo test --workspace --lib --bins --tests --locked --offline --jobs 1 --features industrialis-app/packaged-updates
```

The actual Linux app is rebuilt from this source as the independent
`profile_ownership_smoke` executable, SHA256
`188cdbf2890ffd1373e2ed0f3481c17a4a71536a1192e3b5f3b5b6c602c0ff1a`.
On a private Debian 13 x64 container/Xvfb, the native Copy action creates a
byte-identical destination and emits exactly **one genuine XKB BellNotify**
request: percent 50, pitch 400 Hz, duration 100 ms, `eventOnly=false`.
An independent fresh profile opens the same native Copy dialog, retires its
generated source before submission, and reports `Error: source instance not
found` in Processes. It creates no destination and emits **zero bell requests**.
Both app instances close normally with exit 0. The screenshot of the actual
failed process/log was inspected; this is not a silent pre-submit rejection.

Run only in an explicitly owned disposable Linux container, with networking
disabled, repository/executable mounts read-only and a generated output mount:

```sh
LAUNCHER_ISOLATED_SOUND_CONTAINER=1 sh scripts/test-linux-copy-sound.sh \
  /target/debug/examples/profile_ownership_smoke /output
```

The runner creates its own Xvfb/D-Bus and fresh app profiles. Every input action
verifies the mapped window's owning PID. A missing ownership marker rejects with
exit 2 before desktop setup. Receipt, native screenshots, Linux test log and
source/container provenance are under `target/live-acceptance/copy-sound-20261011/`.
Early harness probes used incorrect tile/focus assumptions; a repeat exposed
new-copy ordering, so the final runner uses one known tile in separate success
and failure profiles. A failed run removes the prior passing receipt.

Pinned `gpui-pre` 0.3.8 dispatches Windows `MessageBeep(MB_OK)`, macOS `NSBeep`, X11
`xcb.bell(0)` or the available Wayland system-bell protocol. The Windows route is
the user-configured [Default Beep](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-messagebeep).
[XKB bell notifications](https://xorg.freedesktop.org/archive/current/doc/libX11/XKB/xkblib.html)
observe the native request, not speakers or headphones. Physical audibility and
Windows/macOS/Wayland playback remain in the existing F38 platform gate; muting
or disabling OS sounds is respected.

**24 migration tickets are complete; 15 migration gates remain.** F15 supersedes
its row in the preceding external-gates snapshot. F40 separately tracks the
requested sound selector and does not block Electron removal. No personal
profile, installation, physical desktop, production feed or release is changed.

### 2026-10-12 Files contrast and native code editor repair

The user's supplied Sandstone screenshot exposed a real layout problem: the
service returned the complete directory, but shrinking rows spilled outside a
fixed list region. F01 now has independent bounded scrolling, 28px rows, a wide
sidebar and a stacked narrow layout. The launcher regression reaches the last entry
in 120 generated configs, opens it and saves an Easy edit in both copies. A wide
layout regression also checks that the clickable bounds match the painted row.

F02 retains the pinned native gpui-base editing engine and adds Syntect token
styles with Forge and common-language grammars. The code view has line numbers,
wrapping, selection, clipboard and undo. Persistent Easy/Advanced tabs expose the
lossless Forge controls; unsupported formats explain why Easy is disabled. Code
uses actual platform monospace families. Sandstone keeps bronze controls with
neutral dark/light surfaces, readable muted text and distinct panel boundaries.
Stored custom overrides are retained. Contrast regressions check button labels,
Sandstone text/borders and syntax colors over normal and active code lines.

Native acceptance caught a second gap that the previous engine-only tests missed:
Ctrl+F opened search state without rendering a query field. The wrapper now shows
a focused Find bar, match count, Previous/Next and Close; Escape returns to code.
Its regression finds two matches without altering the document. The native runner
types a query, then edits the boolean through Easy and checks exact final source
bytes, including the unchanged comments, list and Unicode, in game and overlay.
Both Sandstone modes save successfully and close with exit 0 on a private Linux
Xvfb. Actual highlighted-code, Find and Easy screenshots were inspected. The
container lacks a CJK font; byte preservation does not certify physical font/IME
rendering. F20/F38 remain open.

Final serial Rust suites pass **228 Windows / 225 Linux**: 39 app, 133/130 backend,
seven profile integration and 49 core tests; one opt-in NSIS test remains ignored
on Windows. The final Windows app suite also passes after the platform-font fix.

```text
cargo test --workspace --locked --jobs 1 --features stable-updates -- --test-threads=1
cargo test -p industrialis-app --bin industrialis --locked --jobs 1 --features stable-updates -- --test-threads=1
cargo test --workspace --lib --bins --tests --locked --offline --jobs 1 -- --test-threads=1
```

One earlier parallel Windows rerun observed the unrelated backup upload fixture's
30 ms overlap assertion fail (maximum concurrency 1, expected 2). Its focused
rerun and the final serial suite pass. Production backup code was not changed;
the fixture timing limitation remains recorded for F37. A broad Linux command
was intentionally stopped after compiling unused acceptance examples; the final
command explicitly selects all library/bin/integration tests and exits 0.

Native runner, ownership guards and reproduction are documented in
[LIVE_ACCEPTANCE.md](LIVE_ACCEPTANCE.md#native-files-editor); the brief and design
decisions are in [EDITOR_DESIGN.md](EDITOR_DESIGN.md). Evidence lives under
`target/live-acceptance/editor-20261012/`: native screenshots, `verification.json`,
Windows/Linux test logs and `provenance.json` with executable/source/container
hashes. Missing container ownership rejects with exit 2 before desktop setup.
The final native acceptance executable SHA256 is
`38062f9991536279b56fcf5d4cfac1886dc6479424bb35edc1cdcdc59097c6c2`.
The acceptance binary links the actual app as `profile_ownership_smoke`, leaving
the user's running Windows development executable open.

This repair strengthens F01/F02; the totals remain **24 completed tickets and
15 external migration gates**, with optional F40 separate. No original profile,
physical desktop, installed application, release feed or source removal is used.

### 2026-10-12 Hourly pack checks and drag insertion feedback

Pack catalog status checks now run immediately at startup and every 3,600 seconds.
One root-owned Launcher task shares the catalog across all instances, guards duplicate
starts and avoids overlapping fetches. Manual refresh lives in the instance
context menu, including its accessible ellipsis entry point. The library toolbar
and unknown/current detail status no longer expose check buttons. Known available
updates retain the maintenance action; checks do not install updates automatically.
Cached successful versions survive a failed request, and an unavailable catalog
never implies an instance is current.

Dragging renders a rounded 144px-wide dashed slot at the actual insertion position.
The pointer's left/right half chooses before/after a tile; holding it inside the
slot keeps that target stable. Cross-group moves reserve faded source geometry to
avoid shifting the target vertically. Empty and collapsed group targets can show
an insertion slot. Hover changes only view state; release commits once through the
original instgroups.json transaction, while Escape/outside release cancels.
A persistent library focus handle keeps Escape reachable after hiding the source.
The native editor also receives Find Escape before global popup dismissal.

The new first-reorder regression exposed unordered discovery in the backend:
HashSet iteration could shuffle other ungrouped members on their first edit.
Saved order is now retained, with newly discovered members appended in sorted
order matching the library. Existing group fields and instance data are preserved.

Current serial suites pass **233 Windows / 230 Linux**: 44 app, 133/130 backend,
seven profile integration and 49 core tests, with one opt-in NSIS test ignored on
Windows. The fake-clock test checks startup, 3,599/3,600-second boundaries, a second
hourly tick, duplicate starts and task cancellation. Launcher tests cover stable
hover, left/right drops, empty-group moves, cancelled gestures, actual saved order,
manual menu availability and the in-flight guard. The existing file workflow also
checks that Find Escape reaches the native editor inside the real launcher root.

```text
cargo test --workspace --lib --bins --tests --locked --jobs 1 --features stable-updates -- --test-threads=1
cargo test --workspace --lib --bins --tests --locked --offline --jobs 1 -- --test-threads=1
```

The actual app, built independently as profile_ownership_smoke, SHA256
`a35d1b0cae0b50b663b1cc4230467a447e77d98a4ec604d81f701660168e3937`, passes
native dark/light dragging, matching saved order, preserved group extensions,
context-menu check dispatch and two normal closes on an owned private Linux Xvfb.
The network-disabled manual check reports failure for its generated C instance;
it establishes dispatch/failure handling, not an online metadata fetch. Preview,
post-drop and menu screenshots were inspected. Native receipt, tests, screenshots
and source/container provenance are in target/live-acceptance/library-20261012/.
The ownership-negative runner exits 2 before desktop creation. Physical desktop
checks remain open; no personal profile or running Windows app is operated.

These improvements strengthen F08/F09/F12 and leave the migration totals at
24 completed tickets and 15 external gates, with optional F40 separate.

### 2026-10-12 Modernity release assets and third-party content

The former Mods detail tab is now Add-ons, containing the existing Mods manager
and Resource packs. Local ZIP import, active/persistent inventories and confirmed
managed removal are reachable. Existing active-only packs/folders remain visible
and protected. Main packs and optional extras are installed individually from the
newest published weekly release matching the instance GTNH 2.8/2.9 series. The
current 2.9 release has 12 verified assets: ten resource packs and two alternative
MyCTMLib JARs. Only one mod identity can be installed, including conflicts with
pack-provided mods. Both UI variants remain choices; dependencies are explained
through the exact upstream release notes rather than silently installed.

The original profile and persistent-minecraft overlays are unchanged. Resource
packs and source receipts survive the actual staged update/reinstall transaction.
Downloads have bounded sizes/timeouts/redirects, exact upstream SHA-256 and size
checks, archive path/CRC/expansion checks and pack_format 1 validation. Repository
ZIP wrappers are removed without discarding textures, licenses or attribution.
Per-instance reservations and stopped-game checks apply before writes. Collisions
never overwrite active pack/user files. Atomic paired writes roll back on failure;
read-only receipt failure keeps the old receipt and leaves no installed artifact.
An orphan receipt can be replaced by a later successful import. Removal protects
externally changed active files and stages every managed target before disposal.
The existing Mods removal path also cleans up downloaded-mod provenance.

Current serial suites: **242 Windows / 239 Linux**, comprising 46 app,
140/137 backend, seven profile integration and 49 core tests. One opt-in NSIS test
is ignored on Windows. The launcher picker workflow tests cancel/import/invalid ZIP/
paired files/remove/busy state. Backend tests cover archive wrappers and unsafe
paths, source receipts, mod identity conflicts, failed-write cleanup and orphan
receipt recovery. The actual maintenance test retains an imported pack and receipt.
A launcher regression clicks separate release assets with the same button label and
verifies that the selected asset is dispatched. Each catalog control now has a
stable per-asset ID; managed resource rows also have distinct IDs.

The headless online smoke installed upstream Modernity-2.9.x-7180fe9.zip,
Outlined Ores and MyCTMLib 1.4.2 from weekly-2.9.x-2026-10-12, verified checksums,
restored overlays after deleting only its generated active game folder, removed
all three artifacts and retained unrelated instance settings. Receipts and native
screenshots are under target/live-acceptance/content-20261012/.

The final native Docker/Xvfb run also passes through the actual Add-ons controls:
the main pack, Outlined Ores and MyCTMLib 1.4.2 install individually, paired files
match, source receipts identify the exact upstream release and unrelated metadata
survives. The downloaded mod appears in the existing Mods manager. The alternative
1.4.1 release is blocked and labelled "Other version installed"; 1.4.2 is labelled
"Installed". Both the final content UI tests pass after this wording correction.
The native screenshot was inspected and normal close exits 0. The owned window
manager is awaited before app boot; no activation fallback was needed. Earlier
failed native attempts remain recorded in their container logs. The runner's
unowned-session check exits 2 before desktop creation. No physical desktop or
original user profile is used. `native-verification.json`, `verification.json`
and `source-hashes.json` capture the tested binary and final sources.

```text
cargo test --workspace --lib --bins --tests --locked --jobs 1 --features stable-updates -- --test-threads=1
cargo test --workspace --lib --bins --tests --locked --offline --jobs 1 -- --test-threads=1
cargo run -p industrialis-backend --example third_party_content_smoke --locked --jobs 1
LAUNCHER_ISOLATED_CONTENT_CONTAINER=1 sh scripts/test-linux-content.sh /target/debug/examples/profile_ownership_smoke /output
```

F41 closes the requested enhancement. The original parity count remains 24 done /
15 external gates; optional F40 remains planned. Minecraft visual rendering,
connected-texture acceptance, user-profile/physical-desktop checks, general
Modrinth/CurseForge catalogs, dependency planning, third-party updates and resource
pack activation/order controls are not claimed. The latter product follow-ups
are tracked in THIRD_PARTY_CONTENT.md. Electron removal remains gated.

### 2026-10-13 Files Explorer tree

The supplied VS Code Explorer screenshot now guides the Files sidebar. Folders
expand in place, with lazy reads, cached children, file-type SVG icons, indentation
guides, a selected row and a collapsible .minecraft root. The toolbar provides
Refresh, Collapse all and Pop out with accessible names. Persistent overlays keep
their own real marker; Git statuses and unsupported creation controls are not
invented. The wide sidebar starts at 280 pixels and has a draggable divider.
Narrow panels keep a bounded tree above the native editor.

Folder expansion/collapse/refresh is separate from document reads and saves.
Requests use a tree refresh generation and original instance-relative file service.
An unsaved draft remains open while exploring other branches. File changes retain
save/conflict protection; each instance keeps independent browser/draft state.
Keyboard Up/Down/Home/End, Right/Left and Enter navigate the real tree.

The app suite passes 47 tests on both Windows and Linux. The new regression opens
nested folders, preserves an unsaved draft through pointer and keyboard expansion,
refreshes an external file and checks instance isolation. Existing last-file,
Easy/Advanced save/conflict and independent pop-out tests pass. The compact editor
retains its bounded browser and verifies Save is within the editor above the
launcher action bar. Every newly bundled Lucide SVG loads and rasterizes visibly.
The backend/core implementation was unchanged; its full baseline remains the
2026-10-12 record rather than a new full-workspace claim.

Native checks use generated profiles on an owned Docker/Xvfb desktop, in dark and
light Sandstone at 1600x900 and a compact 1100x900 layout. They expand nested folders,
open a highlighted file, edit an Easy value, expand another branch without saving
that draft, explicitly save matching game/persistent bytes, use Find without
changing the document, navigate the tree with keys, collapse, refresh an external
entry and resize the sidebar. Source comments and extension metadata survive.
Normal close exits 0. Screenshots and binary/source hashes are under
`target/live-acceptance/explorer-20261013/`; personal profiles and the physical
Windows desktop are untouched. Earlier failed test/build/harness attempts remain
in their container logs and are not counted as acceptance passes.

```text
cargo test -p industrialis-app --bin industrialis --locked --jobs 1 --features stable-updates -- --test-threads=1
cargo test -p industrialis-app --bin industrialis --locked --offline --jobs 1 -- --test-threads=1
LAUNCHER_ISOLATED_EDITOR_CONTAINER=1 sh scripts/test-linux-editor.sh /target/debug/examples/profile_ownership_smoke /output explorer
```

This improves F12 without changing the original parity totals or external cutover
gates. Design and behavior are documented in EDITOR_DESIGN.md.

### 2026-10-14 Open VSX theme imports

F42 adds Appearance browsing/search/pagination, pinned-version installs, saved
variants and explicit package updates. Public registry downloads require the
registry SHA-256, and only bounded static theme data is retained. JSONC includes,
XML TextMate files and publisher/name checks are covered. Native workbench colors
and TextMate syntax styles share the existing settings path. Imported theme
selection chooses its own brightness; custom copies retain the mode/syntax being
saved. Existing presets, unrelated settings and unknown theme/token fields survive
updates and reload. Editor theme changes preserve the editing entity and undo.

Validation on Windows: 50 core, 144 backend and 51 app tests pass. The backend
NSIS integration remains intentionally opt-in. The separate ignored-by-default
Open VSX Launcher test was explicitly run and passed against the public registry,
dispatching actual Install, Apply and Search controls in a virtual TestAppContext.
The backend online smoke also passed: Dracula 2.25.1 imported two variants with
85 syntax rules each, occupying 51,139 persisted bytes. Generated profiles were
removed. Logs and a source/log hash receipt are under
`target/live-acceptance/openvsx-20261014/`. No host desktop, personal profile,
rendered native desktop acceptance or release was involved. This closes the new
enhancement and leaves the original parity/cutover counts unchanged.

### 2026-10-14 Group context menu

The visible group Rename button is removed. Right-click a group header for
Create instance or Rename group. Shift+F10/the keyboard Menu key opens the same
menu beneath the focused header. The menu uses the existing deferred library
popup shell, with viewport clamping, Escape and outside-click dismissal; only
one instance/group menu can be open. Group rename uses the existing transactional
service. Creation opens the existing install dialog with the stored group key
prefilled; Main/Ungrouped retains the empty key even when its display name is
customized. Global Add retains its normal empty-group default. Named Delete
buttons retain their existing behavior.

The Windows Launcher suite passes 52 tests. The new virtual-window regression checks
right-click without collapse, actual Rename dispatch and disk persistence,
Shift+F10/Escape, creation into a populated group, an empty group and the renamed
Ungrouped section, the rendered field's copied value, cancel without writes and
no stale group on global Add. Catalog work is deliberately held unpolled in a
current-thread runtime, so these immediate controls do not depend on live HTTP.
The existing group/order/drag and instance-menu regressions pass. The disposable
Windows native acceptance script now uses Shift+F10 for Rename; its updated path
has not been run in a native guest in this pass. Logs are under
`target/live-acceptance/group-context-20261014/`. No personal profile or physical
desktop input was used. This changes F06 interaction, not the external cutover gates.

### Library background context menu

The permanent New group button is removed. Right-click blank space in the left
instance library for New group, using the existing creation dialog and shared
group file. The library is keyboard-focusable; Shift+F10/the Menu key opens the
same action. Group headers retain Create instance/Rename group and tiles retain
their instance actions. In-progress cards do not open the background menu. The
popup uses viewport clamping, Escape and a capture-phase
outside-click dismissal so underlying controls cannot swallow the close event.

The virtual Launcher regression exercises empty and populated libraries, keyboard
opening, typed group creation and disk persistence, header/tile menu separation,
and dismissal without extra writes. The existing keyboard-create/drag regression
now opens creation through blank-space right-click. The disposable Windows
native harness now opens the library context menu before New group; it is syntax
checked but this new path has not been run in a native guest in this pass.
All 54 app tests pass on Windows with virtual Launcher windows; the live Open VSX
network test remains opt-in/ignored. Rust formatting and guest-script syntax
checks pass. This adjusts F06 interaction without closing external cutover gates.

### Settings alignment and silent Java discovery

Settings uses left alignment while retaining its existing content width/padding,
matching Accounts. Java discovery and Test Java now share a process builder that
sets Windows CREATE_NO_WINDOW and kills cancelled/timed-out probe children.
Opening Java preferences from a GUI-subsystem launcher previously created a
console for every inspected java.exe. Only metadata probes use this builder;
game launch/lifetime handling is unchanged.

All eight Java tests pass, including a real Windows console-subsystem child
checking GetConsoleWindow is null and stdout/stderr remain captured. Read-only
CLI discovery still finds all nine installed runtimes; Test Java succeeds with
the installed Java 26.0.2 executable. No personal profile writes or desktop input
were used for validation.

### Automatic instance disk usage

Refresh sizes is removed from the library toolbar, and Refresh size is removed
from instance menus. Main-window startup measures idle instances asynchronously,
skipping running/recovering/busy instances individually instead of rejecting the
entire refresh. Successful background measurements leave activity status alone.

Owned-game and restored-game exit handling refresh the cache after game/capture
cleanup and before the UI reloads instances. This also covers crashes and stops;
detaching from a still-running game does not measure it. A cache-write error is
logged without replacing the actual game exit result. Size-only transactions
reload the latest instance settings, preserving concurrent preferences and
unknown fields. Install/update/copy continue using their existing size handling.

Validation: 146 backend and 56 app tests pass; two external opt-in tests remain
ignored. New virtual Launcher coverage verifies startup filtering, duplicate-call
guards, absence of the toolbar action, quiet completion, and recovered crash
exit -> persisted size -> visible library snapshot. Backend recovery checks
also cover updated size alongside playtime/user edits and unchanged size when
closing the launcher with a game alive. Native guest menu expectations are
updated and syntax-checked; that guest path was not run for this change.

### 2026-10-18 Release CI corrections

PR #65 prepares stable 0.4.0; publication is still pending. macOS arm64 now
builds and packages successfully after Linux credential iterators received their
missing platform guards (run 39074721691). Windows also built/verified the actual
installer and portable archive and passed the native updater handoff, but its
retirement-failure fixture exposed a silent-mode error dialog blocking exit. All
installer error dialogs now supply a silent default; native failure/rollback,
nonzero exit and profile-preservation checks pass locally.

Linux CI caught a same-timestamp rename missing from the cached filename index.
The regression reproduces that failure deterministically on Windows. Missing
cached paths now invalidate the next query, and a five-second safety refresh
bounds stale additions when directory timestamps do not change. Four search
tests pass, with 2.20 ms per cached query across 2,001 files. No new dependency or
on-disk index is introduced. Artifact verification also writes the JSON receipt
required by the isolated Squirrel replacement harness. These corrections require
a new complete remote run; no failed run is counted as release-ready evidence.

### 2026-10-18 Linux portable render acceptance

Run 39111974284 passed Linux tests, credential interoperability, native renderer
startup and packaging, then rejected the portable capture for fewer than eight
sparsely sampled colors. The assertion ran before PNG output, so no failed frame
was retained and it did not distinguish sampling, presentation delay or paint
failure. A deterministic sparse-label frame reproduces that false rejection.
The capture now counts the full client image, retains every attempted PNG, and
waits up to 60 seconds for a presented frame. The same eight-color requirement
remains; a truly blank frame still fails and leaves evidence. Five isolated Linux
Python regressions pass, including delayed paint, sparse UI, blank rejection and
archive guards. The actual portable smoke must pass in the next remote run before
publication; these harness regressions alone do not certify Linux rendering.

### 2026-10-19 Linux software-renderer diagnosis

Run 39113723778 passed Windows and macOS, but Linux retained a genuinely black
portable client image after the full 60-second deadline. The earlier sampling
correction did not fix that failure. A differential probe of the retained Linux
release binary reproduced one client color with Ubuntu 22.04/Mesa 23 Vulkan
software rendering and 6,435 colors with OpenGL software rendering. The newer
local Mesa stack also drew normally. The Ubuntu graphics probe used a newer
libc loader for that retained binary, so it is driver-isolation evidence rather
than current-artifact or old-OS compatibility certification.

The owned Xvfb portable runner now explicitly selects Mesa OpenGL software
rendering. It retains the same nonblank-client, same-owner activation, protocol
and normal-close assertions and labels the renderer fixture in its receipt.
Normal desktop launches keep the default renderer selection. The package
README and release notes document the affected older software Vulkan setup and
the OpenGL launch workaround; physical GPU/Vulkan and Wayland remain unverified.
The app now writes renderer selection/errors to stderr using the already
resolved `log` crate, without enabling account/network debug logs or adding
transitive packages. Windows stable-feature `cargo check`, six packaging/notice
contracts and all five isolated Linux Python regressions pass. The full current
portable must pass a fresh remote run before merge/publication.

### 2026-10-19 Windows compiler download resilience

Run 39277588176's Windows job failed before compilation because Chocolatey's
NSIS feed returned HTTP 503. GitHub rejected a single-job rerun while the other
platform jobs were still running. The pinned NSIS 3.11 install now retries up to
three times, with 15 seconds between attempts; permanent failures still fail
the job with the installer's exit code. The extracted PowerShell step parses
successfully and all three packaging/channel contracts pass. This is dependency
download resilience, not evidence that the failed Windows run passed.

### 2026-10-19 Explicit Linux OpenGL runtime

The retained log from run 39277588176 also reports a Linux surface-creation
failure before the window opens. The CI host dependency list installed Vulkan
drivers but did not explicitly provide the EGL/Mesa runtime used by the OpenGL
fixture. Those packages are now explicit, and portable acceptance checks that
the dynamically loaded `libEGL.so.1` can load; `ldd` alone cannot check it.
The preflight rejects an isolated runtime without EGL and passes in the existing
graphics fixture. Five Python regressions and three packaging contracts pass.
Failed Linux binaries/archives are retained for three days for exact-artifact
diagnosis. The missing-runtime correction remains subject to a successful full
remote native portable run; no surface-creation failure is waived.

### 2026-10-19 Pinned portable Windows compiler

The Windows job in run 39281549599 exhausted all three Chocolatey attempts with
HTTP 499 before compilation. CI now downloads the official NSIS 3.11 portable
ZIP directly, verifies its pinned SHA-256 before extraction, checks `/VERSION`,
and shares `MAKENSIS` with packaging and both installer smoke tests. It does not
install a global tool or accept an unverified fallback compiler. A fresh official
download matches the previously retained ZIP (`c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1`).
The exact extracted CI bootstrap step passes locally, as do the actual NSIS
retirement/failure/rollback/profile guards, updater install/restart/uninstall
smoke and three packaging contracts. The
fresh full remote platform checks remain required before release.

## 2026-10-19 canonical launcher workspace

The native workspace now lives at `apps/launcher` with package name
`@industrialis/launcher`. The Electron source was removed in PR #65. Root commands,
CI, packaging, acceptance scripts, lockfile importers and agent source maps now
use the canonical path. Existing release tags, asset names and profile locations
retain their compatibility contracts. The local build cache moved with the source.

Validation: Cargo workspace check passed offline from the new directory; ten Node
watcher, packaging, notice and Windows-runtime tests passed; five Linux portable
archive tests passed in the owned Linux container. Frozen offline pnpm lockfile
validation, lint and the canonical development command help dispatch passed.
This source-path change does not close the accepted provider/platform follow-ups.

## 2026-10-19 Prism Launcher import

Added `backend::prism` (per-OS detection, custom `InstanceDir`, manual folder
fallback, GTNH heuristic, metadata/icon mapping, collision-safe ids, staged
copy-only import, and version assignment) plus the launcher first-run prompt and the
permanent Add/library-menu entry points. Imported copies are ordinary instances
whose settings carry `imported: true` and, for detected GTNH packs,
`imported_gtnh: true`. GTNH copies without a `pack_version` stay frozen and the
update/check/reinstall controls are hidden; non-GTNH copies are always frozen.
The onboarding marker lives in `launcher-settings.json` and is written on either
Import or Start fresh, but not when the prompt is dismissed with Escape.

Verification on Windows with disposable Prism-shaped fixtures: 10 `prism::tests`
pass (detection parsing, GTNH detection, version suggestions, groups, collision
ids, copy/failure/cancel, non-GTNH version refusal, assigned-version metadata,
ignored formatting); a launcher onboarding test imports a GTNH and a non-GTNH
instance, checks source files remain, marker persistence, name/RAM/group/icon
mapping and the frozen state; a second Launcher test covers the Start fresh marker.
The full local run passed 65 app (1 ignored), 164 backend (1 ignored), 7
profile-persistence and 53 core tests. No real Prism installation was written to.
macOS, Linux, and Flatpak detection paths are implemented but were not exercised
on those platforms; real Prism profiles and assigning a version from the live
catalog remain platform/live verification.
