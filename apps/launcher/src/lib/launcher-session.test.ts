import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_INSTANCE_SETTINGS } from "./instance-settings";
import { createLauncherSession, type LauncherSessionDesktop } from "./launcher-session";
import type { LaunchLogLine } from "./launch-log";
import { resetLauncherStore, useLauncherStore } from "../stores/launcher-store";

const instance = {
  id: "alpha",
  installed: true,
  size_bytes: 1024,
  settings: { ...DEFAULT_INSTANCE_SETTINGS, name: "Alpha" },
  group: "",
};

function createHarness({ launchError }: { launchError?: string } = {}) {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  const invoked: Array<{ command: string; args: unknown }> = [];
  let persistedLogCalls = 0;
  const persistedLogResolvers: Array<(lines: LaunchLogLine[]) => void> = [];

  const invoke: LauncherSessionDesktop["invoke"] = async <T>(command: string, args?: unknown) => {
    invoked.push({ command, args });
    switch (command) {
      case "get_instances":
        return [instance] as T;
      case "get_instance_groups":
        return {
          collapsed: {},
          groups: [],
          instance_order: {},
          ungrouped_name: "Ungrouped",
        } as T;
      case "get_accounts":
        return [] as T;
      case "detect_java":
        return [{ path: "C:/Java/bin/java.exe", version: 17 }] as T;
      case "get_versions":
        return {} as T;
      case "check_launcher_update":
        return { status: "up-to-date", current_version: "0.1.0" } as T;
      case "launch_instance":
        if (launchError) throw new Error(launchError);
        return undefined as T;
      case "get_instance_console_log":
        persistedLogCalls += 1;
        return new Promise<T>((resolve) => {
          persistedLogResolvers.push(resolve as (lines: LaunchLogLine[]) => void);
        });
      default:
        return undefined as T;
    }
  };

  const listen: LauncherSessionDesktop["listen"] = async <T>(event: string, listener: (event: { payload: T }) => void) => {
    const callback = (event: { payload: unknown }) => listener(event as { payload: T });
    const callbacks = listeners.get(event) ?? new Set();
    callbacks.add(callback);
    listeners.set(event, callbacks);
    return () => callbacks.delete(callback);
  };

  const emit = <T>(event: string, payload: T) => {
    for (const callback of listeners.get(event) ?? []) callback({ payload });
  };

  const session = createLauncherSession({
    desktop: { invoke, listen, hideWindow: async () => undefined },
    store: useLauncherStore,
  });

  return {
    session,
    emit,
    invoked,
    listeners,
    get persistedLogCalls() {
      return persistedLogCalls;
    },
    persistedLogResolvers,
  };
}

describe("launcher session", () => {
  beforeEach(() => {
    resetLauncherStore();
  });

  it("hydrates shared launcher state and session state through one start operation", async () => {
    const harness = createHarness();

    await harness.session.start();

    expect(useLauncherStore.getState().instances).toEqual([instance]);
    expect(useLauncherStore.getState().accountsLoaded).toBe(true);
    expect(harness.session.snapshot.javaOptions).toEqual([{ path: "C:/Java/bin/java.exe", version: 17 }]);
    expect(harness.session.snapshot.launcherUpdate.status).toBe("up-to-date");
    expect(harness.listeners.size).toBe(5);
  });

  it("owns process transitions and batches live log events", async () => {
    vi.useFakeTimers();
    try {
      const harness = createHarness();
      await harness.session.start();
      const key = harness.session.startProcess("install", "alpha", "Alpha");

      harness.emit("launch-log", { id: "alpha", stream: "stdout", line: "one" });
      harness.emit("launch-log", { id: "alpha", stream: "stdout", line: "two" });
      expect(harness.session.snapshot.instanceLogs).toEqual({});

      vi.advanceTimersByTime(50);
      expect(harness.session.snapshot.instanceLogs.alpha).toEqual([
        { stream: "stdout", line: "one" },
        { stream: "stdout", line: "two" },
      ]);

      harness.emit("dl-progress", { id: "alpha", operation: "install", stage: "done", pct: 1 });
      expect(useLauncherStore.getState().processes.get(key)?.status).toBe("done");
      expect(useLauncherStore.getState().selectedInstanceId).toBe("alpha");
      expect(useLauncherStore.getState().showNewInstance).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("coordinates launch and stop transitions behind the session interface", async () => {
    const harness = createHarness();
    await harness.session.start();

    await harness.session.launch("alpha", {
      ...DEFAULT_INSTANCE_SETTINGS,
      override_console: true,
      show_console_on_launch: true,
      auto_close_console: true,
    });

    expect(harness.invoked.some(({ command }) => command === "launch_instance")).toBe(true);
    expect(useLauncherStore.getState().selectedInstanceId).toBe("alpha");
    expect(useLauncherStore.getState().detailTab).toBe("info");
    expect(useLauncherStore.getState().launching).toBeNull();

    useLauncherStore.setState({ runningInstanceIds: new Set(["alpha"]) });
    await harness.session.kill("alpha");

    expect(useLauncherStore.getState().runningInstanceIds).toEqual(new Set());
    expect(useLauncherStore.getState().launching).toBeNull();
  });

  it("publishes launch failures and opens the console when configured", async () => {
    const harness = createHarness({ launchError: "game process could not start" });
    await harness.session.start();

    await harness.session.launch("alpha", {
      ...DEFAULT_INSTANCE_SETTINGS,
      override_console: true,
      show_console_on_error: true,
    });

    expect(harness.session.snapshot.error).toContain("game process could not start");
    expect(useLauncherStore.getState().detailTab).toBe("logs");
    expect(useLauncherStore.getState().launching).toBeNull();
  });

  it("ignores an older persisted-log response for the same instance", async () => {
    const harness = createHarness();
    await harness.session.start();

    const first = harness.session.loadLogs("alpha");
    const second = harness.session.loadLogs("alpha");
    expect(harness.persistedLogCalls).toBe(2);

    harness.persistedLogResolvers[0]?.([{ stream: "stdout", line: "old" }]);
    harness.persistedLogResolvers[1]?.([{ stream: "stdout", line: "new" }]);
    await Promise.all([first, second]);

    expect(harness.session.snapshot.instanceLogs.alpha).toEqual([{ stream: "stdout", line: "new" }]);
  });

  it("unsubscribes and drops pending log work when disposed", async () => {
    vi.useFakeTimers();
    try {
      const harness = createHarness();
      await harness.session.start();
      harness.session.dispose();

      harness.emit("launch-log", { id: "alpha", stream: "stderr", line: "late" });
      vi.advanceTimersByTime(50);

      expect(harness.session.snapshot.instanceLogs).toEqual({});
      expect([...harness.listeners.values()].every((callbacks) => callbacks.size === 0)).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
