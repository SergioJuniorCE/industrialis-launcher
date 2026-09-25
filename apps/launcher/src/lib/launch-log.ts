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

export const LAUNCH_SEPARATOR_PATTERN = /─{3,}\s*Launch\s*─{3,}/;
export const CRASH_EXCERPT_MAX_LINES = 400;
export const CRASH_EXCERPT_HEADER_LINES = 15;

export function isLaunchStartLine(line: string): boolean {
  return LAUNCH_SEPARATOR_PATTERN.test(line);
}

function findLatestLaunchStart(log: readonly LaunchLogLine[]): number {
  for (let index = log.length - 1; index >= 0; index -= 1) {
    if (isLaunchStartLine(log[index].line)) return index;
  }
  return -1;
}

/** Slice log lines belonging to the most recent launch. Falls back to the full log when no launch marker exists. */
export function sliceLatestLaunch(log: readonly LaunchLogLine[]): LaunchLogLine[] {
  const start = findLatestLaunchStart(log);
  if (start < 0) return [...log];
  return log.slice(start);
}

/**
 * Extract a copy-paste friendly excerpt of the latest launch for AI debugging.
 * The persisted console log spans multiple launches, so this isolates the latest
 * launch (from the last `──────── Launch ────────` marker). Small launches are
 * returned whole; large ones keep the launch header plus the tail where the
 * crash/error output lives, with a truncation marker in between.
 */
export function extractLatestCrashLines(log: readonly LaunchLogLine[], maxLines: number = CRASH_EXCERPT_MAX_LINES): LaunchLogLine[] {
  if (log.length === 0) return [];
  const latest = sliceLatestLaunch(log);
  if (latest.length === 0) return [];
  const limit = Number.isInteger(maxLines) && maxLines > 1 ? maxLines : CRASH_EXCERPT_MAX_LINES;
  if (latest.length <= limit) return latest;

  const headerCount = Math.min(CRASH_EXCERPT_HEADER_LINES, Math.max(2, Math.floor(limit / 4)), limit - 2);
  const header = latest.slice(0, Math.max(0, headerCount));
  const tailCount = limit - header.length - 1;
  const tail = latest.slice(-tailCount);
  const truncated = latest.length - header.length - tail.length;
  const hasBoundary = findLatestLaunchStart(log) >= 0;
  const marker: LaunchLogLine = {
    stream: "system",
    line: hasBoundary
      ? `... [truncated ${truncated} lines from latest launch — showing launch header + last ${tail.length} lines for AI debugging] ...`
      : `... [truncated ${truncated} lines — showing first ${header.length} + last ${tail.length} lines for AI debugging] ...`,
  };
  return [...header, marker, ...tail];
}
