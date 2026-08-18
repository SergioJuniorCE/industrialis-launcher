import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { StringDecoder } from "node:string_decoder";

export interface RunningProcess {
  pid: number;
  creationId?: string;
  child?: ChildProcess;
  capture?: WindowsProcessCapture;
  stopRequested?: boolean;
  waitStarted?: boolean;
}

export type EmitProcessLog = (stream: string, line: string) => void;

interface CaptureTailer {
  stopAndDrain(): Promise<void>;
  dispose(): Promise<void>;
}

interface WindowsProcessCapture {
  wrapperPid: number;
  directory: string;
  stdoutPath: string;
  stderrPath: string;
  readyPath: string;
  exitCodePath: string;
  acknowledgementPath: string;
  wrapperPath: string;
  payloadPath: string;
  tailer: CaptureTailer;
}

interface WindowsSpawnResult {
  pid: number;
  creationId?: string;
  capture: Omit<WindowsProcessCapture, "tailer">;
}

const CAPTURE_POLL_MS = 50;
const WINDOWS_CAPTURE_READY_TIMEOUT_MS = 30_000;
const WINDOWS_CAPTURE_WRAPPER = String.raw`param(
  [Parameter(Mandatory = $true)]
  [string]$PayloadPath,
  [Parameter(Mandatory = $true)]
  [string]$StartupErrorPath,
  [Parameter(Mandatory = $true)]
  [string]$ExitCodePath
)

$ErrorActionPreference = "Stop"
$payload = $null

try {
  $payload = Get-Content -LiteralPath $PayloadPath -Raw | ConvertFrom-Json
  [System.IO.File]::Delete($PayloadPath)
} catch {
  [System.IO.File]::WriteAllText($StartupErrorPath, ($_ | Out-String), [System.Text.Encoding]::UTF8)
  [System.IO.File]::WriteAllText($ExitCodePath, "1", [System.Text.Encoding]::ASCII)
  exit 1
}

$child = $null
$stdoutFile = $null
$stderrFile = $null
$exitCode = 1
$launchError = $null

try {
  $stdoutFile = New-Object System.IO.FileStream(
    [string]$payload.stdout_path,
    [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::Write,
    [System.IO.FileShare]::ReadWrite,
    1,
    ([System.IO.FileOptions]::Asynchronous -bor [System.IO.FileOptions]::WriteThrough)
  )
  $stderrFile = New-Object System.IO.FileStream(
    [string]$payload.stderr_path,
    [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::Write,
    [System.IO.FileShare]::ReadWrite,
    1,
    ([System.IO.FileOptions]::Asynchronous -bor [System.IO.FileOptions]::WriteThrough)
  )

  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.FileName = [string]$payload.executable
  $startInfo.Arguments = [string]$payload.arguments_line
  $startInfo.WorkingDirectory = [string]$payload.current_directory
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true

  $child = New-Object System.Diagnostics.Process
  $child.StartInfo = $startInfo
  if (-not $child.Start()) {
    throw "The game process did not start."
  }

  $stdoutCopy = $child.StandardOutput.BaseStream.CopyToAsync($stdoutFile)
  $stderrCopy = $child.StandardError.BaseStream.CopyToAsync($stderrFile)
  $readyTempPath = [string]$payload.ready_path + ".tmp"
  [System.IO.File]::WriteAllText(
    $readyTempPath,
    "$($child.Id)|$($child.StartTime.ToUniversalTime().Ticks)",
    [System.Text.Encoding]::ASCII
  )
  [System.IO.File]::Move($readyTempPath, [string]$payload.ready_path)
  $child.WaitForExit()
  $stdoutCopy.Wait()
  $stderrCopy.Wait()
  $exitCode = [int]$child.ExitCode
} catch {
  $launchError = ($_ | Out-String)
} finally {
  if ($null -ne $stdoutFile) { $stdoutFile.Dispose() }
  if ($null -ne $stderrFile) { $stderrFile.Dispose() }
  if ($null -ne $child) { $child.Dispose() }

  if ($null -ne $launchError) {
    [System.IO.File]::AppendAllText(
      [string]$payload.stderr_path,
      $launchError,
      [System.Text.Encoding]::UTF8
    )
  }
  [System.IO.File]::WriteAllText(
    [string]$payload.exit_code_path,
    [string]$exitCode,
    [System.Text.Encoding]::ASCII
  )
}

# The launcher acknowledges after draining the files. If it has already closed,
# keep them briefly so a slow final read cannot race cleanup, then clean up here.
for ($attempt = 0; $attempt -lt 300; $attempt += 1) {
  if (
    [System.IO.File]::Exists([string]$payload.acknowledgement_path) -or
    -not [System.IO.Directory]::Exists([string]$payload.capture_directory)
  ) { break }
  Start-Sleep -Milliseconds 100
}

foreach ($file in @(
  [string]$payload.stdout_path,
  [string]$payload.stderr_path,
  [string]$payload.ready_path,
  ([string]$payload.ready_path + ".tmp"),
  [string]$payload.exit_code_path,
  [string]$payload.acknowledgement_path,
  [string]$MyInvocation.MyCommand.Path
)) {
  try { [System.IO.File]::Delete($file) } catch { }
}
try { [System.IO.Directory]::Delete([string]$payload.capture_directory, $false) } catch { }

exit $exitCode
`;

