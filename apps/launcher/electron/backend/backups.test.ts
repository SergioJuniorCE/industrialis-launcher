// @vitest-environment node

import fs from "node:fs/promises";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackupStore, DownloadOptions, RemoteObject, UploadOptions, UploadSource } from "./backup-types";

const electronState = vi.hoisted(() => ({ appData: "" }));

vi.mock("electron", () => ({
  app: { getPath: () => electronState.appData },
}));

import { BackupService } from "./backups";
import { instanceBackupsDir } from "./paths";

class MemoryBackupStore implements BackupStore {
  readonly objects = new Map<string, Uint8Array>();
  readonly uploads: string[] = [];

  async list(prefix: string): Promise<RemoteObject[]> {
    return [...this.objects.entries()]
      .filter(([key]) => key.startsWith(prefix))
      .map(([key, data]) => ({
        key,
        size_bytes: data.byteLength,
        content_type: key.endsWith("manifest.json") ? "application/json" : "application/octet-stream",
      }));
  }

  async upload(key: string, source: UploadSource, options?: UploadOptions): Promise<RemoteObject> {
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
  tempRoot = await fs.mkdtemp(path.join(process.env.TEMP ?? process.env.TMP ?? ".", "industrialis-backups-"));
  electronState.appData = tempRoot;
});

afterEach(async () => {
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("BackupService", () => {
  it("ignores partial local files and writes the manifest after the archive", async () => {
    const directory = instanceBackupsDir("alpha");
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, "world.zip"), "minecraft world");
    await fs.writeFile(path.join(directory, "world.zip.part"), "partial");
    const service = new BackupService();
    const store = new MemoryBackupStore();

    await expect(service.listLocalBackups("alpha")).resolves.toEqual([expect.objectContaining({ file_name: "world.zip", size_bytes: 15 })]);
    const uploaded = await service.uploadLocalBackup("alpha", "world.zip", store);

    expect(uploaded.file_name).toBe("world.zip");
    expect(store.uploads).toHaveLength(2);
    expect(store.uploads[0]).toContain("/artifacts/world.zip");
    expect(store.uploads[1]).toMatch(/\/manifest\.json$/u);
  });

  it("lists, verifies, restores, and deletes a provider-neutral snapshot", async () => {
    const directory = instanceBackupsDir("alpha");
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, "world.zip"), "minecraft world");
    const service = new BackupService();
    const store = new MemoryBackupStore();
    const snapshot = await service.uploadLocalBackup("alpha", "world.zip", store);

    await expect(service.listRemoteBackups("alpha", store)).resolves.toEqual([expect.objectContaining({ snapshot_id: snapshot.snapshot_id })]);
    const restored = await service.downloadRemoteBackup("alpha", snapshot.snapshot_id, store);
    expect(restored.file_name).toBe("world-cloud-1.zip");
    await expect(fs.readFile(path.join(directory, restored.file_name), "utf8")).resolves.toBe("minecraft world");

    await service.deleteRemoteBackup("alpha", snapshot.snapshot_id, store);
    await expect(service.listRemoteBackups("alpha", store)).resolves.toEqual([]);
  });
});
