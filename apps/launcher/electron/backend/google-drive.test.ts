// @vitest-environment node

import fs from "node:fs/promises";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const electronState = vi.hoisted(() => ({
  appData: "",
  safeStorage: {
    isEncryptionAvailable: vi.fn(() => true),
    encryptString: vi.fn((value: string) => Buffer.from(value, "utf8")),
    decryptString: vi.fn((value: Buffer) => value.toString("utf8")),
  },
  shell: { openExternal: vi.fn(async () => undefined) },
}));

vi.mock("electron", () => ({
  app: { getPath: () => electronState.appData },
  safeStorage: electronState.safeStorage,
  shell: electronState.shell,
}));

import { googleDriveStatePath } from "./paths";
import { GoogleDriveAdapter } from "./google-drive";

let tempRoot = "";

beforeEach(async () => {
  tempRoot = await fs.mkdtemp(path.join(process.env.TEMP ?? process.env.TMP ?? ".", "industrialis-google-drive-"));
  electronState.appData = tempRoot;
  vi.stubEnv("INDUSTRIALIS_GOOGLE_DRIVE_CLIENT_ID", "");
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await fs.rm(tempRoot, { recursive: true, force: true });
});

describe("GoogleDriveAdapter configuration", () => {
  it("stores the public client ID separately from connection state", async () => {
    const adapter = new GoogleDriveAdapter();

    await expect(adapter.getStatus()).resolves.toEqual({ configured: false, connected: false });
    await expect(adapter.configure("1234567890123456.apps.googleusercontent.com")).resolves.toEqual({
      configured: true,
      connected: false,
      client_id: "1234567890123456.apps.googleusercontent.com",
    });
    await expect(fs.stat(googleDriveStatePath())).resolves.toBeTruthy();
    await expect(adapter.disconnect()).resolves.toEqual({
      configured: true,
      connected: false,
      client_id: "1234567890123456.apps.googleusercontent.com",
    });
  });
});
