import type { GtnhVersion } from "../stores/launcher-store";
import { useRef, type DragEvent } from "react";
import { Copy, Ellipsis, FolderOpen, Images, Loader2, Pencil, Play, RefreshCw, SlidersHorizontal, Square, Trash2, X, GripVertical } from "lucide-react";
import { Button } from "./ui/button";
import { Progress } from "./ui/progress";
import { InstanceAvatar, type InstanceAvatarHandle } from "./InstanceAvatar";
import { PackVersionStatus } from "./PackVersionStatus";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "./ui/context-menu";
import { cn } from "../lib/utils";
import type { BackgroundProcess } from "../lib/background-processes";
import { formatDownloadProgress, getInstanceProcess, isInstanceBusy, stageLabel } from "../lib/background-processes";
import { useLauncherStore } from "../stores/launcher-store";

export interface InstanceGridCardData {
  id: string;
  size_bytes: number;
  settings: {
    name: string;
    pack_version: string;
  };
  icon_path?: string | null;
}

export interface InstanceGridCardCommands {
  launch: (id: string) => void;
  kill: (id: string) => void;
  openFolder: (id: string) => void;
  delete: (id: string) => void;
  cancelDelete: (id: string) => void;
  iconChanged: () => void;
  iconError: (message: string) => void;
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
  commands,
  isDragging,
  isDragOver,
  onDragHandleStart,
  onDragEnd,
  onDragOver,
  onDragLeave,
  onDrop,
}: {
  inst: InstanceGridCardData;
  commands: InstanceGridCardCommands;
  isDragging?: boolean;
  isDragOver?: boolean;
  onDragHandleStart?: (event: DragEvent<HTMLDivElement>) => void;
  onDragEnd?: () => void;
  onDragOver?: (event: DragEvent<HTMLDivElement>) => void;
  onDragLeave?: () => void;
  onDrop?: (event: DragEvent<HTMLDivElement>) => void;
}) {
  const selected = useLauncherStore((state) => state.selectedInstanceId === inst.id);
  const sizesRefreshing = useLauncherStore((state) => state.sizesRefreshing);
  const running = useLauncherStore((state) => state.runningInstanceIds.has(inst.id));
  const starting = useLauncherStore((state) => state.launching === inst.id);
  const busy = useLauncherStore((state) => state.launching !== null);
  const instanceBusy = useLauncherStore((state) => isInstanceBusy(state.processes, inst.id));
  const deleteProcess = useLauncherStore((state) => getInstanceProcess(state.processes, "delete", inst.id));
  const updateProcess = useLauncherStore((state) => getInstanceProcess(state.processes, "update-pack", inst.id));
  const reinstallProcess = useLauncherStore((state) => getInstanceProcess(state.processes, "reinstall", inst.id));
  const versions = useLauncherStore((state) => state.gtnhVersions);
  const setSelectedInstanceId = useLauncherStore((state) => state.setSelectedInstanceId);
  const openInstanceSettings = useLauncherStore((state) => state.openInstanceSettings);
  const setUpdatePackInstanceId = useLauncherStore((state) => state.setUpdatePackInstanceId);
  const setReinstallInstanceId = useLauncherStore((state) => state.setReinstallInstanceId);
  const setCopyInstanceId = useLauncherStore((state) => state.setCopyInstanceId);
  const setRenameInstanceId = useLauncherStore((state) => state.setRenameInstanceId);
  const cardRef = useRef<HTMLDivElement>(null);
  const avatarRef = useRef<InstanceAvatarHandle>(null);
  const name = displayName(inst);
  const deleting = deleteProcess?.status === "running";
  const updating = updateProcess?.status === "running";
  const reinstalling = reinstallProcess?.status === "running";
  const packBusy = deleting || updating || reinstalling;
  const canLaunch = !running && !busy && !instanceBusy;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          ref={cardRef}
          className={cn(
            "instance-grid-card w-36 shrink-0 group/card relative flex flex-col rounded-lg p-2.5 transition-colors",
            packBusy && "opacity-80",
            isDragging && "opacity-40",
            isDragOver && "bg-primary/15 ring-2 ring-primary/35",
            selected ? "instance-row-selected" : "hover:bg-accent/70",
          )}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
        >
          <Button
            variant="ghost"
            size="icon"
            className="instance-card-menu absolute right-1.5 top-1.5 z-10 size-7 bg-card/80 text-muted-foreground shadow-sm backdrop-blur-sm hover:text-foreground"
            aria-label={`More actions for ${name}`}
            aria-haspopup="menu"
            title="More actions"
            onClick={(event) => {
              event.stopPropagation();
              setSelectedInstanceId(inst.id);
              const bounds = event.currentTarget.getBoundingClientRect();
              cardRef.current?.dispatchEvent(
                new MouseEvent("contextmenu", {
                  bubbles: true,
                  cancelable: true,
                  button: 2,
                  buttons: 2,
                  clientX: bounds.right,
                  clientY: bounds.bottom,
                  view: window,
                }),
              );
            }}
          >
            <Ellipsis className="size-4" />
          </Button>

          {!packBusy && onDragHandleStart && <InstanceDragHandle onDragHandleStart={onDragHandleStart} onDragEnd={onDragEnd} />}
          <InstanceCardLaunchTarget
            setSelectedInstanceId={setSelectedInstanceId}
            inst={inst}
            canLaunch={canLaunch}
            commands={commands}
            avatarRef={avatarRef}
            name={name}
            packBusy={packBusy}
            deleting={deleting}
            deleteProcess={deleteProcess}
            updating={updating}
            updateProcess={updateProcess}
            reinstalling={reinstalling}
            reinstallProcess={reinstallProcess}
            sizesRefreshing={sizesRefreshing}
          />

          <InstanceCardStatusActions
            deleting={deleting}
            commands={commands}
            inst={inst}
            running={running}
            starting={starting}
            packBusy={packBusy}
            versions={versions}
            setUpdatePackInstanceId={setUpdatePackInstanceId}
            busy={busy}
            instanceBusy={instanceBusy}
          />

          {running && !packBusy && <span className="status-running absolute right-10 top-2.5 size-2 rounded-full animate-pulse" title="Running" />}

          <InstanceCardProgress processes={[deleteProcess, updateProcess, reinstallProcess]} />
        </div>
      </ContextMenuTrigger>
      <InstanceCardMenu
        commands={commands}
        inst={inst}
        openInstanceSettings={openInstanceSettings}
        avatarRef={avatarRef}
        packBusy={packBusy}
        setRenameInstanceId={setRenameInstanceId}
        deleting={deleting}
        running={running}
        busy={busy}
        starting={starting}
        instanceBusy={instanceBusy}
        setCopyInstanceId={setCopyInstanceId}
        setReinstallInstanceId={setReinstallInstanceId}
      />
    </ContextMenu>
  );
}

