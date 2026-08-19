import { describe, expect, it } from "vitest";
import {
  DEFAULT_LAUNCHER_WINDOW_HEIGHT,
  DEFAULT_LAUNCHER_WINDOW_WIDTH,
  LAUNCHER_WINDOW_MIN_HEIGHT,
  LAUNCHER_WINDOW_MIN_WIDTH,
  isValidLauncherWindowSettings,
  normalizeLauncherWindowSettings,
  validateLauncherWindowDimension,
} from "./launcher-window";

describe("launcher window settings", () => {
  it("normalizes missing or invalid values to safe defaults", () => {
    expect(normalizeLauncherWindowSettings({ window_width: 799, window_height: 99999 })).toEqual({
      launch_maximized: false,
      window_width: DEFAULT_LAUNCHER_WINDOW_WIDTH,
      window_height: DEFAULT_LAUNCHER_WINDOW_HEIGHT,
    });
  });

  it("preserves a valid launch size and maximize preference", () => {
    const settings = normalizeLauncherWindowSettings({ launch_maximized: true, window_width: 1920, window_height: 1080 });
    expect(settings).toEqual({ launch_maximized: true, window_width: 1920, window_height: 1080 });
    expect(isValidLauncherWindowSettings(settings)).toBe(true);
  });

  it("rejects dimensions below the native minimum", () => {
    expect(
      isValidLauncherWindowSettings({
        launch_maximized: false,
        window_width: LAUNCHER_WINDOW_MIN_WIDTH - 1,
        window_height: LAUNCHER_WINDOW_MIN_HEIGHT,
      }),
    ).toBe(false);
  });

  it("validates a draft without changing it while it is being typed", () => {
    expect(validateLauncherWindowDimension("1", "Width", LAUNCHER_WINDOW_MIN_WIDTH, 7680)).toEqual({
      value: null,
      error: "Width must be at least 800 pixels.",
    });
    expect(validateLauncherWindowDimension("1280", "Width", LAUNCHER_WINDOW_MIN_WIDTH, 7680)).toEqual({
      value: 1280,
      error: null,
    });
  });
});
