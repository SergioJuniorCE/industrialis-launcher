import { useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { ProcessCard } from "./ProcessesTab";
import {
  runningProcessCount,
  sortedProcesses,
  type BackgroundProcess,
} from "../lib/background-processes";

interface ProcessesDropdownProps {
  processes: Map<string, BackgroundProcess>;
  onDismiss: (key: string) => void;
  onCancelDelete?: (id: string) => void;
  onOpenProcesses: (key?: string) => void;
}

export function ProcessesDropdown({
  processes,
  onDismiss,
  onCancelDelete,
  onOpenProcesses,
}: ProcessesDropdownProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const items = sortedProcesses(processes);
  const running = runningProcessCount(processes);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <Button
        variant="ghost"
        size="sm"
        className="gap-1.5 pr-2"
        onClick={() => {
          if (items.length === 0) {
            onOpenProcesses();
            return;
          }
          setOpen((v) => !v);
        }}
        onDoubleClick={() => onOpenProcesses()}
        aria-expanded={open}
        aria-haspopup="listbox"
        title="Background processes"
      >
        <Loader2 className={`size-4 ${running > 0 ? "animate-spin" : ""}`} />
        <span className="text-xs">Processes</span>
        {running > 0 && (
          <Badge variant="secondary" className="h-5 min-w-5 justify-center px-1.5">
            {running}
          </Badge>
        )}
      </Button>

      {open && items.length > 0 && (
        <div className="absolute right-0 top-full z-50 mt-1 w-72 max-h-[min(20rem,70vh)] overflow-y-auto rounded-md border border-border bg-popover p-1 shadow-lg">
          <div className="flex items-center justify-between px-1 pb-2">
            <span className="text-xs font-medium text-muted-foreground">
              {running > 0 ? `${running} running` : "Recent processes"}
            </span>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 text-xs"
              onClick={() => {
                setOpen(false);
                onOpenProcesses();
              }}
            >
              View all
            </Button>
          </div>
          <div className="border border-border rounded-md overflow-hidden divide-y divide-border">
            {items.map((proc) => (
              <ProcessCard
                key={proc.key}
                proc={proc}
                onSelect={() => {
                  setOpen(false);
                  onOpenProcesses(proc.key);
                }}
                onDismiss={onDismiss}
                onCancelDelete={onCancelDelete}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}