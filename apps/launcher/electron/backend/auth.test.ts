// @vitest-environment node

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openExternal: vi.fn(),
  readJson: vi.fn(),
  writeJson: vi.fn(),
}));

vi.mock("electron", () => ({ shell: { openExternal: mocks.openExternal } }));
vi.mock("./fs-utils", () => ({ readJson: mocks.readJson, writeJson: mocks.writeJson }));
vi.mock("./paths", () => ({ accountsPath: () => "accounts.json" }));

import { cancelMicrosoftLogin, startMicrosoftLogin } from "./auth";

const deviceCodeResponse = {
  device_code: "device-code",
  user_code: "ABCD-EFGH",
  verification_uri: "https://www.microsoft.com/link",
  message: "Enter the code to continue.",
  expires_in: 900,
  interval: 5,
};

describe("Microsoft account login lifecycle", () => {
  beforeEach(() => {
    cancelMicrosoftLogin();
    vi.clearAllMocks();
    mocks.readJson.mockResolvedValue([]);
    mocks.openExternal.mockResolvedValue(undefined);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(deviceCodeResponse), { status: 200, headers: { "Content-Type": "application/json" } })),
    );
  });

  afterEach(() => {
    cancelMicrosoftLogin();
    vi.unstubAllGlobals();
  });

  it("aborts a cancelled login before account persistence", async () => {
    const login = startMicrosoftLogin(vi.fn());

    cancelMicrosoftLogin();

    await expect(login).rejects.toMatchObject({ name: "AbortError" });
    expect(mocks.writeJson).not.toHaveBeenCalled();
  });

  it("allows only one active Microsoft login", async () => {
    const emit = vi.fn();
    const firstLogin = startMicrosoftLogin(emit);
    await vi.waitFor(() => expect(emit).toHaveBeenCalledOnce());

    await expect(startMicrosoftLogin(vi.fn())).rejects.toThrow("Microsoft sign-in is already in progress");

    cancelMicrosoftLogin();
    await expect(firstLogin).rejects.toMatchObject({ name: "AbortError" });
    expect(mocks.writeJson).not.toHaveBeenCalled();
  });
});
