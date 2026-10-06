import { afterEach, describe, expect, it, vi } from "vitest";
import { dashboardRequestUrl } from "./dashboard-api.js";
import { proxyToDaemon } from "./daemon.js";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("dashboard daemon routing", () => {
  it("adds the proxy prefix exactly once", () => {
    expect(dashboardRequestUrl("/servers")).toBe("/api/daemon/servers");
    expect(() => dashboardRequestUrl("/api/servers")).toThrow("after the daemon proxy prefix");
  });

  it("forwards the path, query, method, body, and server-side bearer token", async () => {
    vi.stubEnv("INDUSTRIALIS_API_TOKEN", "proxy-test-token");
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ status: "ok" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const request = new Request("http://dashboard/api/daemon/servers?status=running", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: "Assembly Line" }),
    });

    const response = await proxyToDaemon(request, ["servers"]);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "ok" });
    expect(fetchMock).toHaveBeenCalledWith(
      new URL("http://127.0.0.1:4310/api/servers?status=running"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ name: "Assembly Line" }),
        headers: expect.objectContaining({ authorization: "Bearer proxy-test-token" }),
      }),
    );
  });

  it("forwards bodyless server actions without an empty JSON body", async () => {
    vi.stubEnv("INDUSTRIALIS_API_TOKEN", "proxy-test-token");
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({ status: "running" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const request = new Request("http://dashboard/api/daemon/servers/live-verification/start", { method: "POST" });

    await proxyToDaemon(request, ["servers", "live-verification", "start"]);

    const [, init] = fetchMock.mock.calls[0]!;
    expect(init?.method).toBe("POST");
    expect(init?.body).toBeUndefined();
    expect(new Headers(init?.headers).has("content-type")).toBe(false);
  });
});
