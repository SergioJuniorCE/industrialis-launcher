import { useRef, useState } from "react";
import { ArrowUpCircle, Download, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { Button } from "./ui/button";
import type { LauncherUpdateState } from "../lib/launcher-update";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";

export function LauncherUpdateDialog({
  state,
  open,
  onInstall,
  onDismiss,
  onRetry,
}: {
  state: LauncherUpdateState;
  open?: boolean;
  onInstall: () => void | Promise<void>;
  onDismiss: () => void;
  onRetry: () => void;
}) {
  const [installRequested, setInstallRequested] = useState(false);
  const installRequestRef = useRef(false);
  const dialogOpen = open ?? ["available", "downloading", "deferred", "manual", "installing", "failed"].includes(state.status);
  const busy = state.status === "downloading" || state.status === "installing";
  const installBusy = busy || installRequested;
  const progress = Math.round((state.progress ?? 0) * 100);

  const handleInstall = async () => {
    if (installRequestRef.current || busy) return;
    installRequestRef.current = true;
    setInstallRequested(true);
    try {
      await onInstall();
    } finally {
      installRequestRef.current = false;
      setInstallRequested(false);
    }
  };

  return (
    <Dialog
      open={dialogOpen}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !installBusy) onDismiss();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ArrowUpCircle className="size-5 text-primary" />
            Launcher update
          </DialogTitle>
          <DialogDescription>
            {state.status === "failed"
              ? "The launcher update could not be completed."
              : state.status === "manual"
                ? `Version ${state.version ?? "a new launcher version"} is available. You are running ${state.current_version}. The release page is open. Download and run the signed installer there to update.`
                : state.status === "deferred"
                  ? "The update will wait until active instances and operations finish."
                  : `Version ${state.version} is available. You are running ${state.current_version}.`}
          </DialogDescription>
        </DialogHeader>

        {state.body && <p className="max-h-28 overflow-auto rounded border bg-muted/30 p-2 text-xs text-muted-foreground whitespace-pre-wrap">{state.body}</p>}

        {state.status === "downloading" && (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span className="flex items-center gap-2">
                <Loader2 className="size-3.5 animate-spin" /> Downloading update
              </span>
              <span>{progress}%</span>
            </div>
            <div className="h-2 overflow-hidden rounded-full bg-secondary">
              <div className="h-full bg-primary transition-[width]" style={{ width: `${progress}%` }} />
            </div>
          </div>
        )}

        {state.error && <p className="text-sm text-destructive">{state.error}</p>}

        <div className="flex justify-end gap-2 pt-2">
          {state.status === "failed" ? (
            <>
              <Button variant="ghost" onClick={onDismiss}>
                Later
              </Button>
              <Button onClick={onRetry}>
                <RefreshCw className="size-4" /> Retry
              </Button>
            </>
          ) : state.status === "deferred" ? (
            <>
              <Button variant="ghost" onClick={onDismiss}>
                Later
              </Button>
              <Button onClick={handleInstall} disabled={installBusy}>
                <RefreshCw className="size-4" /> Try again
              </Button>
            </>
          ) : state.status === "manual" ? (
            <>
              <Button variant="ghost" onClick={onDismiss}>
                Later
              </Button>
              <Button onClick={handleInstall} disabled={installBusy}>
                <ExternalLink className="size-4" /> Open release page
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" onClick={onDismiss} disabled={installBusy}>
                Later
              </Button>
              <Button onClick={handleInstall} disabled={installBusy}>
                {busy ? <Loader2 className="size-4 animate-spin" /> : <Download className="size-4" />}
                {state.status === "available" ? "Install update" : state.status === "installing" ? "Restarting…" : `Downloading ${progress}%`}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
