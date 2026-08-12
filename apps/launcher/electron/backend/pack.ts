import fs from "node:fs/promises";
import path from "node:path";
import { unzipSync } from "fflate";
import { copyTree, exists, mapConcurrent, readJson } from "./fs-utils";
import { lookupPackCache, copyCachedPackToStaging, storePackCache } from "./pack-cache";
import type { GtnhVersion, ModEntry, ModPreviewEntry, UpdateModPreview } from "./types";

export const versionsUrl = "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json";

export type EmitProgress = (payload: Record<string, unknown>) => void;

function progress(emit: EmitProgress, stage: string, pct: number, operation: string, id?: string, extra?: Record<string, unknown>): void {
  emit({ stage, pct, operation, ...(id ? { id } : {}), ...extra });
}

export async function fetchGtnhVersions(): Promise<Record<string, GtnhVersion>> {
  const response = await fetch(versionsUrl);
  if (!response.ok) throw new Error(`failed to fetch pack versions: HTTP ${response.status}`);
  return (await response.json()) as Record<string, GtnhVersion>;
}

export function resolvePackDownloadUrl(version: GtnhVersion, javaType: string): string {
  return javaType === "java8" ? version.mmc.java8Url : version.mmc.java17_2XUrl;
}

export function modIdentityFromFilename(filename: string): string {
  let name = filename.toLowerCase().replace(/\.(jar|zip)$/u, "");
  for (const suffix of ["-client", "-universal", "-dev", "-sources"]) {
    if (name.endsWith(suffix)) name = name.slice(0, -suffix.length);
  }
  const looksLikeVersion = (segment: string): boolean => {
    if (!segment) return false;
    if (/^v\d/u.test(segment) || /^rv.+/u.test(segment)) return true;
    if (/(beta|alpha|snapshot|pre)/u.test(segment)) return true;
    return /^\d/u.test(segment);
  };
  const parts = name.split(/[-_]/u);
  while (parts.length > 1 && looksLikeVersion(parts.at(-1) ?? "")) parts.pop();
  return parts.join("-");
}

export async function listMods(dir: string): Promise<ModEntry[]> {
  if (!(await exists(dir))) return [];
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const mods = (
    await mapConcurrent(entries, async (entry) => {
      if (!entry.isFile() || !/\.(jar|zip)$/iu.test(entry.name)) return null;
      const stat = await fs.stat(path.join(dir, entry.name));
      return { filename: entry.name, identity: modIdentityFromFilename(entry.name), size_bytes: stat.size };
    })
  ).filter((mod): mod is ModEntry => mod !== null);
  return mods.sort((a, b) => a.identity.localeCompare(b.identity));
}

export function classifyModUpdates(oldMods: ModEntry[], newMods: ModEntry[]): { updated: number; added: number; removed: number } {
  const oldIds = new Set(oldMods.map((mod) => mod.identity));
  const newIds = new Set(newMods.map((mod) => mod.identity));
  return {
    updated: oldMods.filter((mod) => newIds.has(mod.identity)).length,
    added: newMods.filter((mod) => !oldIds.has(mod.identity)).length,
    removed: oldMods.filter((mod) => !newIds.has(mod.identity)).length,
  };
}

export function persistentCustomModsDir(instance: string): string {
  return path.join(instance, "persistent-minecraft", "mods");
}

export async function listCustomMods(instance: string): Promise<ModEntry[]> {
  return listMods(persistentCustomModsDir(instance));
}

export async function addCustomMod(instance: string, source: string): Promise<ModEntry> {
  const filename = path.basename(source);
  if (!/\.(jar|zip)$/iu.test(filename)) throw new Error("only .jar and .zip mods are supported");
  if (!(await exists(source))) throw new Error("mod file not found");
  const persistent = persistentCustomModsDir(instance);
  const active = path.join(instance, ".minecraft", "mods");
  await fs.mkdir(persistent, { recursive: true });
  await fs.mkdir(active, { recursive: true });
  await fs.copyFile(source, path.join(persistent, filename));
  await fs.copyFile(source, path.join(active, filename));
  const size = (await fs.stat(path.join(persistent, filename))).size;
  return { filename, identity: modIdentityFromFilename(filename), size_bytes: size };
}

export async function removeCustomMod(instance: string, identity: string): Promise<void> {
  const normalized = identity.trim().toLowerCase();
  if (!normalized) throw new Error("mod identity is required");
  let removed = 0;
  for (const dir of [persistentCustomModsDir(instance), path.join(instance, ".minecraft", "mods")]) {
    for (const mod of await listMods(dir)) {
      if (mod.identity === normalized) {
        await fs.rm(path.join(dir, mod.filename), { force: true });
        removed += 1;
      }
    }
  }
  if (!removed) throw new Error("custom mod not found");
}

export async function removeCustomModsExcept(dir: string, keep: Set<string>): Promise<number> {
  let removed = 0;
  for (const mod of await listMods(dir)) {
    if (!keep.has(mod.identity)) {
      await fs.rm(path.join(dir, mod.filename), { force: true });
      removed += 1;
    }
  }
  return removed;
}

