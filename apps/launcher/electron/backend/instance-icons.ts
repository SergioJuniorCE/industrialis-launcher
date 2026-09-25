import fs from "node:fs/promises";
import path from "node:path";
import { app, shell } from "electron";
import { exists } from "./fs-utils";
import { iconsDir, instanceDir, sanitizeName } from "./paths";
import { loadInstanceSettings, saveInstanceSettings } from "./settings";
import type { SetInstanceIconArgs, SetInstanceIconFromLibraryArgs } from "./backend-context";
import type { InstanceSettings } from "./types";

const supportedIconExtension = /\.(png|jpe?g|webp|gif|bmp|ico)$/iu;
const defaultIconId = "gtnh-logo.png";

export interface InstanceIconEntry {
  id: string;
  label: string;
  path: string;
  built_in: boolean;
}

function bundledIconsDir(): string {
  return app.isPackaged ? path.join(process.resourcesPath, "icons") : path.join(app.getAppPath(), "electron", "icons");
}

function iconLabel(id: string, builtIn: boolean): string {
  if (builtIn && id === defaultIconId) return "GT New Horizons";
  const stem = path
    .basename(id, path.extname(id))
    .replace(/^custom-/u, "")
    .replace(/[-_]+/gu, " ")
    .trim();
  return stem || "Custom icon";
}

async function bundledIconIds(): Promise<string[]> {
  const ids: string[] = [];
  for (const entry of await fs.readdir(bundledIconsDir(), { withFileTypes: true }).catch(() => [])) {
    if (entry.isFile() && supportedIconExtension.test(entry.name)) ids.push(entry.name);
  }
  return ids.sort((a, b) => a.localeCompare(b));
}

export async function ensureInstanceIconLibrary(): Promise<Set<string>> {
  const destination = iconsDir();
  await fs.mkdir(destination, { recursive: true });
  const builtIns = new Set(await bundledIconIds());
  await Promise.all(
    [...builtIns].map(async (id) => {
      const target = path.join(destination, id);
      if (await exists(target)) return;
      await fs.copyFile(path.join(bundledIconsDir(), id), target);
    }),
  );
  return builtIns;
}

export async function listInstanceIcons(): Promise<InstanceIconEntry[]> {
  const builtIns = await ensureInstanceIconLibrary();
  const icons: InstanceIconEntry[] = [];
  for (const entry of await fs.readdir(iconsDir(), { withFileTypes: true })) {
    if (!entry.isFile() || !supportedIconExtension.test(entry.name)) continue;
    icons.push({
      id: entry.name,
      label: iconLabel(entry.name, builtIns.has(entry.name)),
      path: path.join(iconsDir(), entry.name),
      built_in: builtIns.has(entry.name),
    });
  }
  return icons.sort((a, b) => Number(b.built_in) - Number(a.built_in) || a.label.localeCompare(b.label));
}

export async function importInstanceIcon(source: string): Promise<InstanceIconEntry> {
  const stat = await fs.stat(source).catch(() => null);
  if (!stat?.isFile()) throw new Error("image file not found");
  if (stat.size > 4 * 1024 * 1024) throw new Error("image must be under 4 MB");
  const extension = path.extname(source).toLowerCase();
  if (!supportedIconExtension.test(extension)) throw new Error("unsupported image type; use PNG, JPG, WebP, GIF, BMP, or ICO");
  await ensureInstanceIconLibrary();
  const sourceStem =
    path
      .basename(source, path.extname(source))
      .replace(/[^\p{L}\p{N}._-]+/gu, "-")
      .replace(/^-+|-+$/gu, "") || "icon";
  const base = `custom-${sourceStem}`;
  let id = `${base}${extension}`;
  for (let suffix = 2; await exists(path.join(iconsDir(), id)); suffix += 1) id = `${base}-${suffix}${extension}`;
  const destination = path.join(iconsDir(), id);
  await fs.copyFile(source, destination);
  return { id, label: iconLabel(id, false), path: destination, built_in: false };
}

export async function instanceIconLibraryPath(rawId: string): Promise<string> {
  const id = path.basename(rawId);
  if (id !== rawId || !supportedIconExtension.test(id)) throw new Error("invalid icon id");
  await ensureInstanceIconLibrary();
  const source = path.join(iconsDir(), id);
  const stat = await fs.stat(source).catch(() => null);
  if (!stat?.isFile()) throw new Error("icon not found in the icon library");
  return source;
}

export function defaultInstanceIconPath(): Promise<string> {
  return instanceIconLibraryPath(defaultIconId);
}

const defaultInstanceIconFilename = "instance-icon.png";

export async function installDefaultInstanceIcon(instance: string): Promise<string> {
  const source = await defaultInstanceIconPath();
  await fs.copyFile(source, path.join(instance, defaultInstanceIconFilename));
  return defaultInstanceIconFilename;
}

export async function resolveIconPath(id: string, settings: InstanceSettings): Promise<string | null> {
  const instance = instanceDir(id);
  if (settings.custom_icon && (await exists(path.join(instance, settings.custom_icon)))) return path.join(instance, settings.custom_icon);
  for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => [])) {
    if (entry.name.startsWith("instance-icon") && /\.(png|jpe?g|webp|gif|bmp|ico)$/iu.test(entry.name)) return path.join(instance, entry.name);
  }
  return null;
}

export async function setInstanceIcon(args: SetInstanceIconArgs): Promise<void> {
  const id = sanitizeName(args.id);
  const source = String(args.sourcePath ?? "");
  const instance = instanceDir(id);
  const stat = await fs.stat(source).catch(() => null);
  if (!stat?.isFile()) throw new Error("image file not found");
  if (stat.size > 4 * 1024 * 1024) throw new Error("image must be under 4 MB");
  const extension = path.extname(source).toLowerCase();
  if (!/\.(png|jpe?g|webp|gif|bmp|ico)$/u.test(extension)) throw new Error("unsupported image type; use PNG, JPG, WebP, GIF, BMP, or ICO");
  await fs.mkdir(instance, { recursive: true });
  await clearInstanceIcon(id);
  const filename = `instance-icon${extension}`;
  await fs.copyFile(source, path.join(instance, filename));
  const settings = await loadInstanceSettings(id);
  settings.custom_icon = filename;
  await saveInstanceSettings(id, settings);
}

export async function setInstanceIconFromLibrary(args: SetInstanceIconFromLibraryArgs): Promise<void> {
  const sourcePath = await instanceIconLibraryPath(String(args.iconId ?? ""));
  await setInstanceIcon({ id: args.id, sourcePath });
}

export async function openInstanceIconsFolder(): Promise<void> {
  await ensureInstanceIconLibrary();
  const error = await shell.openPath(iconsDir());
  if (error) throw new Error(`failed to open icons folder: ${error}`);
}

export async function clearInstanceIcon(rawId: string): Promise<void> {
  const id = sanitizeName(rawId.trim());
  const instance = instanceDir(id);
  for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => []))
    if (entry.name.startsWith("instance-icon")) await fs.rm(path.join(instance, entry.name), { force: true });
  const settings = await loadInstanceSettings(id);
  settings.custom_icon = null;
  await saveInstanceSettings(id, settings);
}
