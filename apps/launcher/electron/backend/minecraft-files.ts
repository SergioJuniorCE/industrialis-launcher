import { watch, type FSWatcher } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import { exists, runConcurrent } from "./fs-utils";
import type { MinecraftDirEntry } from "./types";

const maxReadBytes = 2 * 1024 * 1024;
const excludedPrefixes = ["saves/", "assets/", "logs/", "crash-reports/", "mods/"];
const externalFileMirrors = new Map<
  string,
  {
    watcher: FSWatcher;
    timer: ReturnType<typeof setTimeout> | null;
    pending: Promise<void>;
    instance: string;
    sourcePath: string;
    relPath: string;
  }
>();

export function persistentMinecraftDir(instance: string): string {
  return path.join(instance, "persistent-minecraft");
}

export function minecraftGameDir(instance: string): string {
  return path.join(instance, ".minecraft");
}

export function sanitizeMinecraftRelPath(input: string): string {
  const trimmed = input.trim().replaceAll("\\", "/");
  if (!trimmed) return "";
  if (/^(?:\/+|[A-Za-z]:(?:\/|$))/u.test(trimmed) || trimmed.includes("\0")) throw new Error("invalid path");
  const parts = trimmed.split("/");
  if (parts.some((part) => part === ".." || part.includes("\0"))) throw new Error("invalid path");
  return parts.filter((part) => part && part !== ".").join("/");
}

export function isOverlayExcluded(input: string): boolean {
  const normalized = input.replaceAll("\\", "/").toLowerCase().replace(/^\.\//, "");
  return excludedPrefixes.some((prefix) => normalized === prefix.slice(0, -1) || normalized.startsWith(prefix));
}

export function isPathEditable(input: string): boolean {
  if (isOverlayExcluded(input)) return false;
  const lower = input.toLowerCase();
  return ![".jar", ".zip", ".png", ".jpg"].some((extension) => lower.endsWith(extension));
}

export async function resolveMinecraftFilePath(instance: string, relPath: string): Promise<string> {
  const rel = sanitizeMinecraftRelPath(relPath);
  if (!rel || isOverlayExcluded(rel) || !isPathEditable(rel)) throw new Error("file type is not editable");
  if (/\.(?:exe|com|bat|cmd|msi|msp|scr|ps1|sh|app|lnk|url|desktop|dll|so|dylib)$/iu.test(rel)) {
    throw new Error("executable files cannot be opened from the file editor");
  }
  const gameRoot = await fs.realpath(minecraftGameDir(instance));
  const target = path.resolve(gameRoot, rel);
  const actual = await fs.realpath(target).catch(() => null);
  if (!actual || (actual !== gameRoot && !actual.startsWith(`${gameRoot}${path.sep}`))) throw new Error("file path is outside the Minecraft directory");
  const stat = await fs.stat(actual).catch(() => null);
  if (!stat?.isFile()) throw new Error("file not found");
  return actual;
}

function externalMirrorKey(instance: string, relPath: string): string {
  return path.resolve(minecraftGameDir(instance), sanitizeMinecraftRelPath(relPath));
}

async function mirrorExternalMinecraftFile(instance: string, sourcePath: string, relPath: string): Promise<void> {
  const gameRoot = await fs.realpath(minecraftGameDir(instance));
  const actualSource = await fs.realpath(sourcePath).catch(() => null);
  if (!actualSource || (actualSource !== gameRoot && !actualSource.startsWith(`${gameRoot}${path.sep}`))) return;
  const stat = await fs.stat(actualSource).catch(() => null);
  if (!stat?.isFile()) return;

  const overlayRoot = persistentMinecraftDir(instance);
  const overlayPath = path.resolve(overlayRoot, sanitizeMinecraftRelPath(relPath));
  await fs.mkdir(path.dirname(overlayPath), { recursive: true });
  const overlayParent = await fs.realpath(path.dirname(overlayPath));
  const instanceRoot = await fs.realpath(instance);
  if (overlayParent !== instanceRoot && !overlayParent.startsWith(`${instanceRoot}${path.sep}`)) return;
  const existing = await fs.lstat(overlayPath).catch(() => null);
  if (existing?.isSymbolicLink()) return;
  await fs.copyFile(actualSource, overlayPath);
}

export async function watchExternalMinecraftFile(instance: string, relPath: string): Promise<void> {
  const sourcePath = await resolveMinecraftFilePath(instance, relPath);
  const key = externalMirrorKey(instance, relPath);
  if (externalFileMirrors.has(key)) return;

  const watcher = watch(path.dirname(sourcePath), { persistent: false }, (_event, filename) => {
    if (filename != null && filename.toString() !== path.basename(sourcePath)) return;
    const entry = externalFileMirrors.get(key);
    if (!entry) return;
    if (entry.timer) clearTimeout(entry.timer);
    entry.timer = setTimeout(() => {
      entry.timer = null;
      entry.pending = entry.pending.then(() => mirrorExternalMinecraftFile(instance, sourcePath, relPath)).catch(() => undefined);
    }, 250);
  });
  watcher.on("error", () => void stopExternalMinecraftFileWatch(instance, relPath));
  externalFileMirrors.set(key, { watcher, timer: null, pending: Promise.resolve(), instance, sourcePath, relPath });
}

export async function stopExternalMinecraftFileWatch(instance: string, relPath: string): Promise<void> {
  const key = externalMirrorKey(instance, relPath);
  const entry = externalFileMirrors.get(key);
  if (!entry) return;
  if (entry.timer) clearTimeout(entry.timer);
  entry.watcher.close();
  externalFileMirrors.delete(key);
  await entry.pending;
}

export async function stopAllExternalMinecraftFileWatches(): Promise<void> {
  const pending: Promise<void>[] = [];
  for (const entry of externalFileMirrors.values()) {
    if (entry.timer) clearTimeout(entry.timer);
    entry.watcher.close();
    entry.pending = entry.pending.then(() => mirrorExternalMinecraftFile(entry.instance, entry.sourcePath, entry.relPath)).catch(() => undefined);
    pending.push(entry.pending);
  }
  externalFileMirrors.clear();
  await Promise.all(pending);
}

async function persistentPaths(instance: string): Promise<Set<string>> {
  const overlay = persistentMinecraftDir(instance);
  const result = new Set<string>();
  if (!(await exists(overlay))) return result;
  await runConcurrent([overlay], async (directory, enqueue) => {
    const entries = await fs.readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const child = path.join(directory, entry.name);
      const rel = path.relative(overlay, child).replaceAll("\\", "/");
      if (entry.isDirectory()) enqueue(child);
      else result.add(rel);
    }
  });
  return result;
}

