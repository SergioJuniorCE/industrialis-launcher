import { forwardRef, useImperativeHandle, useState } from "react";
import { createPortal } from "react-dom";
import { invoke, convertFileSrc } from "../lib/desktop";
import { FolderOpen, Images, ImagePlus, ImageOff, Loader2 } from "lucide-react";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "./ui/context-menu";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./ui/dialog";

type InstanceAvatarSize = "sm" | "md";

const sizeClasses: Record<InstanceAvatarSize, string> = {
  sm: "size-8 text-xs",
  md: "size-10 text-sm",
};

interface InstanceIconEntry {
  id: string;
  label: string;
  path: string;
  built_in: boolean;
}

export interface InstanceAvatarHandle {
  openIconGallery: () => Promise<void>;
}

export const InstanceAvatar = forwardRef<
  InstanceAvatarHandle,
  {
    instanceId: string;
    name: string;
    iconPath?: string | null;
    size?: InstanceAvatarSize;
    loading?: boolean;
    onIconChanged?: () => void;
    onError?: (message: string) => void;
    onOpenFolder?: () => void;
    className?: string;
  }
>(function InstanceAvatar({ instanceId, name, iconPath, size = "sm", loading = false, onIconChanged, onError, onOpenFolder, className }, ref) {
  const [busy, setBusy] = useState(false);
  const [galleryOpen, setGalleryOpen] = useState(false);
  const [galleryLoading, setGalleryLoading] = useState(false);
  const [galleryError, setGalleryError] = useState<string | null>(null);
  const [icons, setIcons] = useState<InstanceIconEntry[]>([]);
  const showSpinner = loading || busy;
  const imageSrc = iconPath ? convertFileSrc(iconPath) : null;

  const reportError = (error: unknown) => {
    const message = String(error);
    setGalleryError(message);
    onError?.(message);
  };

  const openGallery = async () => {
    setGalleryOpen(true);
    setGalleryError(null);
    setGalleryLoading(true);
    try {
      setIcons(await invoke<InstanceIconEntry[]>("list_instance_icons"));
    } catch (error) {
      reportError(error);
    } finally {
      setGalleryLoading(false);
    }
  };

  useImperativeHandle(ref, () => ({ openIconGallery: openGallery }));

  const applyLibraryIcon = async (iconId: string) => {
    setBusy(true);
    setGalleryError(null);
    try {
      await invoke("set_instance_icon_from_library", { id: instanceId, iconId });
      onIconChanged?.();
      setGalleryOpen(false);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const importCustomIcon = async () => {
    setGalleryError(null);
    try {
      const picked = await invoke<string | null>("browse_instance_icon_file");
      if (!picked) return;
      setBusy(true);
      const imported = await invoke<InstanceIconEntry>("import_instance_icon", { sourcePath: picked });
      setIcons((current) => [...current, imported]);
      await invoke("set_instance_icon_from_library", { id: instanceId, iconId: imported.id });
      onIconChanged?.();
      setGalleryOpen(false);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const removeIcon = async () => {
    setBusy(true);
    try {
      await invoke("clear_instance_icon", { id: instanceId });
      onIconChanged?.();
      setGalleryOpen(false);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const openIconsFolder = async () => {
    try {
      await invoke("open_instance_icons_folder");
    } catch (error) {
      reportError(error);
    }
  };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className={cn("instance-avatar rounded-lg flex items-center justify-center font-semibold shrink-0 overflow-hidden", sizeClasses[size], className)}
            onContextMenu={(e) => e.stopPropagation()}
          >
            {showSpinner ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : imageSrc ? (
              <img src={imageSrc} alt="" className="size-full object-cover" draggable={false} />
            ) : (
              name.charAt(0).toUpperCase()
            )}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          {onOpenFolder ? (
            <>
              <ContextMenuItem onSelect={onOpenFolder}>
                <FolderOpen />
                Open folder
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          ) : null}
          <ContextMenuItem onSelect={() => void openGallery()} disabled={showSpinner}>
            <Images />
            Choose icon…
          </ContextMenuItem>
          {iconPath ? (
            <ContextMenuItem onSelect={() => void removeIcon()} disabled={showSpinner}>
              <ImageOff />
              Use initials
            </ContextMenuItem>
          ) : null}
        </ContextMenuContent>
      </ContextMenu>

      {galleryOpen
        ? createPortal(
            <Dialog open={galleryOpen} onOpenChange={setGalleryOpen}>
              <DialogContent className="w-[min(92vw,32rem)] max-w-lg">
                <DialogHeader>
                  <DialogTitle>Choose an instance icon</DialogTitle>
                  <DialogDescription>Pick a built-in icon or add an image to your reusable icon library.</DialogDescription>
                </DialogHeader>

                <div className="max-h-[50vh] overflow-y-auto rounded-md border border-border p-2">
                  {galleryLoading ? (
                    <div className="flex min-h-32 items-center justify-center text-muted-foreground">
                      <Loader2 className="size-5 animate-spin" />
                    </div>
                  ) : icons.length ? (
                    <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
                      {icons.map((icon) => (
                        <button
                          key={icon.id}
                          type="button"
                          className="min-w-0 rounded-md border border-border bg-muted/25 p-2 text-left transition-colors hover:border-primary/55 hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
                          onClick={() => void applyLibraryIcon(icon.id)}
                          disabled={busy}
                          aria-label={`Use ${icon.label}`}
                        >
                          <span className="flex aspect-square items-center justify-center overflow-hidden rounded-md bg-background">
                            <img src={convertFileSrc(icon.path)} alt="" className="size-full object-contain" draggable={false} />
                          </span>
                          <span className="mt-2 block truncate text-xs font-medium">{icon.label}</span>
                          <span className="block text-[10px] text-muted-foreground">{icon.built_in ? "Built in" : "Custom"}</span>
                        </button>
                      ))}
                    </div>
                  ) : (
                    <div className="flex min-h-32 items-center justify-center text-sm text-muted-foreground">No icons in the library.</div>
                  )}
                </div>

                {galleryError ? <p className="mt-3 text-sm text-destructive">{galleryError}</p> : null}

                <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border pt-4">
                  <div className="flex flex-wrap gap-2">
                    <Button variant="outline" onClick={() => void importCustomIcon()} disabled={busy || galleryLoading}>
                      <ImagePlus />
                      Add custom icon
                    </Button>
                    <Button variant="ghost" onClick={() => void openIconsFolder()} disabled={busy}>
                      <FolderOpen />
                      Icons folder
                    </Button>
                  </div>
                  <Button variant="secondary" onClick={() => setGalleryOpen(false)} disabled={busy}>
                    Done
                  </Button>
                </div>
              </DialogContent>
            </Dialog>,
            document.body,
          )
        : null}
    </>
  );
});
