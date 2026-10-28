# Profile preservation and saves

## Edited game configuration

Easy and Advanced editing use the same save path. **Save** / **Ctrl+S** writes both
the active `.minecraft/<path>` file and a separate
`persistent-minecraft/<path>` copy. Switching files saves the current valid draft
first; unsaved or invalid drafts block destructive instance operations and closing.
Merely typing or toggling an Easy control creates an unsaved draft.

Pack updates and clean reinstalls preserve that overlay while replacing pack files,
then restore it over the new defaults. Launch preparation also reapplies it. This
preserves the entire saved file, including comments and unmodified properties; it
does not merge individual properties with new pack defaults. **Remove override**
stops future restoration but keeps the current active file. External-editor changes
use the existing persistent mirror.

`install::tests::editor_saved_forge_values_survive_updates_and_clean_reinstalls`
saves through the editor backend, then replaces fresh pack configs through the
actual staged update and reinstall flow. It uses a path outside the legacy
preservation allowlist so passing requires the editor's persistent overlay.

Launcher is a drop-in replacement: it opens the same `industrialis-launcher` directory
and existing files as Electron, in place. Users must not copy their data, import a
profile, run a migration wizard or sign in again just because the UI was replaced.
Windows uses `%APPDATA%/industrialis-launcher`, macOS uses
`~/Library/Application Support/industrialis-launcher`, and Linux uses
`$XDG_CONFIG_HOME/industrialis-launcher` (or `~/.config/industrialis-launcher`).
`INDUSTRIALIS_DATA_DIR` is an explicit development/test override, not a new default.
Opening the library is read-only: missing pack versions are filled in for display
without rewriting the source document.

Launcher and instance settings retain unrecognized JSON fields. Theme overrides
support all twenty theme tokens and retain unrecognized tokens. Custom preset
documents preserve extension fields through the native theme editor. Saves compare interpreted
settings and patch only changed fields into the original JSON. Absent defaults and
untouched legacy values keep their original representation; no-op saves preserve
the original bytes. Existing known fields with invalid types use defaults in memory,
while their raw values remain intact unless edited. Malformed JSON,
wrong document shapes and read failures are errors and block rewriting that file.

Accounts load current version 3 and legacy entries independently. A malformed or
unsupported entry cannot hide valid accounts, and mutations keep that entry in
`accounts.json` for recovery. Unchanged legacy accounts retain their existing shape
even when another account is added, refreshed or removed. Unchanged rows retain
optional nulls and their original order. A changed account uses the same current
version 3 format supported by Electron; no separate Launcher account format is introduced. Unrecognized
account, token, profile, entitlement and skin fields survive serialization and
refresh. A late token refresh cannot restore an account removed during the request.
These files still use the existing plaintext credential format; existing encrypted
cloud credentials keep the original safeStorage envelope and provider file.

