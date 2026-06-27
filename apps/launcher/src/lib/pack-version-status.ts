export interface PackVersionMeta {
  title: string;
  releaseDate: string;
  maxJavaVersion: number;
}

export type PackUpdateStatus = "unknown" | "up-to-date" | "update-available";

export interface PackVersionInfo {
  status: PackUpdateStatus;
  currentVersion: string;
  latestVersion: string | null;
}

export function parseReleaseDate(value: string): number {
  const parts = value.trim().split(/[/-]/).map((part) => Number.parseInt(part, 10));
  if (parts.length !== 3 || parts.some((part) => Number.isNaN(part))) {
    return 0;
  }
  const [year, month, day] = parts;
  return Date.UTC(year, month - 1, day);
}

export function compareVersionsByReleaseDate(
  leftKey: string,
  rightKey: string,
  versions: Record<string, PackVersionMeta> | null,
): number {
  const leftDate = parseReleaseDate(versions?.[leftKey]?.releaseDate ?? "");
  const rightDate = parseReleaseDate(versions?.[rightKey]?.releaseDate ?? "");
  if (leftDate !== rightDate) {
    return rightDate - leftDate;
  }
  return rightKey.localeCompare(leftKey, undefined, { numeric: true });
}

export function getLatestPackVersion(versions: Record<string, PackVersionMeta> | null): string | null {
  if (!versions) return null;
  const keys = Object.keys(versions);
  if (keys.length === 0) return null;
  return keys.sort((a, b) => compareVersionsByReleaseDate(a, b, versions))[0] ?? null;
}

export function getPackVersionInfo(
  currentVersion: string,
  versions: Record<string, PackVersionMeta> | null,
): PackVersionInfo {
  const latestVersion = getLatestPackVersion(versions);
  if (!latestVersion) {
    return { status: "unknown", currentVersion, latestVersion: null };
  }

  if (compareVersionsByReleaseDate(currentVersion, latestVersion, versions) > 0) {
    return { status: "update-available", currentVersion, latestVersion };
  }

  return { status: "up-to-date", currentVersion, latestVersion };
}