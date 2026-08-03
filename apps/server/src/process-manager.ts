import { spawn, type ChildProcess } from "node:child_process";
import { openSync } from "node:fs";
import { readFile, unlink, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import type { RuntimePaths } from "./paths.js";

export type ManagedService = "daemon" | "dashboard";

export interface ServiceState {
  name: ManagedService;
  pid: number | null;
  running: boolean;
  logPath: string;
  pidPath: string;
}

function pidPath(runDir: string, name: ManagedService): string {
  return join(runDir, `${name}.pid`);
}

function logPath(runDir: string, name: ManagedService): string {
  return join(runDir, `${name}.log`);
}

export async function ensureRunDir(runDir: string): Promise<void> {
  await mkdir(runDir, { recursive: true });
}

export async function readPid(runDir: string, name: ManagedService): Promise<number | null> {
  try {
    const raw = (await readFile(pidPath(runDir, name), "utf8")).trim();
    const pid = Number(raw);
    return Number.isInteger(pid) && pid > 0 ? pid : null;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

export function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function getServiceState(runDir: string, name: ManagedService): Promise<ServiceState> {
  const pid = await readPid(runDir, name);
  const running = pid !== null && isPidAlive(pid);
  if (pid !== null && !running) {
    await clearPid(runDir, name).catch(() => undefined);
  }
  return {
    name,
    pid: running ? pid : null,
    running,
    logPath: logPath(runDir, name),
    pidPath: pidPath(runDir, name),
  };
}

async function writePid(runDir: string, name: ManagedService, pid: number): Promise<void> {
  await writeFile(pidPath(runDir, name), `${pid}\n`, { encoding: "utf8", mode: 0o600 });
}

async function clearPid(runDir: string, name: ManagedService): Promise<void> {
  try {
    await unlink(pidPath(runDir, name));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
}

async function waitFor(
  predicate: () => Promise<boolean>,
  timeoutMs: number,
  intervalMs = 250,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return predicate();
}

export async function probeUrl(url: string): Promise<boolean> {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(1500) });
    return response.ok;
  } catch {
    return false;
  }
}

async function startDetached(
  name: ManagedService,
  runDir: string,
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv,
): Promise<ChildProcess> {
  await ensureRunDir(runDir);
  // Keep the fd open for the lifetime of the detached child.
  const logFd = openSync(logPath(runDir, name), "a");
  const child = spawn(command, args, {
    detached: true,
    stdio: ["ignore", logFd, logFd],
    env,
    windowsHide: true,
  });
  child.unref();
  if (child.pid == null) throw new Error(`Failed to start ${name}`);
  await writePid(runDir, name, child.pid);
  return child;
}

export async function startDaemon(paths: RuntimePaths): Promise<ServiceState> {
  const existing = await getServiceState(paths.runDir, "daemon");
  if (existing.running && (await probeUrl(`${paths.apiUrl}/health`))) return existing;

  if (existing.running) {
    await stopService(paths.runDir, "daemon");
  }

  await startDetached("daemon", paths.runDir, process.execPath, [paths.cliEntry, "daemon"], {
    ...process.env,
    INDUSTRIALIS_HOST: process.env.INDUSTRIALIS_HOST ?? "127.0.0.1",
    INDUSTRIALIS_PORT: process.env.INDUSTRIALIS_PORT ?? "4310",
    INDUSTRIALIS_SERVER_DATA: paths.dataDir,
  });

  const ready = await waitFor(() => probeUrl(`${paths.apiUrl}/health`), 15_000);
  if (!ready) {
    throw new Error(
      `Daemon started but did not become healthy at ${paths.apiUrl}/health. Check ${logPath(paths.runDir, "daemon")}.`,
    );
  }
  return getServiceState(paths.runDir, "daemon");
}

export async function startDashboard(paths: RuntimePaths): Promise<ServiceState> {
  const existing = await getServiceState(paths.runDir, "dashboard");
  if (existing.running && (await probeUrl(paths.dashboardUrl))) return existing;

  if (existing.running) {
    await stopService(paths.runDir, "dashboard");
  }

  if (!paths.dashboardEntry) {
    throw new Error(
      "Dashboard build not found. Set INDUSTRIALIS_DASHBOARD_DIR or install a release that includes the dashboard.",
    );
  }

  await startDetached("dashboard", paths.runDir, process.execPath, [paths.dashboardEntry], {
    ...process.env,
    HOST: paths.dashboardHost,
    PORT: String(paths.dashboardPort),
    INDUSTRIALIS_API_URL: paths.apiUrl,
    INDUSTRIALIS_SERVER_DATA: paths.dataDir,
  });

  const ready = await waitFor(() => probeUrl(paths.dashboardUrl), 20_000);
  if (!ready) {
    throw new Error(
      `Dashboard started but did not become ready at ${paths.dashboardUrl}. Check ${logPath(paths.runDir, "dashboard")}.`,
    );
  }
  return getServiceState(paths.runDir, "dashboard");
}

export async function stopService(runDir: string, name: ManagedService): Promise<ServiceState> {
  const state = await getServiceState(runDir, name);
  if (!state.running || state.pid == null) {
    await clearPid(runDir, name);
    return { ...state, running: false, pid: null };
  }

  const pid = state.pid;
  try {
    process.kill(pid, "SIGTERM");
  } catch {
    await clearPid(runDir, name);
    return { ...state, running: false, pid: null };
  }

  const stopped = await waitFor(async () => !isPidAlive(pid), 10_000);
  if (!stopped) {
    try {
      process.kill(pid, "SIGKILL");
    } catch {
      // already gone
    }
  }
  await clearPid(runDir, name);
  return { name, pid: null, running: false, logPath: state.logPath, pidPath: state.pidPath };
}

export async function stopAll(runDir: string): Promise<ServiceState[]> {
  const dashboard = await stopService(runDir, "dashboard");
  const daemon = await stopService(runDir, "daemon");
  return [dashboard, daemon];
}
