# Industrialis Launcher

Native production launcher as of 0.4.0. See [release notes](RELEASE_NOTES.md)
for downloads, accepted limitations and rollback.

`@industrialis/launcher` is the Rust desktop app,
replacing the former Electron + React launcher. Single native binary, no Chromium,
no Node runtime.

The Files panel uses the native gpui-base code editor with Syntect syntax
highlighting, line numbers, search, wrapping, selection and undo. Forge configs
also have lossless Easy controls. The browser and editor scroll independently;
wide panels place them side by side, with a stacked layout for smaller windows.
See [editor design and acceptance](EDITOR_DESIGN.md).

Pack status checks run at startup and hourly. Right-click an instance (or use its
actions menu) for a manual check. Dragging shows a rounded dashed insertion slot;
only release writes the order, and Escape cancels. Existing group files remain
the source of truth.
Right-click empty space in the left library for New group. Group headers offer
Create instance and Rename group. Focus the library and press Shift+F10 for its
keyboard context menu.
Instance disk usage is recalculated in the background at startup for idle
instances and after a game stops, including recovered sessions and crash exits.
Running instances retain their last cached size until exit; there is no manual
size-refresh control.

On first run, Industrialis can copy instances from Prism Launcher. The same import
is permanent from the toolbar Add menu and the library background menu. GTNH
instances are pre-selected; other instances can be selected with a warning and
stay frozen. Imports are copies only: Prism's folder is never moved, changed, or
registered. Display names, icons, Java/RAM/JVM overrides and Prism groups are
mapped; accounts are not copied. Imported GTNH copies start without update checks
until a GTNH version is assigned in the import dialog or later from Info, and
assigning a version enables pack update checks, updates, and clean reinstall.
Non-GTNH instances stay frozen with those actions unavailable. Discovery
checks the standard per-OS Prism data folders and honors a custom `InstanceDir`,
with a manual folder field as fallback. Imports run as cancellable Processes
operations with per-instance failure isolation; the Prism source stays untouched.

The instance Add-ons tab manages custom mods and resource packs. Import a local
resource-pack ZIP or browse Modernity's matching weekly release, including its
optional UI/texture add-ons and MyCTMLib mods. Downloads are checksum-verified and
stored in the original persistent overlay. Enable resource packs in Minecraft;
see [third-party content and provider work](THIRD_PARTY_CONTENT.md).

Appearance can browse and import public Open VSX color themes. Install a package,
then apply one of its variants to the launcher and highlighted file editor.
Variants are saved in the existing settings and work offline. See
[Open VSX themes](OPEN_VSX_THEMES.md).

## Linux rendering

The native backend selects Vulkan or OpenGL. If an older Mesa software driver
opens a black window, select OpenGL software rendering for that launch:

```sh
VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null LIBGL_ALWAYS_SOFTWARE=1 ./industrialis
```

This was reproduced with Mesa 23 on an owned Ubuntu 22.04 Xvfb desktop. The
portable acceptance fixture explicitly uses OpenGL there and still requires a
nonblank native frame, activation and normal closure. It does not certify
physical GPU/Vulkan or Wayland behavior. Renderer selection and GPU errors are
written to stderr for troubleshooting.

## Development storage

Development and test builds use limited debug information. Incremental caching
is enabled only for the frequently edited UI crate in dev, with 32 codegen units;
backend and test builds remain non-incremental. This retains module/line
information, reuses compiled dependencies and avoids storing separate large
incremental test variants. the framework's development optimization remains enabled for
smooth native window movement. Dev builds use two jobs; tests and packaging keep
their existing limits. See [build measurements and dependencies](DEV_BUILDS.md).

`target/` contains generated build outputs and disposable acceptance work, not
the launcher source or its installed application size. Keep logs, receipts and
screenshots for verified scenarios, and remove obsolete compiler caches rather
than retaining every experimental build. Preserve generated profile fixtures
until their acceptance/recovery data has been reviewed. The original user data
directory is separate and must never be treated as a build cache.

When full variable/type inspection is needed temporarily, override the profile
for that session instead of changing the default:

```sh
cargo run -p industrialis-app --config profile.dev.debug=2
```

## Layout

