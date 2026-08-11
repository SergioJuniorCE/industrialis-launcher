<img width="1086" height="743" alt="image" src="https://github.com/user-attachments/assets/f2ee98ce-3847-4c94-978f-eb56dd53640d" />

# Industrialis

Turborepo monorepo for the Industrialis GT New Horizons launcher and website.

## Apps

| App                         | Package                   | Description                                                              |
| --------------------------- | ------------------------- | ------------------------------------------------------------------------ |
| [Launcher](apps/launcher)   | `@industrialis/launcher`  | Electron desktop app - install GTNH, manage Java, Microsoft auth, launch |
| [Website](apps/website)     | `@industrialis/website`   | Next.js marketing site                                                   |
| [Server](apps/server)       | `@industrialis/server`    | Linux CLI + Docker daemon for hosting GTNH servers                       |
| [Dashboard](apps/dashboard) | `@industrialis/dashboard` | Astro UI for managing hosted servers                                     |

### Host a GTNH server (Linux)

```bash
curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
industrialis up
# open http://127.0.0.1:3001
```

See [docs/server-hosting.md](docs/server-hosting.md) for Docker, systemd, and configuration details.

## Prerequisites

| Tool                                    | Notes                                   |
| --------------------------------------- | --------------------------------------- |
| [Node.js](https://nodejs.org/)          | LTS recommended                         |
| [pnpm](https://pnpm.io/)                | Package manager (`npm install -g pnpm`) |
| [Electron](https://www.electronjs.org/) | Desktop runtime bundled by the launcher |
| Java 17+                                | Required to run GTNH instances          |

Launcher release packaging requires Node.js 24.x, matching the GitHub Actions
release environment. Development, builds, and tests can use the current LTS.

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
pnpm dev:launcher    # Electron desktop app (Vite renderer on :5173)
pnpm dev:website     # Next.js site on :3000
pnpm dev:server      # GTNH server daemon on :4310
pnpm dev:dashboard   # Astro server console on :3001
```

Build all apps:

```bash
pnpm build
```

Launcher-only:

```bash
pnpm build:launcher
pnpm test --filter=@industrialis/launcher
pnpm --filter=@industrialis/launcher build
```

Create release artifacts for the current OS (installers + portable):

```bash
pnpm build:launcher:release
```

The artifacts are written to `artifacts/launcher`.

| Platform | Installer         | Portable            |
| -------- | ----------------- | ------------------- |
| Windows  | Squirrel (`.exe`) | ZIP of packaged app |
| macOS    | DMG               | ZIP of `.app`       |
| Linux    | DEB/RPM           | ZIP of packaged app |

Build only one format with `pnpm build:launcher:installer` or
`pnpm build:launcher:portable`. On Windows you can target the installer with
`powershell -File scripts/build-launcher.ps1 -Target installer`.

Run the exact Linux ZIP, DEB, and RPM makers locally in an ephemeral Podman or
Docker container:

```bash
pnpm test:launcher:linux
```

On Windows with Podman, start the machine once with `podman machine start`.
The script automatically uses a running Podman or Docker engine.

Launcher pull requests run the Windows, macOS, and Linux packaging jobs without
publishing. Pushes to `master` run the same jobs and publish a GitHub Release
(`launcher-v0.1.<run number>`) with all artifacts.

## Microsoft login

The launcher uses Prism Launcher's public Microsoft application ID, embedded at
build time. Microsoft login uses the device-code flow: follow the link shown in
the Accounts tab and enter the displayed code.

Use of the application ID is subject to the
[Microsoft Identity Platform terms of use](https://learn.microsoft.com/en-us/legal/microsoft-identity-platform/terms-of-use).

## Project structure

```
industrialis/
|-- apps/
|   |-- launcher/          # Electron + React desktop app
|   |-- website/           # Next.js marketing site
|   |-- server/            # industrialis CLI + Docker daemon
|   `-- dashboard/         # Astro server console
|-- packages/
|   `-- server-contracts/
|-- docs/server-hosting.md
|-- package.json
|-- pnpm-workspace.yaml
`-- turbo.json
```

## Tech stack

- **Monorepo:** pnpm workspaces, Turborepo
- **Launcher:** Electron, React 19, TypeScript, Vite, Tailwind CSS 4, Electron Forge
- **Website:** Next.js 16, React 19
- **Server:** Node, Fastify, Dockerode, Commander
- **Dashboard:** Astro 5, React islands, Tailwind CSS 4

## License

See repository license file if present.
