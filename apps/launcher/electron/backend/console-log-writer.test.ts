// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { ConsoleLogWriter } from "./console-log-writer";

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
});
