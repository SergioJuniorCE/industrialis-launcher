# Industrialis Launcher (GPUI port)

Rust + [GPUI](https://gpui.rs/) rewrite of `@industrialis/launcher`
(Electron + React). Single native binary, no Chromium, no Node runtime.

## Layout

```
apps/launcher-gpui/
  crates/
    core/   # Pure logic ported from apps/launcher TS (no GPUI dep)
    app/    # GPUI window + views, depends on core
```

`core` mirrors the TS sources 1:1 in behavior, including serde shapes,
so existing user data dirs stay compatible:

| Rust module         | TS source                        |
| ------------------- | -------------------------------- |
| `launcher_window`   | `src/lib/launcher-window.ts`     |
| `launcher_settings` | `src/lib/launcher-settings.ts`   |
| `instance_settings` | `src/lib/instance-settings.ts`   |
| `launch_log`        | `src/lib/launch-log.ts`          |
| `log_buffer`        | `src/lib/log-buffer.ts`          |
| `pack_version`      | `src/lib/pack-version-status.ts` |

## GPUI pins

`industrialis-app` uses the published `gpui-pre` snapshots
(`gpui = { package = "gpui-pre", version = "=0.3.8" }`), the same pin the
`gpui-component` 0.7.x releases build against. Bump `gpui`,
`gpui_platform`, and `gpui-component` together — GPUI is pre-1.0 and
snapshots may change the API.

## Commands

```bash
pnpm --filter @industrialis/launcher-gpui dev    # run GPUI window
pnpm --filter @industrialis/launcher-gpui build
pnpm --filter @industrialis/launcher-gpui test
```

Or with cargo directly from this directory:

```bash
cargo run -p industrialis-app
cargo test --workspace
```
