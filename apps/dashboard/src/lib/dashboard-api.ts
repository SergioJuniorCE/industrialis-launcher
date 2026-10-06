const API_URL = "/api/daemon";

export function dashboardRequestUrl(path: string): string {
  if (!path.startsWith("/") || path.startsWith("//") || path.startsWith("/api/")) {
    throw new Error("Dashboard API paths must start after the daemon proxy prefix");
  }
  return `${API_URL}${path}`;
}

export async function dashboardApi<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(dashboardRequestUrl(path), {
    ...init,
    headers: { "content-type": "application/json", ...init?.headers },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new Error(body?.error ?? `${response.status} ${response.statusText}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
