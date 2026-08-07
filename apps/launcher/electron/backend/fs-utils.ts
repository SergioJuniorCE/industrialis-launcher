import fs from "node:fs/promises";
import path from "node:path";

export async function exists(target: string): Promise<boolean> {
  try {
    await fs.access(target);
    return true;
  } catch {
    return false;
  }
}

export async function readJson<T>(target: string): Promise<T | null> {
  try {
    return JSON.parse(await fs.readFile(target, "utf8")) as T;
  } catch {
    return null;
  }
}

export async function writeJson(target: string, value: unknown): Promise<void> {
  await fs.mkdir(path.dirname(target), { recursive: true });
  await fs.writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export async function dirSize(target: string): Promise<number> {
  let total = 0;
  let entries;
  try {
    entries = await fs.readdir(target, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const child = path.join(target, entry.name);
    if (entry.isDirectory()) total += await dirSize(child);
    else {
      try { total += (await fs.stat(child)).size; } catch { /* file disappeared */ }
    }
  }
  return total;
}

export async function copyTree(source: string, destination: string): Promise<void> {
  const stat = await fs.stat(source);
  if (stat.isDirectory()) {
    await fs.mkdir(destination, { recursive: true });
    for (const entry of await fs.readdir(source, { withFileTypes: true })) {
      await copyTree(path.join(source, entry.name), path.join(destination, entry.name));
    }
    return;
  }
  await fs.mkdir(path.dirname(destination), { recursive: true });
  await fs.copyFile(source, destination);
}

export async function removeIfExists(target: string): Promise<void> {
  await fs.rm(target, { recursive: true, force: true });
}

export async function listFiles(target: string): Promise<string[]> {
  const result: string[] = [];
  async function visit(dir: string): Promise<void> {
    for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
      const child = path.join(dir, entry.name);
      if (entry.isDirectory()) await visit(child);
      else result.push(child);
    }
  }
  if (await exists(target)) await visit(target);
  return result;
}

export function parseNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
