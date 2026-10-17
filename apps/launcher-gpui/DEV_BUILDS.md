# GPUI development builds

The reported delay occurs while Cargo says Compiling, before application startup.
`pnpm --filter @industrialis/launcher-gpui dev` runs the dependency-free Node
watcher in `scripts/dev-launcher-gpui.mjs`. It builds with Cargo, launches the
reported executable and automatically rebuilds/restarts on source or asset saves.
Watch mode builds `industrialis-dev` with the `dev-watch` feature, so a running
ordinary `industrialis.exe` cannot lock its output on Windows. Dependencies use
the same target directory/cache; this adds only another app target/cache rather
than rebuilding the dependency graph into a separate target directory. Ordinary
runs and distribution builds continue to select `industrialis`. The feature is
not enabled by distribution builds. Close any other launcher using the same
profile before opening the watched version; the profile ownership guard still
allows only one writer.
On Windows, the watch target uses the GUI subsystem to avoid an extra console.
The runner must launch it with `windowsHide: false`: Node's hide flag sets
`STARTF_USESHOWWINDOW`/`SW_HIDE`, which can hide GPUI's first native window too.
The Electron,
website and dashboard JavaScript dependencies are not compiled into this native
build. pnpm may verify/install workspace dependencies when package manifests
change: it performed a 33.5-second one-time verification install after the dev
script update. A warm pnpm-to-Cargo help dispatch then took 0.23 seconds. For Rust
iteration without pnpm workspace verification, run `node
../../scripts/dev-launcher-gpui.mjs` from `apps/launcher-gpui`. For a single run,
use `pnpm --filter @industrialis/launcher-gpui dev:once` or `cargo run -p
industrialis-app --locked --jobs 2` from that directory.

## Automatic rebuild and restart

The watcher polls only the `crates/` tree, workspace `.cargo/`, Cargo manifests,
lockfile and Rust toolchain file every 500 ms, grouping save bursts for 250 ms.
Generated `target/`, `dist/`, node_modules and temporary editor files are excluded.
It adds no npm/Rust packages and uses the existing Cargo build caches. Cargo's
reported artifact path is used, including custom target-directory configuration.

This is a full process restart, not HMR. Saved settings/files are retained, while
open panels, text selections and other transient UI state reset. Before building,
the runner asks only its own launcher process to close through a private stdin
pipe enabled only for debug builds with `INDUSTRIALIS_DEV_WATCH=1`. The normal
unsaved-file prompts and backend drain remain in effect. Resolve the prompt (or
close the launcher manually after cancelling it) to continue. Running games are
handled by the existing broker/recovery flow. The runner does not force-kill the
launcher, an existing launcher, or Minecraft.

Compile errors leave the watcher waiting for the next save. Saves during a build
queue one more build before launch. Ctrl+C waits for the current build and safe
launcher shutdown. Closing the app yourself leaves the watcher waiting for a
source change. Stop an existing one-shot `dev` process and rerun the command once
to switch to watching.

Validation: `node --test scripts/dev-launcher-gpui.test.mjs` covers atomic saves,
new folders, ignored build output, build serialization, save/drain waits and
failure recovery. Its end-to-end test uses real Cargo and disposable headless
Rust processes; it does not launch the personal profile or control the desktop.
The GPUI app tests compile the development pipe and existing shutdown path.
The headless watcher regression also keeps an ordinary executable running on
Windows through startup, edits, compilation failures and shutdown, verifying
that it stays alive while watch mode rebuilds its own target.
Its watched fixture uses the Windows GUI subsystem and inspects real
`GetStartupInfoW` flags, rejecting `SW_HIDE`; it still creates no desktop window.

