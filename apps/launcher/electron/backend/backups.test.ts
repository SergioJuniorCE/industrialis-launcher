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
  readonly downloadFailures = new Set<string>();

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
    if (this.downloadFailures.has(key)) throw new Error(`download failed: ${key}`);
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

  it("skips malformed manifests while listing valid snapshots", async () => {
    const directory = instanceBackupsDir("alpha");
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, "first.zip"), "first world");
    await fs.writeFile(path.join(directory, "second.zip"), "second world");
    const service = new BackupService();
    const store = new MemoryBackupStore();
    const first = await service.uploadLocalBackup("alpha", "first.zip", store);
    const second = await service.uploadLocalBackup("alpha", "second.zip", store);
    const malformedKey = [...store.objects.keys()].find((key) => key.includes(first.snapshot_id) && key.endsWith("/manifest.json"));
    expect(malformedKey).toBeDefined();
    store.objects.set(malformedKey!, new TextEncoder().encode("{"));

    await expect(service.listRemoteBackups("alpha", store)).resolves.toEqual([expect.objectContaining({ snapshot_id: second.snapshot_id })]);
  });

  it("isolates one manifest download failure but reports a provider-wide failure", async () => {
    const directory = instanceBackupsDir("alpha");
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, "first.zip"), "first world");
    await fs.writeFile(path.join(directory, "second.zip"), "second world");
    const service = new BackupService();
    const store = new MemoryBackupStore();
    const first = await service.uploadLocalBackup("alpha", "first.zip", store);
    const second = await service.uploadLocalBackup("alpha", "second.zip", store);
    const manifestKeys = [...store.objects.keys()].filter((key) => key.endsWith("/manifest.json"));
    const firstManifestKey = manifestKeys.find((key) => key.includes(first.snapshot_id));
    const secondManifestKey = manifestKeys.find((key) => key.includes(second.snapshot_id));
    expect(firstManifestKey).toBeDefined();
    expect(secondManifestKey).toBeDefined();
    store.downloadFailures.add(firstManifestKey!);

    await expect(service.listRemoteBackups("alpha", store)).resolves.toEqual([expect.objectContaining({ snapshot_id: second.snapshot_id })]);

    store.downloadFailures.add(secondManifestKey!);
    await expect(service.listRemoteBackups("alpha", store)).rejects.toThrow("download failed");
  });

  it("rejects remote archive filenames that escape the backups directory", async () => {
    const directory = instanceBackupsDir("alpha");
    await fs.mkdir(directory, { recursive: true });
    await fs.writeFile(path.join(directory, "world.zip"), "minecraft world");
    const service = new BackupService();
    const store = new MemoryBackupStore();
    const snapshot = await service.uploadLocalBackup("alpha", "world.zip", store);
    const manifestKey = [...store.objects.keys()].find((key) => key.endsWith("/manifest.json"));
    expect(manifestKey).toBeDefined();
    const manifest = JSON.parse(new TextDecoder().decode(store.objects.get(manifestKey!))) as { artifacts: Array<{ file_name: string }> };
    manifest.artifacts[0]!.file_name = "../escaped.zip";
    store.objects.set(manifestKey!, new TextEncoder().encode(JSON.stringify(manifest)));

    await expect(service.downloadRemoteBackup("alpha", snapshot.snapshot_id, store)).rejects.toThrow("invalid backup manifest");
    await expect(fs.access(path.join(directory, "..", "escaped.zip"))).rejects.toThrow();
  });
});
