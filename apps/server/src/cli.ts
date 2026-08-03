#!/usr/bin/env node
import { Command } from "commander";
import type { GtnhServer } from "@industrialis/server-contracts";
import { startApi } from "./api.js";
import { resolveApiToken } from "./auth.js";
import { ServerApiClient } from "./client.js";
import { getConfig } from "./config.js";
import { getRuntimePaths } from "./paths.js";
import {
  ensureRunDir,
  getServiceState,
  probeUrl,
  startDaemon,
  startDashboard,
  stopAll,
} from "./process-manager.js";

function printServers(servers: GtnhServer[]): void {
  if (servers.length === 0) {
    console.log("No GTNH servers configured.");
    return;
  }
  console.table(
    servers.map(({ id, name, version, status, port, memoryMb }) => ({
      id,
      name,
      version,
      status,
      address: `localhost:${port}`,
      memory: `${memoryMb} MB`,
    })),
  );
}

async function client(apiUrl: string): Promise<ServerApiClient> {
  const config = getConfig({ apiUrl });
  return new ServerApiClient(apiUrl, await resolveApiToken(config));
}

const program = new Command()
  .name("industrialis")
  .description("Host and manage Docker-backed GregTech: New Horizons servers")
  .version("0.1.0")
  .option(
    "--api-url <url>",
    "daemon API URL",
    process.env.INDUSTRIALIS_API_URL ?? "http://127.0.0.1:4310",
  );

program
  .command("up")
  .description("start the daemon and local dashboard")
  .action(async () => {
    const paths = getRuntimePaths({ apiUrl: program.opts().apiUrl });
    await ensureRunDir(paths.runDir);
    const config = getConfig({
      apiUrl: paths.apiUrl,
      dataDir: paths.dataDir,
    });
    await resolveApiToken(config);

    console.log("Starting Industrialis daemon…");
    const daemon = await startDaemon(paths);
    console.log(`  daemon  pid ${daemon.pid}  ${paths.apiUrl}`);

    console.log("Starting dashboard…");
    const dashboard = await startDashboard(paths);
    console.log(`  dashboard  pid ${dashboard.pid}  ${paths.dashboardUrl}`);
    console.log("");
    console.log(`Dashboard ready: ${paths.dashboardUrl}`);
    console.log("Create GTNH servers from the UI, or use: industrialis create <name>");
  });

program
  .command("down")
  .description("stop the daemon and dashboard")
  .action(async () => {
    const paths = getRuntimePaths({ apiUrl: program.opts().apiUrl });
    const results = await stopAll(paths.runDir);
    for (const service of results) {
      console.log(`${service.name}: stopped`);
    }
  });

program
  .command("status")
  .description("show process health and managed servers")
  .action(async () => {
    const paths = getRuntimePaths({ apiUrl: program.opts().apiUrl });
    const daemon = await getServiceState(paths.runDir, "daemon");
    const dashboard = await getServiceState(paths.runDir, "dashboard");
    const apiHealthy = await probeUrl(`${paths.apiUrl}/health`);
    const dashboardHealthy = await probeUrl(paths.dashboardUrl);

    console.log("Processes");
    console.log(
      `  daemon     ${daemon.running ? `running (pid ${daemon.pid})` : "stopped"}${apiHealthy ? " · healthy" : daemon.running ? " · not healthy" : ""}`,
    );
    console.log(
      `  dashboard  ${dashboard.running ? `running (pid ${dashboard.pid})` : "stopped"}${dashboardHealthy ? " · ready" : dashboard.running ? " · not ready" : ""}`,
    );
    console.log(`  api        ${paths.apiUrl}`);
    console.log(`  ui         ${paths.dashboardUrl}`);
    console.log(`  data       ${paths.dataDir}`);
    if (daemon.running) console.log(`  daemon log ${daemon.logPath}`);
    if (dashboard.running) console.log(`  ui log     ${dashboard.logPath}`);
    console.log("");

    if (!apiHealthy) {
      console.log("Daemon API is not reachable. Run: industrialis up");
      return;
    }

    console.log("Servers");
    printServers(await (await client(paths.apiUrl)).list());
  });

program
  .command("daemon")
  .description("run the API daemon in the foreground (for systemd)")
  .option("--host <host>", "listen host", process.env.INDUSTRIALIS_HOST ?? "127.0.0.1")
  .option("--port <port>", "listen port", process.env.INDUSTRIALIS_PORT ?? "4310")
  .action(async ({ host, port }: { host: string; port: string }) => {
    await startApi(getConfig({ host, port: Number(port) }));
  });

program.command("list").description("list managed servers").action(async () => {
  printServers(await (await client(program.opts().apiUrl)).list());
});

program
  .command("create <name>")
  .description("create a GTNH server container")
  .option("--version <version>", "GTNH image tag", "stable-latest")
  .option("--port <port>", "host game port", "25565")
  .option("--memory <megabytes>", "container memory limit in MB", "6144")
  .action(async (name: string, options: { version: string; port: string; memory: string }) => {
    const server = await (
      await client(program.opts().apiUrl)
    ).create({
      name,
      version: options.version,
      port: Number(options.port),
      memoryMb: Number(options.memory),
    });
    console.log(`Created ${server.name} (${server.id}) on port ${server.port}.`);
  });

for (const action of ["start", "stop", "restart"] as const) {
  program
    .command(`${action} <id>`)
    .description(`${action} a server`)
    .action(async (id: string) => {
      const server = await (await client(program.opts().apiUrl)).action(id, action);
      console.log(`${server.name}: ${server.status}`);
    });
}

program
  .command("remove <id>")
  .description("remove a server container (world data is retained)")
  .option("--yes", "skip confirmation requirement")
  .action(async (id: string, options: { yes?: boolean }) => {
    if (!options.yes) throw new Error("Pass --yes to confirm removal");
    await (await client(program.opts().apiUrl)).remove(id);
    console.log(`Removed ${id}. Persistent files were retained.`);
  });

program
  .command("logs <id>")
  .description("show recent server logs")
  .option("--tail <lines>", "number of lines", "200")
  .action(async (id: string, options: { tail: string }) => {
    const logs = await (await client(program.opts().apiUrl)).logs(id, Number(options.tail));
    process.stdout.write(logs.lines);
  });

program.parseAsync().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