function InstanceCardMenu({
  commands,
  inst,
  openInstanceSettings,
  avatarRef,
  packBusy,
  setRenameInstanceId,
  deleting,
  running,
  busy,
  starting,
  instanceBusy,
  setCopyInstanceId,
  setReinstallInstanceId,
}: {
  commands: InstanceGridCardCommands;
  inst: InstanceGridCardData;
  openInstanceSettings: (id: string) => void;
  avatarRef: React.RefObject<InstanceAvatarHandle | null>;
  packBusy: boolean;
  setRenameInstanceId: (update: React.SetStateAction<string | null>) => void;
  deleting: boolean;
  running: boolean;
  busy: boolean;
  starting: boolean;
  instanceBusy: boolean;
  setCopyInstanceId: (update: React.SetStateAction<string | null>) => void;
  setReinstallInstanceId: (update: React.SetStateAction<string | null>) => void;
}) {
  return (
    <ContextMenuContent className="w-52">
      <ContextMenuItem onSelect={() => commands.openFolder(inst.id)}>
        <FolderOpen />
        Open folder
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => openInstanceSettings(inst.id)}>
        <SlidersHorizontal />
        Settings
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void avatarRef.current?.openIconGallery()} disabled={packBusy}>
        <Images />
        Change icon…
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => setRenameInstanceId(inst.id)} disabled={deleting}>
        <Pencil />
        Rename…
      </ContextMenuItem>
      {deleting ? (
        <ContextMenuItem onSelect={() => commands.cancelDelete(inst.id)} className="text-destructive focus:text-destructive">
          <X />
          Cancel deletion
        </ContextMenuItem>
      ) : running ? (
        <ContextMenuItem onSelect={() => commands.kill(inst.id)} className="text-destructive focus:text-destructive">
          <Square className="fill-current" />
          Stop
        </ContextMenuItem>
      ) : (
        <ContextMenuItem onSelect={() => commands.launch(inst.id)} disabled={busy || starting || instanceBusy}>
          <Play />
          {starting ? "Launching…" : "Launch"}
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => setCopyInstanceId(inst.id)} disabled={running || starting || packBusy || instanceBusy}>
        <Copy />
        Copy instance…
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => setReinstallInstanceId(inst.id)} disabled={running || starting || packBusy || instanceBusy}>
        <RefreshCw />
        Clean reinstall…
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => commands.delete(inst.id)}
        disabled={running || starting || deleting || instanceBusy}
        className="text-destructive focus:text-destructive"
      >
        <Trash2 />
        Delete instance
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function InstanceCardStatusActions({
  deleting,
  commands,
  inst,
  running,
  starting,
  packBusy,
  versions,
  setUpdatePackInstanceId,
  busy,
  instanceBusy,
}: {
  deleting: boolean;
  commands: InstanceGridCardCommands;
  inst: InstanceGridCardData;
  running: boolean;
  starting: boolean;
  packBusy: boolean;
  versions: Record<string, GtnhVersion> | null;
  setUpdatePackInstanceId: (update: React.SetStateAction<string | null>) => void;
  busy: boolean;
  instanceBusy: boolean;
}) {
  return (
    <div className="mt-2 flex items-center justify-center gap-0.5">
      {deleting ? (
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-destructive hover:text-destructive"
          title="Cancel deletion"
          onClick={(e) => {
            e.stopPropagation();
            commands.cancelDelete(inst.id);
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
            commands.kill(inst.id);
          }}
        >
          <Square className="size-3.5 fill-current" />
        </Button>
      ) : starting ? (
        <Button variant="ghost" size="icon" className="size-7" disabled>
          <Loader2 className="size-3.5 animate-spin" />
        </Button>
      ) : null}
      {!packBusy && (
        <PackVersionStatus
          currentVersion={packVersion(inst)}
          versions={versions}
          onUpdate={() => setUpdatePackInstanceId(inst.id)}
          disabled={busy || running || starting || instanceBusy}
          compact
        />
      )}
    </div>
  );
}

