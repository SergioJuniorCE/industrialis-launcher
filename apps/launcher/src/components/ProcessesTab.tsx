import { useEffect, useRef } from "react";
import { Loader2, X } from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Progress } from "./ui/progress";
import { ScrollArea } from "./ui/scroll-area";
import {
  formatDownloadProgress,
  formatDownloadSpeed,
  operationLabel,
  runningProcessCount,
  sortedProcesses,
  stageLabel,
  type BackgroundProcess,
} from "../lib/background-processes";

export function ProcessCard({
  proc,
  selected = false,
  onSelect,
  onDismiss,
  onCancelDelete,
}: {
  proc: BackgroundProcess;
  selected?: boolean;
  onSelect?: () => void;
  onDismiss?: (key: string) => void;
  onCancelDelete?: (id: string) => void;
}) {
  const canDismiss = proc.status === "done" || proc.status === "failed";
  const clickable = Boolean(onSelect);

  return (
    <div
      className={`border-b border-border last:border-b-0 transition-colors ${
        selected ? "bg-muted" : "bg-card"
      } ${clickable ? "hover:bg-muted/60 cursor-pointer" : ""}`}
    >
      <Button
        type="button"
        variant="ghost"
        className="h-auto w-full justify-start space-y-1.5 px-2.5 py-2 text-left font-normal"
        onClick={onSelect}
        disabled={!clickable}
      >
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {proc.status === "running" && (
                <Loader2 className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
              )}
              <span className="text-xs font-medium truncate">{proc.name}</span>
            </div>
            <p className="text-xs text-muted-foreground mt-0.5">
              {operationLabel(proc.operation)}
              {proc.status === "running" && proc.stage !== "done" && ` · ${stageLabel(proc.stage)}`}
              {proc.status === "running" && proc.stage === "downloading" && formatDownloadSpeed(proc.speedMbps) && (
                ` · ${formatDownloadSpeed(proc.speedMbps)}`
              )}
              {proc.status === "done" && " · Complete"}
              {proc.status === "failed" && " · Failed"}
            </p>
          </div>
          {proc.logs.length > 0 && (
            <Badge variant="secondary" className="shrink-0 h-5 text-[10px]">
              {proc.logs.length} lines
            </Badge>
          )}
        </div>

        {proc.status === "running" && (
          <div className="space-y-1" onClick={(e) => e.stopPropagation()}>
            <Progress value={proc.pct * 100} />
            <p className="text-xs text-muted-foreground text-right">{formatDownloadProgress(proc)}</p>
          </div>
        )}
      </Button>

      {(canDismiss || (proc.status === "running" && proc.operation === "delete" && onCancelDelete)) && (
        <div className="flex justify-end gap-0.5 px-2 pb-1.5 -mt-0.5">
          {proc.status === "running" && proc.operation === "delete" && onCancelDelete && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs text-destructive hover:text-destructive"
              onClick={(e) => {
                e.stopPropagation();
                onCancelDelete(proc.id);
              }}
            >
              <X className="size-3.5" />
              Cancel
            </Button>
          )}
          {canDismiss && onDismiss && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={(e) => {
                e.stopPropagation();
                onDismiss(proc.key);
              }}
            >
              <X className="size-3.5" />
              Dismiss
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

function ProcessLogPanel({ proc }: { proc: BackgroundProcess }) {
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [proc.logs]);

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="shrink-0 px-3 py-2 border-b border-border space-y-1.5">
        <div className="flex items-center gap-2">
          {proc.status === "running" && (
            <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />
          )}
          <h2 className="font-medium truncate">{proc.name}</h2>
          <Badge
            variant={
              proc.status === "failed" ? "destructive" : proc.status === "done" ? "success" : "secondary"
            }
          >
            {proc.status === "running"
              ? stageLabel(proc.stage)
              : proc.status === "done"
                ? "Complete"
                : "Failed"}
          </Badge>
        </div>
        <p className="text-xs text-muted-foreground">
          {operationLabel(proc.operation)} · {proc.id}
        </p>
        {proc.status === "running" && (
          <div className="space-y-1 max-w-md">
            <Progress value={proc.pct * 100} />
            <p className="text-xs text-muted-foreground">{formatDownloadProgress(proc)}</p>
          </div>
        )}
      </div>

      <div
        ref={logRef}
        className="flex-1 min-h-0 overflow-y-auto bg-black/40 font-mono text-[11px] px-3 py-2 space-y-0.5 leading-relaxed"
      >
        {proc.logs.length === 0 ? (
          <p className="text-muted-foreground">No log output yet.</p>
        ) : (
          proc.logs.map((line, i) => (
            <div
              key={i}
              className={line.startsWith("Error:") ? "text-destructive" : "text-muted-foreground"}
            >
              {line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

export function ProcessesTab({
  processes,
  selectedKey,
  onSelect,
  onDismiss,
  onCancelDelete,
}: {
  processes: Map<string, BackgroundProcess>;
  selectedKey: string | null;
  onSelect: (key: string) => void;
  onDismiss: (key: string) => void;
  onCancelDelete?: (id: string) => void;
}) {
  const items = sortedProcesses(processes);
  const running = runningProcessCount(processes);
  const selected = selectedKey ? processes.get(selectedKey) : undefined;

  return (
    <div className="flex-1 flex overflow-hidden min-h-0">
      <div className="w-[min(280px,32%)] shrink-0 border-r border-border flex flex-col overflow-hidden">
        <div className="shrink-0 px-3 py-2 border-b border-border">
          <h1 className="text-xs font-semibold">Background processes</h1>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            {running > 0
              ? `${running} running · ${items.length} total`
              : items.length > 0
                ? `${items.length} recent`
                : "No active or recent processes"}
          </p>
        </div>
        <ScrollArea className="flex-1">
          <div className="border-b border-border">
            {items.length === 0 ? (
              <p className="text-xs text-muted-foreground p-3">
                Install, update, or delete an instance to see progress here.
              </p>
            ) : (
              items.map((proc) => (
                <ProcessCard
                  key={proc.key}
                  proc={proc}
                  selected={proc.key === selectedKey}
                  onSelect={() => onSelect(proc.key)}
                  onDismiss={onDismiss}
                  onCancelDelete={onCancelDelete}
                />
              ))
            )}
          </div>
        </ScrollArea>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden min-w-0">
        {selected ? (
          <ProcessLogPanel proc={selected} />
        ) : (
          <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground p-4">
            {items.length > 0
              ? "Select a process to view its full log."
              : "Process logs will appear here."}
          </div>
        )}
      </div>
    </div>
  );
}