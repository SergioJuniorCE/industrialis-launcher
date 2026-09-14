import { ArrowUpCircle, CircleAlert, CircleCheck, Loader2, RefreshCw } from "lucide-react";
import type { LauncherUpdateState } from "../lib/launcher-update";
import { cn } from "../lib/utils";

const DIALOG_STATUSES = new Set<LauncherUpdateState["status"]>(["available", "downloading", "deferred", "manual", "installing", "failed"]);

export function LauncherUpdateStatus({ state, onCheck, onOpen }: { state: LauncherUpdateState; onCheck: () => void; onOpen: () => void }) {
  const { label, busy, disabled, Icon } = updateStatusPresentation(state);

  return (
    <button
      type="button"
      className={cn(
        "ml-auto inline-flex h-5 items-center gap-1.5 rounded px-1.5 text-[11px] transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        disabled ? "cursor-default" : "hover:bg-primary/12 hover:text-foreground",
        (state.status === "available" || state.status === "deferred" || state.status === "manual") && "text-primary",
        state.status === "failed" && "text-destructive",
      )}
      onClick={DIALOG_STATUSES.has(state.status) ? onOpen : onCheck}
      disabled={disabled}
      aria-label={`${label}. Current version ${state.current_version || "unknown"}.`}
      title={state.status === "up-to-date" ? `Launcher ${state.current_version}. Click to check again.` : label}
    >
      <Icon className={cn("size-3", busy && "animate-spin")} aria-hidden="true" />
      <span aria-live="polite">{label}</span>
    </button>
  );
}

function updateStatusPresentation(state: LauncherUpdateState) {
  const progress = Math.round((state.progress ?? 0) * 100);
  const label =
    state.status === "available"
      ? `Update ${state.version ?? "available"}`
      : state.status === "downloading"
        ? `Downloading update ${progress}%`
        : state.status === "deferred" || state.status === "manual"
          ? `Update ${state.version ?? "pending"} pending`
          : state.status === "installing"
            ? "Restarting to update"
            : state.status === "up-to-date"
              ? "Launcher up to date"
              : state.status === "failed"
                ? "Update check failed"
                : state.status === "disabled"
                  ? "Updates unavailable"
                  : "Checking for updates";
  const busy = state.status === "checking" || state.status === "downloading" || state.status === "installing" || state.status === "idle";
  const disabled = state.status === "disabled" || busy;
  const Icon = busy
    ? Loader2
    : state.status === "up-to-date"
      ? CircleCheck
      : state.status === "failed"
        ? CircleAlert
        : state.status === "available" || state.status === "deferred" || state.status === "manual"
          ? ArrowUpCircle
          : RefreshCw;

  return { label, busy, disabled, Icon };
}