Google Drive reads the same schema-1 `google-drive-state.json` and encrypted
refresh token. Windows uses Electron's v10 AES-GCM/DPAPI Local State key; native
Electron 43.3.0/Rust interoperability was tested with an isolated profile in both
directions without changing the key file. macOS v10 Keychain and Linux v11 Secret
Service/KWallet formats use the original Chromium CBC/PBKDF2 construction.
Linux legacy v10 basic-text envelopes can be read, while new credentials require
a secure store. Missing, denied or corrupt existing keys fail visibly and are
never replaced with a new key. Native GNOME Secret Service and KWallet 6 now pass
both Electron/Rust directions in fresh owned Linux sessions with unchanged keys.
Their reproducible offline runners and clean recipe are recorded in the tracker.
macOS Keychain, older native stores and live Google authorization remain acceptance
gates. Primary behavior references are
[Electron safeStorage](https://www.electronjs.org/docs/latest/api/safe-storage) and
[Electron 43.3.0 implementation](https://github.com/electron/electron/blob/v43.3.0/shell/browser/api/electron_api_safe_storage.cc).

The same `backup-state.json` retains queued delivery, retries, retention, deletion
tombstones and activity. Cloud artifacts keep the original content-addressed
Drive layout. Restore downloads a verified archive into `.minecraft/backups`,
without unpacking a world or replacing a local archive. Retention removes cloud
copies only. Transfers are bounded to two different instances; one instance's
uploads and retention run serially.

Group changes preserve extension fields at the root, group and ungrouped levels,
including empty named groups. Invalid group documents block mutation. Existing
icons, console logs, PID documents, overlays, mods, pack cache and backup metadata
are untouched by preference/account/group edits.

## Save ownership

`backend/profile_owner.rs` acquires cross-process ownership before Launcher startup
reads (which can also prune PID state). The guard outlives the backend runtime.
Electron uses the matching `profile-owner.mjs` protocol before constructing its
backend, and holds ownership through disposal. CLI commands that can write acquire
operation ownership; read-only inspection remains available while a launcher runs.
A second launcher signals the owner and exits. Launcher polls the activation signal on
the UI executor and activates/restores its existing window; Electron restores,
shows and focuses its existing main window. Signals received during startup wait
for window creation. On Windows the Rust client identifies the owner PID, calls
[`AllowSetForegroundWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow),
then requests activation. OS foreground policy can still restrict focus.

Ownership is the exclusive binding of a loopback TCP listener, released by the
kernel on process exit or crash. The canonical profile path (with Windows prefix,
separator and case normalization) is hashed with SHA-256. The first two hash bytes
select a port in 20000–39999; the complete hash identifies the profile in bounded
128-byte messages. The protocol accepts only probe/focus requests and replies with
owner kind/PID; it never carries credentials, deep links or profile documents.
An occupied endpoint that cannot identify the profile blocks startup/writes;
there is no alternate-port fallback that could admit a second writer. Independent
profiles can rarely collide on a port and fail closed. Requests have bounded
timeouts. This is an ownership mechanism, not an access-control boundary against
unrelated tools editing files directly. See the primary API references:
[Rust TCP listener](https://doc.rust-lang.org/std/net/struct.TcpListener.html) and
[Node net server](https://nodejs.org/api/net.html#serverlistenoptions-callback).

Old released Electron/Launcher binaries do not implement this shared protocol and
cannot be retroactively locked by it. Installed-version cutover must stop the old
process before starting its replacement. Startup now detects packaged Electron
processes before admitting a new default-profile owner and refuses to write while
one is found. This is a best-effort check: it cannot prevent an uncooperative old
binary from starting afterward or reliably identify every custom development
launch. The installed cutover must remove the old entry point. Verification of
that path, physical
minimized-window focus and macOS/Linux behavior keeps F26 open. No user data copy
or migration is involved. URL activation is retained; the original had no UI
subscriber that consumed OAuth deep links.

`backend/profile_store.rs` serializes profile read/modify/write transactions in
one process. Account creation, removal, token persistence, default selection,
launcher preferences, instance edits and group changes use this boundary. No
network request or game/pack operation runs while holding it. An admitted
transaction owns its task, so cancellation of its caller does not release the
mutex during an unfinished file replacement.

The launcher preference queue coalesces rapid edits and writes the latest snapshot.
Its backend save merges the current disk account default instead of copying the
UI's older default. Account changes update only account/default fields in the UI.
Instance editor patches merge into the latest document and cannot modify
backend-owned metadata or unrecognized fields.

Account removal clears the default before deleting credentials. The two files
are individually atomic rather than one multi-file commit: if the second write
fails, the account remains with no default and removal can be retried. If saving
the default fails, the account is retained.

## Atomic replacement and recovery

The shared JSON writer creates an exclusive temporary file beside the target,
writes and syncs it, closes it, then replaces the target without first deleting
or truncating it. Unix also syncs the parent directory and preserves the existing
file's permissions; new documents use mode 0600. Windows uses
[`MoveFileExW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
with replacement and write-through flags. Transient Windows access/sharing
errors get a bounded retry. Read-only and permanently locked destinations report
failure; the original remains intact and the temporary file is removed.

A process killed before replacement may leave an ignored `.industrialis-*.tmp`
file alongside the intact original. It is never treated as a new profile or
promoted automatically. Filesystem/storage durability under a physical power
failure has not been tested.

Failed preference saves retain the latest pending edits and show **Retry save**
and **Discard unsaved changes** in Settings. Discard restores the last successful
preference snapshot without writing to disk or changing account defaults. Native
close and the titlebar close button refuse to close during pending preference,
account or instance editor saves. Quit-after-game-stop uses the same guard.
This is a save guard; F27 still needs a full maintenance/log shutdown coordinator.

Startup and refresh report read failures. Healthy instances remain visible when
another instance has corrupt settings. Refresh retains previous snapshots on
read failure and rejects requests superseded by a newer refresh or profile edit.
Other maintenance/launch refresh paths and detached process recovery remain F30
and F28 work.

## Verification boundaries

`crates/backend/tests/profile_persistence.rs` creates an Electron-shaped profile
with synthetic current/legacy credentials, copies it, edits the copy, and checks
preservation of extension fields and untouched resource bytes. It also covers
concurrent preferences/default/removal/group operations, corrupt documents and
failed account-default writes. Backend tests exercise simultaneous readers,
read-only and Windows sharing failures, retry and caller cancellation. A Launcher
pointer/close test exercises failure retention, the Retry button and saved values.

Run from `apps/launcher`:

```sh
cargo test --workspace --locked --offline
cargo build -p industrialis-app --locked --offline
```

These are fixture and Windows checks. Synthetic copies are test fixtures only;
copying a real profile is not part of the product or a prerequisite for replacement.
`cargo run -p industrialis-cli -- verify-profile` reads the existing profile in place
and reports counts/read issues without writing files or printing credentials.
macOS/Linux runtime paths and permissions, existing encrypted cloud credentials
and crash/power-loss rehearsal still need verification. Cloud support must consume
the existing credential store without a user migration requirement (F16).
The mutex alone does not lock other processes; the ownership endpoint protects the
current Launcher/Electron/CLI entry points. Older-installed-version coexistence still
needs the F26/F33 cutover checks. Do not run write rehearsals against live profiles.

Cross-runtime checks use actual Rust/Node child processes and the real native app
as a secondary launcher, without creating a second window or reading user data:

```sh
cargo build -p industrialis-cli -p industrialis-app -p industrialis-backend --bin industrialis-cli --example profile_owner --example profile_ownership_smoke --locked --offline
node --test ../../scripts/profile-owner.test.mjs
```

From the repository root, run `node --test scripts/profile-owner.test.mjs`.
The separate app example links the same app source without replacing a running
development executable. Tests cover competing writers, read-only inspection,
activation handoff, simultaneous launches, graceful/forced-exit release and
unrecognized endpoints. Full physical foreground behavior is not proven by IPC tests.

## Game lifetime and shutdown

The GUI and CLI call the game broker entrypoint before profile ownership or UI
initialization. A broker owns only its UUID capture directory, never launcher
metadata. It removes its argument request before launch, redirects game output
to files, waits independently of the GUI and writes a durable exit receipt after
the post-exit command. Windows requests breakaway from inherited jobs and falls
back to the original WMI escape behavior when necessary.

Running PID entries keep Electron's pid/creation_id fields and optional native
session metadata. Windows uses Electron-compatible .NET epoch milliseconds;
macOS uses ps lstart; Linux uses boot identity and start ticks. Early Launcher Unix
second identities remain readable without rewriting records. Native capture
positions and play-time checkpoints are saved in place. Time is credited once,
up to the recorded exit time; a missing receipt for a dead restored process does
not authorize counting the entire closed-launcher interval.

Closing blocks new actions and drains admitted maintenance before signaling
game monitors to detach. Profile transactions, pending captures and console
queues flush before the runtime and exclusive owner are released. A failure
keeps the launcher open for Retry. File editors/watchers and backup work must
join this admission and drain mechanism when ported.

Synthetic lifetime checks: build the backend game_lifecycle example and app
profile_ownership_smoke example, then run
`node --test scripts/game-broker.test.mjs`. This exercises Windows
parent termination, native identity compatibility, output/exit/hook durability,
graceful close and reopening without accessing the user's profile.
