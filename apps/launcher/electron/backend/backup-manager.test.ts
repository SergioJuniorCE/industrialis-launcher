// @vitest-environment node

import fs from "node:fs/promises";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackupStore, DownloadOptions, RemoteObject, UploadOptions, UploadSource } from "./backup-types";

const electronState = vi.hoisted(() => ({ appData: "" }));

vi.mock("electron", () => ({
  app: { getPath: () => electronState.appData },
}));

import { BackupManager } from "./backup-manager";
import { BackupService } from "./backups";
import { instanceBackupsDir } from "./paths";

class MemoryBackupStore implements BackupStore {
  readonly objects = new Map<string, Uint8Array>();
  readonly uploads: string[] = [];
  failuresRemaining = 0;

  async list(prefix: string): Promise<RemoteObject[]> {
    return [...this.objects.entries()].filter(([key]) => key.startsWith(prefix)).map(([key, data]) => ({ key, size_bytes: data.byteLength }));
  }

  async upload(key: string, source: UploadSource, options?: UploadOptions): Promise<RemoteObject> {
    if (this.failuresRemaining > 0) {
      this.failuresRemaining -= 1;
      throw new Error("temporary provider failure");
    }
    const data = source.kind === "file" ? new Uint8Array(await fs.readFile(source.path)) : source.data;
    this.objects.set(key, new Uint8Array(data));
    this.uploads.push(key);
    options?.on_progress?.({ completed_bytes: data.byteLength, total_bytes: data.byteLength });
    return { key, size_bytes: data.byteLength };
  }

  async download(key: string, destination: string, _options?: DownloadOptions): Promise<void> {
    const data = this.objects.get(key);
    if (!data) throw new Error(`missing object: ${key}`);
    await fs.mkdir(path.dirname(destination), { recursive: true });
    await fs.writeFile(destination, data);
  }

  async delete(key: string): Promise<void> {
    this.objects.delete(key);
  }
}

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(process.env.TEMP ?? process.env.TMP ?? ".", "industrialis-backup-manager-"));
  electronState.appData = tempRoot;
});

afterEach(async () => {
  await fs.rm(tempRoot, { recursive: true, force: true });
});

function createManager(store: MemoryBackupStore, now: () => Date, retentionLimit: number | (() => number) = 10, stabilityMs = 60_000): BackupManager {
  return new BackupManager({
    service: new BackupService(),
    providers: [
      {
        id: "memory",
        label: "Memory",
        store,
        getStatus: async () => ({ configured: true, connected: true }),
      },
    ],
    listInstances: async () => [
      { instance_id: "alpha", enabled: true, retention_limit: typeof retentionLimit === "function" ? retentionLimit() : retentionLimit },
    ],
    statePath: path.join(tempRoot, "manager-state.json"),
    emit: () => undefined,
    now,
    stabilityMs,
  });
}

async function writeBackup(fileName: string, contents: string, modifiedAt: Date): Promise<void> {
  const directory = instanceBackupsDir("alpha");
  await fs.mkdir(directory, { recursive: true });
  const target = path.join(directory, fileName);
  await fs.writeFile(target, contents);
  await fs.utimes(target, modifiedAt, modifiedAt);
}

describe("BackupManager", () => {
  it("waits for a file to remain unchanged before uploading it automatically", async () => {
    let currentTime = new Date("2026-08-22T12:00:00.000Z");
    const store = new MemoryBackupStore();
    const manager = createManager(store, () => currentTime);
    await writeBackup("world.zip", "minecraft world", currentTime);

    await manager.runOnce();
    expect(store.uploads).toEqual([]);

    currentTime = new Date(currentTime.getTime() + 60_000);
    await manager.runOnce();
    expect(store.uploads).toHaveLength(2);
    expect(store.uploads[1]).toMatch(/\/manifest\.json$/u);
    await expect(manager.getDashboard()).resolves.toMatchObject({
      providers: [{ status: "healthy", pending_count: 0, failed_count: 0 }],
      instances: [{ instance_id: "alpha", status: "healthy" }],
    });
  });

  it("backfills only the newest retention window of distinct content hashes", async () => {
    const currentTime = new Date("2026-08-22T12:00:00.000Z");
    const store = new MemoryBackupStore();
    const manager = createManager(store, () => currentTime, 10, 0);
    for (let index = 0; index < 12; index += 1) {
      await writeBackup(`world-${index}.zip`, index === 11 ? "world-10" : `world-${index}`, new Date(currentTime.getTime() - index * 1_000));
    }

    await manager.runOnce();

    expect(store.uploads.filter((key) => key.endsWith("/manifest.json"))).toHaveLength(10);
    await expect(manager.listSnapshots("alpha")).resolves.toHaveLength(10);
    await expect(new BackupService().listLocalBackups("alpha")).resolves.toHaveLength(12);
  });

  it("persists a failed delivery and retries it with exponential backoff", async () => {
    let currentTime = new Date("2026-08-22T12:00:00.000Z");
    const store = new MemoryBackupStore();
    store.failuresRemaining = 1;
    const manager = createManager(store, () => currentTime, 10, 0);
    await writeBackup("world.zip", "minecraft world", new Date(currentTime.getTime() - 1_000));

    await manager.runOnce();
    await expect(manager.getDashboard()).resolves.toMatchObject({
      providers: [{ status: "pending-retry", pending_count: 1 }],
    });

    currentTime = new Date(currentTime.getTime() + 60_000);
    await manager.runOnce();
    await expect(manager.getDashboard()).resolves.toMatchObject({
      providers: [{ status: "healthy", pending_count: 0 }],
    });
  });

  it("applies a changed retention limit without deleting local backup files", async () => {
    const currentTime = new Date("2026-08-22T12:00:00.000Z");
    let retentionLimit = 3;
    const store = new MemoryBackupStore();
    const manager = createManager(
      store,
      () => currentTime,
      () => retentionLimit,
      0,
    );
    for (let index = 0; index < 3; index += 1) {
      await writeBackup(`world-${index}.zip`, `world-${index}`, new Date(currentTime.getTime() - index * 1_000));
    }
    await manager.runOnce();
    await expect(manager.listSnapshots("alpha")).resolves.toHaveLength(3);

    retentionLimit = 1;
    await manager.wake();

    await expect(manager.listSnapshots("alpha")).resolves.toHaveLength(1);
    await expect(new BackupService().listLocalBackups("alpha")).resolves.toHaveLength(3);
  });
});
