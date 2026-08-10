import { beforeEach, describe, expect, it } from "vitest";
import { createProcess } from "../lib/background-processes";
import { DEFAULT_INSTANCE_SETTINGS } from "../lib/instance-settings";
import { resetLauncherStore, useLauncherStore } from "./launcher-store";

describe("launcher store", () => {
  beforeEach(() => resetLauncherStore());

  it("supports functional updates for shared instance data", () => {
    const { setInstances } = useLauncherStore.getState();
    setInstances([
      {
        id: "gtnh-1",
        installed: true,
        size_bytes: 0,
        settings: { ...DEFAULT_INSTANCE_SETTINGS, name: "Main instance" },
        group: "",
      },
    ]);

    setInstances((instances) => instances.map((instance) => ({ ...instance, size_bytes: 1024 })));

    expect(useLauncherStore.getState().instances[0]?.size_bytes).toBe(1024);
  });

  it("opens instance settings as one atomic navigation update", () => {
    useLauncherStore.getState().openInstanceSettings("gtnh-1");

    const state = useLauncherStore.getState();
    expect(state.selectedInstanceId).toBe("gtnh-1");
    expect(state.detailTab).toBe("settings");
  });

  it("keeps process map updates immutable", () => {
    const original = useLauncherStore.getState().processes;
    useLauncherStore.getState().setProcesses((processes) => {
      const next = new Map(processes);
      const process = createProcess("update-pack", "gtnh-1", "Main instance");
      next.set(process.key, process);
      return next;
    });

    const next = useLauncherStore.getState().processes;
    expect(next).not.toBe(original);
    expect(next.get("update-pack:gtnh-1")?.status).toBe("running");
  });
});
