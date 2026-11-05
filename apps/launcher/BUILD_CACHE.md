# Shared Cargo build cache (mbx)

Measured 2026-11-05 on Windows, 12 logical processors, rustc/cargo 1.99.1,
mbx 1.29.2 (`x86_64-pc-windows-msvc`). This records what
[mr-boxington](https://mr-boxington.jdx.dev) does for this workspace and the
configuration now in place.

## Results

| Scenario                                               | Wall time | Cache                 |
| ------------------------------------------------------ | --------: | --------------------- |
| Cold build, empty target and empty store (`mbx build`) |   397.4 s | 0 hits / 539 compiled |
| Same checkout, nothing changed                         |     2.0 s | —                     |
| New worktree, warm store                               |   168.4 s | 524 hits / 14 misses  |
| Empty target, warm store (`mbx clean` then build)      |   123.6 s | 540 hits / 0 misses   |
| App edit, default policy (learned incremental)         |   20–45 s | —                     |
| App edit, `MBX_INCREMENTAL=1`                          |    8–18 s | —                     |
| App edit, plain `cargo build` (today's loop)           |    15.2 s | —                     |

Plain `cargo` reproduced the 13.7 s edit figure from `DEV_BUILDS.md`, so the
plain measurements are the fair baseline on this machine.

Conclusions:

- Cross-worktree reuse is real: a new worktree recompiles 14 crates instead of
  539, but it still takes 168 s because the app crate, its dependents and the
  link are compiled there. mbx is not a 1.3 s restore on NTFS.
- An empty target in an unchanged checkout restores all 540 compilations and
  still takes 124 s: on NTFS every restored output is a copy, so the store is
  read and written back in full.
- The default incremental policy is slower than Cargo incremental for edits to
  this app crate. `MBX_INCREMENTAL=1` (Cargo keeps the `profile.dev` incremental
  state for `industrialis-app`) is within noise of plain Cargo and keeps the
  shared cache for dependencies. Use it for anything interactive.
- `mbx clean` fails with `Access is denied` while a launcher runs from the
  target, and the renamed `*.removing-*` directory stays until that process
  exits. A later `mbx gc` retries and removes it, so this recovers by itself.

## Disk

Plain Cargo leaves one full `target/` per worktree. Measured before adoption:

| Location                                                |     Size |
| ------------------------------------------------------- | -------: |
| `%LOCALAPPDATA%\mbx\actions` (store, one workspace)     |  2.2 GiB |
| Nine existing worktree `target/` directories, adoptable | 76.0 GiB |

The store does not deduplicate restored outputs on NTFS, so a new worktree costs
its own target bytes; the saving comes from enforcing a budget and collecting
dead checkouts, which plain Cargo never does (`DEV_BUILDS.md`: "no automatic hard
size cap").

## Configuration in place

- `apps/launcher/.mbx.toml` (checked in): `incremental = true`. Workspace policy
  allows only this subset of settings; the budgets below are machine-level.
- `%APPDATA%\mbx\config.toml`: `gc.max_total_size = "40GiB"`, and
  `target.evict_first = [".t3/worktrees", ".codex/worktrees"]` so agent
  checkouts are collected before the main checkout.
- `mbx.exe` installed at `%LOCALAPPDATA%\Programs\mbx`, appended to the user
  `PATH`. No `mbx setup`: plain `cargo` is untouched.
- All nine worktree targets adopted (symlink at `target`, contents preserved).
  A first `mbx gc` removed three targets (49.6 GiB logical, 48.9 GiB released)
  and free space went from 63.7 GiB to 111.7 GiB. The cache now holds 34.6 GiB
  against its 40 GiB budget: 2.3 GiB store, 32.2 GiB of targets.

Verified after adoption: new worktree through `mbx build` 144 s / 524 hits,
app edit 14.8 s (same as plain Cargo), plain `cargo build` still works through
the adopted symlink, and `scripts/dev-launcher.mjs` keeps parsing Cargo's JSON
unchanged (492 `compiler-artifact` messages, no non-JSON lines).

## Operations

```powershell
mbx build -p industrialis-app --locked -j 4   # new/agent worktrees instead of cargo build
mbx cache stats                               # store, targets, incremental, budget use
mbx gc --dry-run                              # what the next collection would remove
mbx adopt --recursive --dry-run <root>        # before adopting new worktrees
```

Close launchers before `mbx clean`: a running executable blocks removal on
Windows, and the renamed `*.removing-*` directory stays until the process exits
(a later `mbx gc` collects it).

Rollback: delete `.mbx.toml`, remove the `%APPDATA%\mbx\config.toml` entries,
`mbx cache remove <workspace>` per checkout, replace each `target` symlink with
its managed directory if the contents are wanted, then remove
`%LOCALAPPDATA%\mbx` and the `PATH` entry. Cargo needs no changes either way.
