import { randomUUID } from "node:crypto";
import { readJson, writeJson } from "./fs-utils";
import type {
  BackupActivityEntry,
  BackupDashboard,
  BackupHealthStatus,
  BackupInstanceDashboard,
  BackupProviderAvailability,
  BackupProviderConnectionStatus,
  BackupProviderDashboard,
  BackupProviderDefinition,
  CloudBackupSummary,
  LocalBackupFile,
  PreparedBackup,
  RemoteBackupSummary,
} from "./backup-types";
import { BackupService } from "./backups";

const STATE_SCHEMA_VERSION = 1;
const DEFAULT_STABILITY_MS = 60_000;
const DEFAULT_POLL_INTERVAL_MS = 15_000;
const DEFAULT_MAX_CONCURRENT_UPLOADS = 2;
const MAX_RETRY_WINDOW_MS = 24 * 60 * 60 * 1_000;
const MAX_RETRY_DELAY_MS = 60 * 60 * 1_000;
const MAX_ACTIVITY_ENTRIES = 100;

type DeliveryStatus = "pending" | "uploading" | "pending-retry" | "failed" | "healthy";

interface FileRecord {
  instance_id: string;
  file_name: string;
  size_bytes: number;
  modified_at: string;
  sha256: string;
}

interface DeliveryRecord {
  status: DeliveryStatus;
  attempts: number;
  first_failed_at?: string;
  next_retry_at?: string;
  last_error?: string;
  uploaded_at?: string;
}

interface SnapshotRecord extends RemoteBackupSummary {
  providers: Record<string, DeliveryRecord>;
}

interface BackupManagerState {
  schema_version: 1;
  files: Record<string, FileRecord>;
  snapshots: Record<string, SnapshotRecord>;
  activities: BackupActivityEntry[];
  retention: Record<string, number>;
}

interface FileObservation {
  size_bytes: number;
  modified_at: string;
  unchanged_since_ms: number;
}

export interface BackupInstancePolicy {
  instance_id: string;
  enabled: boolean;
  retention_limit: number;
}

export interface BackupManagerOptions {
  service: BackupService;
  providers: BackupProviderDefinition[];
  listInstances(): Promise<BackupInstancePolicy[]>;
  statePath: string;
  emit(event: string, payload: unknown): void;
  now?: () => Date;
  stabilityMs?: number;
  pollIntervalMs?: number;
  maxConcurrentUploads?: number;
}

function emptyState(): BackupManagerState {
  return { schema_version: STATE_SCHEMA_VERSION, files: {}, snapshots: {}, activities: [], retention: {} };
}

function fileKey(instanceId: string, fileName: string): string {
  return `${instanceId}\u0000${fileName}`;
}

function snapshotKey(instanceId: string, snapshotId: string): string {
  return `${instanceId}\u0000${snapshotId}`;
}

function retentionKey(instanceId: string, providerId: string): string {
  return `${instanceId}\u0000${providerId}`;
}

function messageFromError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalizeRetention(value: number): number {
  return Math.max(1, Math.min(1_000, Math.floor(value)));
}

function deliveryAvailability(provider: BackupProviderDefinition, delivery: DeliveryRecord): BackupProviderAvailability {
  return {
    provider_id: provider.id,
    provider_label: provider.label,
    status: delivery.status === "healthy" ? "available" : delivery.status,
    ...(delivery.last_error ? { last_error: delivery.last_error } : {}),
  };
}

export class BackupManager {
  private readonly service: BackupService;
  private readonly providers: BackupProviderDefinition[];
  private readonly providerById: Map<string, BackupProviderDefinition>;
  private readonly listInstances: () => Promise<BackupInstancePolicy[]>;
  private readonly statePath: string;
  private readonly emit: (event: string, payload: unknown) => void;
  private readonly now: () => Date;
  private readonly stabilityMs: number;
  private readonly pollIntervalMs: number;
  private readonly maxConcurrentUploads: number;
  private readonly observations = new Map<string, FileObservation>();
  private state: BackupManagerState = emptyState();
  private loaded = false;
  private loading: Promise<void> | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private activeRun: Promise<void> | null = null;
  private rerunRequested = false;

