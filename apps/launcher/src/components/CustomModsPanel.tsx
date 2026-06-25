import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PackagePlus, Trash2 } from "lucide-react";
import { Button } from "./ui/button";
import { ScrollArea } from "./ui/scroll-area";

interface CustomModEntry {
  identity: string;
  filename: string;
  size_bytes: number;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function CustomModsPanel({ instanceId }: { instanceId: string }) {
  const [mods, setMods] = useState<CustomModEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadMods = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<CustomModEntry[]>("list_custom_mods", { id: instanceId });
      setMods(list);
    } catch (e) {
      setError(String(e));
    }
  }, [instanceId]);

  useEffect(() => {
    void loadMods();
  }, [loadMods]);

  const addMod = async () => {
    setLoading(true);
    setError(null);
    try {
      const picked = await invoke<string | null>("browse_custom_mod");
      if (!picked) return;
      await invoke("add_custom_mod", { id: instanceId, sourcePath: picked });
      await loadMods();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const removeMod = async (identity: string) => {
    setLoading(true);
    setError(null);
    try {
      await invoke("remove_custom_mod", { id: instanceId, identity });
      await loadMods();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col gap-2 h-full min-h-[200px]">
      <p className="text-[11px] text-muted-foreground leading-snug">
        Mods added here persist across pack updates. Leftover pack files from older versions are replaced on update.
      </p>

      <div className="flex items-center gap-2">
        <Button size="sm" disabled={loading} onClick={() => void addMod()}>
          <PackagePlus className="size-3.5" />
          Add mod
        </Button>
        <span className="text-xs text-muted-foreground">
          {mods.length} custom mod{mods.length === 1 ? "" : "s"}
        </span>
      </div>

      {error && <p className="text-xs text-destructive">{error}</p>}

      <ScrollArea className="flex-1 min-h-0 rounded-md border border-border">
        <div className="divide-y divide-border">
          {mods.length === 0 ? (
            <p className="text-xs text-muted-foreground p-2">No custom mods added yet.</p>
          ) : (
            mods.map((mod) => (
              <div
                key={mod.identity}
                className="flex items-center justify-between gap-2 px-2 py-1.5 hover:bg-muted/30"
              >
                <div className="min-w-0">
                  <div className="font-medium text-xs truncate">{mod.filename}</div>
                  <div className="text-[10px] text-muted-foreground font-mono truncate">
                    {mod.identity} · {formatBytes(mod.size_bytes)}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={loading}
                  title="Remove custom mod"
                  onClick={() => void removeMod(mod.identity)}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </div>
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}