References: [Cargo JSON artifact messages](https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages)
and [Node child processes](https://nodejs.org/api/child_process.html).

## What is in the build

The Windows normal/build graph was 442 unique packages, including the three local
app/backend/core crates, and is now 440. Most packages come through the GPUI
framework/platform, the native code editor and backend functionality such as HTTP,
crypto, archives, authentication and process management. The app has 12 direct
normal dependencies and one build dependency on Windows. A large transitive graph
does not imply every dependency is compiled on each launch.

The safe feature reductions are:

- `sysinfo`: process/system APIs only. Disk, network, user and temperature-sensor
  collectors are not used by the launcher.
- `syntect`: bundled syntaxes/themes, the Rust regex engine and YAML loading for
  the Forge grammar. Its HTML renderer and plist loader are unused. Open VSX
  TextMate XML imports still use the backend's explicit plist parser.
- `zip`: the ordinary flate2 deflate encoder/decoder. Removing the unused optional
  Zopfli encoder removes `zopfli` and `bumpalo`. An explicit flate2 dependency
  selects its Rust backend, so standalone backend/CLI builds also work.

No package versions were upgraded by this change. GPUI currently hardcodes many
image decoders, SVG support and core services; gpui-base also includes Markdown
and HTML parsing. Removing those would require maintaining upstream forks or
replacing the editor kit. They are potential future work, not achieved reductions.

## Faster edit/rebuild cycle

Incremental compilation is enabled only for the app crate in dev, with 32 codegen
units. Backend and tests keep incremental compilation disabled; tests override
the app setting explicitly. Limited debug information remains. GPUI's opt-level 3
stays unchanged because it was needed for smooth window movement. Dev uses two
build jobs, while test/package commands keep their existing limits.

The retained cache is specific to the app target being built. Another target or
feature set can create another app cache. There is no automatic hard size cap;
avoid accumulating experimental dev feature combinations. Registry dependencies
still reuse their normal compiled artifacts. A profile/feature/toolchain change
can require a one-time rebuild of affected dependencies.

## Measurements on 2026-10-14

Windows, 12 logical processors, about 15 GiB free physical memory at measurement.
All builds used the same workspace and compiler. Companion filename
`profile_ownership_smoke` links the real app source, allowing measurement while
the user's normal `industrialis.exe` remains running. No host desktop input,
user-process termination, personal-profile startup or cache deletion was used.

| Scenario                                          | Wall time |
| ------------------------------------------------- | --------: |
| Cached build before                               |    2.18 s |
| App-only compile/link before, cached dependencies |   49.63 s |
| First build after feature/profile changes         |  263.36 s |
| Small UI string edit with the new cache           |   13.72 s |
| Restoring that UI string, with the new cache      |   16.37 s |
| Cached companion build after                      |    2.05 s |

The edit changed the initial Ready status string in main, then restored it. The
source hash was checked before/after. This measures a small UI edit; a broad
refactor or changed dependency can invalidate more work. It does not establish
runtime startup latency or clean-build speed, and is not a comparison of freshly
compiled dependencies. The first attempted timing included 120 seconds waiting
for the user's active Cargo build lock and is excluded from the comparison.

After the incremental measurements, the companion app cache was 293.46 MiB and
its build-script cache 1.26 MiB. These are observed sizes, not a guaranteed limit
for all future dev variants. Timings, dependency graphs and logs are under
`target/live-acceptance/dev-startup-20261014/`.

Linux and macOS graphs resolve with the narrowed features. This pass does not
claim compilation or desktop acceptance on those platforms. The changes are
configuration only; app/backend behavior is checked by their Windows test suites
and a real checksummed Open VSX theme import into a disposable profile.

Sources: [Cargo profiles and overrides](https://doc.rust-lang.org/cargo/reference/profiles.html),
[ZIP library](https://docs.rs/zip/2.4.2/zip/) and
[Syntect modules](https://docs.rs/syntect/5.3.0/syntect/).

Final verification: 144 backend and 51 app tests pass on Windows. Two external
scenarios remain ignored by default (NSIS integration and the Open VSX online
UI test). The explicit backend online theme smoke also passed after feature
narrowing, downloading/reading a real checksummed Dracula VSIX and persisting
both variants in a generated profile. The profile was removed. The app cache
remained at the measured size after tests; no incremental test target was added.
The source/log hash receipt is `target/live-acceptance/dev-startup-20261014/receipt.json`.