  constructor(options: BackupManagerOptions) {
    this.service = options.service;
    this.providers = options.providers;
    this.providerById = new Map(options.providers.map((provider) => [provider.id, provider]));
    this.listInstances = options.listInstances;
    this.statePath = options.statePath;
    this.emit = options.emit;
    this.now = options.now ?? (() => new Date());
    this.stabilityMs = options.stabilityMs ?? DEFAULT_STABILITY_MS;
    this.pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    this.maxConcurrentUploads = options.maxConcurrentUploads ?? DEFAULT_MAX_CONCURRENT_UPLOADS;
  }

  async start(): Promise<void> {
    await this.ensureLoaded();
    if (!this.timer) this.timer = setInterval(() => void this.runOnce(), this.pollIntervalMs);
    await this.runOnce();
  }

  async stop(): Promise<void> {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
    await this.activeRun?.catch(() => undefined);
    if (this.loaded) await this.saveState();
  }

  async runOnce(): Promise<void> {
    if (this.activeRun) return this.activeRun;
    this.activeRun = (async () => {
      do {
        this.rerunRequested = false;
        await this.performRun();
      } while (this.rerunRequested);
    })().finally(() => {
      this.activeRun = null;
    });
    return this.activeRun;
  }

  async wake(): Promise<void> {
    if (this.activeRun) {
      this.rerunRequested = true;
      return this.activeRun;
    }
    return this.runOnce();
  }

  async uploadNow(instanceId: string, fileName: string): Promise<CloudBackupSummary> {
    await this.ensureLoaded();
    await this.activeRun;
    const policy = (await this.listInstances()).find((entry) => entry.instance_id === instanceId);
    if (!policy?.enabled) throw new Error("cloud backups are disabled for this instance");
    const prepared = await this.service.prepareLocalBackup(instanceId, fileName);
    const statuses = await this.providerStatuses();
    const connected = this.providers.filter((provider) => statuses.get(provider.id)?.connected);
    if (connected.length === 0) throw new Error("no cloud backup provider is connected");
    this.rememberPrepared(prepared, connected);
    await this.saveAndPublish();
    await this.wake();
    const snapshots = await this.listSnapshots(instanceId);
    const snapshot = snapshots.find((entry) => entry.snapshot_id === prepared.summary.snapshot_id);
    if (!snapshot) throw new Error("backup could not be queued");
    return snapshot;
  }

  async listSnapshots(instanceId: string): Promise<CloudBackupSummary[]> {
    await this.ensureLoaded();
    const statuses = await this.providerStatuses();
    const merged = new Map<string, CloudBackupSummary>();
    for (const provider of this.providers) {
      if (!statuses.get(provider.id)?.connected) continue;
      try {
        for (const summary of await this.service.listRemoteBackups(instanceId, provider.store)) {
          const existing = merged.get(summary.snapshot_id) ?? { ...summary, providers: [] };
          existing.providers = existing.providers.filter((entry) => entry.provider_id !== provider.id);
          existing.providers.push({ provider_id: provider.id, provider_label: provider.label, status: "available" });
          merged.set(summary.snapshot_id, existing);
        }
      } catch {
        // Provider health is reported by the dashboard; other providers remain usable.
      }
    }
    for (const snapshot of Object.values(this.state.snapshots)) {
      if (snapshot.instance_id !== instanceId) continue;
      const existing = merged.get(snapshot.snapshot_id) ?? {
        snapshot_id: snapshot.snapshot_id,
        instance_id: snapshot.instance_id,
        file_name: snapshot.file_name,
        size_bytes: snapshot.size_bytes,
        created_at: snapshot.created_at,
        sha256: snapshot.sha256,
        providers: [],
      };
      for (const [providerId, delivery] of Object.entries(snapshot.providers)) {
        if (existing.providers.some((entry) => entry.provider_id === providerId && entry.status === "available")) continue;
        const provider = this.providerById.get(providerId);
        if (provider) existing.providers.push(deliveryAvailability(provider, delivery));
      }
      if (existing.providers.length > 0) merged.set(snapshot.snapshot_id, existing);
    }
    return [...merged.values()].sort((left, right) => right.created_at.localeCompare(left.created_at));
  }

