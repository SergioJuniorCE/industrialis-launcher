import { createHash, randomUUID } from "node:crypto";
import fs from "node:fs/promises";
import { createReadStream } from "node:fs";
import os from "node:os";
import path from "node:path";
import { instanceBackupsDir, sanitizeName } from "./paths";
import type { BackupArtifact, BackupManifest, BackupStore, LocalBackupFile, PreparedBackup, RemoteBackupSummary, TransferProgress } from "./backup-types";

const BACKUP_ROOT = "instances";
const MANIFEST_NAME = "manifest.json";
const TEMP_FILE_PATTERN = /(?:\.tmp|\.part|\.partial|\.crdownload)$/iu;
const SHA256_PATTERN = /^[a-f0-9]{64}$/iu;

class InvalidBackupManifestError extends Error {
  constructor() {
    super("invalid backup manifest");
    this.name = "InvalidBackupManifestError";
  }
}

function invalidManifest(): never {
  throw new InvalidBackupManifestError();
}

function backupFileName(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value === "." ||
    value === ".." ||
    value.includes("\0") ||
    path.posix.basename(value) !== value ||
    path.win32.basename(value) !== value
  ) {
    invalidManifest();
  }
  return value;
}

function remoteSegment(value: string): string {
  const sanitized = sanitizeName(value).trim();
  if (!sanitized || sanitized === "_" || sanitized === "..") throw new Error("backup file name cannot be empty");
  return sanitized;
}

function snapshotPrefix(instanceId: string, snapshotId: string): string {
  if (!/^[a-f0-9]{64}$/iu.test(snapshotId)) throw new Error("invalid backup snapshot id");
  return `${BACKUP_ROOT}/${sanitizeName(instanceId)}/snapshots/${snapshotId}`;
}

function artifactKey(manifestKey: string, artifact: BackupArtifact): string {
  return `${manifestKey.slice(0, -MANIFEST_NAME.length)}${artifact.logical_path}`;
}

async function hashFile(filePath: string): Promise<{ size_bytes: number; sha256: string }> {
  const hash = createHash("sha256");
  let size = 0;
  for await (const chunk of createReadStream(filePath)) {
    hash.update(chunk);
    size += chunk.length;
  }
  return { size_bytes: size, sha256: hash.digest("hex") };
}

async function assertStable(filePath: string, before: { size: number; mtimeMs: number }, context: string): Promise<void> {
  const after = await fs.stat(filePath);
  if (after.size !== before.size || after.mtimeMs !== before.mtimeMs) throw new Error(`${context} changed while it was being read`);
}

function parseManifest(value: unknown): BackupManifest {
  if (typeof value !== "object" || value === null) invalidManifest();
  const candidate = value as Partial<BackupManifest>;
  if (
    candidate.format !== "industrialis-backup" ||
    candidate.schema_version !== 1 ||
    typeof candidate.snapshot_id !== "string" ||
    !SHA256_PATTERN.test(candidate.snapshot_id) ||
    typeof candidate.instance_id !== "string" ||
    typeof candidate.created_at !== "string" ||
    !Array.isArray(candidate.artifacts) ||
    candidate.artifacts.length === 0
  ) {
    invalidManifest();
  }

  const artifacts = candidate.artifacts.map((value) => {
    if (typeof value !== "object" || value === null) invalidManifest();
    const artifact = value as Partial<BackupArtifact>;
    const fileName = backupFileName(artifact.file_name);
    if (
      artifact.kind !== "world-archive" ||
      typeof artifact.id !== "string" ||
      !SHA256_PATTERN.test(artifact.id) ||
      artifact.logical_path !== `artifacts/${fileName}` ||
      typeof artifact.size_bytes !== "number" ||
      !Number.isSafeInteger(artifact.size_bytes) ||
      artifact.size_bytes < 0 ||
      typeof artifact.sha256 !== "string" ||
      !SHA256_PATTERN.test(artifact.sha256) ||
      typeof artifact.content_type !== "string"
    ) {
      invalidManifest();
    }
    return { ...artifact, file_name: fileName } as BackupArtifact;
  });

  return { ...candidate, artifacts } as BackupManifest;
}

