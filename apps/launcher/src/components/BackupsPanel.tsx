import { useCallback, useEffect, useState } from "react";
import { BookOpen, Check, Cloud, Download, ExternalLink, FolderOpen, LogIn, LogOut, RefreshCw, Trash2, Upload } from "lucide-react";
import { invoke, listen, openUrl } from "../lib/desktop";
import { ConfirmDialog } from "./ConfirmDialog";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card";
import { Checkbox } from "./ui/checkbox";
import { Input } from "./ui/input";
import { Progress } from "./ui/progress";
import { Select } from "./ui/select";

interface LocalBackupFile {
  file_name: string;
  size_bytes: number;
  modified_at: string;
}

interface CloudBackup {
  snapshot_id: string;
  file_name: string;
  size_bytes: number;
  created_at: string;
  sha256: string;
  providers: Array<{
    provider_id: string;
    provider_label: string;
    status: "available" | "pending" | "uploading" | "pending-retry" | "failed";
    last_error?: string;
  }>;
}

interface BackupDashboard {
  providers: Array<{
    provider_id: string;
    status: string;
    connected: boolean;
    pending_count: number;
    failed_count: number;
    last_error?: string;
  }>;
  instances: Array<{
    instance_id: string;
    status: string;
    retention_limit: number;
    pending_count: number;
    failed_count: number;
  }>;
  activities: Array<{
    id: string;
    created_at: string;
    level: "info" | "error";
    message: string;
  }>;
  queue_size: number;
}

interface GoogleDriveStatus {
  configured: boolean;
  connected: boolean;
  client_id?: string;
}

interface BackupProgress {
  id: string;
  file_name: string;
  completed_bytes: number;
  total_bytes: number;
}

