import type { CreateServerInput, GtnhServer, ServerLog, UpdateServerInput, UpdateServerResourcesInput } from "@industrialis/server-contracts";

export class ServerApiClient {
  constructor(
    private readonly baseUrl: string,
    private readonly apiToken: string,
  ) {}

  list(): Promise<GtnhServer[]> {
    return this.request("/api/servers");
  }

  versions(): Promise<string[]> {
    return this.request("/api/versions");
  }

  create(input: CreateServerInput): Promise<GtnhServer> {
    return this.request("/api/servers", { method: "POST", body: JSON.stringify(input) });
  }

  action(id: string, action: "start" | "stop" | "restart"): Promise<GtnhServer> {
    return this.request(`/api/servers/${encodeURIComponent(id)}/${action}`, { method: "POST" });
  }

  update(id: string, input: UpdateServerInput): Promise<GtnhServer> {
    return this.request(`/api/servers/${encodeURIComponent(id)}/update`, {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  updateResources(id: string, input: UpdateServerResourcesInput): Promise<GtnhServer> {
    return this.request(`/api/servers/${encodeURIComponent(id)}/resources`, {
      method: "PUT",
      body: JSON.stringify(input),
    });
  }

  remove(id: string): Promise<void> {
    return this.request(`/api/servers/${encodeURIComponent(id)}`, { method: "DELETE" });
  }

  logs(id: string, tail: number): Promise<ServerLog> {
    return this.request(`/api/servers/${encodeURIComponent(id)}/logs?tail=${tail}`);
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${this.apiToken}`,
        ...init?.headers,
      },
    });
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as { error?: string } | null;
      throw new Error(body?.error ?? `${response.status} ${response.statusText}`);
    }
    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }
}
