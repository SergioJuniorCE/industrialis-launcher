export type ProcessOperation = "install" | "update" | "delete";

export type ProcessStatus = "running" | "done" | "failed";

export interface BackgroundProcess {
  key: string;
  id: string;
  name: string;
  operation: ProcessOperation;
  stage: string;
  pct: number;
  speedMbps?: number;
  downloadedMb?: number;
  totalMb?: number;
  logs: string[];
  status: ProcessStatus;
  startedAt: number;
}

export interface DlProgressEvent {
  stage: string;
  pct: number;
  id?: string;
  name?: string;
  operation?: ProcessOperation | "preview";
  log_line?: string;
  speed_mbps?: number;
  downloaded_mb?: number;
  total_mb?: number;
}

export function formatDownloadSpeed(mbps?: number): string | null {
  if (mbps == null || !Number.isFinite(mbps) || mbps <= 0) return null;
  return `${mbps.toFixed(1)} MB/s`;
}

export function formatDownloadProgress(proc: Pick<BackgroundProcess, "stage" | "pct" | "speedMbps" | "downloadedMb" | "totalMb">): string {
  const pct = `${(proc.pct * 100).toFixed(0)}%`;
  if (proc.stage !== "downloading") return pct;
  const speed = formatDownloadSpeed(proc.speedMbps);
  const parts = [pct];
  if (proc.downloadedMb != null) {
    if (proc.totalMb != null) {
      parts.push(`${proc.downloadedMb.toFixed(1)} / ${proc.totalMb.toFixed(1)} MB`);
    } else {
      parts.push(`${proc.downloadedMb.toFixed(1)} MB`);
    }
  }
  if (speed) parts.push(speed);
  return parts.join(" · ");
}

export function processKey(operation: ProcessOperation, id: string): string {
  return `${operation}:${id}`;
}

export function getInstanceProcess(
  processes: Map<string, BackgroundProcess>,
  operation: ProcessOperation,
  id: string,
): BackgroundProcess | undefined {
  return processes.get(processKey(operation, id));
}

export function inferOperation(event: DlProgressEvent): ProcessOperation | null {
  if (event.operation === "install" || event.operation === "update" || event.operation === "delete") {
    return event.operation;
  }
  if (event.stage === "deleting") return "delete";
  if (event.stage === "updating") return "update";
  if (event.operation === "preview") return null;
  return "install";
}

export function operationLabel(operation: ProcessOperation): string {
  switch (operation) {
    case "install":
      return "Installing";
    case "update":
      return "Updating";
    case "delete":
      return "Deleting";
  }
}

export function stageLabel(stage: string): string {
  switch (stage) {
    case "downloading":
      return "Downloading";
    case "extracting":
      return "Extracting";
    case "updating":
      return "Updating";
    case "deleting":
      return "Deleting";
    case "done":
      return "Complete";
    default:
      return stage;
  }
}

export function createProcess(
  operation: ProcessOperation,
  id: string,
  name: string,
  initialLog?: string,
): BackgroundProcess {
  return {
    key: processKey(operation, id),
    id,
    name,
    operation,
    stage: operation === "delete" ? "deleting" : operation === "update" ? "updating" : "downloading",
    pct: 0,
    logs: initialLog ? [initialLog] : [],
    status: "running",
    startedAt: Date.now(),
  };
}

export function resolveOperation(
  processes: Map<string, BackgroundProcess>,
  event: DlProgressEvent,
): ProcessOperation | null {
  const inferred = inferOperation(event);
  if (inferred) return inferred;
  if (!event.id) return null;
  for (const proc of processes.values()) {
    if (proc.id === event.id && proc.status === "running") {
      return proc.operation;
    }
  }
  return null;
}

export function applyDlProgressEvent(
  processes: Map<string, BackgroundProcess>,
  event: DlProgressEvent,
): Map<string, BackgroundProcess> {
  const id = event.id;
  const operation = resolveOperation(processes, event);
  if (!operation || !id) return processes;

  const key = processKey(operation, id);
  const next = new Map(processes);
  const current = next.get(key) ?? createProcess(operation, id, event.name ?? id);

  if (event.stage === "done") {
    next.set(key, {
      ...current,
      name: event.name ?? current.name,
      stage: "done",
      pct: 1,
      status: "done",
    });
    return next;
  }

  const logs = event.log_line ? [...current.logs, event.log_line] : current.logs;
  const downloading = event.stage === "downloading";
  next.set(key, {
    ...current,
    name: event.name ?? current.name,
    stage: event.stage,
    pct: event.pct,
    speedMbps: downloading ? event.speed_mbps : undefined,
    downloadedMb: downloading ? event.downloaded_mb : undefined,
    totalMb: downloading ? event.total_mb : undefined,
    logs,
    status: "running",
  });
  return next;
}

export function markProcessFailed(
  processes: Map<string, BackgroundProcess>,
  operation: ProcessOperation,
  id: string,
  error: unknown,
): Map<string, BackgroundProcess> {
  const key = processKey(operation, id);
  const next = new Map(processes);
  const current = next.get(key);
  const message = `Error: ${error}`;
  if (current) {
    next.set(key, {
      ...current,
      status: "failed",
      logs: [...current.logs, message],
    });
  } else {
    next.set(key, {
      ...createProcess(operation, id, id),
      status: "failed",
      logs: [message],
    });
  }
  return next;
}

export function dismissProcess(
  processes: Map<string, BackgroundProcess>,
  key: string,
): Map<string, BackgroundProcess> {
  const next = new Map(processes);
  next.delete(key);
  return next;
}

export function isInstanceBusy(processes: Map<string, BackgroundProcess>, id: string): boolean {
  for (const proc of processes.values()) {
    if (proc.id === id && proc.status === "running") return true;
  }
  return false;
}

export function runningProcessCount(processes: Map<string, BackgroundProcess>): number {
  let count = 0;
  for (const proc of processes.values()) {
    if (proc.status === "running") count += 1;
  }
  return count;
}

export function sortedProcesses(processes: Map<string, BackgroundProcess>): BackgroundProcess[] {
  return [...processes.values()].sort((a, b) => {
    const statusOrder = (s: ProcessStatus) => (s === "running" ? 0 : 1);
    const diff = statusOrder(a.status) - statusOrder(b.status);
    if (diff !== 0) return diff;
    return b.startedAt - a.startedAt;
  });
}