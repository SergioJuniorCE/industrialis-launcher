// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { ConsoleLogWriter, MAX_PERSISTED_CONSOLE_LOG_BYTES } from "./console-log-writer";

describe("ConsoleLogWriter", () => {
  it("flushes every queued entry before returning", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-console-log-"));
    const logPath = path.join(directory, "console.log");
    const writer = new ConsoleLogWriter(() => logPath);

    try {
      writer.append("alpha", { stream: "stdout", line: "first" });
      writer.append("alpha", { stream: "stderr", line: "second" });

      await writer.flush("alpha");

      expect(await fs.readFile(logPath, "utf8")).toBe(
        `${JSON.stringify({ stream: "stdout", line: "first" })}\n${JSON.stringify({ stream: "stderr", line: "second" })}\n`,
      );
    } finally {
      await fs.rm(directory, { recursive: true, force: true });
    }
  });

  it("retains the newest complete entries within the persisted byte limit", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-console-log-"));
    const logPath = path.join(directory, "console.log");
    const writer = new ConsoleLogWriter(() => logPath);
    const filler = "x".repeat(8 * 1024);
    const entryCount = 600;

    try {
      for (let index = 0; index < entryCount; index += 1) {
        writer.append("alpha", { stream: "stdout", line: `${index}:${filler}` });
      }

      await writer.flush("alpha");

      const contents = await fs.readFile(logPath, "utf8");
      const entries = contents
        .trimEnd()
        .split("\n")
        .map((line) => JSON.parse(line) as { stream: string; line: string });
      expect(Buffer.byteLength(contents, "utf8")).toBeLessThanOrEqual(MAX_PERSISTED_CONSOLE_LOG_BYTES);
      expect(entries[0]?.line.startsWith("0:")).toBe(false);
      expect(entries[entries.length - 1]).toEqual({ stream: "stdout", line: `${entryCount - 1}:${filler}` });
    } finally {
      await fs.rm(directory, { recursive: true, force: true });
    }
  });
});
