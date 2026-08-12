import { ArrowUpCircle, Download, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { Button } from "./ui/button";
import type { LauncherUpdateState } from "../lib/launcher-update";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";

export function LauncherUpdateDialog({
  state,
  onInstall,
  onDismiss,
  onRetry,
}: {
  state: LauncherUpdateState;
  onInstall: () => void;
  onDismiss: () => void;
  onRetry: () => void;
}) {
  const open = ["available", "downloading", "deferred", "manual", "installing", "failed"].includes(state.status);
  const busy = state.status === "downloading" || state.status === "installing";
  const progress = Math.round((state.progress ?? 0) * 100);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !busy) onDismiss();
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
                ? "The release page is open. Download and run the signed installer there to update."
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
              <Button onClick={onInstall}>
                <RefreshCw className="size-4" /> Try again
              </Button>
            </>
          ) : state.status === "manual" ? (
            <>
              <Button variant="ghost" onClick={onDismiss}>
                Later
              </Button>
              <Button onClick={onInstall}>
                <ExternalLink className="size-4" /> Open release page
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" onClick={onDismiss} disabled={busy}>
                Later
              </Button>
              <Button onClick={onInstall} disabled={busy}>
                {busy ? <Loader2 className="size-4 animate-spin" /> : <Download className="size-4" />}
                {state.status === "installing" ? "Restarting…" : "Download and restart"}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