export async function restorePersistentCustomMods(persistent: string, active: string): Promise<number> {
  if (!(await exists(persistent))) return 0;
  await fs.mkdir(active, { recursive: true });
  let restored = 0;
  for (const entry of await fs.readdir(persistent, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    await fs.copyFile(path.join(persistent, entry.name), path.join(active, entry.name));
    restored += 1;
  }
  return restored;
}

export async function applyPersistentCustomMods(instance: string): Promise<number> {
  return restorePersistentCustomMods(persistentCustomModsDir(instance), path.join(instance, ".minecraft", "mods"));
}

async function downloadFile(emit: EmitProgress, url: string, destination: string, operation: string, id?: string): Promise<void> {
  progress(emit, "downloading", 0, operation, id, { log_line: "Downloading pack archive…" });
  const response = await fetch(url);
  if (!response.ok || !response.body) throw new Error(`pack download failed: HTTP ${response.status}`);
  const total = Number(response.headers.get("content-length") ?? 0);
  const file = await fs.open(destination, "w");
  const reader = response.body.getReader();
  let downloaded = 0;
  const started = Date.now();
  let lastEmit = 0;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      await file.write(chunk.value);
      downloaded += chunk.value.byteLength;
      const now = Date.now();
      const pct = total > 0 ? downloaded / total : 0;
      if (now - lastEmit >= 500 || Math.floor(pct * 100) % 5 === 0) {
        const elapsed = Math.max((now - started) / 1000, 0.05);
        progress(emit, "downloading", pct, operation, id, {
          speed_mbps: downloaded / elapsed / 1024 / 1024,
          downloaded_mb: downloaded / 1024 / 1024,
          ...(total > 0 ? { total_mb: total / 1024 / 1024 } : {}),
        });
        lastEmit = now;
      }
    }
  } finally {
    await file.close();
  }
  const elapsed = Math.max((Date.now() - started) / 1000, 0.05);
  progress(emit, "downloading", 1, operation, id, {
    log_line: `Download complete (${(downloaded / 1024 / 1024).toFixed(1)} MB, avg ${(downloaded / elapsed / 1024 / 1024).toFixed(1)} MB/s)`,
    speed_mbps: downloaded / elapsed / 1024 / 1024,
    downloaded_mb: downloaded / 1024 / 1024,
    ...(total > 0 ? { total_mb: total / 1024 / 1024 } : {}),
  });
}

export async function extractPackZip(emit: EmitProgress, zipPath: string, destination: string, operation: string, id?: string): Promise<void> {
  progress(emit, "extracting", 0, operation, id, { log_line: "Extracting pack archive…" });
  const archive = unzipSync(new Uint8Array(await fs.readFile(zipPath)));
  const entries = Object.entries(archive);
  for (let index = 0; index < entries.length; index += 1) {
    const [entryName, contents] = entries[index];
    const normalized = entryName.replaceAll("\\", "/");
    if (normalized.split("/").some((part) => part === "..") || normalized.startsWith("/")) continue;
    const target = path.join(destination, ...normalized.split("/"));
    if (entryName.endsWith("/")) await fs.mkdir(target, { recursive: true });
    else {
      await fs.mkdir(path.dirname(target), { recursive: true });
      await fs.writeFile(target, contents);
    }
    const pct = (index + 1) / Math.max(entries.length, 1);
    if (Math.floor(pct * 100) % 10 === 0) progress(emit, "extracting", pct, operation, id);
  }
  progress(emit, "extracting", 1, operation, id, operation === "preview" ? { log_line: `Extracted ${entries.length} files` } : undefined);
}

async function moveInto(destinationRoot: string, source: string): Promise<void> {
  const destination = path.join(destinationRoot, path.basename(source));
  if (!(await exists(source))) return;
  if (await exists(destination)) {
    const srcStat = await fs.stat(source);
    const dstStat = await fs.stat(destination);
    if (srcStat.isDirectory() && dstStat.isDirectory()) {
      const entries = await fs.readdir(source);
      await mapConcurrent(entries, (entry) => moveInto(destination, path.join(source, entry)));
      await fs.rm(source, { recursive: true, force: true });
      return;
    }
    await fs.rm(destination, { recursive: true, force: true });
  }
  await fs.mkdir(path.dirname(destination), { recursive: true });
  try {
    await fs.rename(source, destination);
  } catch {
    await copyTree(source, destination);
    await fs.rm(source, { recursive: true, force: true });
  }
}

export async function flattenNestedPack(instance: string): Promise<void> {
  if (await exists(path.join(instance, "mmc-pack.json"))) return;
  const candidates = (await fs.readdir(instance, { withFileTypes: true }).catch(() => [])).reduce<string[]>((result, entry) => {
    if (entry.isDirectory()) result.push(path.join(instance, entry.name));
    return result;
  }, []);
  const nested = (await mapConcurrent(candidates, async (candidate) => ((await exists(path.join(candidate, "mmc-pack.json"))) ? candidate : null))).filter(
    (candidate): candidate is string => candidate !== null,
  );
  const source = nested.sort()[0];
  if (!source) return;
  const entries = await fs.readdir(source);
  await mapConcurrent(entries, (entry) => moveInto(instance, path.join(source, entry)));
  await fs.rm(source, { recursive: true, force: true });
  if (!(await exists(path.join(instance, "mmc-pack.json")))) throw new Error(`failed to flatten instance pack in ${instance}`);
}

