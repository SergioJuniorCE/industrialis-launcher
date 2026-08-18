// @vitest-environment node

import { describe, expect, it } from "vitest";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile, spawn } from "node:child_process";
import { getProcessCreationId, killGameProcess, spawnGameProcess, waitForGameProcess } from "./process-manager";

const javawExecutable = process.platform === "win32" && process.env.JAVA_HOME ? path.join(process.env.JAVA_HOME, "bin", "javaw.exe") : null;

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`Timed out after ${timeoutMs}ms`)), timeoutMs);
    void promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function processIsAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function terminateProcessTree(pid: number): Promise<void> {
  return new Promise((resolve) => {
    execFile("taskkill.exe", ["/PID", String(pid), "/T", "/F"], { windowsHide: true }, () => resolve());
  });
}

function closeKillOnCloseJob(helperPid: number, launchSignalPath: string, gameStatePath: string): Promise<void> {
  const script = String.raw`
$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class IndustrialisJobTest {
  [StructLayout(LayoutKind.Sequential)]
  public struct BasicLimitInformation {
    public long PerProcessUserTimeLimit;
    public long PerJobUserTimeLimit;
    public uint LimitFlags;
    public UIntPtr MinimumWorkingSetSize;
    public UIntPtr MaximumWorkingSetSize;
    public uint ActiveProcessLimit;
    public UIntPtr Affinity;
    public uint PriorityClass;
    public uint SchedulingClass;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct IoCounters {
    public ulong ReadOperationCount;
    public ulong WriteOperationCount;
    public ulong OtherOperationCount;
    public ulong ReadTransferCount;
    public ulong WriteTransferCount;
    public ulong OtherTransferCount;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct ExtendedLimitInformation {
    public BasicLimitInformation BasicLimitInformation;
    public IoCounters IoInfo;
    public UIntPtr ProcessMemoryLimit;
    public UIntPtr JobMemoryLimit;
    public UIntPtr PeakProcessMemoryUsed;
    public UIntPtr PeakJobMemoryUsed;
  }

  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr CreateJobObject(IntPtr attributes, string name);

  [DllImport("kernel32.dll", SetLastError = true)]
  public static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);

  [DllImport("kernel32.dll", SetLastError = true)]
  public static extern IntPtr OpenProcess(uint access, bool inheritHandle, uint processId);

  [DllImport("kernel32.dll", SetLastError = true)]
  public static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

  [DllImport("kernel32.dll")]
  public static extern bool CloseHandle(IntPtr handle);
}
'@

$job = [IndustrialisJobTest]::CreateJobObject([IntPtr]::Zero, $null)
if ($job -eq [IntPtr]::Zero) { throw "CreateJobObject failed" }
$processHandle = [IntPtr]::Zero
$infoPointer = [IntPtr]::Zero

try {
  $info = New-Object IndustrialisJobTest+ExtendedLimitInformation
  $basic = $info.BasicLimitInformation
  $basic.LimitFlags = [uint32]0x2000
  $info.BasicLimitInformation = $basic
  $infoSize = [Runtime.InteropServices.Marshal]::SizeOf($info)
  $infoPointer = [Runtime.InteropServices.Marshal]::AllocHGlobal($infoSize)
  [Runtime.InteropServices.Marshal]::StructureToPtr($info, $infoPointer, $false)
  if (-not [IndustrialisJobTest]::SetInformationJobObject($job, 9, $infoPointer, [uint32]$infoSize)) {
    throw "SetInformationJobObject failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
  }

  $processHandle = [IndustrialisJobTest]::OpenProcess([uint32]0x1101, $false, [uint32]$env:INDUSTRIALIS_HELPER_PID)
  if ($processHandle -eq [IntPtr]::Zero) { throw "OpenProcess failed" }
  if (-not [IndustrialisJobTest]::AssignProcessToJobObject($job, $processHandle)) {
    throw "AssignProcessToJobObject failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
  }

  [System.IO.File]::WriteAllText($env:INDUSTRIALIS_LAUNCH_SIGNAL, "go")
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (-not [System.IO.File]::Exists($env:INDUSTRIALIS_GAME_STATE)) {
    if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for detached game state" }
    Start-Sleep -Milliseconds 25
  }
} finally {
  if ($infoPointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::FreeHGlobal($infoPointer) }
  if ($processHandle -ne [IntPtr]::Zero) { [void][IndustrialisJobTest]::CloseHandle($processHandle) }
  [void][IndustrialisJobTest]::CloseHandle($job)
}
`;

  return new Promise((resolve, reject) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
      {
        env: {
          ...process.env,
          INDUSTRIALIS_HELPER_PID: String(helperPid),
          INDUSTRIALIS_LAUNCH_SIGNAL: launchSignalPath,
          INDUSTRIALIS_GAME_STATE: gameStatePath,
        },
        windowsHide: true,
        maxBuffer: 1024 * 1024,
      },
      (error, _stdout, stderr) => (error ? reject(new Error(stderr || error.message)) : resolve()),
    );
  });
}

