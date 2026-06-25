export interface LaunchLogLine {
  stream: "stdout" | "stderr" | "system";
  line: string;
}

export type LaunchLogLevel = "error" | "warn" | "info" | "system";

const LOG_LEVEL_CLASS: Record<LaunchLogLevel, string> = {
  error: "text-red-400",
  warn: "text-yellow-400",
  info: "text-green-400",
  system: "text-sky-400",
};

export function launchLogLevelClass(level: LaunchLogLevel): string {
  return LOG_LEVEL_CLASS[level];
}

function minecraftLevel(line: string): LaunchLogLevel | null {
  const match = line.match(/\[[^\]]*\/(ERROR|FATAL|WARN)\]/i);
  if (!match) return null;
  const level = match[1].toUpperCase();
  if (level === "ERROR" || level === "FATAL") return "error";
  if (level === "WARN") return "warn";
  return null;
}

function isErrorLine(line: string): boolean {
  if (/---- Minecraft Crash Report ----/i.test(line)) return true;
  if (/Process exited with code (?!0\b)/i.test(line)) return true;
  if (/launch failed/i.test(line)) return true;
  if (/An uncaught exception/i.test(line)) return true;
  if (/Exception in thread/i.test(line)) return true;
  if (/\bCaused by:/i.test(line)) return true;
  if (/^\s+at [\w.$/]/.test(line)) return true;
  if (/^java\.[\w.]+\b/.test(line.trimStart())) return true;
  if (/\[(ERROR|FATAL)\]/i.test(line)) return true;
  if (/\b(ERROR|FATAL)\]/i.test(line)) return true;
  if (/\b\w+Exception\b/.test(line) && !/without exception/i.test(line)) return true;
  if (/\bError adding\b/i.test(line)) return true;
  if (/\bFailed to\b/i.test(line)) return true;
  return false;
}

function isWarnLine(line: string): boolean {
  const trimmed = line.trimStart();
  if (/^WARNING:/i.test(trimmed)) return true;
  if (/\bWARN\]/i.test(line)) return true;
  if (/\[WARN\]/i.test(line)) return true;
  return false;
}

export function classifyLaunchLogLine(entry: LaunchLogLine): LaunchLogLevel {
  const mcLevel = minecraftLevel(entry.line);
  if (mcLevel) return mcLevel;

  if (isErrorLine(entry.line)) return "error";
  if (isWarnLine(entry.line)) return "warn";
  if (entry.stream === "system") return "system";
  return "info";
}

export function formatLaunchLog(log: LaunchLogLine[]): string {
  return log.map((entry) => entry.line).join("\n");
}