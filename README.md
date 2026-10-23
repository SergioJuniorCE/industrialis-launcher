<img width="1602" height="901" alt="image" src="https://github.com/user-attachments/assets/ffbaadd1-0724-471e-9c13-cc6333adad12" />

# Industrialis

Turborepo monorepo for the Industrialis GT New Horizons launcher and website.

## Apps

| App                         | Package                   | Description                                        |
| --------------------------- | ------------------------- | -------------------------------------------------- |
| [Launcher](apps/launcher)   | `@industrialis/launcher`  | Native Rust/GPUI desktop launcher                  |
| [Website](apps/website)     | `@industrialis/website`   | Next.js marketing site                             |
| [Server](apps/server)       | `@industrialis/server`    | Linux CLI + Docker daemon for hosting GTNH servers |
| [Dashboard](apps/dashboard) | `@industrialis/dashboard` | Astro UI for managing hosted servers               |

### Host a GTNH server (Linux)

```bash
curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
industrialis up
# open http://127.0.0.1:3001
```

See [docs/server-hosting.md](docs/server-hosting.md) for Docker, systemd, and configuration details.

## Prerequisites

| Tool                           | Notes                                               |
| ------------------------------ | --------------------------------------------------- |
| [Node.js](https://nodejs.org/) | LTS recommended                                     |
| [pnpm](https://pnpm.io/)       | Package manager (`npm install -g pnpm`)             |
| Rust 1.96+                     | Native launcher compiler; pinned Cargo dependencies |
| Java 17+                       | Required to run GTNH instances                      |

Launcher release packaging requires Node.js 24.x, matching the GitHub Actions
release environment. Development, builds, and tests can use the current LTS.
GPUI packaging needs NSIS 3.11 on Windows and native platform build libraries on
Linux/macOS; see [GPUI updates and packaging](apps/launcher/UPDATES.md).

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
pnpm dev:launcher    # Native GPUI desktop app
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

Artifacts are written to `apps/launcher/dist`. The native launcher reads
the existing profile in place; users need no data migration.

| Platform | Installer       | Portable                  |
| -------- | --------------- | ------------------------- |
| Windows  | NSIS (`.exe`)   | Native executable ZIP     |
| macOS    | DMG with `.app` | Manual update via DMG     |
| Linux    | tar.gz archive  | Native executable archive |

Build only one format with `pnpm build:launcher:installer` or
`pnpm build:launcher:portable`. On macOS/Linux the packager produces the platform's
single native distribution format.

Stable `launcher-v*` tags build the native launcher on Windows x64, macOS arm64
and Linux x64 and publish the production release. `launcher-preview-v*` tags remain
optional prereleases; ordinary master pushes do not publish.
See [release notes and known limitations](apps/launcher/RELEASE_NOTES.md) and
[cutover and rollback](apps/launcher/CUTOVER.md).

## Microsoft login

The launcher uses Prism Launcher's public Microsoft application ID, embedded at
build time. Microsoft login uses the device-code flow: follow the link shown in
the Accounts tab and enter the displayed code.

Use of the application ID is subject to the
[Microsoft Identity Platform terms of use](https://learn.microsoft.com/en-us/legal/microsoft-identity-platform/terms-of-use).

## Cloud backups

The launcher can automatically upload completed files from each enabled
instance's `.minecraft/backups` folder. Google Drive is available first;
S3-compatible storage, OneDrive, FTP/FTPS, and SFTP are planned.

- [Connect Google Drive and troubleshoot cloud backups](docs/cloud-backups.md)
- [Public cloud-backup guide](https://industrialislauncher.yoggan.dev/docs/cloud-backups)

Provider configuration lives in **Launcher Settings > Backups**. Enable backups
for an individual pack under **Instance Settings > General > Cloud backups**.

## Project structure

```
industrialis/
|-- apps/
|   |-- launcher/          # Rust + GPUI desktop app
|   |-- website/           # Next.js marketing site
|   |-- server/            # industrialis CLI + Docker daemon
|   `-- dashboard/         # Astro server console
|-- packages/
|   `-- server-contracts/
|-- docs/
|   |-- cloud-backups.md
|   `-- server-hosting.md
|-- package.json
|-- pnpm-workspace.yaml
`-- turbo.json
```

## Tech stack

- **Monorepo:** pnpm workspaces, Turborepo
- **Launcher:** Rust, GPUI, Tokio, native gpui-base editor and Syntect
- **Website:** Next.js 16, React 19
- **Server:** Node, Fastify, Dockerode, Commander
- **Dashboard:** Astro 5, React islands, Tailwind CSS 4

## License

See repository license file if present.