async function waitUntil(check: () => boolean | Promise<boolean>, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await check())) {
    if (Date.now() >= deadline) throw new Error(`Condition was not met within ${timeoutMs}ms`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

describe("process creation identity", () => {
  it.skipIf(process.platform !== "linux")("includes the Linux boot ID with the process start time", async () => {
    const [bootId, stat] = await Promise.all([fs.readFile("/proc/sys/kernel/random/boot_id", "utf8"), fs.readFile(`/proc/${process.pid}/stat`, "utf8")]);
    const commandEnd = stat.lastIndexOf(")");
    if (commandEnd < 0) throw new Error("current process stat did not contain a command name");
    const startTime = stat
      .slice(commandEnd + 2)
      .trim()
      .split(/\s+/u)[19];
    if (!startTime) throw new Error("current process stat did not contain a start time");

    await expect(getProcessCreationId(process.pid)).resolves.toBe(`${bootId.trim()}:${startTime}`);
  });
});

describe("detached game process logs", () => {
  it.skipIf(process.platform !== "win32")(
    "forwards stdout and stderr from a Windows process launched outside the launcher job",
    async () => {
      const lines: Array<{ stream: string; line: string }> = [];
      const marker = `industrialis-detached-log-${Date.now()}`;
      const running = await spawnGameProcess(
        process.execPath,
        ["-e", `console.log(${JSON.stringify(marker)}); console.error(${JSON.stringify(`${marker}-error`)})`],
        process.cwd(),
        {},
        (stream, line) => lines.push({ stream, line }),
      );

      await expect(waitForGameProcess(running)).resolves.toBe(0);

      expect(lines).toEqual(
        expect.arrayContaining([
          { stream: "stdout", line: expect.stringContaining(marker) },
          { stream: "stderr", line: expect.stringContaining(`${marker}-error`) },
        ]),
      );
    },
    45_000,
  );

  it.skipIf(process.platform !== "win32")(
    "streams whole lines while the detached Windows game process is still running and stops cleanly",
    async () => {
      const marker = `industrialis-live-log-${Date.now()}`;
      let resolveLiveLogs!: () => void;
      const liveLogs = new Promise<void>((resolve) => {
        resolveLiveLogs = resolve;
      });
      const lines: Array<{ stream: string; line: string }> = [];
      let grandchildPid = 0;
      const running = await spawnGameProcess(
        process.execPath,
        [
          "-e",
          `const { spawn } = require("node:child_process"); const grandchild = spawn(process.execPath, ["-e", "setInterval(() => {}, 1_000)"], { stdio: "ignore" }); process.stdout.write(${JSON.stringify(marker)}); process.stderr.write(${JSON.stringify(`${marker}-error`)}); setTimeout(() => { console.log("-stdout"); console.error("-stderr"); console.log(${JSON.stringify(`${marker}-grandchild:`)} + grandchild.pid) }, 100); setInterval(() => {}, 1_000)`,
        ],
        process.cwd(),
        {},
        (stream, line) => {
          lines.push({ stream, line });
          if (stream === "stdout" && line.startsWith(`${marker}-grandchild:`)) {
            grandchildPid = Number(line.slice(`${marker}-grandchild:`.length));
          }
          if (
            lines.some((entry) => entry.stream === "stdout" && entry.line === `${marker}-stdout`) &&
            lines.some((entry) => entry.stream === "stderr" && entry.line === `${marker}-error-stderr`) &&
            grandchildPid > 0
          ) {
            resolveLiveLogs();
          }
        },
      );
      const waitForExit = waitForGameProcess(running);
      let stopped = false;

      try {
        await withTimeout(liveLogs, 30_000);
        expect(() => process.kill(running.pid, 0)).not.toThrow();
        expect(processIsAlive(grandchildPid)).toBe(true);
        await killGameProcess(running);
        stopped = true;
        await expect(waitForExit).resolves.toBe(0);
        await waitUntil(() => !processIsAlive(grandchildPid), 10_000);
      } finally {
        if (!stopped) {
          await killGameProcess(running).catch(() => undefined);
          await waitForExit.catch(() => undefined);
        }
        if (grandchildPid > 0 && processIsAlive(grandchildPid)) await terminateProcessTree(grandchildPid);
      }
    },
    60_000,
  );

  it.skipIf(process.platform !== "win32")(
    "keeps the detached Windows game process alive after the launcher-side parent exits",
    async () => {
      const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-detach-test-"));
      const statePath = path.join(directory, "state.json");
      const helperReadyPath = path.join(directory, "helper-ready");
      const launchSignalPath = path.join(directory, "launch-game");
      const marker = `detach-${Date.now()}`;
      const viteNode = path.resolve(process.cwd(), "../../node_modules/vite-node/vite-node.mjs");
      const helperScript = path.resolve(process.cwd(), "electron/backend/process-manager-detach-helper.ts");
      const helper = spawn(process.execPath, [viteNode, helperScript, statePath, helperReadyPath, launchSignalPath, marker], {
        cwd: process.cwd(),
        windowsHide: true,
        stdio: "ignore",
      });
      let gamePid = 0;
      const helperPid = helper.pid;

      try {
        expect(helperPid).toBeDefined();
        if (helperPid === undefined || helperPid <= 0) throw new Error("Detached-process helper did not spawn");
        await waitUntil(() => existsSync(helperReadyPath), 30_000);
        await closeKillOnCloseJob(helperPid, launchSignalPath, statePath);
        const state = JSON.parse(await fs.readFile(statePath, "utf8")) as { marker: string; gamePid: number };
        expect(state.marker).toBe(marker);
        gamePid = state.gamePid;
        await waitUntil(() => !processIsAlive(helperPid), 10_000);
        expect(processIsAlive(gamePid)).toBe(true);
      } finally {
        if (gamePid > 0) await terminateProcessTree(gamePid);
        if (helperPid && processIsAlive(helperPid)) await terminateProcessTree(helperPid);
        await fs.rm(directory, { recursive: true, force: true });
      }
    },
    60_000,
  );

  it.skipIf(process.platform !== "win32")(
    "preserves the exit code from the detached Windows game process",
    async () => {
      const running = await spawnGameProcess(process.execPath, ["-e", "process.exit(23)"], process.cwd(), {}, () => undefined);
      const captureDirectory = running.capture?.directory;
      expect(captureDirectory).toBeDefined();

      await expect(waitForGameProcess(running)).resolves.toBe(23);
      if (captureDirectory) await waitUntil(() => !existsSync(captureDirectory), 10_000);
    },
    45_000,
  );

  it.skipIf(process.platform !== "win32")(
    "passes environment variables through the detached Windows launcher",
    async () => {
      const key = `INDUSTRIALIS_PROCESS_MANAGER_${Date.now()}`;
      const value = `value-${Date.now()}`;
      const lines: Array<{ stream: string; line: string }> = [];
      const running = await spawnGameProcess(
        process.execPath,
        ["-e", `console.log(process.env[${JSON.stringify(key)}])`],
        process.cwd(),
        { [key]: value },
        (stream, line) => lines.push({ stream, line }),
      );

      await expect(waitForGameProcess(running)).resolves.toBe(0);
      expect(lines).toEqual(expect.arrayContaining([{ stream: "stdout", line: value }]));
    },
    45_000,
  );

  it.skipIf(javawExecutable === null || !existsSync(javawExecutable))(
    "captures stderr from the same javaw executable used for Minecraft",
    async () => {
      if (javawExecutable === null) throw new Error("JAVA_HOME is not configured");
      const lines: Array<{ stream: string; line: string }> = [];
      const running = await spawnGameProcess(javawExecutable, ["-version"], process.cwd(), {}, (stream, line) => lines.push({ stream, line }));

      await expect(waitForGameProcess(running)).resolves.toBe(0);
      expect(lines.some(({ stream, line }) => stream === "stderr" && /version/iu.test(line))).toBe(true);
    },
    45_000,
  );

  it.skipIf(process.platform !== "win32")(
    "rejects when the detached Windows game executable cannot start",
    async () => {
      const missingExecutable = path.join(os.tmpdir(), `missing-industrialis-game-${Date.now()}.exe`);

      await expect(spawnGameProcess(missingExecutable, [], process.cwd(), {}, () => undefined)).rejects.toThrow(/start|cannot find|not found/iu);
    },
    45_000,
  );
});
