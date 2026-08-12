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
  let entries;
  try {
    entries = await fs.readdir(target, { withFileTypes: true });
  } catch {
    return 0;
  }
  const sizes = await Promise.all(
    entries.map(async (entry) => {
      const child = path.join(target, entry.name);
      if (entry.isDirectory()) return dirSize(child);
      try {
        return (await fs.stat(child)).size;
      } catch {
        return 0;
      }
    }),
  );
  return sizes.reduce((sum, size) => sum + size, 0);
}

export async function copyTree(source: string, destination: string): Promise<void> {
  const stat = await fs.stat(source);
  if (stat.isDirectory()) {
    await fs.mkdir(destination, { recursive: true });
    const entries = await fs.readdir(source, { withFileTypes: true });
    await Promise.all(entries.map((entry) => copyTree(path.join(source, entry.name), path.join(destination, entry.name))));
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
    const entries = await fs.readdir(dir, { withFileTypes: true });
    await Promise.all(
      entries.map(async (entry) => {
        const child = path.join(dir, entry.name);
        if (entry.isDirectory()) await visit(child);
        else result.push(child);
      }),
    );
  }
  if (await exists(target)) await visit(target);
  return result;
}
