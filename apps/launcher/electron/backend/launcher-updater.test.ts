import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { downloadLauncherInstaller, isTrustedLauncherDownloadUrl } from "./launcher-updater";

const temporaryDirectories: string[] = [];
const downloadUrl = "https://github.com/SergioJuniorCE/industrialis-launcher/releases/download/launcher-v0.1.56/IndustrialisLauncherSetup.exe";

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => fs.rm(directory, { recursive: true, force: true })));
});

describe("launcher updater", () => {
  it("streams and verifies the trusted Windows installer", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-launcher-updater-test-"));
    temporaryDirectories.push(directory);
    const destination = path.join(directory, "IndustrialisLauncherSetup.exe");
    const installer = Buffer.from("MZfake-signed-installer");
    const progress = vi.fn();
    const fetchImpl = vi.fn(
      async () =>
        new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              controller.enqueue(installer.subarray(0, 1));
              controller.enqueue(installer.subarray(1));
              controller.close();
            },
          }),
          { headers: { "content-length": String(installer.byteLength) } },
        ),
    );

    await downloadLauncherInstaller({
      url: downloadUrl,
      destination,
      expectedSha256: `sha256:${createHash("sha256").update(installer).digest("hex")}`,
      onProgress: progress,
      fetchImpl,
    });

    expect(await fs.readFile(destination)).toEqual(installer);
    expect(progress).toHaveBeenLastCalledWith({ downloadedBytes: installer.byteLength, totalBytes: installer.byteLength, progress: 1 });
  });

  it("rejects downloads outside the launcher release path", () => {
    expect(isTrustedLauncherDownloadUrl(downloadUrl)).toBe(true);
    expect(isTrustedLauncherDownloadUrl("https://example.com/IndustrialisLauncherSetup.exe")).toBe(false);
    expect(isTrustedLauncherDownloadUrl("https://github.com/another/repo/releases/download/v1/IndustrialisLauncherSetup.exe")).toBe(false);
  });

  it("removes an installer that fails digest verification", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "industrialis-launcher-updater-test-"));
    temporaryDirectories.push(directory);
    const destination = path.join(directory, "IndustrialisLauncherSetup.exe");
    const installer = Buffer.from("MZtampered-installer");

    await expect(
      downloadLauncherInstaller({
        url: downloadUrl,
        destination,
        expectedSha256: `sha256:${"0".repeat(64)}`,
        fetchImpl: async () => new Response(installer, { headers: { "content-length": String(installer.byteLength) } }),
      }),
    ).rejects.toThrow("failed SHA-256 verification");
    await expect(fs.stat(destination)).rejects.toMatchObject({ code: "ENOENT" });
  });
});
