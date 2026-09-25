// @vitest-environment node

import { afterEach, describe, expect, it, vi } from "vitest";
import { formatHttpResult, requestJson, retryWithBackoff, sleep } from "./http";

function jsonResponse(status: number, body: unknown): Response {
  return { ok: status >= 200 && status < 300, status, text: async () => JSON.stringify(body) } as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("requestJson", () => {
  it("returns status and parsed body on success", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(200, { access_token: "abc" })),
    );
    await expect(requestJson("https://example.com", { method: "GET" })).resolves.toEqual({ status: 200, body: { access_token: "abc" } });
  });

  it("returns status and body on HTTP errors instead of throwing", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(429, { path: "/launcher/login" })),
    );
    await expect(requestJson("https://example.com", { method: "POST" })).resolves.toEqual({ status: 429, body: { path: "/launcher/login" } });
  });

  it("falls back to raw text when the body is not JSON", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, status: 500, text: async () => "<html>oops</html>" }) as Response),
    );
    await expect(requestJson("https://example.com", {})).resolves.toEqual({ status: 500, body: { raw: "<html>oops</html>" } });
  });

  it("wraps transport failures with the URL and a connection hint", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("fetch failed");
      }),
    );
    const error = await requestJson("https://example.com/x", {}).catch((e) => e as Error);
    expect(String(error)).toContain("request to https://example.com/x failed (fetch failed)");
    expect(String(error)).toContain("Check your internet connection");
  });

  it("reports timeouts distinctly", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_url: unknown, init?: RequestInit) =>
          new Promise<Response>((_resolve, reject) => {
            init?.signal?.addEventListener("abort", () => {
              const error = new Error("The operation was aborted");
              error.name = "TimeoutError";
              reject(error);
            });
          }),
      ),
    );
    const error = await requestJson("https://example.com/slow", {}, { timeoutMs: 5 }).catch((e) => e as Error);
    expect(String(error)).toContain("request to https://example.com/slow timed out after 0.005s");
  });
});

describe("formatHttpResult", () => {
  it("includes status and body", () => {
    expect(formatHttpResult({ status: 429, body: { path: "/launcher/login" } })).toBe('HTTP 429: {"path":"/launcher/login"}');
  });
});

describe("retryWithBackoff", () => {
  it("returns the first success without waiting", async () => {
    const operation = vi.fn(async () => "ok");
    await expect(retryWithBackoff(operation, { maxAttempts: 3, baseDelayMs: 1 })).resolves.toBe("ok");
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it("retries until success within maxAttempts", async () => {
    const operation = vi.fn(async () => {
      if (operation.mock.calls.length < 3) throw new Error("flaky");
      return "recovered";
    });
    await expect(retryWithBackoff(operation, { maxAttempts: 4, baseDelayMs: 1 })).resolves.toBe("recovered");
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it("throws the last error after exhausting attempts", async () => {
    const operation = vi.fn(async () => {
      throw new Error(`failure ${operation.mock.calls.length}`);
    });
    const error = await retryWithBackoff(operation, { maxAttempts: 3, baseDelayMs: 1 }).catch((e) => e as Error);
    expect(error.message).toBe("failure 3");
    expect(operation).toHaveBeenCalledTimes(3);
  });

  it("stops immediately when shouldRetry declines", async () => {
    const operation = vi.fn(async () => {
      throw new Error("fatal");
    });
    const error = await retryWithBackoff(operation, { maxAttempts: 5, baseDelayMs: 1, shouldRetry: () => false }).catch((e) => e as Error);
    expect(error.message).toBe("fatal");
    expect(operation).toHaveBeenCalledTimes(1);
  });

  it("rejects invalid attempt counts", async () => {
    await expect(retryWithBackoff(async () => "x", { maxAttempts: 0, baseDelayMs: 1 })).rejects.toThrow("maxAttempts");
  });

  it("sleep waits at least the requested time", async () => {
    const start = Date.now();
    await sleep(20);
    expect(Date.now() - start).toBeGreaterThanOrEqual(15);
  });
});
