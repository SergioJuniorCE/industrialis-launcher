import { useMemo, useState } from "react";
import type { ServerVersionDetail } from "@industrialis/server-contracts";

export type VersionFilter = "all" | "stable" | "beta";

export function toVersionDetails(tags: string[]): ServerVersionDetail[] {
  return tags.map((tag) => {
    const channel = /stable/i.test(tag) ? "stable" : /beta|rc|alpha|pre|nightly/i.test(tag) ? "beta" : /^\d+\.\d+\.\d+$/.test(tag) ? "stable" : "beta";
    return {
      tag,
      packVersion: tag,
      channel,
      title: channel === "stable" ? "Stable release" : "Beta release",
      releaseDate: null,
      maxJavaVersion: null,
    } satisfies ServerVersionDetail;
  });
}

export function filterVersionDetails(versions: ServerVersionDetail[], filter: VersionFilter): ServerVersionDetail[] {
  if (filter === "stable") return versions.filter((version) => version.channel === "stable");
  if (filter === "beta") return versions.filter((version) => version.channel === "beta");
  return versions;
}

export default function VersionPicker({
  versions,
  value,
  onChange,
  loading,
  name = "version",
  emptyMessage = "No versions match this filter.",
}: {
  versions: ServerVersionDetail[];
  value: string;
  onChange: (tag: string) => void;
  loading: boolean;
  name?: string;
  emptyMessage?: string;
}) {
  const [filter, setFilter] = useState<VersionFilter>("all");
  const filtered = useMemo(() => filterVersionDetails(versions, filter), [versions, filter]);

  return (
    <div>
      <div className="flex gap-2">
        <select
          aria-label="Version channel filter"
          value={filter}
          onChange={(event) => setFilter(event.currentTarget.value as VersionFilter)}
          className="h-9 border border-line bg-ink px-3 text-xs outline-none focus:border-signal"
        >
          <option value="all">All versions</option>
          <option value="stable">Stable only</option>
          <option value="beta">Beta only</option>
        </select>
        <div aria-hidden className="grid h-9 flex-1 place-items-center border border-line bg-ink px-3 font-mono text-[10px] text-dim">
          {filtered.length} release{filtered.length === 1 ? "" : "s"}
        </div>
      </div>

      <div role="radiogroup" aria-label="GTNH version" className="mt-3 max-h-64 space-y-2 overflow-y-auto border border-line bg-ink p-2">
        {loading && <p className="px-1 py-2 text-xs text-dim">Loading versions…</p>}
        {!loading && filtered.length === 0 && <p className="px-1 py-2 text-xs text-dim">{emptyMessage}</p>}
        {filtered.map((version) => {
          const selected = value === version.tag;
          return (
            <button
              key={version.tag}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => onChange(version.tag)}
              className={`flex w-full items-center justify-between gap-3 border p-3 text-left transition ${
                selected ? "border-signal bg-signal-dark" : "border-line bg-panel hover:border-dim"
              }`}
            >
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium">{version.tag}</span>
                <span className="mt-0.5 block truncate font-mono text-[10px] text-dim">
                  {version.releaseDate ?? "No date"} / Max Java {version.maxJavaVersion ?? "?"}
                </span>
              </span>
              <span
                className={`shrink-0 border px-2 py-0.5 text-[10px] font-medium ${
                  version.channel === "stable" ? "border-online/40 bg-online/10 text-online" : "border-signal/40 bg-signal/10 text-signal"
                }`}
              >
                {version.channel === "stable" ? "Stable" : "Beta"}
              </span>
            </button>
          );
        })}
      </div>
      <input type="hidden" name={name} value={value} />
    </div>
  );
}
