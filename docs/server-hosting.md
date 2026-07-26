# GTNH server hosting

Industrialis Server is a Linux daemon, CLI, and web dashboard for hosting
GregTech: New Horizons servers on a dedicated server or VPS. It talks directly
to Docker Engine through the host socket; Docker Desktop is not used.

## Current scope

- Pull a versioned GTNH server image.
- Create isolated Docker containers and persistent world, backup, and log directories.
- Start, stop, restart, inspect, and remove managed servers.
- View recent server logs from the CLI or dashboard.
- Retain server files when a container is removed.

The daemon uses a generated bearer token and listens on `127.0.0.1` by default.
The dashboard keeps that token server-side and proxies browser requests. Neither
port `4310` nor port `3001` should be publicly exposed. Put the dashboard behind
an authenticated reverse proxy or a private VPN.

## Prerequisites

- A Linux server or VPS, with Ubuntu or Debian recommended
- Node.js 22 or newer
- pnpm 9
- Docker Engine with access to `/var/run/docker.sock`
- At least 6 GB of free memory for the default configuration

Install Docker Engine using Docker's official repository instructions for your
distribution. The user running `industrialis-server` must belong to the
`docker` group, or have equivalent access to the configured socket.

## Development

Install dependencies and build the shared contract once:

```bash
pnpm install
pnpm --filter @industrialis/server-contracts build
```

Start Docker Engine, then run the daemon:

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

## VPS deployment

Build the repository on the VPS under `/opt/industrialis`:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm build:server
pnpm build:dashboard
```

Create a dedicated service account and persistent directories:

```bash
sudo useradd --system --create-home --home-dir /var/lib/industrialis industrialis
sudo usermod --append --groups docker industrialis
sudo install -d -o industrialis -g industrialis /var/lib/industrialis/servers
sudo install -d /etc/industrialis
sudo install -m 640 -o root -g industrialis deploy/systemd/industrialis.env.example /etc/industrialis/industrialis.env
```

Ensure the service account can read the built repository, then install and start
the systemd units:

```bash
sudo install -m 644 deploy/systemd/industrialis-server.service /etc/systemd/system/
sudo install -m 644 deploy/systemd/industrialis-dashboard.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now industrialis-server industrialis-dashboard
```

Inspect service output with:

```bash
sudo journalctl -u industrialis-server -f
sudo journalctl -u industrialis-dashboard -f
```

The supplied units expect the repository at `/opt/industrialis`, store state in
`/var/lib/industrialis`, and run both HTTP services on loopback. Edit the units
if your deployment path differs.

### Public access

Do not open daemon port `4310`. Do not expose dashboard port `3001` directly.
Use `deploy/Caddyfile.example` as an authenticated HTTPS reverse proxy, or
access the dashboard through WireGuard, Tailscale, or an SSH tunnel. Generate a
Caddy password hash before enabling the example:

```bash
caddy hash-password
```

Only expose the Minecraft ports assigned to servers, plus the reverse proxy's
HTTP/HTTPS ports if applicable.

## Storage and images

Daemon state and persistent files default to `~/.industrialis/servers`. The
provided VPS environment changes this to `/var/lib/industrialis/servers`:

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
| `INDUSTRIALIS_DOCKER_SOCKET` | `/var/run/docker.sock` | Docker Engine socket |
| `INDUSTRIALIS_API_TOKEN` | generated in the data root | Shared daemon/dashboard token |
