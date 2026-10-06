export interface PackVersionMeta {
  title: string;
  releaseDate: string;
  maxJavaVersion: number;
}

export type PackVersionCatalog = Record<string, PackVersionMeta>;

export const GTNH_VERSIONS_URL = "https://raw.githubusercontent.com/GTNewHorizons/GTNewHorizons.github.io/refs/heads/master/public/versions.json";

const CACHE_TTL_MS = 60 * 60 * 1000;

let cached: { fetchedAt: number; catalog: PackVersionCatalog } | null = null;
let inflight: Promise<PackVersionCatalog> | null = null;

export function clearPackVersionCache(): void {
  cached = null;
  inflight = null;
}

function normalizeCatalog(value: unknown): PackVersionCatalog {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const catalog: PackVersionCatalog = {};
  for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
    if (!entry || typeof entry !== "object") continue;
    const meta = entry as { title?: unknown; releaseDate?: unknown; maxJavaVersion?: unknown };
    if (typeof meta.title !== "string" || typeof meta.releaseDate !== "string" || typeof meta.maxJavaVersion !== "number") continue;
    catalog[key] = { title: meta.title, releaseDate: meta.releaseDate, maxJavaVersion: meta.maxJavaVersion };
  }
  return catalog;
}

export async function fetchPackVersions(fetcher: typeof fetch = fetch): Promise<PackVersionCatalog> {
  const now = Date.now();
  if (cached && now - cached.fetchedAt < CACHE_TTL_MS) return cached.catalog;
  if (inflight) return inflight;
  inflight = (async () => {
    const response = await fetcher(GTNH_VERSIONS_URL, { headers: { accept: "application/json" } });
    if (!response.ok) throw new Error(`Pack version catalog returned ${response.status}`);
    const catalog = normalizeCatalog(await response.json());
    cached = { fetchedAt: Date.now(), catalog };
    return catalog;
  })();
  try {
    return await inflight;
  } finally {
    inflight = null;
  }
}
