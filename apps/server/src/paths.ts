import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export interface RuntimePaths {
  stateRoot: string;
  runDir: string;
  dataDir: string;
  packageRoot: string;
  cliEntry: string;
  dashboardDir: string | null;
  dashboardEntry: string | null;
  dashboardUrl: string;
  dashboardHost: string;
  dashboardPort: number;
  apiUrl: string;
}

function firstExisting(candidates: string[]): string | null {
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

export function getRuntimePaths(overrides: Partial<RuntimePaths> = {}): RuntimePaths {
  const stateRoot = resolve(
    overrides.stateRoot ?? process.env.INDUSTRIALIS_STATE_DIR ?? join(homedir(), ".industrialis"),
  );
  const runDir = resolve(overrides.runDir ?? join(stateRoot, "run"));
  const dataDir = resolve(
    overrides.dataDir ?? process.env.INDUSTRIALIS_SERVER_DATA ?? join(stateRoot, "servers"),
  );

  const thisFile = fileURLToPath(import.meta.url);
  const distDir = dirname(thisFile);
  const packageRoot = resolve(overrides.packageRoot ?? join(distDir, ".."));
  const cliEntry = resolve(overrides.cliEntry ?? join(distDir, "cli.js"));

  const dashboardHost = process.env.INDUSTRIALIS_DASHBOARD_HOST ?? "127.0.0.1";
  const dashboardPort = Number(process.env.INDUSTRIALIS_DASHBOARD_PORT ?? 3001);
  const dashboardUrl =
    overrides.dashboardUrl ??
    process.env.INDUSTRIALIS_DASHBOARD_URL ??
    `http://${dashboardHost}:${dashboardPort}`;

  const dashboardDir =
    overrides.dashboardDir ??
    firstExisting([
      process.env.INDUSTRIALIS_DASHBOARD_DIR ?? "",
      join(packageRoot, "dashboard"),
      join(packageRoot, "..", "dashboard"),
    ].filter(Boolean));

  const dashboardEntry =
    overrides.dashboardEntry ??
    (dashboardDir
      ? firstExisting([
          join(dashboardDir, "dist", "server", "entry.mjs"),
          join(dashboardDir, "server", "entry.mjs"),
          join(dashboardDir, "entry.mjs"),
        ])
      : null);

  const apiHost = process.env.INDUSTRIALIS_HOST ?? "127.0.0.1";
  const apiPort = Number(process.env.INDUSTRIALIS_PORT ?? 4310);
  const apiUrl =
    overrides.apiUrl ?? process.env.INDUSTRIALIS_API_URL ?? `http://${apiHost}:${apiPort}`;

  return {
    stateRoot,
    runDir,
    dataDir,
    packageRoot,
    cliEntry,
    dashboardDir,
    dashboardEntry,
    dashboardUrl,
    dashboardHost,
    dashboardPort,
    apiUrl,
  };
}
