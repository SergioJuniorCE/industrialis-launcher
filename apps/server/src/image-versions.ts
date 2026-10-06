interface RegistryTagsResponse {
  tags?: unknown;
}

interface RegistryTokenResponse {
  token?: unknown;
  access_token?: unknown;
}

interface RegistryAddress {
  origin: string;
  repository: string;
}

export function isValidImageTag(tag: string): boolean {
  return /^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$/.test(tag);
}

function registryAddress(imageRepository: string): RegistryAddress {
  const parts = imageRepository
    .replace(/^https?:\/\//i, "")
    .replace(/\/$/, "")
    .split("/");
  const first = parts[0] ?? "";
  const hasRegistry = first.includes(".") || first.includes(":") || first === "localhost";
  const origin = hasRegistry ? `https://${parts.shift()}` : "https://registry-1.docker.io";
  if (!hasRegistry && parts.length === 1) parts.unshift("library");
  const repository = parts.join("/");
  if (!repository || repository.includes("@") || repository.includes(":")) {
    throw new Error(`Invalid image repository: ${imageRepository}`);
  }
  return { origin, repository };
}

function bearerChallenge(value: string | null): Record<string, string> | null {
  if (!value?.startsWith("Bearer ")) return null;
  const fields: Record<string, string> = {};
  for (const match of value.slice("Bearer ".length).matchAll(/([\w-]+)="([^"]*)"/g)) {
    const key = match[1];
    const fieldValue = match[2];
    if (key && fieldValue !== undefined) fields[key] = fieldValue;
  }
  return fields.realm ? fields : null;
}

async function authenticatedRegistryRequest(url: URL, fetcher: typeof fetch): Promise<Response> {
  const response = await fetcher(url, { headers: { accept: "application/json" } });
  if (response.status !== 401) return response;

  const challenge = bearerChallenge(response.headers.get("www-authenticate"));
  if (!challenge) return response;
  const tokenUrl = new URL(challenge.realm!);
  for (const key of ["service", "scope"]) {
    const value = challenge[key];
    if (value) tokenUrl.searchParams.set(key, value);
  }
  const tokenResponse = await fetcher(tokenUrl, { headers: { accept: "application/json" } });
  if (!tokenResponse.ok) return response;
  const tokenPayload = (await tokenResponse.json()) as RegistryTokenResponse;
  const token = tokenPayload.token ?? tokenPayload.access_token;
  if (typeof token !== "string" || token.length === 0) return response;
  return fetcher(url, { headers: { accept: "application/json", authorization: `Bearer ${token}` } });
}

function nextPage(link: string | null, current: URL): URL | null {
  const next = link?.match(/<([^>]+)>\s*;\s*rel="?next"?/i)?.[1];
  return next ? new URL(next, current) : null;
}

export async function getImageVersions(imageRepository: string, fetcher: typeof fetch = fetch): Promise<string[]> {
  const { origin, repository } = registryAddress(imageRepository);
  let url: URL | null = new URL(`/v2/${repository.split("/").map(encodeURIComponent).join("/")}/tags/list`, origin);
  url.searchParams.set("n", "1000");
  const tags = new Set<string>();
  let pageCount = 0;

  while (url && tags.size < 5000 && pageCount < 100) {
    pageCount += 1;
    const response = await authenticatedRegistryRequest(url, fetcher);
    if (!response.ok) {
      throw new Error(`Image registry returned ${response.status} while listing ${imageRepository} tags`);
    }
    const payload = (await response.json()) as RegistryTagsResponse;
    if (!Array.isArray(payload.tags)) throw new Error(`Image registry returned an invalid tag list for ${imageRepository}`);
    for (const tag of payload.tags) {
      if (typeof tag === "string" && isValidImageTag(tag)) tags.add(tag);
    }
    url = nextPage(response.headers.get("link"), url) ?? null;
  }
  if (url) throw new Error(`Image registry returned too many tag pages for ${imageRepository}`);

  const versions = [...tags].sort((left, right) => right.localeCompare(left, undefined, { numeric: true, sensitivity: "base" }));
  return ["stable-latest", ...versions.filter((version) => version !== "stable-latest")];
}