  async download(instanceId: string, snapshotId: string, preferredProviderId?: string): Promise<LocalBackupFile> {
    const statuses = await this.providerStatuses();
    const ordered = preferredProviderId ? [...this.providers].sort((left) => (left.id === preferredProviderId ? -1 : 1)) : this.providers;
    const errors: string[] = [];
    for (const provider of ordered) {
      if (!statuses.get(provider.id)?.connected) continue;
      try {
        const result = await this.service.downloadRemoteBackup(instanceId, snapshotId, provider.store);
        this.addActivity("info", `Restored ${result.file_name} from ${provider.label}.`, instanceId, provider.id, snapshotId);
        await this.saveAndPublish();
        return result;
      } catch (error) {
        errors.push(`${provider.label}: ${messageFromError(error)}`);
      }
    }
    throw new Error(errors.length > 0 ? errors.join("; ") : "no cloud backup provider is connected");
  }

  async delete(instanceId: string, snapshotId: string): Promise<void> {
    await this.ensureLoaded();
    const statuses = await this.providerStatuses();
    const errors: string[] = [];
    let attempted = 0;
    const record = this.state.snapshots[snapshotKey(instanceId, snapshotId)];
    for (const provider of this.providers) {
      if (!statuses.get(provider.id)?.connected) continue;
      attempted += 1;
      try {
        await this.service.deleteRemoteBackup(instanceId, snapshotId, provider.store);
        if (record) delete record.providers[provider.id];
      } catch (error) {
        errors.push(`${provider.label}: ${messageFromError(error)}`);
      }
    }
    if (attempted === 0) throw new Error("no cloud backup provider is connected");
    if (record && Object.keys(record.providers).length === 0) delete this.state.snapshots[snapshotKey(instanceId, snapshotId)];
    this.addActivity(
      errors.length > 0 ? "error" : "info",
      errors.length > 0 ? `Backup deletion was incomplete: ${errors.join("; ")}` : "Deleted cloud backup.",
      instanceId,
      undefined,
      snapshotId,
    );
    await this.saveAndPublish();
    if (errors.length > 0) throw new Error(errors.join("; "));
  }

  async retry(instanceId?: string, snapshotId?: string, providerId?: string): Promise<void> {
    await this.ensureLoaded();
    await this.activeRun;
    for (const snapshot of Object.values(this.state.snapshots)) {
      if (instanceId && snapshot.instance_id !== instanceId) continue;
      if (snapshotId && snapshot.snapshot_id !== snapshotId) continue;
      for (const [candidateProviderId, delivery] of Object.entries(snapshot.providers)) {
        if (providerId && candidateProviderId !== providerId) continue;
        if (delivery.status === "healthy") continue;
        delivery.status = "pending";
        delivery.next_retry_at = undefined;
        delivery.first_failed_at = undefined;
        delivery.last_error = undefined;
        delivery.attempts = 0;
      }
    }
    await this.saveAndPublish();
    await this.wake();
  }

  async getDashboard(): Promise<BackupDashboard> {
    await this.ensureLoaded();
    const [policies, statuses] = await Promise.all([this.listInstances(), this.providerStatuses()]);
    const providers = this.providers.map((provider) => this.providerDashboard(provider, statuses.get(provider.id) ?? { configured: false, connected: false }));
    const instances = policies.map((policy) => this.instanceDashboard(policy, providers));
    const queueSize = Object.values(this.state.snapshots).reduce(
      (total, snapshot) => total + Object.values(snapshot.providers).filter((delivery) => delivery.status !== "healthy").length,
      0,
    );
    return { providers, instances, activities: [...this.state.activities], queue_size: queueSize };
  }

  private async performRun(): Promise<void> {
    await this.ensureLoaded();
    const [policies, statuses] = await Promise.all([this.listInstances(), this.providerStatuses()]);
    const connectedProviders = this.providers.filter((provider) => statuses.get(provider.id)?.connected);
    for (const policy of policies) {
      if (policy.enabled) await this.scanInstance(policy, connectedProviders);
    }
    await this.saveState();
    await this.processPending(policies, statuses);
    await this.reconcileRetention(policies, statuses);
    await this.saveAndPublish();
  }