function quoteWindowsArgument(argument: string): string {
  if (argument && !/[\s"]/u.test(argument)) return argument;
  let result = '"';
  let slashes = 0;
  for (const char of argument) {
    if (char === "\\") {
      slashes += 1;
      continue;
    }
    if (char === '"') {
      result += "\\".repeat(slashes * 2 + 1) + '"';
      slashes = 0;
      continue;
    }
    result += "\\".repeat(slashes) + char;
    slashes = 0;
  }
  return result + "\\".repeat(slashes * 2) + '"';
}

async function spawnWindowsViaWmi(executable: string, args: string[], cwd: string, environment: Record<string, string>): Promise<WindowsSpawnResult> {
  const merged = new Map<string, string>();
  for (const [key, value] of Object.entries(process.env)) if (value !== undefined) merged.set(key.toLowerCase(), `${key}=${value}`);
  for (const [key, value] of Object.entries(environment)) merged.set(key.toLowerCase(), `${key}=${value}`);

  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-game-"));
  const stdoutPath = path.join(directory, "stdout.log");
  const stderrPath = path.join(directory, "stderr.log");
  const readyPath = path.join(directory, "ready");
  const exitCodePath = path.join(directory, "exit-code.txt");
  const acknowledgementPath = path.join(directory, "drained");
  const wrapperPath = path.join(directory, "capture.ps1");
  const payloadPath = path.join(directory, "launch.json");
  const powershellExecutable = windowsPowerShellExecutable();
  const wrapperCommandLine = [
    powershellExecutable,
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    wrapperPath,
    "-PayloadPath",
    payloadPath,
    "-StartupErrorPath",
    stderrPath,
    "-ExitCodePath",
    exitCodePath,
  ]
    .map(quoteWindowsArgument)
    .join(" ");

  await fs.writeFile(wrapperPath, WINDOWS_CAPTURE_WRAPPER, "utf8");
  await fs.writeFile(
    payloadPath,
    JSON.stringify({
      command_line: wrapperCommandLine,
      executable,
      arguments_line: args.map(quoteWindowsArgument).join(" "),
      current_directory: cwd,
      environment: [...merged.values()].sort(),
      stdout_path: stdoutPath,
      stderr_path: stderrPath,
      ready_path: readyPath,
      exit_code_path: exitCodePath,
      acknowledgement_path: acknowledgementPath,
      capture_directory: directory,
    }),
    "utf8",
  );
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "$payload = Get-Content -LiteralPath $env:INDUSTRIALIS_WMI_PAYLOAD -Raw | ConvertFrom-Json",
    // Win32_Process.Create escapes Electron's inherited job. ShowWindow keeps the
    // PowerShell broker hidden without DETACHED_PROCESS, which prevents it from initializing.
    "$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0; EnvironmentVariables = [string[]]$payload.environment }",
    "$result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = [string]$payload.command_line; CurrentDirectory = [string]$payload.current_directory; ProcessStartupInformation = $startup }",
    'if ([uint32]$result.ReturnValue -ne 0) { throw "Win32_Process.Create failed with code $($result.ReturnValue)" }',
    "[Console]::Out.Write([string]$result.ProcessId)",
  ].join("\n");
  let wrapperPid: number | null = null;
  try {
    const output = await new Promise<string>((resolve, reject) => {
      execFile(
        powershellExecutable,
        ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
        {
          windowsHide: true,
          env: { ...process.env, INDUSTRIALIS_WMI_PAYLOAD: payloadPath },
          maxBuffer: 1024 * 1024,
        },
        (error, stdout, stderr) => (error ? reject(new Error(`Windows process broker failed: ${stderr || error.message}`)) : resolve(stdout)),
      );
    });
    wrapperPid = Number(output.trim());
    if (!Number.isInteger(wrapperPid) || wrapperPid <= 0) throw new Error("invalid Windows process broker PID");

    const readyDeadline = Date.now() + WINDOWS_CAPTURE_READY_TIMEOUT_MS;
    while (!(await fileExists(readyPath))) {
      if (await fileExists(exitCodePath)) {
        const startupError = (await fs.readFile(stderrPath, "utf8").catch(() => "")).trim();
        throw new Error(startupError || "Windows game process failed to start");
      }
      if (!isAlive(wrapperPid)) throw new Error("Windows process capture wrapper exited before starting the game");
      if (Date.now() >= readyDeadline) throw new Error("Windows process capture wrapper did not initialize");
      await new Promise((resolve) => setTimeout(resolve, CAPTURE_POLL_MS));
    }
    const [readyPidText, rawCreationId] = (await fs.readFile(readyPath, "utf8").catch(() => "")).trim().split("|", 2);
    const readyPid = Number(readyPidText);
    const pid = Number.isInteger(readyPid) && readyPid > 0 ? readyPid : wrapperPid;
    return {
      pid,
      creationId: normalizeWindowsCreationId(rawCreationId) ?? undefined,
      capture: { wrapperPid, directory, stdoutPath, stderrPath, readyPath, exitCodePath, acknowledgementPath, wrapperPath, payloadPath },
    };
  } catch (error) {
    if (wrapperPid !== null) await terminateWindowsProcess(wrapperPid).catch(() => undefined);
    await cleanupCaptureFiles({
      wrapperPid: wrapperPid ?? 0,
      directory,
      stdoutPath,
      stderrPath,
      readyPath,
      exitCodePath,
      acknowledgementPath,
      wrapperPath,
      payloadPath,
    });
    throw error;
  }
}

async function fileExists(filePath: string): Promise<boolean> {
  return fs.stat(filePath).then(
    () => true,
    () => false,
  );
}

async function cleanupCaptureFiles(capture: Omit<WindowsProcessCapture, "tailer">): Promise<void> {
  await Promise.all(
    [
      capture.stdoutPath,
      capture.stderrPath,
      capture.readyPath,
      `${capture.readyPath}.tmp`,
      capture.exitCodePath,
      capture.acknowledgementPath,
      capture.wrapperPath,
      capture.payloadPath,
    ].map((file) => fs.rm(file, { force: true }).catch(() => undefined)),
  );
  await fs.rmdir(capture.directory).catch(() => undefined);
}

async function readCapturedBytes(
  file: Awaited<ReturnType<typeof fs.open>>,
  offset: number,
  decoder: StringDecoder,
  emit: (line: string) => void,
): Promise<number> {
  const size = (await file.stat()).size;
  let nextOffset = size < offset ? 0 : offset;
  while (nextOffset < size) {
    const buffer = Buffer.allocUnsafe(Math.min(64 * 1024, size - nextOffset));
    const { bytesRead } = await file.read(buffer, 0, buffer.length, nextOffset);
    if (bytesRead === 0) break;
    nextOffset += bytesRead;
    const decoded = decoder.write(buffer.subarray(0, bytesRead));
    if (decoded) emit(decoded);
  }
  return nextOffset;
}

function startCaptureTail(capture: Omit<WindowsProcessCapture, "tailer">, emit: EmitProcessLog): CaptureTailer {
  let stdoutOffset = 0;
  let stderrOffset = 0;
  let stdoutRemainder = "";
  let stderrRemainder = "";
  const stdoutDecoder = new StringDecoder("utf8");
  const stderrDecoder = new StringDecoder("utf8");
  const stdoutFile = fs.open(capture.stdoutPath, "r");
  const stderrFile = fs.open(capture.stderrPath, "r");
  let stopping = false;
  let closed = false;
  let completion: Promise<void> | null = null;

  const emitDecoded = (stream: "stdout" | "stderr", decoded: string) => {
    const contents = (stream === "stdout" ? stdoutRemainder : stderrRemainder) + decoded;
    const parts = contents.split(/\r?\n/u);
    const remainder = parts.pop() ?? "";
    if (stream === "stdout") stdoutRemainder = remainder;
    else stderrRemainder = remainder;
    for (const line of parts) if (line) emit(stream, line);
  };

  const drain = async () => {
    stdoutOffset = await readCapturedBytes(await stdoutFile, stdoutOffset, stdoutDecoder, (decoded) => emitDecoded("stdout", decoded));
    stderrOffset = await readCapturedBytes(await stderrFile, stderrOffset, stderrDecoder, (decoded) => emitDecoded("stderr", decoded));
  };

  const closeFiles = async (): Promise<void> => {
    if (closed) return;
    closed = true;
    const [stdout, stderr] = await Promise.all([stdoutFile.catch(() => null), stderrFile.catch(() => null)]);
    await Promise.all([stdout?.close(), stderr?.close()]);
  };

  const loop = (async () => {
    while (!stopping) {
      await drain().catch(() => undefined);
      if (stopping) break;
      await new Promise((resolve) => setTimeout(resolve, CAPTURE_POLL_MS));
    }
  })();

  const stopAndDrain = (): Promise<void> => {
    if (completion) return completion;
    completion = (async () => {
      stopping = true;
      try {
        await loop;
        if (closed) return;
        await drain();
        emitDecoded("stdout", stdoutDecoder.end());
        emitDecoded("stderr", stderrDecoder.end());
        if (stdoutRemainder) emit("stdout", stdoutRemainder);
        if (stderrRemainder) emit("stderr", stderrRemainder);
      } finally {
        await closeFiles();
      }
    })();
    return completion;
  };

  const dispose = (): Promise<void> => {
    if (completion) return completion;
    completion = (async () => {
      stopping = true;
      try {
        await loop;
      } finally {
        await closeFiles();
      }
    })();
    return completion;
  };

  return {
    stopAndDrain,
    dispose,
  };
}

function windowsPowerShellExecutable(): string {
  return path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
}

function normalizeCreationId(value: string): string | null {
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : null;
}

function normalizeWindowsCreationId(value: string | undefined): string | null {
  const normalized = normalizeCreationId(value ?? "");
  if (normalized === null) return null;
  try {
    const ticks = BigInt(normalized);
    return (ticks >= 1_000_000_000_000_000n ? ticks / 10_000n : ticks).toString();
  } catch {
    return null;
  }
}

export function normalizeProcessCreationId(value: string | undefined): string | null {
  return process.platform === "win32" ? normalizeWindowsCreationId(value) : normalizeCreationId(value ?? "");
}

function getWindowsProcessCreationId(pid: number): Promise<string | null> {
  const script = [
    "$ErrorActionPreference = 'Stop'",
    '$process = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $env:INDUSTRIALIS_PROCESS_ID"',
    "if ($null -ne $process) { [Console]::Out.Write(([datetime]$process.CreationDate).ToUniversalTime().Ticks) }",
  ].join("\n");
  return new Promise((resolve) => {
    execFile(
      windowsPowerShellExecutable(),
      ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
      {
        env: { ...process.env, INDUSTRIALIS_PROCESS_ID: String(pid) },
        windowsHide: true,
        maxBuffer: 1024 * 1024,
      },
      (error, stdout) => resolve(error ? null : normalizeWindowsCreationId(stdout)),
    );
  });
}

async function getLinuxProcessCreationId(pid: number): Promise<string | null> {
  const [bootId, stat] = await Promise.all([
    fs
      .readFile("/proc/sys/kernel/random/boot_id", "utf8")
      .then(normalizeCreationId)
      .catch(() => null),
    fs.readFile(`/proc/${pid}/stat`, "utf8").catch(() => null),
  ]);
  if (bootId === null || stat === null) return null;
  const commandEnd = stat.lastIndexOf(")");
  if (commandEnd < 0) return null;
  const fields = stat
    .slice(commandEnd + 2)
    .trim()
    .split(/\s+/u);
  const startTime = normalizeCreationId(fields[19] ?? "");
  return startTime === null ? null : `${bootId}:${startTime}`;
}

function getPsProcessCreationId(pid: number): Promise<string | null> {
  return new Promise((resolve) => {
    execFile("ps", ["-o", "lstart=", "-p", String(pid)], { windowsHide: true }, (error, stdout) => resolve(error ? null : normalizeCreationId(stdout)));
  });
}

export async function getProcessCreationId(pid: number): Promise<string | null> {
  if (!Number.isInteger(pid) || pid <= 0) return null;
  if (process.platform === "win32") return getWindowsProcessCreationId(pid);
  if (process.platform === "linux") return getLinuxProcessCreationId(pid);
  return getPsProcessCreationId(pid);
}

export function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

const isAlive = isProcessAlive;

async function terminateWindowsProcess(pid: number): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    execFile("taskkill.exe", ["/PID", String(pid), "/T", "/F"], { windowsHide: true }, async (error) => {
      if (!error) {
        resolve();
        return;
      }
      const deadline = Date.now() + 2_000;
      while (isAlive(pid) && Date.now() < deadline) await new Promise((wait) => setTimeout(wait, CAPTURE_POLL_MS));
      if (!isAlive(pid)) resolve();
      else reject(error);
    });
  });
}

