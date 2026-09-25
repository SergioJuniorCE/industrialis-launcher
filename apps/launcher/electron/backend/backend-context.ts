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

/** Narrow per-operation views of the flat IPC arg bag. Internal backend
 * functions take these instead of the god-interface LaunchArgs so each
 * module declares only the fields it reads. The Electron IPC boundary in
 * index.ts still receives LaunchArgs and maps fields explicitly. */
export interface CopyInstanceArgs {
  sourceId?: string;
  newId?: string;
  newName?: string;
}

export interface DownloadInstallArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
  name?: string;
  group?: string;
}

export interface PreviewUpdateArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
}

export interface UpdateInstanceArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
  keepModIdentities?: string[];
}

export interface ReinstallInstanceArgs {
  id: string;
  packVersion?: string;
  javaType?: string;
}

export interface SetInstanceIconArgs {
  id: string;
  sourcePath?: string;
}

export interface SetInstanceIconFromLibraryArgs {
  id: string;
  iconId?: string;
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
  /** Read-only operation-state queries. Mutations go through `state`
   * directly so ownership stays visible at the call site. */
  isRunning(id: string): boolean;
  isInstallInProgress(id: string): boolean;
  isUpdateInProgress(id: string): boolean;
  isReinstallInProgress(id: string): boolean;
  isCopyInProgress(id: string): boolean;
  isDeleteInProgress(id: string): boolean;
  getDeleteCancel(id: string): { cancelled: boolean } | undefined;
}
