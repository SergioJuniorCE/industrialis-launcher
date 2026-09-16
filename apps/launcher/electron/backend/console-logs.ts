import fs from "node:fs/promises";
import { MAX_PERSISTED_CONSOLE_LOG_BYTES } from "./console-log-writer";
import { consoleLogPath, sanitizeName } from "./paths";
import { MAX_RETAINED_LOG_LINES, takeLogTail } from "../../src/lib/log-buffer";
import type { LaunchLogLine } from "./types";
import type { BackendContext } from "./backend-context";

const CONSOLE_LOG_TAIL_BYTES = MAX_PERSISTED_CONSOLE_LOG_BYTES;

async function readConsoleLogTail(filePath: string): Promise<string> {
  const file = await fs.open(filePath, "r").catch(() => null);
  if (!file) return "";

  try {
    const size = (await file.stat()).size;
    const start = Math.max(0, size - CONSOLE_LOG_TAIL_BYTES);
    const buffer = Buffer.alloc(size - start);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, start);
    return buffer.subarray(0, bytesRead).toString("utf8");
  } finally {
    await file.close();
  }
}

function isLaunchLogLine(value: unknown): value is LaunchLogLine {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.stream === "string" && typeof entry.line === "string";
}

function parseConsoleLog(contents: string, full: boolean): LaunchLogLine[] {
  const entries = contents
    .split(/\r?\n/u)
    .filter(Boolean)
    .flatMap((line) => {
      try {
        const value: unknown = JSON.parse(line);
        return isLaunchLogLine(value) ? [value] : [];
      } catch {
        return [];
      }
    });
  return full ? entries : takeLogTail(entries, MAX_RETAINED_LOG_LINES);
}

export async function getConsoleLog(ctx: BackendContext, rawId: string, full: boolean): Promise<LaunchLogLine[]> {
  const id = sanitizeName(rawId);
  await ctx.flushConsoleLog(id);
  await ctx.compactConsoleLog(id);
  const filePath = consoleLogPath(id);
  const contents = await readConsoleLogTail(filePath);
  return parseConsoleLog(contents, full);
}