async function readCapturedExitCode(filePath: string): Promise<number | null> {
  const value = await fs.readFile(filePath, "utf8").catch(() => null);
  return value !== null && /^-?\d+\s*$/u.test(value) ? Number.parseInt(value, 10) : null;
}

async function isTrackedProcessAlive(processInfo: RunningProcess): Promise<boolean> {
  if (!isAlive(processInfo.pid)) return false;
  if (!processInfo.creationId) return true;
  const currentCreationId = await getProcessCreationId(processInfo.pid);
  return currentCreationId === normalizeProcessCreationId(processInfo.creationId);
}

export async function spawnGameProcess(
  executable: string,
  args: string[],
  cwd: string,
  environment: Record<string, string>,
  emit: EmitProcessLog,
): Promise<RunningProcess> {
  if (process.platform === "win32") {
    const { pid, creationId, capture } = await spawnWindowsViaWmi(executable, args, cwd, environment);
    const tailer = startCaptureTail(capture, emit);
    emit("system", `Launched outside the launcher job as process ${pid}`);
    return { pid, creationId, capture: { ...capture, tailer } };
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
  const pid = child.pid ?? 0;
  return { pid, creationId: (await getProcessCreationId(pid)) ?? undefined, child };
}

export async function waitForGameProcess(processInfo: RunningProcess): Promise<number> {
  processInfo.waitStarted = true;
  if (processInfo.child) {
    return new Promise((resolve) => processInfo.child?.once("exit", (code, signal) => resolve(code ?? (signal ? 1 : 0))));
  }
  if (!processInfo.capture) {
    while (await isTrackedProcessAlive(processInfo)) await new Promise((resolve) => setTimeout(resolve, 500));
    return 0;
  }

  let exitCode: number | null = null;
  while (exitCode === null && isAlive(processInfo.capture.wrapperPid)) {
    exitCode = await readCapturedExitCode(processInfo.capture.exitCodePath);
    if (exitCode === null) await new Promise((resolve) => setTimeout(resolve, CAPTURE_POLL_MS));
  }
  if (exitCode === null) exitCode = await readCapturedExitCode(processInfo.capture.exitCodePath);

  await processInfo.capture.tailer.stopAndDrain();
  if (exitCode !== null) {
    await fs.writeFile(processInfo.capture.acknowledgementPath, "drained", "utf8").catch(() => undefined);
    const resolved = processInfo.stopRequested ? 0 : exitCode;
    await cleanupCaptureFiles(processInfo.capture);
    return resolved;
  }
  while (await isTrackedProcessAlive(processInfo)) await new Promise((resolve) => setTimeout(resolve, 500));
  await cleanupCaptureFiles(processInfo.capture);
  return processInfo.stopRequested ? 0 : 1;
}

export async function killGameProcess(processInfo: RunningProcess): Promise<void> {
  if (processInfo.child) {
    if (!processInfo.child.killed) processInfo.child.kill();
    return;
  }
  if (!processInfo.capture && processInfo.creationId && !(await isTrackedProcessAlive(processInfo))) return;
  if (process.platform === "win32") {
    processInfo.stopRequested = true;
    try {
      await terminateWindowsProcess(processInfo.pid);
    } finally {
      if (processInfo.capture && !processInfo.waitStarted) await processInfo.capture.tailer.dispose();
    }
  } else {
    try {
      process.kill(processInfo.pid, "SIGTERM");
    } catch {
      /* already exited */
    }
  }
}
