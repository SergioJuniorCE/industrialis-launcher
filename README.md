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

Create Windows release artifacts (NSIS installer, MSI installer, and portable ZIP):

```powershell
$env:MICROSOFT_CLIENT_ID="your-azure-client-id"
pnpm build:launcher:release
```

The artifacts are written to `artifacts/launcher`. You can build only one format with
`pnpm build:launcher:installer`, `pnpm build:launcher:portable`, or
`powershell -File scripts/build-launcher.ps1 -Target msi`.

Pushes to `master` run the same release build in GitHub Actions and retain the
artifacts for 14 days. Add `MICROSOFT_CLIENT_ID` as a repository Actions secret
before running the workflow.

## Microsoft client ID (maintainers only)

End users do **not** configure a client ID. It is embedded at build time.

**Option A — local file (recommended for dev)**

```bash
cp apps/launcher/src-tauri/microsoft-client-id.example apps/launcher/src-tauri/microsoft-client-id
# Paste your Azure Application (client) ID on its own line
```

**Option B — environment variable (CI / release)**

```powershell
$env:MICROSOFT_CLIENT_ID="your-azure-client-id"
pnpm build:launcher
cd apps/launcher && pnpm tauri build
```

### Azure app registration

1. [Azure portal → App registrations](https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade)
2. New registration — **Personal Microsoft accounts only**
3. Authentication → **Mobile and desktop applications** → redirect URI `industrialislauncher://oauth/microsoft`
4. **Allow public client flows** → Yes
5. Submit the client ID for Mojang API access: [https://aka.ms/mce-reviewappid](https://aka.ms/mce-reviewappid)

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
