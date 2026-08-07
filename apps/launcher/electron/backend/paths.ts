import path from "node:path";
import { app } from "electron";

export function dataDir(): string {
  return path.join(app.getPath("appData"), "industrialis-launcher");
}

export function instancesDir(): string {
  return path.join(dataDir(), "instances");
}

export function instanceDir(id: string): string {
  return path.join(instancesDir(), sanitizeName(id));
}

export function settingsPath(id: string): string {
  return path.join(instanceDir(id), "instance.json");
}

export function accountsPath(): string {
  return path.join(dataDir(), "accounts.json");
}

export function launcherSettingsPath(): string {
  return path.join(dataDir(), "launcher-settings.json");
}

export function groupsPath(): string {
  return path.join(instancesDir(), "instgroups.json");
}

export function consoleLogPath(id: string): string {
  return path.join(instanceDir(id), "console.log");
}

export function packCacheRoot(): string {
  return path.join(dataDir(), "pack-cache");
}

export function sanitizeName(value: string): string {
  return [...value].map((char) => /[\s/\\:*?"<>|]/u.test(char) ? "_" : char).join("");
}

export function validateInstanceId(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) throw new Error("instance id cannot be empty");
  const id = sanitizeName(trimmed);
  if (!id) throw new Error("instance id cannot be empty");
  return id;
}