  private async scanInstance(policy: BackupInstancePolicy, connectedProviders: BackupProviderDefinition[]): Promise<void> {
    if (connectedProviders.length === 0) return;
    const files = await this.service.listLocalBackups(policy.instance_id);
    const distinct = new Set<string>();
    for (const file of files) {
      if (distinct.size >= normalizeRetention(policy.retention_limit)) break;
      if (!this.isStable(policy.instance_id, file)) continue;
      const key = fileKey(policy.instance_id, file.file_name);
      const cached = this.state.files[key];
      let prepared: PreparedBackup | null = null;
      let hash = cached?.size_bytes === file.size_bytes && cached.modified_at === file.modified_at ? cached.sha256 : null;
      if (!hash) {
        try {
          prepared = await this.service.prepareLocalBackup(policy.instance_id, file.file_name, file.modified_at);
          hash = prepared.summary.sha256;
          this.state.files[key] = {
            instance_id: policy.instance_id,
            file_name: file.file_name,
            size_bytes: file.size_bytes,
            modified_at: file.modified_at,
            sha256: hash,
          };
        } catch {
          continue;
        }
      }
      if (distinct.has(hash)) continue;
      distinct.add(hash);
      const existing = this.state.snapshots[snapshotKey(policy.instance_id, hash)];
      if (prepared) this.rememberPrepared(prepared, connectedProviders);
      else if (existing) this.ensureDeliveries(existing, connectedProviders);
      else {
        try {
          prepared = await this.service.prepareLocalBackup(policy.instance_id, file.file_name, file.modified_at);
          this.rememberPrepared(prepared, connectedProviders);
        } catch {
          // File changed after listing and will be retried by a later scan.
        }
      }
    }
  }

  private isStable(instanceId: string, file: LocalBackupFile): boolean {
    const now = this.now().getTime();
    if (now - Date.parse(file.modified_at) >= this.stabilityMs) return true;
    const key = fileKey(instanceId, file.file_name);
    const observation = this.observations.get(key);
    if (!observation || observation.size_bytes !== file.size_bytes || observation.modified_at !== file.modified_at) {
      this.observations.set(key, { size_bytes: file.size_bytes, modified_at: file.modified_at, unchanged_since_ms: now });
      return false;
    }
    return now - observation.unchanged_since_ms >= this.stabilityMs;
  }

  private rememberPrepared(prepared: PreparedBackup, providers: BackupProviderDefinition[]): void {
    const summary = prepared.summary;
    const key = snapshotKey(summary.instance_id, summary.snapshot_id);
    const record = this.state.snapshots[key] ?? { ...summary, providers: {} };
    record.file_name = summary.file_name;
    record.size_bytes = summary.size_bytes;
    record.created_at = summary.created_at;
    this.state.snapshots[key] = record;
    this.state.files[fileKey(summary.instance_id, summary.file_name)] = {
      instance_id: summary.instance_id,
      file_name: summary.file_name,
      size_bytes: summary.size_bytes,
      modified_at: new Date(prepared.source_modified_at_ms).toISOString(),
      sha256: summary.sha256,
    };
    this.ensureDeliveries(record, providers);
  }

  private ensureDeliveries(record: SnapshotRecord, providers: BackupProviderDefinition[]): void {
    for (const provider of providers) record.providers[provider.id] ??= { status: "pending", attempts: 0 };
  }

  private async processPending(policies: BackupInstancePolicy[], statuses: Map<string, BackupProviderConnectionStatus>): Promise<void> {
    const policyById = new Map(policies.map((policy) => [policy.instance_id, policy]));
    while (true) {
      const now = this.now().getTime();
      const usedInstances = new Set<string>();
      const tasks: Array<{ snapshot: SnapshotRecord; provider: BackupProviderDefinition; delivery: DeliveryRecord; policy: BackupInstancePolicy }> = [];
      for (const snapshot of Object.values(this.state.snapshots).sort((left, right) => left.created_at.localeCompare(right.created_at))) {
        const policy = policyById.get(snapshot.instance_id);
        if (!policy?.enabled || usedInstances.has(snapshot.instance_id)) continue;
        for (const [providerId, delivery] of Object.entries(snapshot.providers)) {
          const provider = this.providerById.get(providerId);
          if (
            !provider ||
            !statuses.get(providerId)?.connected ||
            delivery.status === "healthy" ||
            delivery.status === "failed" ||
            delivery.status === "uploading"
          )
            continue;
          if (delivery.next_retry_at && Date.parse(delivery.next_retry_at) > now) continue;
          tasks.push({ snapshot, provider, delivery, policy });
          usedInstances.add(snapshot.instance_id);
          break;
        }
        if (tasks.length >= this.maxConcurrentUploads) break;
      }
      if (tasks.length === 0) return;
      await Promise.all(tasks.map((task) => this.processDelivery(task.snapshot, task.provider, task.delivery, task.policy)));
      await this.saveAndPublish();
    }
  }

