import { useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { ImagePlus, ImageOff, Loader2 } from "lucide-react";
import { cn } from "../lib/utils";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "./ui/context-menu";

type InstanceAvatarSize = "sm" | "md";

const sizeClasses: Record<InstanceAvatarSize, string> = {
  sm: "size-8 text-xs",
  md: "size-10 text-sm",
};

export function InstanceAvatar({
  instanceId,
  name,
  iconPath,
  size = "sm",
  loading = false,
  onIconChanged,
  onError,
  className,
}: {
  instanceId: string;
  name: string;
  iconPath?: string | null;
  size?: InstanceAvatarSize;
  loading?: boolean;
  onIconChanged?: () => void;
  onError?: (message: string) => void;
  className?: string;
}) {
  const [busy, setBusy] = useState(false);
  const showSpinner = loading || busy;
  const imageSrc = iconPath ? convertFileSrc(iconPath) : null;

  const uploadIcon = async () => {
    setBusy(true);
    try {
      const picked = await invoke<string | null>("browse_instance_icon_file");
      if (!picked) return;
      await invoke("set_instance_icon", { id: instanceId, sourcePath: picked });
      onIconChanged?.();
    } catch (e) {
      onError?.(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeIcon = async () => {
    setBusy(true);
    try {
      await invoke("clear_instance_icon", { id: instanceId });
      onIconChanged?.();
    } catch (e) {
      onError?.(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          className={cn(
            "instance-avatar rounded-lg flex items-center justify-center font-semibold shrink-0 overflow-hidden",
            sizeClasses[size],
            className,
          )}
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
        <ContextMenuItem onSelect={() => void uploadIcon()} disabled={showSpinner}>
          <ImagePlus />
          Upload custom icon
        </ContextMenuItem>
        {iconPath ? (
          <ContextMenuItem onSelect={() => void removeIcon()} disabled={showSpinner}>
            <ImageOff />
            Remove custom icon
          </ContextMenuItem>
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  );
}