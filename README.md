# Industrialis

Turborepo monorepo for the Industrialis GT New Horizons launcher and website.

## Apps

| App | Package | Description |
|-----|---------|-------------|
| [Launcher](apps/launcher) | `@industrialis/launcher` | Tauri desktop app — install GTNH, manage Java, Microsoft auth, launch |
| [Website](apps/website) | `@industrialis/website` | Next.js marketing site |

## Prerequisites

| Tool | Notes |
|------|-------|
| [Node.js](https://nodejs.org/) | LTS recommended |
| [pnpm](https://pnpm.io/) | Package manager (`npm install -g pnpm`) |
| [Rust](https://rustup.rs/) | Stable toolchain (launcher only) |
| [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) | WebView2 on Windows |
| Java 17+ | Required to run GTNH instances |

## Development

```bash
pnpm install
```

Run everything (launcher + website dev servers):

```bash
pnpm dev
```

Or run a single app:

```bash
pnpm dev:launcher    # Tauri desktop app (Vite on :1420)
pnpm dev:website     # Next.js site on :3000
```

Build all apps:

```bash
pnpm build
```

Launcher-only:

```bash
pnpm build:launcher
pnpm test --filter=@industrialis/launcher
cd apps/launcher/src-tauri && cargo test
```

Create release artifacts for the current OS (installers + portable):

```bash
pnpm build:launcher:release
```

The artifacts are written to `artifacts/launcher`.

| Platform | Installer | Portable |
|----------|-----------|----------|
| Windows | NSIS (`.exe`), MSI (`.msi`) | ZIP of `.exe` |
| macOS | DMG | ZIP of `.app` |
| Linux | DEB | AppImage + ZIP of binary |

Build only one format with `pnpm build:launcher:installer` or
`pnpm build:launcher:portable`. On Windows you can still target a single
bundle with `powershell -File scripts/build-launcher.ps1 -Target msi`.

Pushes to `master` build Windows, macOS, and Linux in GitHub Actions and
publish a GitHub Release (`build-<run number>`) with all artifacts.

## Microsoft login

The launcher uses Prism Launcher's public Microsoft application ID, embedded at
build time. Microsoft login uses the device-code flow: follow the link shown in
the Accounts tab and enter the displayed code.

Use of the application ID is subject to the
[Microsoft Identity Platform terms of use](https://learn.microsoft.com/en-us/legal/microsoft-identity-platform/terms-of-use).

## Project structure

```
industrialis/
├── apps/
│   ├── launcher/          # Tauri + React desktop app
│   │   ├── src/
│   │   └── src-tauri/
│   └── website/           # Next.js marketing site
│       └── app/
├── package.json           # workspace root
├── pnpm-workspace.yaml
└── turbo.json
```

## Tech stack

- **Monorepo:** pnpm workspaces, Turborepo
- **Launcher:** React 19, Vite, Tailwind CSS 4, Tauri 2, Rust
- **Website:** Next.js 16, React 19

## License

See repository license file if present.
