// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { ConsoleLogWriter, MAX_PERSISTED_CONSOLE_LOG_BYTES } from "./console-log-writer";

describe("ConsoleLogWriter", () => {
  it("keeps simultaneous flush callers waiting for entries appended during a write", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-console-log-"));
    const logPath = path.join(directory, "console.log");
    const writer = new ConsoleLogWriter(() => logPath);
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    const appendFile = fs.appendFile.bind(fs);
    const append = vi.spyOn(fs, "appendFile").mockImplementationOnce(async (...args) => {
      await blocked;
      return appendFile(...args);
    });
    try {
      writer.append("alpha", { stream: "stdout", line: "first" });
      let flushed = false;
      const firstFlush = writer.flush("alpha").then(() => {
        flushed = true;
      });
      const secondFlush = writer.flush("alpha");
      await vi.waitFor(() => expect(append).toHaveBeenCalledOnce());
      writer.append("alpha", { stream: "stdout", line: "second" });
      expect(flushed).toBe(false);
      release();
      await Promise.all([firstFlush, secondFlush]);
      const lines = (await fs.readFile(logPath, "utf8"))
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line).line);
      expect(lines).toEqual(["first", "second"]);
      await writer.flush("alpha");
    } finally {
      release();
      await writer.flush("alpha");
      append.mockRestore();
      await fs.rm(directory, { recursive: true, force: true });
    }
  });

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
