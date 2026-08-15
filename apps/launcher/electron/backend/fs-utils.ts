import fs from "node:fs/promises";
import path from "node:path";

const DEFAULT_CONCURRENCY = 32;

function normalizeConcurrency(value: number): number {
  return Math.max(1, Math.floor(Number.isFinite(value) ? value : DEFAULT_CONCURRENCY));
}

export async function mapConcurrent<T, R>(
  items: readonly T[],
  worker: (item: T, index: number) => Promise<R>,
  concurrency = DEFAULT_CONCURRENCY,
): Promise<R[]> {
  if (items.length === 0) return [];

  const results = [] as R[];
  results.length = items.length;
  let nextIndex = 0;
  const workerCount = Math.min(items.length, normalizeConcurrency(concurrency));

  const runWorker = async (): Promise<void> => {
    while (true) {
      const index = nextIndex;
      nextIndex += 1;
      if (index >= items.length) return;
      results[index] = await worker(items[index], index);
    }
  };

  await Promise.all(Array.from({ length: workerCount }, () => runWorker()));
  return results;
}

export async function runConcurrent<T>(
  initialItems: readonly T[],
  worker: (item: T, enqueue: (item: T) => void) => Promise<void>,
  concurrency = DEFAULT_CONCURRENCY,
): Promise<void> {
  if (initialItems.length === 0) return;

  const queue = [...initialItems];
  const workerCount = normalizeConcurrency(concurrency);
  let nextIndex = 0;
  let pending = queue.length;
  let active = 0;
  let settled = false;

  await new Promise<void>((resolve, reject) => {
    const finish = () => {
      if (!settled && pending === 0 && active === 0) {
        settled = true;
        resolve();
      }
    };

    const fail = (error: unknown) => {
      if (settled) return;
      settled = true;
      reject(error);
    };

    const pump = () => {
      if (settled) return;
      while (active < workerCount && nextIndex < queue.length) {
        const item = queue[nextIndex];
        nextIndex += 1;
        active += 1;
        const enqueue = (child: T) => {
          if (settled) return;
          queue.push(child);
          pending += 1;
          pump();
        };
        void Promise.resolve()
          .then(() => worker(item, enqueue))
          .then(() => {
            active -= 1;
            pending -= 1;
            finish();
            pump();
          }, fail);
      }
      finish();
    };

    pump();
  });
}

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
  type SizeTask = { kind: "directory" | "file"; path: string };
  let total = 0;
  await runConcurrent<SizeTask>([{ kind: "directory", path: target }], async (task, enqueue) => {
    if (task.kind === "directory") {
      const entries = await fs.readdir(task.path, { withFileTypes: true }).catch(() => []);
      for (const entry of entries) {
        const child = path.join(task.path, entry.name);
        enqueue({ kind: entry.isDirectory() ? "directory" : "file", path: child });
      }
      return;
    }
    try {
      total += (await fs.stat(task.path)).size;
    } catch {
      // Files that disappear while sizing do not contribute to the total.
    }
  });
  return total;
}

export async function copyTree(source: string, destination: string): Promise<void> {
  await runConcurrent([{ source, destination }], async (current, enqueue) => {
    const stat = await fs.lstat(current.source);
    if (stat.isSymbolicLink()) return;
    if (stat.isDirectory()) {
      await fs.mkdir(current.destination, { recursive: true });
      const entries = await fs.readdir(current.source, { withFileTypes: true });
      for (const entry of entries) {
        enqueue({
          source: path.join(current.source, entry.name),
          destination: path.join(current.destination, entry.name),
        });
      }
      return;
    }
    await fs.mkdir(path.dirname(current.destination), { recursive: true });
    await fs.copyFile(current.source, current.destination);
  });
}

export async function removeIfExists(target: string): Promise<void> {
  await fs.rm(target, { recursive: true, force: true });
}

export async function listFiles(target: string): Promise<string[]> {
  const result: string[] = [];
  if (!(await exists(target))) return result;
  await runConcurrent([target], async (directory, enqueue) => {
    const entries = await fs.readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const child = path.join(directory, entry.name);
      if (entry.isDirectory()) enqueue(child);
      else result.push(child);
    }
  });
  return result;
}
