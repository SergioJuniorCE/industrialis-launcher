import { describe, expect, it } from "vitest";
import {
  applyDlProgressEvent,
  inferOperation,
  isInstanceBusy,
  normalizeProcessOperation,
  operationLabel,
  runningProcessCount,
} from "./background-processes";

describe("background process operations", () => {
  it("treats pack updates as background processes", () => {
    expect(normalizeProcessOperation("update-pack")).toBe("update-pack");
    expect(inferOperation({ stage: "updating", pct: 0.5, operation: "update-pack", id: "a" })).toBe(
      "update-pack",
    );
    expect(operationLabel("update-pack")).toBe("Updating pack");
  });

  it("accepts legacy update operation names from the backend", () => {
    expect(normalizeProcessOperation("update")).toBe("update-pack");
    expect(inferOperation({ stage: "updating", pct: 0.5, operation: "update", id: "a" })).toBe(
      "update-pack",
    );
  });

  it("does not treat preview or unknown events as background processes", () => {
    expect(inferOperation({ stage: "preview", pct: 0.5, operation: "preview", id: "a" })).toBeNull();
    expect(inferOperation({ stage: "downloading", pct: 0.5, id: "a" })).toBeNull();
  });

  it("tracks pack updates in the process list and busy state", () => {
    const initial = new Map();
    const withUpdate = applyDlProgressEvent(initial, {
      stage: "updating",
      pct: 0.2,
      operation: "update-pack",
      id: "inst-1",
      name: "GTNH",
      log_line: "Downloading pack archive…",
    });

    expect(runningProcessCount(withUpdate)).toBe(1);
    expect(isInstanceBusy(withUpdate, "inst-1")).toBe(true);
    expect(isInstanceBusy(withUpdate, "inst-2")).toBe(false);
  });

  it("marks pack updates failed from progress events", () => {
    const initial = applyDlProgressEvent(new Map(), {
      stage: "updating",
      pct: 0.2,
      operation: "update-pack",
      id: "inst-1",
      name: "GTNH",
    });

    const failed = applyDlProgressEvent(initial, {
      stage: "failed",
      pct: 0,
      operation: "update-pack",
      id: "inst-1",
      log_line: "Error: download failed",
    });

    const proc = failed.get("update-pack:inst-1");
    expect(proc?.status).toBe("failed");
    expect(proc?.logs).toContain("Error: download failed");
  });

  it("tracks clean reinstalls as background processes", () => {
    const withReinstall = applyDlProgressEvent(new Map(), {
      stage: "reinstalling",
      pct: 0.2,
      operation: "reinstall",
      id: "inst-1",
      name: "GTNH",
      log_line: "Backing up saves…",
    });

    expect(operationLabel("reinstall")).toBe("Clean reinstall");
    expect(runningProcessCount(withReinstall)).toBe(1);
    expect(isInstanceBusy(withReinstall, "inst-1")).toBe(true);
  });

  it("tracks instance copies as background processes", () => {
    const withCopy = applyDlProgressEvent(new Map(), {
      stage: "copying",
      pct: 0.4,
      operation: "copy",
      id: "inst-copy",
      name: "GTNH Copy",
    });

    expect(operationLabel("copy")).toBe("Copying instance");
    expect(inferOperation({ stage: "copying", pct: 0.4, operation: "copy", id: "inst-copy" })).toBe(
      "copy",
    );
    expect(runningProcessCount(withCopy)).toBe(1);
    expect(isInstanceBusy(withCopy, "inst-copy")).toBe(true);
  });

  it("does not create a background process from unrelated progress events", () => {
    const processes = new Map();

    const next = applyDlProgressEvent(processes, {
      stage: "downloading",
      pct: 0.4,
      id: "inst-1",
      name: "GTNH",
    });

    expect(next.size).toBe(0);
    expect(runningProcessCount(next)).toBe(0);
    expect(isInstanceBusy(next, "inst-1")).toBe(false);
  });
});