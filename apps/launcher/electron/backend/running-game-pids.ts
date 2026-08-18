import fs from "node:fs/promises";
import { readJson, writeJson } from "./fs-utils";
import { runningGamePidsPath, validateInstanceId } from "./paths";

export type RunningGamePids = ReadonlyMap<string, number>;

function isPid(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

export async function loadRunningGamePids(): Promise<Map<string, number>> {
  const saved = await readJson<Record<string, unknown>>(runningGamePidsPath());
  if (!saved || typeof saved !== "object" || Array.isArray(saved)) return new Map();

  const pids = new Map<string, number>();
  for (const [rawId, value] of Object.entries(saved)) {
    if (!isPid(value)) continue;
    try {
      const id = validateInstanceId(rawId);
      if (id === rawId) pids.set(id, value);
    } catch {
      // Ignore malformed state left by an interrupted or older launcher run.
    }
  }
  return pids;
}

export async function saveRunningGamePids(pids: RunningGamePids): Promise<void> {
  const saved = Object.fromEntries([...pids].filter(([id, pid]) => id.length > 0 && isPid(pid)).sort(([left], [right]) => left.localeCompare(right)));
  const statePath = runningGamePidsPath();
  if (Object.keys(saved).length === 0) {
    await fs.rm(statePath, { force: true });
    return;
  }
  await writeJson(statePath, saved);
}
