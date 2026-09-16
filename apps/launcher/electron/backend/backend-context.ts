import type { RunningProcess } from "./process-manager";
import type { DownloadProgress, InstanceSettings, LauncherSettings } from "./types";

export interface LaunchArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
  keepModIdentities?: string[];
  group?: string;
  name?: string;
  sourceId?: string;
  newId?: string;
  newName?: string;
  settings?: InstanceSettings;
  username?: string;
  accountId?: string;
  pathOverride?: string;
  javaPath?: string;
  subpath?: string | null;
  relPath?: string;
  content?: string;
  persist?: boolean;
  sourcePath?: string;
  iconId?: string;
  identity?: string;
  oldName?: string;
  order?: string[];
  collapsed?: boolean;
  direction?: string;
  ids?: string[] | null;
  full?: boolean;
  groupName?: string;
  idPreset?: string;
  instanceId?: string;
  enabled?: boolean;
  launcherSettings?: LauncherSettings;
  fileName?: string;
  snapshotId?: string;
  clientId?: string;
  providerId?: string;
}

export interface LaunchState {
  running: Map<string, RunningProcess>;
  installInProgress: Set<string>;
  updateInProgress: Set<string>;
  reinstallInProgress: Set<string>;
  copyInProgress: Set<string>;
  deleteCancel: Map<string, { cancelled: boolean }>;
}

export interface BackendContext {
  readonly state: LaunchState;
  emit(event: string, payload: unknown): void;
  emitProgress(payload: DownloadProgress): void;
  emitLog(id: string, stream: string, line: string): void;
  knownInstanceIds(): Promise<Set<string>>;
  saveAndRefreshSize(id: string, settings: InstanceSettings): Promise<void>;
  notifyInstanceOperationWaiters(): void;
  waitForInstanceOperations(): Promise<void>;
  persistRunningProcesses(): Promise<void>;
  flushConsoleLog(id: string): Promise<void>;
  compactConsoleLog(id: string): Promise<void>;
}