export async function listMinecraftEntries(instance: string, subpath: string): Promise<MinecraftDirEntry[]> {
  const rel = sanitizeMinecraftRelPath(subpath);
  const dir = path.join(minecraftGameDir(instance), rel);
  if (!(await exists(dir))) return [];
  const stat = await fs.stat(dir);
  if (!stat.isDirectory()) throw new Error("not a directory");
  const persistent = await persistentPaths(instance);
  const entries: MinecraftDirEntry[] = await Promise.all(
    (await fs.readdir(dir, { withFileTypes: true })).map(async (entry) => {
      const entryRel = rel ? `${rel}/${entry.name}` : entry.name;
      const isDir = entry.isDirectory();
      const editable = !isDir && isPathEditable(entryRel);
      const stat = editable ? await fs.stat(path.join(dir, entry.name)).catch(() => null) : null;
      return {
        name: entry.name,
        rel_path: entryRel,
        is_dir: isDir,
        has_persistent_override: !isDir && persistent.has(entryRel),
        editable,
        too_large_to_edit: Boolean(stat?.isFile() && stat.size > maxReadBytes),
      };
    }),
  );
  return entries.sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

export async function readMinecraftFile(instance: string, relPath: string): Promise<string> {
  const rel = sanitizeMinecraftRelPath(relPath);
  if (isOverlayExcluded(rel)) throw new Error("path is not readable");
  if (!isPathEditable(rel)) throw new Error("file is not editable");
  const target = path.join(minecraftGameDir(instance), rel);
  const stat = await fs.stat(target).catch(() => null);
  if (!stat?.isFile()) throw new Error("file not found");
  if (stat.size > maxReadBytes) throw new Error("file too large to edit in launcher");
  return fs.readFile(target, "utf8");
}

export async function writeMinecraftFile(instance: string, relPath: string, content: string, persist: boolean): Promise<void> {
  const rel = sanitizeMinecraftRelPath(relPath);
  if (isOverlayExcluded(rel) || !isPathEditable(rel)) throw new Error("file type is not editable");
  const target = path.join(minecraftGameDir(instance), rel);
  await fs.mkdir(path.dirname(target), { recursive: true });
  await fs.writeFile(target, content, "utf8");
  if (persist) {
    const overlay = path.join(persistentMinecraftDir(instance), rel);
    await fs.mkdir(path.dirname(overlay), { recursive: true });
    await fs.writeFile(overlay, content, "utf8");
  }
}

export async function deletePersistentFile(instance: string, relPath: string): Promise<void> {
  const rel = sanitizeMinecraftRelPath(relPath);
  await stopExternalMinecraftFileWatch(instance, rel);
  await fs.rm(path.join(persistentMinecraftDir(instance), rel), { force: true });
}

export async function listPersistentFiles(instance: string): Promise<string[]> {
  return [...(await persistentPaths(instance))].sort();
}

export async function applyPersistentMinecraft(instance: string): Promise<void> {
  const overlay = persistentMinecraftDir(instance);
  const targetRoot = minecraftGameDir(instance);
  async function visit(dir: string): Promise<void> {
    for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
      const child = path.join(dir, entry.name);
      const rel = path.relative(overlay, child).replaceAll("\\", "/");
      if (entry.isDirectory()) {
        if (rel === "mods" || isOverlayExcluded(`${rel}/`)) continue;
        await visit(child);
      } else if (!isOverlayExcluded(rel)) {
        const target = path.join(targetRoot, rel);
        await fs.mkdir(path.dirname(target), { recursive: true });
        await fs.copyFile(child, target);
      }
    }
  }
  if (await exists(overlay)) await visit(overlay);
}
