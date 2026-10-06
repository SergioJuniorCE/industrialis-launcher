import type { ServerVersionChannel, ServerVersionDetail } from "@industrialis/server-contracts";
import type { PackVersionCatalog } from "./pack-versions.js";

export function parseReleaseDate(value: string | null): number {
  if (!value) return 0;
  const parts = value
    .trim()
    .split(/[/-]/)
    .map((part) => Number.parseInt(part, 10));
  if (parts.length !== 3 || parts.some((part) => Number.isNaN(part))) return 0;
  const [year, month, day] = parts as [number, number, number];
  return Date.UTC(year, month - 1, day);
}

function channelFromTitle(title: string | null, tag: string): ServerVersionChannel {
  if (title) return title === "Stable release" ? "stable" : "beta";
  const release = stripReleasePrefix(tag);
  if (/beta|rc|alpha|pre|nightly/i.test(release)) return "beta";
  return /^\d+\.\d+\.\d+$/.test(release) ? "stable" : "beta";
}

function titleFromChannel(channel: ServerVersionChannel): string {
  return channel === "stable" ? "Stable release" : "Beta release";
}

function stripReleasePrefix(tag: string): string {
  return tag.replace(/^(stable|beta|latest)-/i, "");
}

const RELEASE_TAG_PATTERN = /^\d+\.\d+\.\d+([.-][A-Za-z0-9.-]+)?$/;

function tagPreference(tag: string, release: string, catalog: PackVersionCatalog | null): number {
  if (tag === release) return 0;
  if (catalog?.[tag]) return 1;
  return 2;
}

function packKeyForTag(tag: string, catalog: PackVersionCatalog | null): string | null {
  if (!catalog) return null;
  if (catalog[tag]) return tag;
  const withoutPrefix = tag.replace(/^(stable|beta|latest)-/i, "");
  if (catalog[withoutPrefix]) return withoutPrefix;
  const lower = tag.toLowerCase();
  const match = Object.keys(catalog).find((key) => key.toLowerCase() === lower);
  return match ?? null;
}

export function buildVersionDetails(tags: string[], catalog: PackVersionCatalog | null): ServerVersionDetail[] {
  const byRelease = new Map<string, string>();
  for (const tag of tags) {
    if (tag === "stable-latest") {
      if (!byRelease.has(tag)) byRelease.set(tag, tag);
      continue;
    }
    if (/^nightly-/i.test(tag)) continue;
    const release = stripReleasePrefix(tag);
    if (!RELEASE_TAG_PATTERN.test(release)) continue;
    const existing = byRelease.get(release);
    if (existing === undefined || tagPreference(tag, release, catalog) < tagPreference(existing, release, catalog)) {
      byRelease.set(release, tag);
    }
  }

  const details = [...byRelease.values()].map((tag) => {
    if (tag === "stable-latest") {
      return {
        tag,
        packVersion: tag,
        channel: "stable" as const,
        title: "Stable release",
        releaseDate: null,
        maxJavaVersion: null,
      } satisfies ServerVersionDetail;
    }
    const packVersion = packKeyForTag(tag, catalog) ?? tag;
    const meta = catalog?.[packVersion];
    const channel = channelFromTitle(meta?.title ?? null, tag);
    return {
      tag,
      packVersion,
      channel,
      title: meta?.title ?? titleFromChannel(channel),
      releaseDate: meta?.releaseDate ?? null,
      maxJavaVersion: meta?.maxJavaVersion ?? null,
    } satisfies ServerVersionDetail;
  });

  return details.sort((left, right) => {
    if (left.tag === "stable-latest") return -1;
    if (right.tag === "stable-latest") return 1;
    const dateDiff = parseReleaseDate(right.releaseDate) - parseReleaseDate(left.releaseDate);
    if (dateDiff !== 0) return dateDiff;
    return right.tag.localeCompare(left.tag, undefined, { numeric: true, sensitivity: "base" });
  });
}