  private async processDelivery(
    snapshot: SnapshotRecord,
    provider: BackupProviderDefinition,
    delivery: DeliveryRecord,
    policy: BackupInstancePolicy,
  ): Promise<void> {
    delivery.status = "uploading";
    delivery.last_error = undefined;
    this.publish();
    try {
      const prepared = await this.service.prepareLocalBackup(snapshot.instance_id, snapshot.file_name, snapshot.created_at);
      if (prepared.summary.sha256 !== snapshot.snapshot_id) throw new Error("local backup changed before it could be uploaded");
      await this.service.uploadPreparedBackup(prepared, provider.store, (progress) => {
        this.emit("backup-progress", { id: snapshot.instance_id, file_name: snapshot.file_name, provider_id: provider.id, ...progress });
      });
      delivery.status = "healthy";
      delivery.uploaded_at = this.now().toISOString();
      delivery.next_retry_at = undefined;
      delivery.first_failed_at = undefined;
      delivery.last_error = undefined;
      this.addActivity("info", `Uploaded ${snapshot.file_name} to ${provider.label}.`, snapshot.instance_id, provider.id, snapshot.snapshot_id);
      try {
        await this.applyRetention(snapshot.instance_id, provider, normalizeRetention(policy.retention_limit));
        this.state.retention[retentionKey(snapshot.instance_id, provider.id)] = normalizeRetention(policy.retention_limit);
      } catch (retentionError) {
        this.addActivity(
          "error",
          `Uploaded ${snapshot.file_name}, but retention cleanup failed on ${provider.label}: ${messageFromError(retentionError)}`,
          snapshot.instance_id,
          provider.id,
          snapshot.snapshot_id,
        );
      }
    } catch (error) {
      const now = this.now();
      delivery.attempts += 1;
      delivery.first_failed_at ??= now.toISOString();
      delivery.last_error = messageFromError(error);
      const retryWindowExpired = now.getTime() - Date.parse(delivery.first_failed_at) >= MAX_RETRY_WINDOW_MS;
      if (retryWindowExpired) {
        delivery.status = "failed";
        delivery.next_retry_at = undefined;
      } else {
        delivery.status = "pending-retry";
        const delay = Math.min(60_000 * 2 ** Math.max(0, delivery.attempts - 1), MAX_RETRY_DELAY_MS);
        delivery.next_retry_at = new Date(now.getTime() + delay).toISOString();
      }
      this.addActivity(
        "error",
        `Could not upload ${snapshot.file_name} to ${provider.label}: ${delivery.last_error}`,
        snapshot.instance_id,
        provider.id,
        snapshot.snapshot_id,
      );
    }
  }

  private async applyRetention(instanceId: string, provider: BackupProviderDefinition, retentionLimit: number): Promise<void> {
    const remote = await this.service.listRemoteBackups(instanceId, provider.store);
    for (const expired of remote.slice(retentionLimit)) {
      await this.service.deleteRemoteBackup(instanceId, expired.snapshot_id, provider.store);
      const record = this.state.snapshots[snapshotKey(instanceId, expired.snapshot_id)];
      if (record) {
        delete record.providers[provider.id];
        if (Object.keys(record.providers).length === 0) delete this.state.snapshots[snapshotKey(instanceId, expired.snapshot_id)];
      }
      this.addActivity("info", `Removed ${expired.file_name} from ${provider.label} due to retention.`, instanceId, provider.id, expired.snapshot_id);
    }
  }

  private async reconcileRetention(policies: BackupInstancePolicy[], statuses: Map<string, BackupProviderConnectionStatus>): Promise<void> {
    for (const policy of policies) {
      if (!policy.enabled) continue;
      const limit = normalizeRetention(policy.retention_limit);
      for (const provider of this.providers) {
        if (!statuses.get(provider.id)?.connected) continue;
        const key = retentionKey(policy.instance_id, provider.id);
        if (this.state.retention[key] === limit) continue;
        try {
          await this.applyRetention(policy.instance_id, provider, limit);
          this.state.retention[key] = limit;
        } catch (error) {
          this.addActivity("error", `Retention cleanup failed on ${provider.label}: ${messageFromError(error)}`, policy.instance_id, provider.id);
        }
      }
    }
  }

