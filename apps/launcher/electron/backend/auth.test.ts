// @vitest-environment node

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const electronState = vi.hoisted(() => ({ appData: "" }));

vi.mock("electron", () => ({
  app: {
    isPackaged: false,
    getPath: () => electronState.appData,
    getAppPath: () => electronState.appData,
    getVersion: () => "test",
    quit: vi.fn(),
  },
  BrowserWindow: { getFocusedWindow: () => null, getAllWindows: () => [] },
  dialog: { showOpenDialog: vi.fn() },
  shell: { openExternal: vi.fn(), openPath: vi.fn(async () => "") },
}));

import { ensureFreshToken } from "./auth";
import type { AccountData } from "./types";

let tempRoot = "";
let originalFetch: typeof fetch;

function jsonResponse(status: number, body: unknown) {
  return { ok: status >= 200 && status < 300, status, text: async () => JSON.stringify(body) } as Response;
}

function expiredAccount(): AccountData {
  return {
    format_version: 3,
    account_type: "msa",
    id: "test-account",
    msa_token: { access_token: "old", refresh_token: "refresh", expires_at: 0 },
  } as AccountData;
}

function stubAuthNetwork(primary: { status: number; body: unknown }, fallback: { status: number; body: unknown }) {
  const xboxToken = {
    Token: "xbox-token",
    NotAfter: new Date(Date.now() + 3600_000).toISOString(),
    DisplayClaims: { xui: [{ uhs: "U" }] },
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: unknown) => {
      const target = String(url);
      if (target.includes("login.microsoftonline.com")) return jsonResponse(200, { access_token: "msa", refresh_token: "refresh", expires_in: 3600 });
      if (target.includes("user.auth.xboxlive.com")) return jsonResponse(200, xboxToken);
      if (target.includes("xsts.auth.xboxlive.com")) return jsonResponse(200, xboxToken);
      if (target.includes("/launcher/login")) return jsonResponse(primary.status, primary.body);
      if (target.includes("/authentication/login_with_xbox")) return jsonResponse(fallback.status, fallback.body);
      if (target.includes("/entitlements/license")) return jsonResponse(200, { items: [] });
      if (target.includes("/minecraft/profile")) return jsonResponse(404, { path: "/minecraft/profile", error: "NOT_FOUND" });
      throw new Error(`unexpected fetch: ${target}`);
    }),
  );
}

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-auth-"));
  electronState.appData = tempRoot;
  originalFetch = globalThis.fetch;
});

afterEach(async () => {
  vi.unstubAllGlobals();
  globalThis.fetch = originalFetch;
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("ensureFreshToken Minecraft login", () => {
  it("recovers via fallback when launcher/login returns a bare 429 body", async () => {
    stubAuthNetwork({ status: 429, body: { path: "/launcher/login" } }, { status: 200, body: { access_token: "mc-token", expires_in: 86400 } });
    await expect(ensureFreshToken(expiredAccount())).resolves.toBe("mc-token");
  });

  it("throws a friendly rate-limit error when both endpoints are rate-limited", async () => {
    stubAuthNetwork({ status: 429, body: { path: "/launcher/login" } }, { status: 429, body: { path: "/authentication/login_with_xbox" } });
    await expect(ensureFreshToken(expiredAccount())).rejects.toThrow(/rate-limiting/i);
  });
});
