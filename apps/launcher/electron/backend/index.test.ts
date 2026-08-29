// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MAX_PERSISTED_CONSOLE_LOG_BYTES, type ConsoleLogWriter } from "./console-log-writer";
import { consoleLogPath, instanceDir, instancesDir } from "./paths";
import { getProcessCreationId, killGameProcess, normalizeProcessCreationId, spawnGameProcess, waitForGameProcess } from "./process-manager";
import { loadRunningGamePids, saveRunningGamePids } from "./running-game-pids";

const electronState = vi.hoisted(() => ({
  appData: "",
  shell: { openExternal: vi.fn(), openPath: vi.fn(async () => "") },
}));

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
  shell: electronState.shell,
}));

import { LauncherBackend } from "./index";

interface BackendInternals {
  consoleLogWriter: ConsoleLogWriter;
  emitLog(id: string, stream: string, line: string): void;
  notifyInstanceOperationWaiters(): void;
  trackFilesystemOperation<T>(operation: () => Promise<T>): Promise<T>;
  state: {
    running: Map<string, { pid: number; creationId?: string }>;
    installInProgress: Set<string>;
    updateInProgress: Set<string>;
    reinstallInProgress: Set<string>;
    copyInProgress: Set<string>;
    deleteCancel: Map<string, { cancelled: boolean }>;
  };
}

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-backend-"));
  electronState.appData = tempRoot;
  vi.clearAllMocks();
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

describe("LauncherBackend shutdown", () => {
  it("waits for active instance operations before completing disposal", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const internals = backend as unknown as BackendInternals;
    internals.state.updateInProgress.add("alpha");

    let disposed = false;
    const disposal = backend.dispose().then(() => {
      disposed = true;
    });
    await Promise.resolve();
    expect(disposed).toBe(false);

    internals.state.updateInProgress.delete("alpha");
    internals.notifyInstanceOperationWaiters();
    await disposal;
    expect(disposed).toBe(true);
  });

  it("waits for other filesystem mutations before completing disposal", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const internals = backend as unknown as BackendInternals;
    let release!: () => void;
    const mutation = internals.trackFilesystemOperation(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );

    let disposed = false;
    const disposal = backend.dispose().then(() => {
      disposed = true;
    });
    await Promise.resolve();
    expect(disposed).toBe(false);

    release();
    await mutation;
    await disposal;
    expect(disposed).toBe(true);
  });
});