const BACKUP_GUIDE_URL = "https://industrialislauncher.yoggan.dev/docs/cloud-backups";
const GOOGLE_CLOUD_CONSOLE_URL = "https://console.cloud.google.com/apis/dashboard";

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0.1, bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function BackupsPanel({ instanceId, enabled = true }: { instanceId: string; enabled?: boolean }) {
  const [status, setStatus] = useState<GoogleDriveStatus>({ configured: false, connected: false });
  const [clientId, setClientId] = useState("");
  const [localBackups, setLocalBackups] = useState<LocalBackupFile[]>([]);
  const [cloudBackups, setCloudBackups] = useState<CloudBackup[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleteSnapshotId, setDeleteSnapshotId] = useState<string | null>(null);
  const [dashboard, setDashboard] = useState<BackupDashboard | null>(null);
  const [openFolderAfterRestore, setOpenFolderAfterRestore] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextStatus, local, nextDashboard] = await Promise.all([
        invoke<GoogleDriveStatus>("get_google_drive_status"),
        invoke<LocalBackupFile[]>("list_local_backups", { id: instanceId }),
        invoke<BackupDashboard>("get_backup_dashboard"),
      ]);
      setStatus(nextStatus);
      setClientId(nextStatus.client_id ?? "");
      setLocalBackups(local);
      setDashboard(nextDashboard);
      setCloudBackups(await invoke<CloudBackup[]>("list_cloud_backups", { id: instanceId }));
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [instanceId]);

  useEffect(() => {
    void refresh();
    let unsubscribeProgress: (() => void) | undefined;
    let unsubscribeStatus: (() => void) | undefined;
    void listen<BackupProgress>("backup-progress", ({ payload }) => {
      if (payload.id === instanceId) setProgress(payload);
    }).then((cleanup) => {
      unsubscribeProgress = cleanup;
    });
    void listen("backup-status", () => void refresh()).then((cleanup) => {
      unsubscribeStatus = cleanup;
    });
    return () => {
      unsubscribeProgress?.();
      unsubscribeStatus?.();
    };
  }, [instanceId, refresh]);

  const connect = async () => {
    setBusyKey("connect");
    setError(null);
    try {
      await invoke("connect_google_drive");
      await refresh();
    } catch (connectError) {
      setError(errorMessage(connectError));
    } finally {
      setBusyKey(null);
    }
  };

  const configure = async () => {
    setBusyKey("configure");
    setError(null);
    try {
      await invoke("configure_google_drive", { clientId });
      await refresh();
    } catch (configureError) {
      setError(errorMessage(configureError));
    } finally {
      setBusyKey(null);
    }
  };

  const disconnect = async () => {
    setBusyKey("disconnect");
    setError(null);
    try {
      await invoke("disconnect_google_drive");
      await refresh();
    } catch (disconnectError) {
      setError(errorMessage(disconnectError));
    } finally {
      setBusyKey(null);
    }
  };

  const openFolder = async () => {
    setBusyKey("folder");
    setError(null);
    try {
      await invoke("open_backups_folder", { id: instanceId });
    } catch (openError) {
      setError(errorMessage(openError));
    } finally {
      setBusyKey(null);
    }
  };

  const openExternal = async (url: string) => {
    setError(null);
    try {
      await openUrl(url);
    } catch (openError) {
      setError(errorMessage(openError));
    }
  };

  const upload = async (fileName: string) => {
    setBusyKey(`upload:${fileName}`);
    setProgress(null);
    setError(null);
    try {
      await invoke("upload_backup", { id: instanceId, fileName });
      await refresh();
    } catch (uploadError) {
      setError(errorMessage(uploadError));
    } finally {
      setBusyKey(null);
      setProgress(null);
    }
  };

  const download = async (snapshotId: string) => {
    setBusyKey(`download:${snapshotId}`);
    setError(null);
    try {
      await invoke("download_backup", { id: instanceId, snapshotId });
      if (openFolderAfterRestore) await invoke("open_backups_folder", { id: instanceId });
      await refresh();
    } catch (downloadError) {
      setError(errorMessage(downloadError));
    } finally {
      setBusyKey(null);
    }
  };

  const retry = async (snapshotId?: string) => {
    setBusyKey(snapshotId ? `retry:${snapshotId}` : "retry:provider");
    setError(null);
    try {
      await invoke("retry_backup", snapshotId ? { id: instanceId, snapshotId } : { providerId: "google-drive" });
      await refresh();
    } catch (retryError) {
      setError(errorMessage(retryError));
    } finally {
      setBusyKey(null);
    }
  };

  const deleteBackup = async () => {
    if (!deleteSnapshotId) return;
    const snapshotId = deleteSnapshotId;
    setDeleteSnapshotId(null);
    setBusyKey(`delete:${snapshotId}`);
    setError(null);
    try {
      await invoke("delete_backup", { id: instanceId, snapshotId });
      await refresh();
    } catch (deleteError) {
      setError(errorMessage(deleteError));
    } finally {
      setBusyKey(null);
    }
  };

  const configuredMessage = status.configured
    ? "Connect your Google account to upload and restore snapshots."
    : "Configure a Google OAuth desktop client ID to enable Google Drive.";
  const providerHealth = dashboard?.providers.find((provider) => provider.provider_id === "google-drive");
  const instanceHealth = dashboard?.instances.find((instance) => instance.instance_id === instanceId);

  return (
    <div className="flex h-full min-h-[200px] flex-col gap-3 overflow-auto">
      <Card>
        <CardHeader className="flex-row items-center justify-between gap-3">
          <div>
            <CardTitle className="flex items-center gap-2 text-sm">
              <Cloud className="size-4" /> Google Drive
              {status.connected && <Badge variant="success">Connected</Badge>}
              {providerHealth?.status && (
                <Badge variant={providerHealth.status === "healthy" ? "success" : "outline"} className="capitalize">
                  {providerHealth.status.replaceAll("-", " ")}
                </Badge>
              )}
              {!enabled && <Badge variant="warning">Disabled for instance</Badge>}
            </CardTitle>
            <CardDescription>{configuredMessage}</CardDescription>
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-2">
            <Button size="sm" variant="outline" onClick={() => void openExternal(BACKUP_GUIDE_URL)}>
              <BookOpen className="size-3.5" /> Setup guide
            </Button>
            {status.connected ? (
              <>
                {providerHealth?.pending_count || providerHealth?.failed_count ? (
                  <Button size="sm" variant="outline" disabled={busyKey !== null} onClick={() => void retry()}>
                    <RefreshCw className={busyKey === "retry:provider" ? "animate-spin" : ""} /> Retry now
                  </Button>
                ) : null}
                <Button size="sm" variant="outline" disabled={busyKey !== null} onClick={() => void disconnect()}>
                  <LogOut className="size-3.5" /> Disconnect
                </Button>
              </>
            ) : (
              <Button size="sm" disabled={!status.configured || busyKey !== null} onClick={() => void connect()}>
                <LogIn className="size-3.5" /> Connect
              </Button>
            )}
            <Button size="sm" variant="outline" disabled={busyKey !== null} onClick={() => void openFolder()}>
              <FolderOpen className="size-3.5" /> Open folder
            </Button>
            <Button size="icon" variant="ghost" title="Refresh backups" disabled={loading || busyKey !== null} onClick={() => void refresh()}>
              <RefreshCw className={loading ? "animate-spin" : ""} />
            </Button>
          </div>
        </CardHeader>
        {!status.connected && (
          <CardContent className="space-y-2 pt-0">
            <div className="flex gap-2">
              <Input
                value={clientId}
                placeholder="Google OAuth desktop client ID"
                aria-label="Google OAuth desktop client ID"
                disabled={busyKey !== null}
                onChange={(event) => setClientId(event.target.value)}
              />
              <Button size="sm" disabled={busyKey !== null || clientId.trim().length < 16} onClick={() => void configure()}>
                Save ID
              </Button>
            </div>
            <p className="text-[11px] text-muted-foreground">
              Create a Desktop app OAuth client in Google Cloud, enable the Google Drive API, then paste its client ID here. The client ID is public; the
              account refresh token is stored securely by the operating system.
            </p>
            <Button size="sm" variant="ghost" className="px-0" onClick={() => void openExternal(GOOGLE_CLOUD_CONSOLE_URL)}>
              Open Google Cloud Console <ExternalLink className="size-3.5" />
            </Button>
          </CardContent>
        )}
        {error && (
          <CardContent className="pt-0">
            <p className="text-xs text-destructive">{error}</p>
          </CardContent>
        )}
        {providerHealth?.last_error ? (
          <CardContent className="pt-0">
            <p className="text-xs text-destructive">{providerHealth.last_error}</p>
          </CardContent>
        ) : null}
      </Card>

      {!enabled && (
        <div className="rounded-md border border-border bg-muted/30 p-2 text-xs text-muted-foreground">
          Cloud uploads are disabled for this instance. Enable <span className="font-medium text-foreground">Cloud backups</span> in the instance settings to
          upload new snapshots; existing snapshots can still be restored or deleted.
        </div>
      )}

      {enabled && instanceHealth ? (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/30 p-2 text-xs">
          <Badge variant={instanceHealth.status === "healthy" ? "success" : "outline"} className="capitalize">
            {instanceHealth.status.replaceAll("-", " ")}
          </Badge>
          <span className="text-muted-foreground">Keep {instanceHealth.retention_limit} snapshots per provider</span>
          {instanceHealth.pending_count > 0 ? <span className="text-muted-foreground">{instanceHealth.pending_count} queued</span> : null}
          {instanceHealth.failed_count > 0 ? <span className="text-destructive">{instanceHealth.failed_count} failed</span> : null}
        </div>
      ) : null}

      {progress && busyKey?.startsWith("upload:") && (
        <div className="rounded-md border border-border bg-card/50 p-2">
          <div className="mb-1 flex justify-between gap-2 text-xs">
            <span className="truncate">Uploading {progress.file_name}</span>
            <span className="shrink-0 text-muted-foreground">
              {formatBytes(progress.completed_bytes)} / {formatBytes(progress.total_bytes)}
            </span>
          </div>
          <Progress value={progress.total_bytes ? (progress.completed_bytes / progress.total_bytes) * 100 : 0} className="h-1.5" />
        </div>
      )}

      <div className="grid min-h-0 gap-3 xl:grid-cols-2">
        <Card className="min-h-0">
          <CardHeader className="flex-row items-center justify-between gap-2 pb-2">
            <div>
              <CardTitle className="text-sm">Local snapshots</CardTitle>
              <CardDescription>Completed files from this instance&apos;s backups folder.</CardDescription>
            </div>
            <Badge variant="outline">{localBackups.length}</Badge>
          </CardHeader>
          <CardContent className="space-y-1.5">
            {localBackups.length === 0 ? (
              <p className="text-xs text-muted-foreground">No backup files found yet.</p>
            ) : (
              localBackups.map((backup) => {
                const uploading = busyKey === `upload:${backup.file_name}`;
                return (
                  <div key={backup.file_name} className="flex items-center justify-between gap-2 rounded-md border border-border/70 px-2 py-1.5">
                    <div className="min-w-0">
                      <p className="truncate text-xs font-medium">{backup.file_name}</p>
                      <p className="text-[10px] text-muted-foreground">
                        {formatBytes(backup.size_bytes)} · {formatDate(backup.modified_at)}
                      </p>
                    </div>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!enabled || !status.connected || busyKey !== null}
                      onClick={() => void upload(backup.file_name)}
                    >
                      {uploading ? <RefreshCw className="animate-spin" /> : <Upload />} {uploading ? "Uploading" : "Upload"}
                    </Button>
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>

        <Card className="min-h-0">
          <CardHeader className="flex-row items-center justify-between gap-2 pb-2">
            <div>
              <CardTitle className="text-sm">Cloud snapshots</CardTitle>
              <CardDescription>Unified snapshots across connected providers.</CardDescription>
            </div>
            <Badge variant="outline">{cloudBackups.length}</Badge>
          </CardHeader>
          <CardContent className="space-y-1.5">
            {cloudBackups.length === 0 ? (
              <p className="text-xs text-muted-foreground">No cloud snapshots for this instance.</p>
            ) : (
              cloudBackups.map((backup) => {
                const downloading = busyKey === `download:${backup.snapshot_id}`;
                const deleting = busyKey === `delete:${backup.snapshot_id}`;
                return (
                  <div key={backup.snapshot_id} className="flex items-center justify-between gap-2 rounded-md border border-border/70 px-2 py-1.5">
                    <div className="min-w-0">
                      <p className="truncate text-xs font-medium">{backup.file_name}</p>
                      <div className="mt-1 flex flex-wrap gap-1">
                        {backup.providers.map((provider) => (
                          <Badge
                            key={provider.provider_id}
                            variant={provider.status === "available" ? "success" : provider.status === "failed" ? "destructive" : "outline"}
                            className="text-[9px] capitalize"
                          >
                            {provider.provider_label}: {provider.status.replaceAll("-", " ")}
                          </Badge>
                        ))}
                      </div>
                      <p className="text-[10px] text-muted-foreground">
                        {formatBytes(backup.size_bytes)} · {formatDate(backup.created_at)}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-1">
                      <Button size="icon" variant="ghost" title="Restore backup" disabled={busyKey !== null} onClick={() => void download(backup.snapshot_id)}>
                        {downloading ? <RefreshCw className="animate-spin" /> : <Download />}
                      </Button>
                      {backup.providers.some((provider) => provider.status !== "available") ? (
                        <Button size="icon" variant="ghost" title="Retry backup" disabled={busyKey !== null} onClick={() => void retry(backup.snapshot_id)}>
                          <RefreshCw className={busyKey === `retry:${backup.snapshot_id}` ? "animate-spin" : ""} />
                        </Button>
                      ) : null}
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Delete cloud backup"
                        disabled={busyKey !== null}
                        onClick={() => setDeleteSnapshotId(backup.snapshot_id)}
                      >
                        {deleting ? <RefreshCw className="animate-spin" /> : <Trash2 />}
                      </Button>
                    </div>
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>
      </div>

      <Checkbox
        checked={openFolderAfterRestore}
        onChange={(event) => setOpenFolderAfterRestore(event.target.checked)}
        label="Open the backups folder after restoring a snapshot"
        className="text-xs"
      />

      {dashboard?.activities.length ? (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Backup activity</CardTitle>
            <CardDescription>Recent background uploads, restores, retries, and retention cleanup.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-1.5">
            {dashboard.activities.slice(0, 12).map((activity) => (
              <div key={activity.id} className="flex items-start justify-between gap-3 border-b border-border/50 pb-1.5 text-xs last:border-0">
                <span className={activity.level === "error" ? "text-destructive" : "text-foreground"}>{activity.message}</span>
                <span className="shrink-0 text-[10px] text-muted-foreground">{formatDate(activity.created_at)}</span>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}

      <div className="flex items-start gap-2 text-[11px] text-muted-foreground">
        <Check className="mt-0.5 size-3.5 shrink-0 text-primary" />
        <p>Backups are uploaded as versioned snapshot files. The manifest is written last, so interrupted uploads do not appear as restorable snapshots.</p>
      </div>

      <ConfirmDialog
        open={deleteSnapshotId !== null}
        onOpenChange={(open) => !open && setDeleteSnapshotId(null)}
        title="Delete cloud backup?"
        description="This removes the snapshot archive and manifest from every connected provider. It cannot be undone from the launcher."
        confirmLabel="Delete backup"
        destructive
        onConfirm={() => void deleteBackup()}
      />
    </div>
  );
}

interface BackupInstanceOption {
  id: string;
  settings: {
    name: string;
    pack_version: string;
    backups_enabled: boolean;
    backup_retention_override: number | null;
  };
}

const PLANNED_PROVIDERS = [
  { name: "S3-compatible storage", description: "Amazon S3, MinIO, Backblaze B2, and other S3-compatible endpoints." },
  { name: "OneDrive", description: "Microsoft OneDrive personal and business storage." },
  { name: "FTP / FTPS", description: "Traditional FTP servers, with encrypted FTPS support planned." },
  { name: "SFTP", description: "SSH-based file transfer for self-hosted backup servers." },
] as const;

function PlannedProviders() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">More providers</CardTitle>
        <CardDescription>These providers use the same backup format and storage interface.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-2 sm:grid-cols-2">
        {PLANNED_PROVIDERS.map((provider) => (
          <div key={provider.name} className="flex items-start justify-between gap-3 rounded-md border border-border/70 bg-muted/20 p-2.5">
            <div className="flex min-w-0 items-start gap-2">
              <Cloud className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0">
                <p className="text-xs font-medium">{provider.name}</p>
                <p className="mt-0.5 text-[11px] text-muted-foreground">{provider.description}</p>
              </div>
            </div>
            <Badge className="shrink-0" variant="warning">
              Coming soon
            </Badge>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

export function LauncherBackupsSettings({
  instances,
  retentionLimit,
  onRetentionLimitChange,
}: {
  instances: readonly BackupInstanceOption[];
  retentionLimit: number;
  onRetentionLimitChange: (value: number) => void;
}) {
  const [selectedInstanceId, setSelectedInstanceId] = useState(instances[0]?.id ?? "");
  const selectedInstance = instances.find((instance) => instance.id === selectedInstanceId) ?? instances[0];

  useEffect(() => {
    if (selectedInstance && selectedInstance.id !== selectedInstanceId) setSelectedInstanceId(selectedInstance.id);
  }, [selectedInstance, selectedInstanceId]);

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-3">
          <div>
            <CardTitle>Cloud backups</CardTitle>
            <CardDescription>
              Enabled instances are watched automatically. Configure providers here, then inspect or restore snapshots by instance.
            </CardDescription>
          </div>
          <Button size="sm" variant="outline" className="shrink-0" onClick={() => void openUrl(BACKUP_GUIDE_URL).catch(() => undefined)}>
            <BookOpen className="size-3.5" /> Connection guide
          </Button>
        </CardHeader>
        <CardContent>
          <div className="mb-3 flex flex-wrap items-end gap-3 border-b border-border/60 pb-3">
            <div className="space-y-1">
              <label htmlFor="backup-retention-limit" className="text-xs font-medium">
                Default retention per provider
              </label>
              <Input
                id="backup-retention-limit"
                type="number"
                min={1}
                max={1000}
                className="w-28"
                value={retentionLimit}
                onChange={(event) => onRetentionLimitChange(Math.max(1, Math.min(1000, Number(event.target.value) || 1)))}
              />
            </div>
            <p className="max-w-lg text-[11px] text-muted-foreground">
              Older cloud snapshots are removed independently from each provider. Local files are never deleted.
            </p>
          </div>
          {selectedInstance ? (
            <div className="flex flex-wrap items-center gap-3">
              <Select
                aria-label="Backup instance"
                className="min-w-56 flex-1"
                value={selectedInstance.id}
                onChange={(event) => setSelectedInstanceId(event.target.value)}
              >
                {instances.map((instance) => (
                  <option key={instance.id} value={instance.id}>
                    {instance.settings.name || `GTNH ${instance.settings.pack_version || instance.id}`}
                  </option>
                ))}
              </Select>
              <Badge variant={selectedInstance.settings.backups_enabled ? "success" : "warning"}>
                {selectedInstance.settings.backups_enabled ? "Backups enabled" : "Backups disabled"}
              </Badge>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">Install an instance to configure its cloud backups.</p>
          )}
          <p className="mt-2 text-[11px] text-muted-foreground">The per-instance toggle lives in Instances → Settings → General.</p>
        </CardContent>
      </Card>

      {selectedInstance ? <BackupsPanel instanceId={selectedInstance.id} enabled={selectedInstance.settings.backups_enabled} /> : null}
      <PlannedProviders />
    </div>
  );
}