export async function seedPackConfigs(instance: string, overwrite: boolean): Promise<void> {
  const source = path.join(instance, "config");
  if (!(await exists(source))) return;
  const destination = path.join(instance, ".minecraft", "config");
  async function visit(from: string, to: string): Promise<void> {
    await fs.mkdir(to, { recursive: true });
    for (const entry of await fs.readdir(from, { withFileTypes: true })) {
      const sourcePath = path.join(from, entry.name);
      const destPath = path.join(to, entry.name);
      if (entry.isDirectory()) await visit(sourcePath, destPath);
      else if (overwrite || !(await exists(destPath))) await fs.copyFile(sourcePath, destPath);
    }
  }
  await visit(source, destination);
}

export async function prepareInstanceConfigs(instance: string, overwrite: boolean): Promise<void> {
  await seedPackConfigs(instance, overwrite);
  for (const rel of ["config/gadomancy.cfg", ".minecraft/config/gadomancy.cfg"]) {
    const target = path.join(instance, rel);
    if (await exists(target)) {
      const content = await fs.readFile(target, "utf8");
      await fs.writeFile(target, content.replaceAll("B:ancientStoneRecipes=false", "B:ancientStoneRecipes=true"), "utf8");
    }
  }
}

export async function resolveModsDir(instance: string): Promise<string> {
  const direct = path.join(instance, ".minecraft", "mods");
  if (await exists(direct)) return direct;
  for (const entry of await fs.readdir(instance, { withFileTypes: true }).catch(() => [])) {
    if (!entry.isDirectory()) continue;
    const nested = path.join(instance, entry.name, ".minecraft", "mods");
    if (await exists(nested)) return nested;
  }
  return direct;
}

export async function installStagingContents(staging: string, instance: string): Promise<void> {
  for (const entry of await fs.readdir(staging, { withFileTypes: true })) {
    const target = path.join(instance, entry.name);
    await fs.rm(target, { recursive: true, force: true });
    await copyTree(path.join(staging, entry.name), target);
  }
}

export async function downloadAndExtractToStaging(
  emit: EmitProgress,
  packVersion: string,
  javaType: string,
  stagingParent: string,
  operation: string,
  id?: string,
): Promise<string> {
  const versions = await fetchGtnhVersions();
  const version = versions[packVersion];
  if (!version) throw new Error("pack version not found");
  const url = resolvePackDownloadUrl(version, javaType);
  const staging = path.join(stagingParent, "staging");
  const cached = await lookupPackCache(packVersion, javaType, url);
  if (cached) {
    progress(emit, "cached", 1, operation, id, { log_line: `Using cached pack ${packVersion} (${javaType}) — skipping download` });
    await copyCachedPackToStaging(cached, staging);
    progress(emit, "extracting", 1, operation, id, { log_line: "Cached pack ready" });
    if (operation === "update-pack") progress(emit, "updating", 0.55, operation, id, { log_line: "Pack ready from cache" });
    return staging;
  }
  const zipPath = path.join(stagingParent, "pack.zip");
  await fs.rm(staging, { recursive: true, force: true });
  await fs.mkdir(staging, { recursive: true });
  await downloadFile(emit, url, zipPath, operation, id);
  await extractPackZip(emit, zipPath, staging, operation, id);
  await flattenNestedPack(staging);
  await storePackCache(packVersion, javaType, url, staging);
  if (operation === "update-pack") progress(emit, "updating", 0.55, operation, id, { log_line: "Pack downloaded and extracted to staging" });
  await fs.rm(zipPath, { force: true });
  return staging;
}

export async function buildUpdatePreview(instance: string, targetVersion: string, javaType: string, emit: EmitProgress): Promise<UpdateModPreview> {
  const settings = await readJson<{ pack_version?: string }>(path.join(instance, "instance.json"));
  if (!settings) throw new Error("instance settings are invalid");
  const currentVersion = settings.pack_version ?? "";
  const oldMods = await listMods(await resolveModsDir(instance));
  const stagingParent = path.join(instance, ".update-preview");
  const staging = await downloadAndExtractToStaging(emit, targetVersion, javaType, stagingParent, "preview", path.basename(instance));
  const newMods = await listMods(await resolveModsDir(staging));
  const classification = classifyModUpdates(oldMods, newMods);
  const customMods: ModPreviewEntry[] = (await listCustomMods(instance)).map((mod) => ({ ...mod, in_persistent_overlay: true }));
  return {
    current_pack_version: currentVersion,
    target_pack_version: targetVersion,
    custom_mods: customMods,
    new_pack_mods_count: classification.added,
    updated_pack_mods_count: classification.updated,
    removed_from_pack_count: classification.removed,
  };
}
