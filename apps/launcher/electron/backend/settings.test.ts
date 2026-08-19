// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LauncherSettings } from "./types";

const electronState = vi.hoisted(() => ({ appData: "" }));

vi.mock("electron", () => ({
  app: {
    getPath: () => electronState.appData,
  },
}));

import { loadLauncherSettings, validateLauncherSettings } from "./settings";
import { defaultLauncherSettings } from "./types";

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-settings-"));
  electronState.appData = tempRoot;
});

afterEach(async () => {
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("launcher window settings persistence", () => {
  it("loads a saved launch size and maximize preference", async () => {
    const settingsPath = path.join(tempRoot, "industrialis-launcher", "launcher-settings.json");
    await fs.mkdir(path.dirname(settingsPath), { recursive: true });
    await fs.writeFile(settingsPath, JSON.stringify({ launch_maximized: true, window_width: 1440, window_height: 900 }), "utf8");

    await expect(loadLauncherSettings()).resolves.toMatchObject({
      launch_maximized: true,
      window_width: 1440,
      window_height: 900,
    });
  });

  it("falls back to defaults for invalid saved dimensions", async () => {
    const settingsPath = path.join(tempRoot, "industrialis-launcher", "launcher-settings.json");
    await fs.mkdir(path.dirname(settingsPath), { recursive: true });
    await fs.writeFile(settingsPath, JSON.stringify({ window_width: 1, window_height: "large" }), "utf8");

    const settings = await loadLauncherSettings();
    const defaults = defaultLauncherSettings();
    expect(settings.window_width).toBe(defaults.window_width);
    expect(settings.window_height).toBe(defaults.window_height);
  });

  it("rejects invalid launch dimensions before saving", () => {
    const settings = {
      ...defaultLauncherSettings(),
      window_width: 799,
    } as LauncherSettings;
    expect(() => validateLauncherSettings(settings)).toThrow("invalid launcher window settings");
  });
});