function InstanceCardCaption({
  name,
  deleting,
  deleteProcess,
  updating,
  updateProcess,
  reinstalling,
  reinstallProcess,
  inst,
  sizesRefreshing,
}: {
  name: string;
  deleting: boolean;
  deleteProcess: BackgroundProcess | undefined;
  updating: boolean;
  updateProcess: BackgroundProcess | undefined;
  reinstalling: boolean;
  reinstallProcess: BackgroundProcess | undefined;
  inst: InstanceGridCardData;
  sizesRefreshing: boolean;
}) {
  return (
    <div className="w-full min-w-0 text-center">
      <div className="text-xs font-semibold truncate leading-tight">{name}</div>
      <div className="mt-0.5 text-[10px] text-muted-foreground truncate leading-tight">
        {deleting ? (
          <>Deleting… {(deleteProcess!.pct * 100).toFixed(0)}%</>
        ) : updating ? (
          <>{formatRowUpdateProgress(updateProcess!)}</>
        ) : reinstalling ? (
          <>{formatRowUpdateProgress(reinstallProcess!)}</>
        ) : (
          <>
            {packVersion(inst)} · {formatSize(inst.size_bytes, sizesRefreshing)}
          </>
        )}
      </div>
    </div>
  );
}

function InstanceDragHandle({
  onDragHandleStart,
  onDragEnd,
}: {
  onDragHandleStart: (event: DragEvent<HTMLDivElement>) => void;
  onDragEnd: (() => void) | undefined;
}) {
  return (
    <div
      draggable
      className="instance-drag-handle absolute left-1.5 top-1.5 flex cursor-grab items-center gap-0.5 rounded bg-card/80 p-0.5 text-muted-foreground shadow-sm backdrop-blur-sm active:cursor-grabbing"
      title="Drag to reorder"
      onDragStart={(event) => {
        event.stopPropagation();
        const card = event.currentTarget.parentElement;
        if (card) {
          event.dataTransfer.setDragImage(card, card.offsetWidth / 2, card.offsetHeight / 2);
        }
        onDragHandleStart(event);
      }}
      onDragEnd={onDragEnd}
    >
      <GripVertical className="size-3.5 shrink-0" />
    </div>
  );
}

