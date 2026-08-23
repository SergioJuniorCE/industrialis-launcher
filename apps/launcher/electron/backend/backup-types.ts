export type BackupArtifactKind = "world-archive";

export interface BackupArtifact {
  id: string;
  kind: BackupArtifactKind;
  logical_path: string;
  file_name: string;
  size_bytes: number;
  sha256: string;
  content_type: string;
}

export interface BackupManifest {
  format: "industrialis-backup";
  schema_version: 1;
  snapshot_id: string;
  instance_id: string;
  created_at: string;
  launcher_version?: string;
  pack_version?: string;
  artifacts: BackupArtifact[];
}

export interface LocalBackupFile {
  file_name: string;
  size_bytes: number;
  modified_at: string;
}

export interface RemoteBackupSummary {
  snapshot_id: string;
  instance_id: string;
  file_name: string;
  size_bytes: number;
  created_at: string;
  sha256: string;
}

export interface PreparedBackup {
  source_path: string;
  source_size_bytes: number;
  source_modified_at_ms: number;
  manifest: BackupManifest;
  summary: RemoteBackupSummary;
}

export type BackupHealthStatus = "not-configured" | "healthy" | "uploading" | "pending-retry" | "failed" | "disabled";

export interface BackupProviderAvailability {
  provider_id: string;
  provider_label: string;
  status: "available" | "pending" | "uploading" | "pending-retry" | "failed";
  last_error?: string;
}

export interface CloudBackupSummary extends RemoteBackupSummary {
  providers: BackupProviderAvailability[];
}

export interface BackupProviderDashboard {
  provider_id: string;
  provider_label: string;
  configured: boolean;
  connected: boolean;
  status: BackupHealthStatus;
  pending_count: number;
  failed_count: number;
  last_error?: string;
}

export interface BackupInstanceDashboard {
  instance_id: string;
  enabled: boolean;
  retention_limit: number;
  status: BackupHealthStatus;
  pending_count: number;
  failed_count: number;
}

export interface BackupActivityEntry {
  id: string;
  created_at: string;
  level: "info" | "error";
  message: string;
  instance_id?: string;
  provider_id?: string;
  snapshot_id?: string;
}

export interface BackupDashboard {
  providers: BackupProviderDashboard[];
  instances: BackupInstanceDashboard[];
  activities: BackupActivityEntry[];
  queue_size: number;
}

export interface BackupProviderConnectionStatus {
  configured: boolean;
  connected: boolean;
}

export interface BackupProviderDefinition {
  id: string;
  label: string;
  store: BackupStore;
  getStatus(): Promise<BackupProviderConnectionStatus>;
}

export interface RemoteObject {
  key: string;
  size_bytes?: number;
  modified_at?: string;
  content_type?: string;
}

export type UploadSource = { kind: "file"; path: string } | { kind: "bytes"; data: Uint8Array };

export interface TransferProgress {
  completed_bytes: number;
  total_bytes: number;
}

export interface UploadOptions {
  content_type?: string;
  on_progress?: (progress: TransferProgress) => void;
}

export interface DownloadOptions {
  on_progress?: (progress: TransferProgress) => void;
}

export interface BackupStore {
  list(prefix: string): Promise<RemoteObject[]>;
  upload(key: string, source: UploadSource, options?: UploadOptions): Promise<RemoteObject>;
  download(key: string, destination: string, options?: DownloadOptions): Promise<void>;
  delete(key: string): Promise<void>;
}
