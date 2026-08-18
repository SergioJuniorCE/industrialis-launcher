export const LAUNCHER_WINDOW_MIN_WIDTH = 800;
export const LAUNCHER_WINDOW_MIN_HEIGHT = 600;
export const LAUNCHER_WINDOW_MAX_WIDTH = 7680;
export const LAUNCHER_WINDOW_MAX_HEIGHT = 4320;
export const DEFAULT_LAUNCHER_WINDOW_WIDTH = 1100;
export const DEFAULT_LAUNCHER_WINDOW_HEIGHT = 750;

export interface LauncherWindowSettings {
  launch_maximized: boolean;
  window_width: number;
  window_height: number;
}

export interface LauncherWindowDimensionValidation {
  value: number | null;
  error: string | null;
}

function normalizeDimension(value: unknown, fallback: number, minimum: number, maximum: number): number {
  return typeof value === "number" && Number.isInteger(value) && value >= minimum && value <= maximum ? value : fallback;
}

export function validateLauncherWindowDimension(raw: string, label: string, minimum: number, maximum: number): LauncherWindowDimensionValidation {
  const trimmed = raw.trim();
  if (!/^\d+$/u.test(trimmed)) return { value: null, error: `${label} must be a whole number.` };

  const value = Number(trimmed);
  if (!Number.isSafeInteger(value)) return { value: null, error: `${label} must be a whole number.` };
  if (value < minimum) return { value: null, error: `${label} must be at least ${minimum} pixels.` };
  if (value > maximum) return { value: null, error: `${label} must be at most ${maximum} pixels.` };
  return { value, error: null };
}

export function normalizeLauncherWindowSettings(settings: Partial<LauncherWindowSettings>): LauncherWindowSettings {
  return {
    launch_maximized: settings.launch_maximized === true,
    window_width: normalizeDimension(settings.window_width, DEFAULT_LAUNCHER_WINDOW_WIDTH, LAUNCHER_WINDOW_MIN_WIDTH, LAUNCHER_WINDOW_MAX_WIDTH),
    window_height: normalizeDimension(settings.window_height, DEFAULT_LAUNCHER_WINDOW_HEIGHT, LAUNCHER_WINDOW_MIN_HEIGHT, LAUNCHER_WINDOW_MAX_HEIGHT),
  };
}

export function isValidLauncherWindowSettings(settings: LauncherWindowSettings): boolean {
  return (
    typeof settings.launch_maximized === "boolean" &&
    normalizeDimension(settings.window_width, Number.NaN, LAUNCHER_WINDOW_MIN_WIDTH, LAUNCHER_WINDOW_MAX_WIDTH) === settings.window_width &&
    normalizeDimension(settings.window_height, Number.NaN, LAUNCHER_WINDOW_MIN_HEIGHT, LAUNCHER_WINDOW_MAX_HEIGHT) === settings.window_height
  );
}