function InstanceCardLaunchTarget({
  setSelectedInstanceId,
  inst,
  canLaunch,
  commands,
  avatarRef,
  name,
  packBusy,
  deleting,
  deleteProcess,
  updating,
  updateProcess,
  reinstalling,
  reinstallProcess,
  sizesRefreshing,
}: {
  setSelectedInstanceId: (update: React.SetStateAction<string | null>) => void;
  inst: InstanceGridCardData;
  canLaunch: boolean;
  commands: InstanceGridCardCommands;
  avatarRef: React.RefObject<InstanceAvatarHandle | null>;
  name: string;
  packBusy: boolean;
  deleting: boolean;
  deleteProcess: BackgroundProcess | undefined;
  updating: boolean;
  updateProcess: BackgroundProcess | undefined;
  reinstalling: boolean;
  reinstallProcess: BackgroundProcess | undefined;
  sizesRefreshing: boolean;
}) {
  return (
    <button
      type="button"
      onClick={() => setSelectedInstanceId(inst.id)}
      onDoubleClick={() => {
        if (canLaunch) {
          commands.launch(inst.id);
        }
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" && canLaunch) {
          event.preventDefault();
          commands.launch(inst.id);
        }
      }}
      className="flex min-w-0 flex-1 flex-col items-center gap-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
      draggable={false}
      title="Double-click to launch"
    >
      <InstanceAvatar
        ref={avatarRef}
        instanceId={inst.id}
        name={name}
        iconPath={inst.icon_path}
        size="md"
        loading={packBusy}
        onIconChanged={commands.iconChanged}
        onError={commands.iconError}
        onOpenFolder={() => commands.openFolder(inst.id)}
        className="size-14 text-base rounded-xl"
      />
      <InstanceCardCaption
        name={name}
        deleting={deleting}
        deleteProcess={deleteProcess}
        updating={updating}
        updateProcess={updateProcess}
        reinstalling={reinstalling}
        reinstallProcess={reinstallProcess}
        inst={inst}
        sizesRefreshing={sizesRefreshing}
      />
    </button>
  );
}

function InstanceCardProgress({ processes }: { processes: Array<BackgroundProcess | undefined> }) {
  const active = processes.find((proc) => proc?.status === "running");
  if (!active) return null;
  return (
    <div className="mt-2">
      <Progress value={active.pct * 100} className="h-1" />
    </div>
  );
}
