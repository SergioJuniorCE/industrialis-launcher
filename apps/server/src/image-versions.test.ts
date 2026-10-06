import { describe, expect, it, vi } from "vitest";
import { getImageVersions } from "./image-versions.js";

describe("getImageVersions", () => {
  it("uses the registry bearer challenge and returns stable plus live release tags", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(String(input));
      if (url.pathname === "/v2/debuas/gtnhserverdocker/tags/list") {
        if (new Headers(init?.headers).has("authorization")) {
          return Response.json({ tags: ["2.9.0-RC-2", "2.8.4", "invalid/tag", "stable-latest"] });
        }
        return new Response(JSON.stringify({ tags: ["2.9.0-RC-2", "2.8.4", "invalid/tag", "stable-latest"] }), {
          status: 401,
          headers: {
            "www-authenticate": 'Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:debuas/gtnhserverdocker:pull"',
          },
        });
      }
      if (url.pathname === "/token") return Response.json({ token: "registry-token" });
      return Response.json({ tags: ["2.9.0-RC-2", "2.8.4", "invalid/tag", "stable-latest"] });
    });

    const versions = await getImageVersions("ghcr.io/debuas/gtnhserverdocker", fetcher as typeof fetch);

    expect(versions[0]).toBe("stable-latest");
    expect(versions).toContain("2.9.0-RC-2");
    expect(versions).toContain("2.8.4");
    expect(versions).not.toContain("invalid/tag");
    expect(fetcher).toHaveBeenCalledWith(
      expect.objectContaining({ href: expect.stringContaining("scope=repository%3Adebuas%2Fgtnhserverdocker%3Apull") }),
      expect.objectContaining({ headers: { accept: "application/json" } }),
    );
    expect(fetcher).toHaveBeenCalledWith(
      expect.any(URL),
      expect.objectContaining({ headers: expect.objectContaining({ authorization: "Bearer registry-token" }) }),
    );
  });

  it("follows registry pagination and keeps the stable alias first", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input));
      return url.searchParams.get("page") === "2"
        ? Response.json({ tags: ["2.9.0-RC-2"] })
        : new Response(JSON.stringify({ tags: ["stable-latest", "2.9.0-RC-1"] }), {
            headers: { link: '<https://ghcr.io/v2/debuas/gtnhserverdocker/tags/list?page=2>; rel="next"' },
          });
    });

    await expect(getImageVersions("ghcr.io/debuas/gtnhserverdocker", fetcher as typeof fetch)).resolves.toEqual(["stable-latest", "2.9.0-RC-2", "2.9.0-RC-1"]);
  });
});
