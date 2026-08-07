import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile, spawn, type ChildProcess } from "node:child_process";

export interface RunningProcess {
  pid: number;
  child?: ChildProcess;
}

export type EmitProcessLog = (stream: string, line: string) => void;

function quoteWindowsArgument(argument: string): string {
  if (argument && !/[\s"]/u.test(argument)) return argument;
  let result = '"';
  let slashes = 0;
  for (const char of argument) {
    if (char === "\\") { slashes += 1; continue; }
    if (char === '"') { result += "\\".repeat(slashes * 2 + 1) + '"'; slashes = 0; continue; }
    result += "\\".repeat(slashes) + char;
    slashes = 0;
  }
  return result + "\\".repeat(slashes * 2) + '"';
}

async function spawnWindowsViaWmi(executable: string, args: string[], cwd: string, environment: Record<string, string>): Promise<number> {
  const merged = new Map<string, string>();
  for (const [key, value] of Object.entries(process.env)) if (value !== undefined) merged.set(key.toLowerCase(), `${key}=${value}`);
  for (const [key, value] of Object.entries(environment)) merged.set(key.toLowerCase(), `${key}=${value}`);
  const payloadPath = path.join(os.tmpdir(), `industrialis-wmi-launch-${cryptoRandomId()}.json`);
  await fs.writeFile(payloadPath, JSON.stringify({
    command_line: [executable, ...args].map(quoteWindowsArgument).join(" "),
    current_directory: cwd,
    environment: [...merged.values()].sort(),
  }), "utf8");
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "$payload = Get-Content -LiteralPath $env:INDUSTRIALIS_WMI_PAYLOAD -Raw | ConvertFrom-Json",
    "$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ CreateFlags = [uint32]520; ShowWindow = [uint16]0; EnvironmentVariables = [string[]]$payload.environment }",
    "$result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = [string]$payload.command_line; CurrentDirectory = [string]$payload.current_directory; ProcessStartupInformation = $startup }",
    "if ([uint32]$result.ReturnValue -ne 0) { throw \"Win32_Process.Create failed with code $($result.ReturnValue)\" }",
    "[Console]::Out.Write([string]$result.ProcessId)",
  ].join("\n");
  try {
    const output = await new Promise<string>((resolve, reject) => {
      execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script], {
        windowsHide: true,
        env: { ...process.env, INDUSTRIALIS_WMI_PAYLOAD: payloadPath },
        maxBuffer: 1024 * 1024,
      }, (error, stdout, stderr) => error ? reject(new Error(`Windows process broker failed: ${stderr || error.message}`)) : resolve(stdout));
    });
    const pid = Number(output.trim());
    if (!Number.isInteger(pid) || pid <= 0) throw new Error("invalid Windows process broker PID");
    return pid;
  } finally {
    await fs.rm(payloadPath, { force: true });
  }
}

function cryptoRandomId(): string {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function isAlive(pid: number): boolean {
  try { process.kill(pid, 0); return true; } catch { return false; }
}

export async function spawnGameProcess(
  executable: string,
  args: string[],
  cwd: string,
  environment: Record<string, string>,
  emit: EmitProcessLog,
): Promise<RunningProcess> {
  if (process.platform === "win32") {
    const pid = await spawnWindowsViaWmi(executable, args, cwd, environment);
    emit("system", `Launched outside the launcher job as process ${pid}`);
    return { pid };
  }
  const child = spawn(executable, args, {
    cwd,
    env: { ...process.env, ...environment },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout?.on("data", (data: Buffer) => emit("stdout", data.toString("utf8")));
  child.stderr?.on("data", (data: Buffer) => emit("stderr", data.toString("utf8")));
  await new Promise<void>((resolve, reject) => {
    child.once("spawn", () => resolve());
    child.once("error", reject);
  });
  return { pid: child.pid ?? 0, child };
}

export async function waitForGameProcess(processInfo: RunningProcess): Promise<number> {
  if (processInfo.child) {
    return new Promise((resolve) => processInfo.child?.once("exit", (code, signal) => resolve(code ?? (signal ? 1 : 0))));
  }
  while (isAlive(processInfo.pid)) await new Promise((resolve) => setTimeout(resolve, 500));
  return 0;
}

export async function killGameProcess(processInfo: RunningProcess): Promise<void> {
  if (processInfo.child) {
    if (!processInfo.child.killed) processInfo.child.kill();
    return;
  }
  if (process.platform === "win32") {
    await new Promise<void>((resolve, reject) => {
      execFile("taskkill.exe", ["/PID", String(processInfo.pid), "/T", "/F"], { windowsHide: true }, (error) => error && !/not found|no running instance/iu.test(error.message) ? reject(error) : resolve());
    });
  } else {
    try { process.kill(processInfo.pid, "SIGTERM"); } catch { /* already exited */ }
  }
}