  private async providerStatuses(): Promise<Map<string, BackupProviderConnectionStatus>> {
    const entries = await Promise.all(
      this.providers.map(async (provider) => {
        try {
          return [provider.id, await provider.getStatus()] as const;
        } catch {
          return [provider.id, { configured: false, connected: false }] as const;
        }
      }),
    );
    return new Map(entries);
  }

  private providerDashboard(provider: BackupProviderDefinition, connection: BackupProviderConnectionStatus): BackupProviderDashboard {
    const deliveries = Object.values(this.state.snapshots).flatMap((snapshot) => (snapshot.providers[provider.id] ? [snapshot.providers[provider.id]] : []));
    const pending = deliveries.filter((delivery) => delivery.status === "pending" || delivery.status === "pending-retry" || delivery.status === "uploading");
    const failed = deliveries.filter((delivery) => delivery.status === "failed");
    let status: BackupHealthStatus = "healthy";
    if (!connection.configured || !connection.connected) status = "not-configured";
    else if (deliveries.some((delivery) => delivery.status === "uploading")) status = "uploading";
    else if (pending.length > 0) status = "pending-retry";
    else if (failed.length > 0) status = "failed";
    const lastError = [...deliveries].reverse().find((delivery) => delivery.last_error)?.last_error;
    return {
      provider_id: provider.id,
      provider_label: provider.label,
      configured: connection.configured,
      connected: connection.connected,
      status,
      pending_count: pending.length,
      failed_count: failed.length,
      ...(lastError ? { last_error: lastError } : {}),
    };
  }

  private instanceDashboard(policy: BackupInstancePolicy, providers: BackupProviderDashboard[]): BackupInstanceDashboard {
    const deliveries = Object.values(this.state.snapshots)
      .filter((snapshot) => snapshot.instance_id === policy.instance_id)
      .flatMap((snapshot) => Object.values(snapshot.providers));
    const pending = deliveries.filter((delivery) => delivery.status === "pending" || delivery.status === "pending-retry" || delivery.status === "uploading");
    const failed = deliveries.filter((delivery) => delivery.status === "failed");
    let status: BackupHealthStatus = "healthy";
    if (!policy.enabled) status = "disabled";
    else if (!providers.some((provider) => provider.connected)) status = "not-configured";
    else if (deliveries.some((delivery) => delivery.status === "uploading")) status = "uploading";
    else if (pending.length > 0) status = "pending-retry";
    else if (failed.length > 0) status = "failed";
    return {
      instance_id: policy.instance_id,
      enabled: policy.enabled,
      retention_limit: normalizeRetention(policy.retention_limit),
      status,
      pending_count: pending.length,
      failed_count: failed.length,
    };
  }

  private addActivity(level: "info" | "error", message: string, instanceId?: string, providerId?: string, snapshotId?: string): void {
    this.state.activities.unshift({
      id: randomUUID(),
      created_at: this.now().toISOString(),
      level,
      message,
      ...(instanceId ? { instance_id: instanceId } : {}),
      ...(providerId ? { provider_id: providerId } : {}),
      ...(snapshotId ? { snapshot_id: snapshotId } : {}),
    });
    this.state.activities = this.state.activities.slice(0, MAX_ACTIVITY_ENTRIES);
  }

  private async ensureLoaded(): Promise<void> {
    if (this.loaded) return;
    if (!this.loading) {
      this.loading = (async () => {
        const saved = await readJson<BackupManagerState>(this.statePath);
        if (saved?.schema_version === STATE_SCHEMA_VERSION) {
          this.state = saved;
          this.state.retention ??= {};
          for (const snapshot of Object.values(this.state.snapshots)) {
            for (const delivery of Object.values(snapshot.providers)) {
              if (delivery.status === "uploading") delivery.status = "pending-retry";
            }
          }
        }
        this.loaded = true;
      })().finally(() => {
        this.loading = null;
      });
    }
    await this.loading;
  }

  private saveState(): Promise<void> {
    return writeJson(this.statePath, this.state);
  }

  private async saveAndPublish(): Promise<void> {
    await this.saveState();
    this.publish();
  }

  private publish(): void {
    this.emit("backup-status", { changed_at: this.now().toISOString() });
  }
}
