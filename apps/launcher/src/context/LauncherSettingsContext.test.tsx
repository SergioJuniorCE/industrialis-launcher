import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_LAUNCHER_SETTINGS, type LauncherSettingsData } from "../lib/launcher-settings";
import type { ElectronLauncherApi } from "../lib/desktop";
import { LauncherSettingsProvider } from "./LauncherSettingsContext";
import { useLauncherSettings } from "./launcher-settings-context";

type SaveRequest = {
  snapshot: LauncherSettingsData;
  resolve: () => void;
};

let root: Root;
let container: HTMLDivElement;
let saveRequests: SaveRequest[];
let persistedSettings: LauncherSettingsData | null;
const reactActGlobal = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
const originalActEnvironment = reactActGlobal.IS_REACT_ACT_ENVIRONMENT;

function SettingsHarness({ onReady }: { onReady: (settings: ReturnType<typeof useLauncherSettings>) => void }) {
  const settings = useLauncherSettings();

  useEffect(() => {
    if (settings.loaded) onReady(settings);
  }, [onReady, settings]);

  return null;
}

function deferredSave(snapshot: LauncherSettingsData): Promise<void> {
  return new Promise<void>((resolve) => {
    saveRequests.push({
      snapshot,
      resolve: () => {
        persistedSettings = snapshot;
        resolve();
      },
    });
  });
}

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  saveRequests = [];
  persistedSettings = null;
  window.electronAPI = {
    platform: "win32",
    invoke: vi.fn((command: string, args?: unknown) => {
      if (command === "get_launcher_settings") return Promise.resolve({ ...DEFAULT_LAUNCHER_SETTINGS });
      if (command === "save_launcher_settings") {
        const snapshot = (args as { launcherSettings: LauncherSettingsData }).launcherSettings;
        return deferredSave(snapshot);
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }),
    listen: vi.fn(),
    openUrl: vi.fn(),
    convertFileSrc: vi.fn(),
    hideWindow: vi.fn(),
    minimizeWindow: vi.fn(),
    toggleMaximizeWindow: vi.fn(),
    isWindowMaximized: vi.fn(),
    closeWindow: vi.fn(),
  } satisfies ElectronLauncherApi;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  delete window.electronAPI;
  if (originalActEnvironment === undefined) delete reactActGlobal.IS_REACT_ACT_ENVIRONMENT;
  else reactActGlobal.IS_REACT_ACT_ENVIRONMENT = originalActEnvironment;
});

describe("LauncherSettingsProvider persistence", () => {
  it("does not let an older overlapping save overwrite newer window settings", async () => {
    const settingsRef: { current: ReturnType<typeof useLauncherSettings> | null } = { current: null };
    const onReady = (value: ReturnType<typeof useLauncherSettings>) => {
      settingsRef.current = value;
    };

    await act(async () => {
      root.render(
        <LauncherSettingsProvider>
          <SettingsHarness onReady={onReady} />
        </LauncherSettingsProvider>,
      );
      await Promise.resolve();
    });

    if (!settingsRef.current) throw new Error("Launcher settings did not load");
    const settings = settingsRef.current;
    expect(settings.loaded).toBe(true);

    await act(async () => {
      settings!.updateSettings({ window_width: 1280 });
      void settings!.saveSettingsNow();
      settings!.updateSettings({ window_height: 900 });
      void settings!.saveSettingsNow();
    });

    expect(saveRequests).toHaveLength(1);
    expect(saveRequests[0].snapshot).toMatchObject({ window_width: 1280, window_height: DEFAULT_LAUNCHER_SETTINGS.window_height });

    await act(async () => {
      saveRequests[0].resolve();
      await Promise.resolve();
    });

    expect(saveRequests).toHaveLength(2);
    expect(saveRequests[1].snapshot).toMatchObject({ window_width: 1280, window_height: 900 });

    await act(async () => {
      saveRequests[1].resolve();
      await Promise.resolve();
    });

    expect(persistedSettings).toMatchObject({ window_width: 1280, window_height: 900 });
  });
});
