import { ArrowUpCircle, CircleCheck } from "lucide-react";
import { Button } from "./ui/button";
import {
  getPackVersionInfo,
  type PackVersionMeta,
} from "../lib/pack-version-status";

export function PackVersionStatus({
  currentVersion,
  versions,
  onUpdate,
  disabled = false,
  compact = false,
}: {
  currentVersion: string;
  versions: Record<string, PackVersionMeta> | null;
  onUpdate?: () => void;
  disabled?: boolean;
  compact?: boolean;
}) {
  const info = getPackVersionInfo(currentVersion, versions);

  if (info.status === "unknown") {
    return null;
  }

  if (info.status === "up-to-date") {
    return (
      <span
        className="inline-flex items-center gap-1 text-[#37d67a] shrink-0"
        title="Pack is up to date"
      >
        <CircleCheck className={compact ? "size-3.5" : "size-4"} />
        {!compact && <span className="text-[11px] font-medium">Up to date</span>}
      </span>
    );
  }

  const label = compact ? "Update" : `Update to ${info.latestVersion}`;

  return (
    <Button
      type="button"
      variant={compact ? "ghost" : "outline"}
      size="sm"
      className={
        compact
          ? "h-6 px-1.5 gap-1 text-primary hover:text-primary"
          : "h-7 gap-1.5 text-primary border-primary/40 hover:bg-primary/10"
      }
      title={`Update pack to ${info.latestVersion}`}
      aria-label={`Update pack to ${info.latestVersion}`}
      onClick={(e) => {
        e.stopPropagation();
        onUpdate?.();
      }}
      disabled={disabled || !onUpdate}
    >
      <ArrowUpCircle className={compact ? "size-3.5" : "size-4"} />
      <span className={compact ? "text-[11px] font-medium" : "text-xs font-medium"}>{label}</span>
    </Button>
  );
}