async function readRemoteManifest(store: BackupStore, key: string, tempDirectory: string): Promise<BackupManifest> {
  const target = path.join(tempDirectory, `${randomUUID()}.json`);
  await store.download(key, target);
  try {
    try {
      return parseManifest(JSON.parse(await fs.readFile(target, "utf8")) as unknown);
    } catch (error) {
      if (error instanceof InvalidBackupManifestError) throw error;
      throw new InvalidBackupManifestError();
    }
  } finally {
    await fs.rm(target, { force: true });
  }
}

export class BackupService {
  async listLocalBackups(instanceId: string): Promise<LocalBackupFile[]> {
    const directory = instanceBackupsDir(instanceId);
    const entries = await fs.readdir(directory, { withFileTypes: true }).catch(() => []);
    const files: LocalBackupFile[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || TEMP_FILE_PATTERN.test(entry.name)) continue;
      const filePath = path.join(directory, entry.name);
      const stat = await fs.stat(filePath).catch(() => null);
      if (!stat) continue;
      files.push({ file_name: entry.name, size_bytes: stat.size, modified_at: stat.mtime.toISOString() });
    }
    return files.sort((left, right) => right.modified_at.localeCompare(left.modified_at));
  }

  async uploadLocalBackup(
    instanceId: string,
    fileName: string,
    store: BackupStore,
    onProgress?: (progress: TransferProgress) => void,
  ): Promise<RemoteBackupSummary> {
    const prepared = await this.prepareLocalBackup(instanceId, fileName);
    return this.uploadPreparedBackup(prepared, store, onProgress);
  }

  async prepareLocalBackup(instanceId: string, fileName: string, createdAt?: string): Promise<PreparedBackup> {
    if (path.basename(fileName) !== fileName) throw new Error("backup file name must be a file in the backups folder");
    const safeFileName = remoteSegment(fileName);
    const sourcePath = path.join(instanceBackupsDir(instanceId), fileName);
    const sourceStat = await fs.stat(sourcePath);
    if (!sourceStat.isFile()) throw new Error("selected backup is not a file");
    const digest = await hashFile(sourcePath);
    await assertStable(sourcePath, sourceStat, "backup");

    const snapshotId = digest.sha256;
    const artifact: BackupArtifact = {
      id: digest.sha256,
      kind: "world-archive",
      logical_path: `artifacts/${safeFileName}`,
      file_name: safeFileName,
      size_bytes: digest.size_bytes,
      sha256: digest.sha256,
      content_type: "application/octet-stream",
    };
    const manifest: BackupManifest = {
      format: "industrialis-backup",
      schema_version: 1,
      snapshot_id: snapshotId,
      instance_id: sanitizeName(instanceId),
      created_at: createdAt ?? sourceStat.mtime.toISOString(),
      artifacts: [artifact],
    };
    return {
      source_path: sourcePath,
      source_size_bytes: sourceStat.size,
      source_modified_at_ms: sourceStat.mtimeMs,
      manifest,
      summary: {
        snapshot_id: snapshotId,
        instance_id: manifest.instance_id,
        file_name: artifact.file_name,
        size_bytes: artifact.size_bytes,
        created_at: manifest.created_at,
        sha256: artifact.sha256,
      },
    };
  }

  async uploadPreparedBackup(prepared: PreparedBackup, store: BackupStore, onProgress?: (progress: TransferProgress) => void): Promise<RemoteBackupSummary> {
    const { manifest, source_path: sourcePath } = prepared;
    const artifact = manifest.artifacts.find((entry) => entry.kind === "world-archive");
    if (!artifact) throw new Error("prepared backup has no world archive");
    const sourceStat = { size: prepared.source_size_bytes, mtimeMs: prepared.source_modified_at_ms };
    await assertStable(sourcePath, sourceStat, "backup");
    const prefix = snapshotPrefix(manifest.instance_id, manifest.snapshot_id);
    const artifactPath = `${prefix}/${artifact.logical_path}`;
    await store.upload(artifactPath, { kind: "file", path: sourcePath }, { content_type: artifact.content_type, on_progress: onProgress });
    await assertStable(sourcePath, sourceStat, "backup");

    const manifestBytes = new TextEncoder().encode(`${JSON.stringify(manifest, null, 2)}\n`);
    await store.upload(`${prefix}/${MANIFEST_NAME}`, { kind: "bytes", data: manifestBytes }, { content_type: "application/json" });
    return prepared.summary;
  }

  async listRemoteBackups(instanceId: string, store: BackupStore): Promise<RemoteBackupSummary[]> {
    const prefix = `${BACKUP_ROOT}/${sanitizeName(instanceId)}/snapshots/`;
    const objects = await store.list(prefix);
    const manifestKeys = objects.map((object) => object.key).filter((key) => key.endsWith(`/${MANIFEST_NAME}`));
    if (manifestKeys.length === 0) return [];
    const tempDirectory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-backup-manifests-"));
    try {
      const summaries: RemoteBackupSummary[] = [];
      let completedManifestRead = false;
      let firstProviderError: unknown;
      for (const manifestKey of manifestKeys) {
        let manifest: BackupManifest;
        try {
          manifest = await readRemoteManifest(store, manifestKey, tempDirectory);
          completedManifestRead = true;
        } catch (error) {
          if (error instanceof InvalidBackupManifestError) {
            completedManifestRead = true;
          } else {
            firstProviderError ??= error;
          }
          continue;
        }
        if (manifest.instance_id !== sanitizeName(instanceId)) continue;
        const artifact = manifest.artifacts.find((entry) => entry.kind === "world-archive");
        if (!artifact) continue;
        summaries.push({
          snapshot_id: manifest.snapshot_id,
          instance_id: manifest.instance_id,
          file_name: artifact.file_name,
          size_bytes: artifact.size_bytes,
          created_at: manifest.created_at,
          sha256: artifact.sha256,
        });
      }
      if (!completedManifestRead && firstProviderError) throw firstProviderError;
      return summaries.sort((left, right) => right.created_at.localeCompare(left.created_at));
    } finally {
      await fs.rm(tempDirectory, { recursive: true, force: true });
    }
  }

  async downloadRemoteBackup(instanceId: string, snapshotId: string, store: BackupStore): Promise<LocalBackupFile> {
    const prefix = snapshotPrefix(instanceId, snapshotId);
    const objects = await store.list(`${prefix}/`);
    const manifestKey = objects.find((object) => object.key === `${prefix}/${MANIFEST_NAME}`)?.key;
    if (!manifestKey) throw new Error("cloud backup manifest was not found");
    const tempDirectory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-backup-download-"));
    try {
      const manifest = await readRemoteManifest(store, manifestKey, tempDirectory);
      const artifact = manifest.artifacts.find((entry) => entry.kind === "world-archive");
      if (!artifact) throw new Error("cloud backup has no world archive");
      const remotePath = artifactKey(manifestKey, artifact);
      const tempFile = path.join(tempDirectory, artifact.file_name);
      await store.download(remotePath, tempFile);
      const downloaded = await hashFile(tempFile);
      if (downloaded.size_bytes !== artifact.size_bytes || downloaded.sha256 !== artifact.sha256) {
        throw new Error("cloud backup failed integrity verification");
      }

      const directory = instanceBackupsDir(instanceId);
      await fs.mkdir(directory, { recursive: true });
      const extension = path.extname(artifact.file_name);
      const stem = artifact.file_name.slice(0, artifact.file_name.length - extension.length);
      let destinationName = artifact.file_name;
      let suffix = 1;
      while (await fileExists(path.join(directory, destinationName))) {
        destinationName = `${stem}-cloud-${suffix}${extension}`;
        suffix += 1;
      }
      const destination = path.join(directory, destinationName);
      await fs.copyFile(tempFile, destination);
      const stat = await fs.stat(destination);
      return { file_name: destinationName, size_bytes: stat.size, modified_at: stat.mtime.toISOString() };
    } finally {
      await fs.rm(tempDirectory, { recursive: true, force: true });
    }
  }

  async deleteRemoteBackup(instanceId: string, snapshotId: string, store: BackupStore): Promise<void> {
    const prefix = snapshotPrefix(instanceId, snapshotId);
    const objects = await store.list(`${prefix}/`);
    await Promise.all(objects.map((object) => store.delete(object.key)));
  }
}

async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}
