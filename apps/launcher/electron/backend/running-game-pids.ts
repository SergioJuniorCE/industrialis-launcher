import fs from "node:fs/promises";
import { readJson, writeJson } from "./fs-utils";
import { runningGamePidsPath, validateInstanceId } from "./paths";

export interface RunningGamePid {
  pid: number;
  creationId: string;
}

export type RunningGamePids = ReadonlyMap<string, RunningGamePid>;

function isPid(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

function isCreationId(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

export async function loadRunningGamePids(): Promise<Map<string, RunningGamePid>> {
  const saved = await readJson<Record<string, unknown>>(runningGamePidsPath());
  if (!saved || typeof saved !== "object" || Array.isArray(saved)) return new Map();

  const pids = new Map<string, RunningGamePid>();
  for (const [rawId, value] of Object.entries(saved)) {
    if (!value || typeof value !== "object" || Array.isArray(value)) continue;
    const process = value as { pid?: unknown; creation_id?: unknown };
    if (!isPid(process.pid) || !isCreationId(process.creation_id)) continue;
    try {
      const id = validateInstanceId(rawId);
      if (id === rawId) pids.set(id, { pid: process.pid, creationId: process.creation_id });
    } catch {
      // Ignore malformed state left by an interrupted or older launcher run.
    }
  }
  return pids;
}

export async function saveRunningGamePids(pids: RunningGamePids): Promise<void> {
  const saved = Object.fromEntries(
    [...pids]
      .filter(([id, process]) => id.length > 0 && isPid(process.pid) && isCreationId(process.creationId))
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([id, process]) => [id, { pid: process.pid, creation_id: process.creationId }] as const),
  );
  const statePath = runningGamePidsPath();
  if (Object.keys(saved).length === 0) {
    await fs.rm(statePath, { force: true });
    return;
  }
  await writeJson(statePath, saved);
}
