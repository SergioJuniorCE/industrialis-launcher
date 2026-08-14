import { useMemo, useState } from "react";
import { ArrowDown, ArrowUp, ArrowUpDown, Check, Search } from "lucide-react";
import {
  filterJavaInstallations,
  sameJavaPath,
  sortJavaInstallations,
  type JavaInfo,
  type JavaSortDirection,
  type JavaSortKey,
} from "../lib/java-installations";
import { cn } from "../lib/utils";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { ScrollArea } from "./ui/scroll-area";

interface JavaSort {
  key: JavaSortKey;
  direction: JavaSortDirection;
}

export function JavaInstallationPicker({
  installations,
  refreshing,
  selectedPath,
  onBrowse,
  onSelect,
}: {
  installations: JavaInfo[];
  refreshing: boolean;
  selectedPath: string | null;
  onBrowse: () => Promise<void>;
  onSelect: (path: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<JavaSort>({ key: "version", direction: "desc" });
  const filteredInstallations = useMemo(
    () => sortJavaInstallations(filterJavaInstallations(installations, query), sort.key, sort.direction),
    [installations, query, sort],
  );
  const selectedInstallation = installations.find((java) => sameJavaPath(java.path, selectedPath));

  const changeSort = (key: JavaSortKey) => {
    setSort((current) =>
      current.key === key ? { key, direction: current.direction === "asc" ? "desc" : "asc" } : { key, direction: key === "version" ? "desc" : "asc" },
    );
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="default-java-path">Default Java executable</Label>
        <div className="flex flex-wrap gap-2">
          <Input
            id="default-java-path"
            className="min-w-64 flex-1 font-mono text-xs"
            value={selectedPath ?? ""}
            placeholder="Automatic (JAVA_HOME / PATH)"
            readOnly
          />
          <Button type="button" variant="outline" onClick={() => void onBrowse()}>
            Browse
          </Button>
          <Button type="button" variant="ghost" disabled={!selectedPath} onClick={() => onSelect(null)}>
            Use automatic
          </Button>
        </div>

        <div className="flex min-h-6 flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {selectedInstallation ? (
            <>
              <Badge variant="secondary">Java {selectedInstallation.version}</Badge>
              <Badge variant="outline">{selectedInstallation.architecture}</Badge>
              <span>{selectedInstallation.vendor}</span>
            </>
          ) : selectedPath ? (
            <span>Custom executable. Refresh detection to inspect its version and architecture.</span>
          ) : (
            <span>Automatically uses JAVA_HOME or the first compatible Java on PATH.</span>
          )}
        </div>
        <p className="text-xs text-muted-foreground">Used by every instance unless that instance has a Java location override.</p>
      </div>

      <section className="overflow-hidden rounded-lg border border-border/80" aria-labelledby="java-installations-title">
        <div className="flex flex-wrap items-end justify-between gap-3 px-4 py-3">
          <div>
            <h3 id="java-installations-title" className="text-sm font-semibold">
              Detected installations
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">Select a row to use that Java as the launcher-wide default.</p>
          </div>
          <span className="text-xs tabular-nums text-muted-foreground">
            {query ? `${filteredInstallations.length} of ${installations.length}` : installations.length} installation{installations.length === 1 ? "" : "s"}
          </span>
        </div>

        <div className="border-t border-border/70 px-4 py-3">
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="pl-9"
              placeholder="Search version, architecture, vendor, or path"
              aria-label="Search Java installations"
            />
          </div>
        </div>

        <div className="border-t border-border/80 bg-muted/20">
          <div className="grid grid-cols-[7.5rem_7.5rem_minmax(0,1fr)] gap-3 border-b border-border/70 px-4 py-2">
            <SortHeader label="Version" sortKey="version" sort={sort} onSort={changeSort} />
            <SortHeader label="Architecture" sortKey="architecture" sort={sort} onSort={changeSort} />
            <SortHeader label="Path" sortKey="path" sort={sort} onSort={changeSort} />
          </div>

          <ScrollArea className="h-64" role="radiogroup" aria-label="Detected Java installations">
            {filteredInstallations.length > 0 ? (
              <div className="p-1.5">
                {filteredInstallations.map((java) => {
                  const selected = sameJavaPath(java.path, selectedPath);
                  return (
                    <button
                      key={java.path}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      className={cn(
                        "grid w-full grid-cols-[7.5rem_7.5rem_minmax(0,1fr)] items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm text-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                        selected ? "bg-primary/15" : "hover:bg-muted/70",
                      )}
                      onClick={() => onSelect(java.path)}
                    >
                      <span className="flex min-w-0 items-center gap-2 font-medium">
                        <Check className={cn("size-3.5 shrink-0 text-primary", selected ? "opacity-100" : "opacity-0")} aria-hidden="true" />
                        <span className="truncate">{java.version}</span>
                      </span>
                      <span className="truncate">{java.architecture}</span>
                      <span className="truncate font-mono text-xs text-muted-foreground" title={`${java.vendor} - ${java.path}`}>
                        {java.path}
                      </span>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="flex h-full min-h-40 flex-col items-center justify-center gap-1 px-6 text-center">
                <p className="text-sm font-medium">
                  {refreshing ? "Scanning for Java installations" : installations.length === 0 ? "No Java installations found" : "No matching installations"}
                </p>
                <p className="text-xs text-muted-foreground">
                  {refreshing
                    ? "Detected runtimes will appear here."
                    : installations.length === 0
                      ? "Refresh detection or browse to a Java executable manually."
                      : "Try a version number, architecture, vendor, or part of the path."}
                </p>
              </div>
            )}
          </ScrollArea>
        </div>
      </section>
    </div>
  );
}

function SortHeader({ label, sortKey, sort, onSort }: { label: string; sortKey: JavaSortKey; sort: JavaSort; onSort: (key: JavaSortKey) => void }) {
  const active = sort.key === sortKey;
  const SortIcon = !active ? ArrowUpDown : sort.direction === "asc" ? ArrowUp : ArrowDown;

  return (
    <div>
      <button
        type="button"
        className={cn(
          "flex w-full items-center gap-1.5 rounded-sm text-left text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          active && "text-foreground",
        )}
        onClick={() => onSort(sortKey)}
      >
        <span>{label}</span>
        <SortIcon className="size-3.5" aria-hidden="true" />
        <span className="sr-only">Sort {active && sort.direction === "asc" ? "descending" : "ascending"}</span>
      </button>
    </div>
  );
}
