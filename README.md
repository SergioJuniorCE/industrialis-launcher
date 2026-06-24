# Industrialis Launcher

A desktop launcher for [GT New Horizons](https://gtnewhorizons.com/) built with Tauri, React, and Rust. Install modpack instances, manage Java and memory settings, sign in with Microsoft, and launch the game from one app.

## Features

- **Instance management** — browse GTNH versions (stable and beta), install from official downloads, rename instances, and delete them
- **Java detection** — finds Java on `PATH`, `JAVA_HOME`, and common install locations; per-instance override
- **Launch console** — live stdout/stderr from the game process, persisted per instance
- **Microsoft authentication** — OAuth (browser) and device code run in parallel; tokens refresh automatically
- **Theming** — dark/light mode with customizable colors in Settings

## Prerequisites

| Tool | Notes |
|------|-------|
| [Node.js](https://nodejs.org/) | LTS recommended |
| [pnpm](https://pnpm.io/) | Package manager (`npm install -g pnpm`) |
| [Rust](https://rustup.rs/) | Stable toolchain |
| [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) | Platform-specific build deps (WebView2 on Windows) |
| Java 17+ | Required to run GTNH instances (Java 8 option available for older packs) |

## Development

```bash
pnpm install
pnpm tauri dev
```

This starts the Vite dev server and the Tauri app. Frontend changes hot-reload; Rust changes trigger a cargo rebuild and app restart.

Other scripts:

```bash
pnpm test          # Vitest (frontend)
pnpm build         # Production frontend build
pnpm tauri build   # Release installer
```

Rust tests:

```bash
cd src-tauri
cargo test
```

## Microsoft client ID (maintainers only)

End users do **not** configure a client ID. It is embedded at build time so distributed builds work out of the box.

Before building or running dev with Microsoft login, set the Industrialis Azure Application (client) ID using one of:

**Option A — local file (recommended for dev)**

```bash
cp src-tauri/microsoft-client-id.example src-tauri/microsoft-client-id
# Edit microsoft-client-id and paste your client ID on its own line
```

`microsoft-client-id` is gitignored. If `pnpm tauri dev` is already running, saving the file triggers a rebuild automatically.

**Option B — environment variable (CI / release builds)**

```powershell
# PowerShell
$env:MICROSOFT_CLIENT_ID="your-azure-client-id"
pnpm tauri build
```

```bash
# bash
MICROSOFT_CLIENT_ID=your-azure-client-id pnpm tauri build
```

The env var must be set **before** starting dev if you use that method; changing it mid-session requires restarting `pnpm tauri dev`.

### Azure app registration

Create a single app registration for Industrialis (one-time setup):

1. [Azure portal → App registrations](https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade)
2. New registration — supported account type: **Personal Microsoft accounts only**
3. Authentication → add redirect URI `industrialislauncher://oauth/microsoft` (platform: **Mobile and desktop applications**)
4. Authentication → enable **Allow public client flows** → Yes (needed for device code fallback)
5. Copy the Application (client) ID into `microsoft-client-id` or `MICROSOFT_CLIENT_ID`

No API permissions are required; scopes `XboxLive.SignIn` and `XboxLive.offline_access` are requested at login.

## User data

On Windows, data lives under `%APPDATA%\industrialis-launcher\`:

| Path | Contents |
|------|----------|
| `instances/<version>/` | Installed modpack files, `instance.json`, `console.log` |
| `accounts.json` | Linked Microsoft accounts and tokens |
| `launcher-settings.json` | Theme preferences |

## Usage

1. **Add Instance** — pick a GTNH version and Java variant (17+ or 8)
2. **Accounts** — add a Microsoft account (browser opens; device code appears as fallback at [microsoft.com/link](https://microsoft.com/link))
3. **Instance Settings** — RAM, JVM args, Java path, auth mode (`offline` or `microsoft`)
4. **Play** — launches the instance; open **Console** to view logs

Offline auth still requires at least one valid Microsoft account with Minecraft on the machine (used for entitlement checks).

## Project structure

```
industrialis-launcher/
├── src/                    # React frontend
│   ├── App.tsx             # Main UI (instances, settings, accounts)
│   ├── components/         # Theme editor, UI primitives
│   └── context/            # Launcher settings provider
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs          # Instances, download, launch, Java detection
│   │   └── auth.rs         # Microsoft / Xbox / Minecraft auth pipeline
│   ├── build.rs            # Embeds MICROSOFT_CLIENT_ID at compile time
│   └── tauri.conf.json
└── package.json
```

## Tech stack

- **Frontend:** React 19, TypeScript, Vite, Tailwind CSS 4
- **Backend:** Rust, Tauri 2, reqwest, tokio
- **Pack source:** [GTNewHorizons versions.json](https://github.com/GTNewHorizons/GTNewHorizons.github.io)

## IDE setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## License

See repository license file if present.