```
apps/launcher/
  crates/
    core/   # Pure logic ported from apps/launcher TS (no framework dependency)
    backend/ # Tokio services: accounts, Java, install, launch, instances
    app/    # UI window + views, depends on core
    cli/    # Headless backend harness
```

`core` ports the TS settings and log helpers using compatible JSON shapes.
The app uses Electron's existing data directory and files in place. Replacing the
launcher must require no user data migration or reauthentication. An explicit
`INDUSTRIALIS_DATA_DIR` override is available for isolated development/tests.
Real-pack/client and remaining provider/desktop/installer acceptance scenarios
are documented in [LIVE_ACCEPTANCE.md](LIVE_ACCEPTANCE.md). The original profile
remains in place; acceptance fixtures never require copying it.
Windows packages use static CRT linkage and reject external MSVC runtime imports
before distribution. Clean-Windows installer checks use the actual public Electron
installer inside an expendable Sandbox, without touching the personal installation.
Actual Electron/Rust credential interoperability also passes on Windows DPAPI
and disposable Linux GNOME Secret Service/KWallet 6 stores, retaining the original generated
key. Linux keeps its loaded libsecret/GLib libraries alive across calls. Live
Google, macOS Keychain and older KWallet/platform acceptance remain open.
Unknown profile fields and atomic saves are covered in
[profile persistence](PERSISTENCE.md). Full existing-profile compatibility still needs verification and fixes tracked
in [the migration checklist](MIGRATION_TRACKER.md#persistence-process-lifetime-and-desktop-behavior):

| Rust module         | TS source                        |
| ------------------- | -------------------------------- |
| `launcher_window`   | `src/lib/launcher-window.ts`     |
| `launcher_settings` | `src/lib/launcher-settings.ts`   |
| `instance_settings` | `src/lib/instance-settings.ts`   |
| `launch_log`        | `src/lib/launch-log.ts`          |
| `log_buffer`        | `src/lib/log-buffer.ts`          |
| `pack_version`      | `src/lib/pack-version-status.ts` |

## Framework pins

`industrialis-app` uses the published `gpui-pre` snapshots
(`gpui = { package = "gpui-pre", version = "=0.3.8" }`), the same pin the
`gpui-component` 0.7.x releases build against. Bump `gpui`,
`gpui_platform`, and `gpui-component` together — the framework is pre-1.0 and
snapshots may change the API.

## Commands

```bash
pnpm --filter @industrialis/launcher dev    # watch, rebuild and restart the launcher
pnpm --filter @industrialis/launcher dev:once # single run, without watching
pnpm --filter @industrialis/launcher build
pnpm --filter @industrialis/launcher test
```

Dev watches Rust source, embedded assets, Cargo manifests/lockfile and workspace
`.cargo` configuration. Saves are grouped before rebuilding; edits during a
build queue another build, and compile errors wait for the next save. No extra
watcher dependencies are needed. This is a full app restart, so transient UI
state is reset. Restarts use the normal close/save guards and wait for active
operations; resolve unsaved-file prompts or close the app before rebuilding can
continue. Ctrl+C stops the watcher after the current build/save finishes.
`target/` and profile data are excluded from watching. See [dev builds](DEV_BUILDS.md).

Or with cargo directly from this directory:

```bash
cargo run -p industrialis-app
cargo test --workspace
```

## Development rendering performance

The dev profile optimizes `gpui-pre` while keeping launcher code unoptimized
with debug information. Native Windows moves invalidate the UI layout; the
default unoptimized framework made the full launcher redraw too slowly during
dragging. This does not change the release profile.

Run the isolated regression benchmark without showing or focusing a window:

```bash
cargo run -p industrialis-app --example window_move_bench --locked
```

It renders the real launcher with two fictional instances at 1600 by 900, warms
up for ten redraws, and reports median/p95 CPU layout time for 150 bounds-change
redraws. Exit status is nonzero when p95 exceeds the default 16.67 ms budget;
`LAUNCHER_BENCH_BUDGET_MS` can adjust that budget for slower test machines. It does
not read the real user profile, launch games, inject input, or measure GPU
presentation/actual drag FPS. Run it on a desktop-capable machine, separately
from correctness tests. To compare the old dev configuration:

```bash
cargo run -p industrialis-app --example window_move_bench --locked --config 'profile.dev.package.gpui-pre.opt-level=0'
```

## Port status

Launcher self-updates and native preview packaging are implemented. See
[Updates and releases](UPDATES.md) for the isolated Launcher channel, Windows
installer handoff, manual updates on other targets, packaging commands, and
remaining distribution verification. Development builds do not check updates.

See [the functional migration tracker](MIGRATION_TRACKER.md) for the audited
missing features, verification work, and gates for deleting the Electron
launcher. Keep it updated as workflows land; aesthetic work remains in
[UI parity](UI_PARITY.md).

The native window has the top toolbar, Instances, Processes, Accounts, and
Settings views, install/delete/copy dialogs, and live launch logs. Accounts use
inline offline-account creation, Microsoft device-code sign-in, default stars,
removal, skin faces, license messages, and the toolbar account picker.

Instance details include Info, Settings, Logs, Files, Mods and Backups. General, Java, Custom
Commands, and Environment panels autosave serialized patches against the latest
disk settings. Launch and maintenance wait for pending edits; running instances
and active maintenance disable edits. Java controls detect, browse, and test the
executable. Play-time visibility and recording follow the override settings.
Crash/exit console preferences and quit-after-game-stop are wired to launch
completion. Windows hide-after-launch now hides and restores the native window;
macOS uses native window hiding and a save-aware Dock/menu quit guard. Linux
minimizes because the pinned adapter does not implement hide. Physical native
platform acceptance is recorded separately in the tracker.

Copy reports bytes copied, preserves the source, and exclusively claims the
destination. Update analysis lists pack changes and custom mods to keep. Update
and reinstall offer an opt-in copy path, with the operation applied to that new
copy. Target archives are staged outside the installed instance before replacing
pack files. Player data, settings, icons, and selected persistent mods survive
replacement; previous recovery data blocks destructive retries. The operation
reservations are in-process, with pid checks for running games and exclusive
mkdir for destination collisions; they are not a cross-process maintenance lock.

Processes now show background operation history for install, copy, import, update,
reinstall, and delete, with active-first ordering, transfer progress, operation
logs, completion/failure states, dismissal, and a toolbar popup. Active installs
and copies have progress placeholders in the instance grid. Copy-before-update
records the copy and subsequent update as separate operations. Preview analysis
stays in its dialog. Delete reports incremental progress and supports cancellation;
it keeps discovery/group metadata for a partially deleted instance, so deletion
can be retried. Cancellation does not restore already removed files.

Console panels load persisted output, retain 5,000 visible lines, copy the full
persisted window, extract the latest crash, and clear the saved file when the
instance is idle. A single ordered writer queue and read barrier keep live and
saved logs consistent. Live scrolling follows the tail until the user scrolls
away. Game startup sends the actual pid to the UI, and snapshot refreshes preserve
owned active games rather than leaving them on Starting or losing another game.

Files use the native code editor with search, line numbers and wrapping, plus
lossless Easy/Advanced Forge editing. Game files and persistent overlays save
together with conflict checks. Each detached file has an independent native
window; repeated opens focus it, and launch/maintenance save all matching drafts.
External editor saves are mirrored by pathname, including atomic replacements.
The Mods tab adds/removes original persistent custom mods with guarded native
picker and folder actions. The shared Java chooser supports search and sorting.

Backups use the original provider/queue files and Drive folder layout, with manual
upload/verified download/deletion, automatic settling/retries, cloud-only retention,
progress and health. Existing Windows encrypted credentials interoperate with
Electron. Live provider and native macOS/Linux key-store checks remain open.

The retained Aditya configuration service is available through
`industrialis-cli preset-status <id>` and `apply-preset <id> [--disable]`.
Its nine bundled files belong to the framework and are embedded in the binary. The original
React UI had no caller for this service. Disabling removes overlays and keeps game
files. Both mutations and other writer CLI commands participate in profile ownership.

The UI uses owned EBF artwork and Lucide icons, a 44px toolbar, 24px footer,
144px instance tiles, and a resizable library/details split starting at 54/46.
Groups/order/collapse persist in the original `instances/instgroups.json`; only
the split position uses `launcher-workspace.json`. Accounts
use the centered reference layout; launcher preferences use its sidebar layout.
Windows drag, minimize, maximize, and close controls are wired, with an 800×600
minimum window size. See [UI parity](UI_PARITY.md) for the reference dimensions,
remaining differences, and visual verification requirements.

The functional port now includes Files/Mods panels, group management, cloud
backups, delete cancellation, launcher updates/packaging, native text selection,
multiline arguments and Java chooser search/sorting. Actual GTNH Java 8/17+ pack
installation/maintenance, virtual-desktop client loading/Stop, Linux detached
recovery, and newly authorized Microsoft refresh/client launch pass. Replacement
acceptance still needs existing-account refresh, Google authorization/transfers,
physical input/IME and window behavior, installed upgrades, and native platform
checks in [MIGRATION_TRACKER.md](MIGRATION_TRACKER.md). Precise visual
parity is tracked separately. Offline fixtures do not prove live services.

Run `node scripts/rehearse-cutover.mjs` from the repository root to build and
test a source-only snapshot containing no original launcher directory. It uses
the existing Cargo cache and a separate example filename for the real app, so
it does not replace a running development executable. Process tests use synthetic
temporary profiles; the final render benchmark opens a hidden window. Evidence
and source hashes remain under `target/source-removal-*`. The rehearsal does not
delete source, change the user's profile, or switch production downloads.

Root `pnpm dev` and `pnpm dev:launcher` select Launcher. The legacy app is available
as `pnpm dev:launcher:electron` for comparison, after closing Launcher. Turbo preserves
`INDUSTRIALIS_DATA_DIR` and `JAVA_HOME` for native development. Cargo owns native
build/test caching; Turbo does not cache a successful build without its executable.
Root launcher packaging commands also select Launcher. Production downloads and the
Electron release feed remain separate until [cutover acceptance](CUTOVER.md).

## Window materials

Settings > Appearance > Window material offers Opaque, Frosted glass, and Mica.
Changes apply immediately and persist across restarts. Tint strength ranges from
20 to 90 percent; lower values reveal more of the native backdrop. Text, icons,
buttons, dialogs, and popovers retain opaque content for readability.

Frosted glass uses Windows Desktop Acrylic; Mica uses the wallpaper-based system
material. Both use the supported DWM backdrop API on Windows 11 22H2 or newer.
See Microsoft's [system backdrop types](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type).
Older Windows versions, other operating systems, and the renderer with Direct
Composition disabled use an opaque fallback, with an explanation in Appearance.
Windows can also replace the material with a solid system color when effects
are disabled. Existing profiles default to Opaque.

The hidden-window performance benchmark also checks all three material switches
in light and dark themes, including the actual Windows DWM attributes. In
PowerShell, from this directory:

```powershell
$env:LAUNCHER_BENCH_MATERIAL = 'frosted' # opaque, frosted, or mica
$env:LAUNCHER_BENCH_REQUIRE_BACKDROP = '1' # fail if a native material is unavailable
cargo run -p industrialis-app --example window_move_bench --locked
Remove-Item Env:LAUNCHER_BENCH_MATERIAL, Env:LAUNCHER_BENCH_REQUIRE_BACKDROP
```

The benchmark excludes compositor presentation and actual desktop drag cadence.

Linux native rendering can be checked with `sh scripts/test-linux-render.sh`
from the repository root. It needs Xvfb, Openbox, D-Bus, Mesa Vulkan drivers and
the normal native build libraries. It uses fictional instances in a virtual desktop,
with a 45-second timeout. Set `LAUNCHER_BENCH_BUDGET_MS=16.67` for a stricter layout
budget; its default CI budget checks startup/render/shutdown rather than desktop FPS.

## Reusable framework skills

Versioned skills live in the repository and are also installed in the local Codex
skill directory:

- [Framework state and contexts](../../.agents/skills/gpui-state-and-contexts/SKILL.md):
  entities, context lifetimes, application boot, and asynchronous updates.
- [Framework layout and components](../../.agents/skills/gpui-layout-and-components/SKILL.md):
  element composition, scrolling, themes, and navigation of Zed's UI crates.
- [Framework input and actions](../../.agents/skills/gpui-input-and-actions/SKILL.md):
  focus, action/key dispatch, bindings, and native text input.

Each skill links to the supplied official framework/Zed sources and distinguishes
upstream main from the pinned crate API.
