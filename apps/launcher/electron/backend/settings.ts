import { launcherSettingsPath, settingsPath } from "./paths";
import { readJson, writeJson } from "./fs-utils";
import { isValidLauncherWindowSettings, normalizeLauncherWindowSettings } from "../../src/lib/launcher-window";
import { defaultInstanceSettings, defaultLauncherSettings, type InstanceSettings, type LauncherSettings } from "./types";

export async function loadInstanceSettings(id: string): Promise<InstanceSettings> {
  const saved = await readJson<Partial<InstanceSettings>>(settingsPath(id));
  return {
    ...defaultInstanceSettings(),
    ...saved,
    env_vars: { ...saved?.env_vars },
  };
}

export async function saveInstanceSettings(id: string, settings: InstanceSettings): Promise<void> {
  await writeJson(settingsPath(id), settings);
}

export async function loadLauncherSettings(): Promise<LauncherSettings> {
  const saved = await readJson<Partial<LauncherSettings>>(launcherSettingsPath());
  return {
    ...defaultLauncherSettings(),
    ...saved,
    ...normalizeLauncherWindowSettings(saved ?? {}),
    theme_overrides: { ...saved?.theme_overrides },
    custom_theme_presets: [...(saved?.custom_theme_presets ?? [])],
    default_account_id: saved?.default_account_id ?? null,
    default_java_path: saved?.default_java_path ?? null,
  };
}

function validateThemeValue(value: unknown, message: string): void {
  if (typeof value !== "string" || value.length > 32) throw new Error(message);
}

export function validateLauncherSettings(settings: LauncherSettings): void {
  if (!settings.theme_preset || settings.theme_preset.length > 64) {
    throw new Error("invalid theme preset id");
  }
  if (!Number.isInteger(settings.instance_grid_columns) || settings.instance_grid_columns < 2 || settings.instance_grid_columns > 5) {
    throw new Error("instance grid columns must be between 2 and 5");
  }
  if (!Number.isInteger(settings.backup_retention_limit) || settings.backup_retention_limit < 1 || settings.backup_retention_limit > 1_000) {
    throw new Error("backup retention limit must be between 1 and 1000");
  }
  if (!isValidLauncherWindowSettings(settings)) {
    throw new Error("invalid launcher window settings");
  }
  if (settings.default_java_path && (settings.default_java_path.trim().length === 0 || settings.default_java_path.length > 4096)) {
    throw new Error("invalid default Java path");
  }
  for (const value of Object.values(settings.theme_overrides)) {
    if (value !== undefined) validateThemeValue(value, "theme override value too long");
  }
  if (
    settings.theme_preset.startsWith("custom-") &&
    !settings.custom_theme_presets.some((preset) => {
      return typeof preset === "object" && preset !== null && "id" in preset && preset.id === settings.theme_preset;
    })
  ) {
    throw new Error("active custom theme preset not found");
  }
}

export async function saveLauncherSettings(settings: LauncherSettings): Promise<void> {
  validateLauncherSettings(settings);
  await writeJson(launcherSettingsPath(), settings);
}
