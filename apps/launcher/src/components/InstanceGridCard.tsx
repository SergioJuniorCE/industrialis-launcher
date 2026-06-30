import type { DragEvent } from "react";
import {
  Copy,
  FolderOpen,
  Loader2,
  Pencil,
  Play,
  RefreshCw,
  SlidersHorizontal,
  Square,
  Trash2,
  X,
  ArrowUp,
  ArrowDown,
  ChevronLeft,
  ChevronRight,
  GripVertical,
} from "lucide-react";
import { Button } from "./ui/button";
import { Progress } from "./ui/progress";
import { InstanceAvatar } from "./InstanceAvatar";
import { PackVersionStatus } from "./PackVersionStatus";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "./ui/context-menu";
import { cn } from "../lib/utils";
import type { BackgroundProcess } from "../lib/background-processes";
import { formatDownloadProgress, stageLabel } from "../lib/background-processes";
import type { PackVersionMeta } from "../lib/pack-version-status";

export interface InstanceGridCardData {
  id: string;
  size_bytes: number;
  settings: {
    name: string;
    pack_version: string;
  };
  icon_path?: string | null;
}

function displayName(inst: InstanceGridCardData): string {
  return inst.settings.name || `GTNH ${inst.settings.pack_version || inst.id}`;
}

function packVersion(inst: InstanceGridCardData): string {
  return inst.settings.pack_version || inst.id;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatSize(bytes: number, refreshing: boolean): string {
  if (bytes === 0 && refreshing) return "…";
  return formatBytes(bytes);
}

function formatRowUpdateProgress(proc: BackgroundProcess): string {
  const progress = formatDownloadProgress(proc);
  return `${stageLabel(proc.stage)} · ${progress}`;
}

export function InstanceGridCard({
  inst,
  selected,
  sizesRefreshing,
  onSelect,
  running,
  starting,
  busy,
  onLaunch,
  onKill,
  onOpenSettings,
  onOpenFolder,
  onDelete,
  isInstanceBusy,
  deleteProcess,
  updateProcess,
  reinstallProcess,
  onCancelDelete,
  versions,
  onUpdatePack,
  onReinstall,
  onCopy,
  onRename,
  onIconChanged,
  onIconError,
  canMoveUp,
  canMoveDown,
  onMoveUp,
  onMoveDown,
  isDragOver,
  onDragHandleStart,
  onDragEnd,
  onDragOver,
  onDragLeave,
  onDrop,
}: {
  inst: InstanceGridCardData;
  selected: boolean;
  sizesRefreshing: boolean;
  onSelect: () => void;
  running: boolean;
  starting: boolean;
  busy: boolean;
  onLaunch: () => void;
  onKill: () => void;
  onOpenSettings: () => void;
  onOpenFolder: () => void;
  onDelete: () => void;
  isInstanceBusy: (id: string) => boolean;
  deleteProcess?: BackgroundProcess;
  updateProcess?: BackgroundProcess;
  reinstallProcess?: BackgroundProcess;
  onCancelDelete: () => void;
  versions: Record<string, PackVersionMeta> | null;
  onUpdatePack: () => void;
  onReinstall: () => void;
  onCopy: () => void;
  onRename: () => void;
  onIconChanged: () => void;
  onIconError: (message: string) => void;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMoveUp: () => void;
  onMoveDown: () => void;
  isDragOver?: boolean;
  onDragHandleStart?: (event: DragEvent<HTMLDivElement>) => void;
  onDragEnd?: () => void;
  onDragOver?: (event: DragEvent<HTMLDivElement>) => void;
  onDragLeave?: () => void;
  onDrop?: (event: DragEvent<HTMLDivElement>) => void;
}) {
  const name = displayName(inst);
  const deleting = deleteProcess?.status === "running";
  const updating = updateProcess?.status === "running";
  const reinstalling = reinstallProcess?.status === "running";
  const packBusy = deleting || updating || reinstalling;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          draggable={!packBusy && Boolean(onDragHandleStart)}
          className={cn(
            "group/card relative flex flex-col rounded-xl border p-2.5 transition-colors",
            packBusy && "opacity-80",
            isDragOver && "border-primary/60 ring-2 ring-primary/25",
            !packBusy && onDragHandleStart && "cursor-grab active:cursor-grabbing",
            selected
              ? "instance-row-selected border-primary/50 bg-primary/10 shadow-sm"
              : "border-border/70 bg-card/40 hover:border-primary/35 hover:bg-primary/8",
          )}
          onDragStart={onDragHandleStart}
          onDragEnd={onDragEnd}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
        >
          {!packBusy && onDragHandleStart && (
            <div
              className="absolute left-1.5 top-1.5 flex items-center gap-0.5 rounded bg-card/80 p-0.5 text-muted-foreground shadow-sm backdrop-blur-sm"
              title="Drag card to reorder"
            >
              <GripVertical className="size-3.5 shrink-0" />
            </div>
          )}
          <button
            type="button"
            onClick={onSelect}
            className="flex min-w-0 flex-1 flex-col items-center gap-2 text-left"
            draggable={false}
          >
            <InstanceAvatar
              instanceId={inst.id}
              name={name}
              iconPath={inst.icon_path}
              size="md"
              loading={packBusy}
              onIconChanged={onIconChanged}
              onError={onIconError}
              onOpenFolder={onOpenFolder}
              className="size-14 text-base rounded-xl"
            />
            <div className="w-full min-w-0 text-center">
              <div className="text-xs font-semibold truncate leading-tight">{name}</div>
              <div className="mt-0.5 text-[10px] text-muted-foreground truncate leading-tight">
                {deleting ? (
                  <>Deleting… {(deleteProcess.pct * 100).toFixed(0)}%</>
                ) : updating ? (
                  <>{formatRowUpdateProgress(updateProcess)}</>
                ) : reinstalling ? (
                  <>{formatRowUpdateProgress(reinstallProcess!)}</>
                ) : (
                  <>
                    {packVersion(inst)} · {formatSize(inst.size_bytes, sizesRefreshing)}
                  </>
                )}
              </div>
            </div>
          </button>

          <div className="mt-2 flex items-center justify-center gap-0.5">
            {!packBusy && (
              <>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7"
                  title="Move earlier in group"
                  disabled={!canMoveUp}
                  draggable={false}
                  onClick={(e) => {
                    e.stopPropagation();
                    onMoveUp();
                  }}
                >
                  <ChevronLeft className="size-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7"
                  title="Move later in group"
                  disabled={!canMoveDown}
                  draggable={false}
                  onClick={(e) => {
                    e.stopPropagation();
                    onMoveDown();
                  }}
                >
                  <ChevronRight className="size-3.5" />
                </Button>
              </>
            )}
            {deleting ? (
              <Button
                variant="ghost"
                size="icon"
                className="size-7 text-destructive hover:text-destructive"
                title="Cancel deletion"
                onClick={(e) => {
                  e.stopPropagation();
                  onCancelDelete();
                }}
              >
                <X className="size-3.5" />
              </Button>
            ) : running ? (
              <Button
                variant="ghost"
                size="icon"
                className="size-7 text-destructive hover:text-destructive"
                title="Stop"
                onClick={(e) => {
                  e.stopPropagation();
                  onKill();
                }}
              >
                <Square className="size-3.5 fill-current" />
              </Button>
            ) : starting ? (
              <Button variant="ghost" size="icon" className="size-7" disabled>
                <Loader2 className="size-3.5 animate-spin" />
              </Button>
            ) : (
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                title="Launch"
                disabled={busy || isInstanceBusy(inst.id)}
                onClick={(e) => {
                  e.stopPropagation();
                  onLaunch();
                }}
              >
                <Play className="size-3.5" />
              </Button>
            )}
            {!packBusy && (
              <PackVersionStatus
                currentVersion={packVersion(inst)}
                versions={versions}
                onUpdate={onUpdatePack}
                disabled={busy || running || starting || isInstanceBusy(inst.id)}
                compact
              />
            )}
            {!deleting && (
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                title="Settings"
                onClick={(e) => {
                  e.stopPropagation();
                  onOpenSettings();
                }}
              >
                <SlidersHorizontal className="size-3.5" />
              </Button>
            )}
          </div>

          {running && !packBusy && (
            <span
              className="status-running absolute right-2 top-2 size-2 rounded-full animate-pulse"
              title="Running"
            />
          )}

          {(deleting || updating || reinstalling) && (
            <div className="mt-2">
              <Progress
                value={
                  (deleting
                    ? deleteProcess!.pct
                    : updating
                      ? updateProcess!.pct
                      : reinstallProcess!.pct) * 100
                }
                className="h-1"
              />
            </div>
          )}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <ContextMenuItem onSelect={onMoveUp} disabled={!canMoveUp || packBusy}>
          <ArrowUp />
          Move up
        </ContextMenuItem>
        <ContextMenuItem onSelect={onMoveDown} disabled={!canMoveDown || packBusy}>
          <ArrowDown />
          Move down
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onOpenFolder}>
          <FolderOpen />
          Open folder
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onOpenSettings}>
          <SlidersHorizontal />
          Settings
        </ContextMenuItem>
        <ContextMenuItem onSelect={onRename} disabled={deleting}>
          <Pencil />
          Rename…
        </ContextMenuItem>
        {deleting ? (
          <ContextMenuItem onSelect={onCancelDelete} className="text-destructive focus:text-destructive">
            <X />
            Cancel deletion
          </ContextMenuItem>
        ) : running ? (
          <ContextMenuItem onSelect={onKill} className="text-destructive focus:text-destructive">
            <Square className="fill-current" />
            Stop
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={onLaunch} disabled={busy || starting || isInstanceBusy(inst.id)}>
            <Play />
            {starting ? "Launching…" : "Launch"}
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem
          onSelect={onCopy}
          disabled={running || starting || packBusy || isInstanceBusy(inst.id)}
        >
          <Copy />
          Copy instance…
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={onReinstall}
          disabled={running || starting || packBusy || isInstanceBusy(inst.id)}
        >
          <RefreshCw />
          Clean reinstall…
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={onDelete}
          disabled={running || starting || deleting || isInstanceBusy(inst.id)}
          className="text-destructive focus:text-destructive"
        >
          <Trash2 />
          Delete instance
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}