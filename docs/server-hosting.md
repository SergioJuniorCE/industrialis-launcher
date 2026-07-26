# GTNH server hosting

Industrialis Server is a local daemon, CLI, and web dashboard for Docker-hosted
GregTech: New Horizons servers.

## Current scope

- Pull a versioned GTNH server image.
- Create isolated Docker containers and persistent world, backup, and log directories.
- Start, stop, restart, inspect, and remove managed servers.
- View recent server logs from the CLI or dashboard.
- Retain server files when a container is removed.

The daemon uses a generated bearer token and listens on `127.0.0.1` by default.
The dashboard keeps that token server-side and proxies browser requests. Do not
expose the daemon directly to a public network. Use an SSH tunnel if the
dashboard must control a remote host.

## Prerequisites

- Node.js 22 or newer
- pnpm 9
- Docker Engine or Docker Desktop using Linux containers
- At least 6 GB of free memory for the default configuration

## Development

Install dependencies and build the shared contract once:

```bash
pnpm install
pnpm --filter @industrialis/server-contracts build
```

Start Docker, then run the daemon:

```bash
pnpm dev:server
```

In another terminal, run the dashboard at `http://localhost:3001`:

```bash
pnpm dev:dashboard
```

The dashboard server proxies to `http://127.0.0.1:4310` by default. Override it
with `INDUSTRIALIS_API_URL`. On a separate host, also set the same
`INDUSTRIALIS_API_TOKEN` for both processes.

## CLI

Build the CLI and start the daemon:

```bash
pnpm build:server
node apps/server/dist/cli.js daemon
```

Manage servers from a second terminal:

```bash
node apps/server/dist/cli.js list
node apps/server/dist/cli.js create "Assembly Line" --port 25565 --memory 6144
node apps/server/dist/cli.js start assembly-line
node apps/server/dist/cli.js logs assembly-line --tail 100
node apps/server/dist/cli.js stop assembly-line
node apps/server/dist/cli.js remove assembly-line --yes
```

Set `INDUSTRIALIS_API_URL` or pass `--api-url` before the command to target a
different daemon.

## Storage and images

Daemon state and persistent files default to `~/.industrialis/servers`:

```text
servers/
  servers.json
  assembly-line/
    world/
    backups/
    logs/
```

The complete `/app/server` directory is also retained in a named Docker volume.
This preserves configuration, player lists, ServerUtilities data, and other
mutable pack files that are not stored in the three host directories above.

The default image repository is the community-maintained
`ghcr.io/debuas/gtnhserverdocker`, using the `stable-latest` tag for new
servers. Pin a release with `--version`, or set `INDUSTRIALIS_GTNH_IMAGE` to use
a different compatible image repository. Existing servers retain their image
and version until an explicit upgrade workflow is added.

Removing a server deletes its Docker container and registry entry, but does not
delete its directory or named Docker volume. This is intentional protection
against accidental world loss.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `INDUSTRIALIS_HOST` | `127.0.0.1` | Daemon listen address |
| `INDUSTRIALIS_PORT` | `4310` | Daemon listen port |
| `INDUSTRIALIS_API_URL` | `http://127.0.0.1:4310` | CLI API target |
| `INDUSTRIALIS_SERVER_DATA` | `~/.industrialis/servers` | Persistent server root |
| `INDUSTRIALIS_GTNH_IMAGE` | `ghcr.io/debuas/gtnhserverdocker` | GTNH image repository |
| `INDUSTRIALIS_API_TOKEN` | generated in the data root | Shared daemon/dashboard token |
