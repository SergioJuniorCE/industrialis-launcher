// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MAX_PERSISTED_CONSOLE_LOG_BYTES, type ConsoleLogWriter } from "./console-log-writer";
import { consoleLogPath } from "./paths";
import { loadRunningGamePids, saveRunningGamePids } from "./running-game-pids";

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
  state: { running: Map<string, { pid: number }> };
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

  it("bounds a legacy oversized file before returning a full retained log", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const logPath = consoleLogPath("alpha");
    const filler = "x".repeat(8 * 1024);
    const entryCount = 600;
    const contents = Array.from({ length: entryCount }, (_, index) => `${JSON.stringify({ stream: "stdout", line: `${index}:${filler}` })}\n`).join("");
    await fs.mkdir(path.dirname(logPath), { recursive: true });
    await fs.writeFile(logPath, contents, "utf8");

    const result = (await backend.invoke("get_instance_console_log", { id: "alpha", full: true })) as Array<{ stream: string; line: string }>;
    const stat = await fs.stat(logPath);

    expect(stat.size).toBeLessThanOrEqual(MAX_PERSISTED_CONSOLE_LOG_BYTES);
    expect(result.length).toBeLessThan(entryCount);
    expect(result[result.length - 1]).toEqual({ stream: "stdout", line: `${entryCount - 1}:${filler}` });
  });
});

describe("LauncherBackend running game PID state", () => {
  it("remembers only live game PIDs and clears the state when no game is running", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const internals = backend as unknown as BackendInternals;
    internals.state.running.set("alpha", { pid: process.pid });
    internals.state.running.set("stale", { pid: Number.MAX_SAFE_INTEGER });

    await backend.dispose();
    await expect(loadRunningGamePids()).resolves.toEqual(new Map([["alpha", process.pid]]));

    internals.state.running.clear();
    await backend.dispose();
    await expect(loadRunningGamePids()).resolves.toEqual(new Map());
  });

  it("restores a live PID after the launcher is recreated", async () => {
    const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
    try {
      await new Promise<void>((resolve, reject) => {
        child.once("spawn", () => resolve());
        child.once("error", reject);
      });
      if (!child.pid) throw new Error("test game process did not start");
      await saveRunningGamePids(new Map([["alpha", child.pid]]));

      const backend = new LauncherBackend({ emit: vi.fn() });
      const internals = backend as unknown as BackendInternals;
      await backend.invoke("get_instances", {});
      expect(internals.state.running.get("alpha")).toMatchObject({ pid: child.pid });

      child.kill();
      await vi.waitFor(() => expect(internals.state.running.has("alpha")).toBe(false), { timeout: 2_000 });
      await backend.dispose();
    } finally {
      if (child.exitCode === null) child.kill();
    }
  });
});