describe("LauncherBackend running game PID state", () => {
  it("remembers only live game PIDs and clears the state when no game is running", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const internals = backend as unknown as BackendInternals;
    const creationId = await getProcessCreationId(process.pid);
    if (!creationId) throw new Error("test process creation ID was unavailable");
    internals.state.running.set("alpha", { pid: process.pid, creationId });
    internals.state.running.set("stale", { pid: Number.MAX_SAFE_INTEGER, creationId });

    await backend.dispose();
    await expect(loadRunningGamePids()).resolves.toEqual(new Map([["alpha", { pid: process.pid, creationId }]]));

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
      const instancePath = path.join(instancesDir(), "alpha");
      await fs.mkdir(instancePath, { recursive: true });
      await fs.writeFile(path.join(instancePath, "mmc-pack.json"), "{}", "utf8");
      const creationId = await getProcessCreationId(child.pid);
      if (!creationId) throw new Error("test game process creation ID was unavailable");
      await saveRunningGamePids(new Map([["alpha", { pid: child.pid, creationId }]]));

      const emit = vi.fn();
      const backend = new LauncherBackend({ emit });
      const internals = backend as unknown as BackendInternals;
      await backend.invoke("get_instances", {});
      expect(internals.state.running.get("alpha")).toMatchObject({ pid: child.pid });
      expect(emit).toHaveBeenCalledWith("instance-started", { id: "alpha", restored: true });

      child.kill();
      await vi.waitFor(() => expect(internals.state.running.has("alpha")).toBe(false), { timeout: 2_000 });
      await backend.dispose();
    } finally {
      if (child.exitCode === null) child.kill();
    }
  });

  it.skipIf(process.platform !== "win32")("restores the identity produced by a detached game launch", async () => {
    const running = await spawnGameProcess(process.execPath, ["-e", "setInterval(() => {}, 1000)"], process.cwd(), {}, () => undefined);
    const waitForExit = waitForGameProcess(running);
    let backend: LauncherBackend | undefined;
    let internals: BackendInternals | undefined;
    try {
      expect(running.creationId).toBeTruthy();
      await expect(getProcessCreationId(running.pid)).resolves.toBe(running.creationId);
      const persistedCreationId = process.platform === "win32" ? (BigInt(running.creationId!) * 10_000n + 4n).toString() : running.creationId!;
      expect(normalizeProcessCreationId(persistedCreationId)).toBe(running.creationId);
      const instancePath = path.join(instancesDir(), "alpha");
      await fs.mkdir(instancePath, { recursive: true });
      await fs.writeFile(path.join(instancePath, "mmc-pack.json"), "{}", "utf8");
      await saveRunningGamePids(new Map([["alpha", { pid: running.pid, creationId: persistedCreationId }]]));

      backend = new LauncherBackend({ emit: vi.fn() });
      const restoredInternals = backend as unknown as BackendInternals;
      internals = restoredInternals;
      await backend.invoke("get_instances", {});
      expect(restoredInternals.state.running.get("alpha")).toMatchObject({ pid: running.pid, creationId: running.creationId });
      await backend.dispose();
    } finally {
      await killGameProcess(running).catch(() => undefined);
      await waitForExit.catch(() => undefined);
      const restoredInternals = internals;
      if (backend && restoredInternals) {
        await vi.waitFor(
          async () => {
            expect(restoredInternals.state.running.has("alpha")).toBe(false);
            await expect(loadRunningGamePids()).resolves.toEqual(new Map());
          },
          { timeout: 2_000 },
        );
      }
    }
  });

  it("ignores a live PID whose creation ID belongs to another process", async () => {
    const game = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
    const unrelated = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
    try {
      await Promise.all(
        [game, unrelated].map(
          (child) =>
            new Promise<void>((resolve, reject) => {
              child.once("spawn", () => resolve());
              child.once("error", reject);
            }),
        ),
      );
      if (!game.pid || !unrelated.pid) throw new Error("test game processes did not start");
      const gameCreationId = await getProcessCreationId(game.pid);
      const unrelatedCreationId = await getProcessCreationId(unrelated.pid);
      if (!gameCreationId || !unrelatedCreationId) throw new Error("test process creation ID was unavailable");
      expect(unrelatedCreationId).not.toBe(gameCreationId);
      const instancePath = path.join(instancesDir(), "alpha");
      await fs.mkdir(instancePath, { recursive: true });
      await fs.writeFile(path.join(instancePath, "mmc-pack.json"), "{}", "utf8");
      await saveRunningGamePids(new Map([["alpha", { pid: game.pid, creationId: unrelatedCreationId }]]));

      const emit = vi.fn();
      const backend = new LauncherBackend({ emit });
      const internals = backend as unknown as BackendInternals;
      await backend.invoke("get_instances", {});

      expect(internals.state.running.has("alpha")).toBe(false);
      expect(emit).not.toHaveBeenCalledWith("instance-started", expect.anything());
      await expect(loadRunningGamePids()).resolves.toEqual(new Map());
      await backend.dispose();
    } finally {
      if (game.exitCode === null) game.kill();
      if (unrelated.exitCode === null) unrelated.kill();
    }
  });

  it("keeps a live PID through repeated launcher recreations", async () => {
    const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
    const backends: Array<{ backend: LauncherBackend; internals: BackendInternals }> = [];
    try {
      await new Promise<void>((resolve, reject) => {
        child.once("spawn", () => resolve());
        child.once("error", reject);
      });
      if (!child.pid) throw new Error("test game process did not start");
      const instancePath = path.join(instancesDir(), "alpha");
      await fs.mkdir(instancePath, { recursive: true });
      await fs.writeFile(path.join(instancePath, "mmc-pack.json"), "{}", "utf8");
      const creationId = await getProcessCreationId(child.pid);
      if (!creationId) throw new Error("test game process creation ID was unavailable");
      await saveRunningGamePids(new Map([["alpha", { pid: child.pid, creationId }]]));

      for (let cycle = 0; cycle < 3; cycle += 1) {
        const backend = new LauncherBackend({ emit: vi.fn() });
        const internals = backend as unknown as BackendInternals;
        backends.push({ backend, internals });

        await backend.invoke("get_instances", {});
        expect(internals.state.running.get("alpha")).toMatchObject({ pid: child.pid });
        await expect(loadRunningGamePids()).resolves.toEqual(new Map([["alpha", { pid: child.pid, creationId }]]));

        await backend.dispose();
        await expect(loadRunningGamePids()).resolves.toEqual(new Map([["alpha", { pid: child.pid, creationId }]]));
      }

      child.kill();
      await vi.waitFor(
        async () => {
          expect(backends.every(({ internals }) => !internals.state.running.has("alpha"))).toBe(true);
          expect(await loadRunningGamePids()).toEqual(new Map());
        },
        { timeout: 2_000 },
      );
    } finally {
      if (child.exitCode === null) child.kill();
      await Promise.all(backends.map(({ backend }) => backend.dispose().catch(() => undefined)));
    }
  });
});

describe("LauncherBackend mods folder endpoint", () => {
  it("creates and opens the active mods folder", async () => {
    const backend = new LauncherBackend({ emit: vi.fn() });
    const instance = instanceDir("alpha");
    const modsDir = path.join(instance, ".minecraft", "mods");
    await fs.mkdir(instance, { recursive: true });

    await expect(backend.invoke("open_mods_folder", { id: "alpha" })).resolves.toBeUndefined();

    expect((await fs.stat(modsDir)).isDirectory()).toBe(true);
    expect(electronState.shell.openPath).toHaveBeenCalledWith(modsDir);
  });
});
