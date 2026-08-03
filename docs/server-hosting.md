# GTNH server hosting

Industrialis is a Linux CLI, daemon, and web dashboard for hosting
GregTech: New Horizons servers on a dedicated server or VPS. It talks directly
to Docker Engine through the host socket; Docker Desktop is not used.

## Quick install

On a Linux host with Docker Engine and Node.js 22+:

```bash
curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
```

To inspect the script first:

```bash
curl -fsSL https://industrialislauncher.yoggan.dev/install.sh -o install.sh
less install.sh
bash install.sh
```

Then:

```bash
industrialis up        # start daemon + dashboard
industrialis status
# open http://127.0.0.1:3001
industrialis down
```

The installer downloads the latest `industrialis-server-linux-x64.tar.gz` release
asset, installs under `~/.local/share/industrialis`, and links
`~/.local/bin/industrialis`. Override with `INDUSTRIALIS_INSTALL_DIR`,
`INDUSTRIALIS_BIN_DIR`, `INDUSTRIALIS_VERSION`, or `INDUSTRIALIS_RELEASE_BASE`.

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
- Docker Engine with access to `/var/run/docker.sock`
- At least 6 GB of free memory for the default configuration

Install Docker Engine using Docker's official repository instructions for your
distribution. The user running Industrialis must belong to the `docker` group,
or have equivalent access to the configured socket.

## CLI

| Command | Description |
| --- | --- |
| `industrialis` | Show help |
| `industrialis up` | Start daemon + dashboard (background) |
| `industrialis status` | Process health + server summary |
| `industrialis down` | Stop daemon + dashboard |
| `industrialis list` | List managed servers |
| `industrialis create <name>` | Create a GTNH server (`--port`, `--memory`, `--version`) |
| `industrialis start\|stop\|restart <id>` | Control a server |
| `industrialis remove <id> --yes` | Remove container (world retained) |
| `industrialis logs <id>` | Recent container logs |
| `industrialis daemon` | Run API only in the foreground (systemd) |

Set `INDUSTRIALIS_API_URL` or pass `--api-url` before the command to target a
different daemon.

## Development

Install dependencies and build shared contracts:

```bash
pnpm install
pnpm --filter @industrialis/server-contracts build
```

Start Docker Engine, then run the daemon:

```bash
pnpm dev:server
```

In another terminal, run the Astro dashboard at `http://127.0.0.1:3001`:

```bash
pnpm dev:dashboard
```

Or use the lifecycle CLI from a built package:

```bash
pnpm build:server
pnpm build:dashboard
node apps/server/dist/cli.js up
```

The dashboard server proxies to `http://127.0.0.1:4310` by default. Override it
with `INDUSTRIALIS_API_URL`. On a separate host, also set the same
`INDUSTRIALIS_API_TOKEN` for both processes.

### Release tarball

```bash
pnpm pack:server
# → artifacts/server/industrialis-server-linux-x64.tar.gz
```

Tag `server-v*` to publish the asset that `install.sh` downloads. Running the
`server-release` workflow manually is useful for producing a test artifact,
but does not create a GitHub Release unless it runs for a matching tag.

## VPS deployment (systemd)

For always-on hosts, prefer systemd over `industrialis up`.

### Option A — installed release

```bash
curl -fsSL https://industrialislauncher.yoggan.dev/install.sh | bash
sudo useradd --system --create-home --home-dir /var/lib/industrialis industrialis
sudo usermod --append --groups docker industrialis
sudo install -d -o industrialis -g industrialis /var/lib/industrialis/servers
sudo install -d /etc/industrialis
sudo install -m 640 -o root -g industrialis deploy/systemd/industrialis.env.example /etc/industrialis/industrialis.env
```

Point the unit `ExecStart` at the installed binary (default
`/home/<user>/.local/bin/industrialis` or a system path you choose), and set
`INDUSTRIALIS_SERVER_DATA=/var/lib/industrialis/servers`.

### Option B — monorepo checkout

Build the repository on the VPS under `/opt/industrialis`:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm build:server
pnpm build:dashboard
```

```bash
sudo useradd --system --create-home --home-dir /var/lib/industrialis industrialis
sudo usermod --append --groups docker industrialis
sudo install -d -o industrialis -g industrialis /var/lib/industrialis/servers
sudo install -d /etc/industrialis
sudo install -m 640 -o root -g industrialis deploy/systemd/industrialis.env.example /etc/industrialis/industrialis.env
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

Daemon state and persistent files default to `~/.industrialis/servers`. Process
logs and PID files live under `~/.industrialis/run`. The provided VPS
environment changes data to `/var/lib/industrialis/servers`:

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
| `INDUSTRIALIS_API_URL` | `http://127.0.0.1:4310` | CLI / dashboard API target |
| `INDUSTRIALIS_SERVER_DATA` | `~/.industrialis/servers` | Persistent server root |
| `INDUSTRIALIS_STATE_DIR` | `~/.industrialis` | PID/log state root |
| `INDUSTRIALIS_DASHBOARD_DIR` | install `dashboard/` | Built Astro dashboard directory |
| `INDUSTRIALIS_DASHBOARD_HOST` | `127.0.0.1` | Dashboard bind host |
| `INDUSTRIALIS_DASHBOARD_PORT` | `3001` | Dashboard bind port |
| `INDUSTRIALIS_GTNH_IMAGE` | `ghcr.io/debuas/gtnhserverdocker` | GTNH image repository |
| `INDUSTRIALIS_DOCKER_SOCKET` | `/var/run/docker.sock` | Docker Engine socket |
| `INDUSTRIALIS_API_TOKEN` | generated in the data root | Shared daemon/dashboard token |
