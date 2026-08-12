import fs from "node:fs/promises";
import path from "node:path";
import { copyTree, exists, readJson, removeIfExists, writeJson } from "./fs-utils";
import { packCacheRoot } from "./paths";

const ttlSeconds = 30 * 24 * 60 * 60;

function sanitizeSegment(value: string): string {
  return [...value].map((char) => (/[\s/\\:*?"<>|]/u.test(char) ? "_" : char)).join("");
}

export function packCacheKey(version: string, javaType: string): string {
  return `${sanitizeSegment(version)}__${sanitizeSegment(javaType)}`;
}

interface CacheMeta {
  pack_version: string;
  java_type: string;
  download_url: string;
  cached_at_secs: number;
  last_used_at_secs: number;
}

function entryDir(key: string): string {
  return path.join(packCacheRoot(), key);
}
function now(): number {
  return Math.floor(Date.now() / 1000);
}

async function valid(entry: string, url: string): Promise<boolean> {
  const meta = await readJson<CacheMeta>(path.join(entry, "meta.json"));
  return Boolean(meta && meta.download_url === url && now() - meta.cached_at_secs <= ttlSeconds && (await exists(path.join(entry, "pack"))));
}

export async function evictExpiredPackCache(): Promise<void> {
  if (!(await exists(packCacheRoot()))) return;
  const entries = await fs.readdir(packCacheRoot(), { withFileTypes: true });
  await Promise.all(
    entries.map(async (entry) => {
      if (!entry.isDirectory()) return;
      const target = path.join(packCacheRoot(), entry.name);
      const meta = await readJson<CacheMeta>(path.join(target, "meta.json"));
      if (!meta || now() - meta.cached_at_secs > ttlSeconds) await removeIfExists(target);
    }),
  );
}

export async function lookupPackCache(version: string, javaType: string, url: string): Promise<string | null> {
  await evictExpiredPackCache();
  const entry = entryDir(packCacheKey(version, javaType));
  if (!(await valid(entry, url))) {
    if (await exists(entry)) await removeIfExists(entry);
    return null;
  }
  const meta = await readJson<CacheMeta>(path.join(entry, "meta.json"));
  if (meta) {
    meta.last_used_at_secs = now();
    await writeJson(path.join(entry, "meta.json"), meta);
  }
  return path.join(entry, "pack");
}

export async function storePackCache(version: string, javaType: string, url: string, staging: string): Promise<void> {
  if (!(await exists(staging))) throw new Error("cannot cache pack: staging directory missing");
  const entry = entryDir(packCacheKey(version, javaType));
  if (await valid(entry, url)) return;
  const temp = `${entry}.tmp`;
  await removeIfExists(temp);
  await copyTree(staging, path.join(temp, "pack"));
  const timestamp = now();
  await writeJson(path.join(temp, "meta.json"), {
    pack_version: version,
    java_type: javaType,
    download_url: url,
    cached_at_secs: timestamp,
    last_used_at_secs: timestamp,
  } satisfies CacheMeta);
  await removeIfExists(entry);
  await fs.mkdir(path.dirname(entry), { recursive: true });
  await fs.rename(temp, entry);
}

export async function copyCachedPackToStaging(cachePack: string, staging: string): Promise<void> {
  await removeIfExists(staging);
  await copyTree(cachePack, staging);
}
