// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConsoleLogWriter } from "./console-log-writer";

const electronState = vi.hoisted(() => ({ appData: "" }));

vi.mock("electron", () => ({
  app: {
    isPackaged: false,
    getPath: () => electronState.appData,
    getAppPath: () => electronState.appData,
    getVersion: () => "test",
    quit: vi.fn(),
  },
  BrowserWindow: {
    getFocusedWindow: () => null,
    getAllWindows: () => [],
  },
  dialog: { showOpenDialog: vi.fn() },
  shell: { openExternal: vi.fn(), openPath: vi.fn() },
}));

import { LauncherBackend } from "./index";

interface BackendInternals {
  consoleLogWriter: ConsoleLogWriter;
  emitLog(id: string, stream: string, line: string): void;
}

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-backend-"));
  electronState.appData = tempRoot;
});

afterEach(async () => {
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("LauncherBackend console log endpoint", () => {
  it("waits for an emitted log entry before reading the console log", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const internals = backend as unknown as BackendInternals;
    const originalFlush = internals.consoleLogWriter.flush.bind(internals.consoleLogWriter);
    let releaseFlush!: () => void;
    const flushGate = new Promise<void>((resolve) => {
      releaseFlush = resolve;
    });
    const flushSpy = vi.spyOn(internals.consoleLogWriter, "flush").mockImplementation(async (id) => {
      await flushGate;
      await originalFlush(id);
    });

    internals.emitLog("alpha", "stdout", "queued output");
    const read = backend.invoke("get_instance_console_log", { id: "alpha", full: true });

    await vi.waitFor(() => expect(flushSpy).toHaveBeenCalledWith("alpha"));
    releaseFlush();

    await expect(read).resolves.toEqual([{ stream: "stdout", line: "queued output" }]);
  });
});
