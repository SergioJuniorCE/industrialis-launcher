import { createHash } from "node:crypto";
import fs from "node:fs/promises";

const MAX_INSTALLER_BYTES = 512 * 1024 * 1024;
const RELEASE_DOWNLOAD_PREFIX = "/SergioJuniorCE/industrialis-launcher/releases/download/launcher-v";

export interface LauncherDownloadProgress {
  downloadedBytes: number;
  totalBytes: number | null;
  progress: number;
}

interface DownloadLauncherInstallerOptions {
  url: string;
  destination: string;
  expectedSha256?: string;
  onProgress?: (progress: LauncherDownloadProgress) => void;
  fetchImpl?: (input: string, init?: RequestInit) => Promise<Response>;
}

export function isTrustedLauncherDownloadUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.hostname === "github.com" &&
      url.pathname.startsWith(RELEASE_DOWNLOAD_PREFIX) &&
      url.pathname.endsWith("/IndustrialisLauncherSetup.exe")
    );
  } catch {
    return false;
  }
}

export async function downloadLauncherInstaller({
  url,
  destination,
  expectedSha256,
  onProgress,
  fetchImpl = fetch,
}: DownloadLauncherInstallerOptions): Promise<void> {
  if (!isTrustedLauncherDownloadUrl(url)) throw new Error("launcher update URL is not trusted");

  const response = await fetchImpl(url, { signal: AbortSignal.timeout(5 * 60_000) });
  if (!response.ok) throw new Error(`launcher update download failed: HTTP ${response.status}`);
  if (!response.body) throw new Error("launcher update download returned an empty response");

  const contentLength = Number(response.headers.get("content-length"));
  const totalBytes = Number.isSafeInteger(contentLength) && contentLength > 0 ? contentLength : null;
  if (totalBytes !== null && totalBytes > MAX_INSTALLER_BYTES) throw new Error("launcher update installer is unexpectedly large");

  const expectedDigest = expectedSha256?.replace(/^sha256:/u, "").toLowerCase();
  if (expectedDigest && !/^[a-f0-9]{64}$/u.test(expectedDigest)) throw new Error("launcher update has an invalid SHA-256 digest");

  const reader = response.body.getReader();
  const file = await fs.open(destination, "wx");
  const hash = createHash("sha256");
  let downloadedBytes = 0;
  let executableHeader = Buffer.alloc(0);

  try {
    onProgress?.({ downloadedBytes, totalBytes, progress: 0 });
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value.byteLength) continue;
      if (downloadedBytes + value.byteLength > MAX_INSTALLER_BYTES) throw new Error("launcher update installer is unexpectedly large");

      let chunkOffset = 0;
      while (chunkOffset < value.byteLength) {
        const { bytesWritten } = await file.write(value, chunkOffset, value.byteLength - chunkOffset, downloadedBytes + chunkOffset);
        if (bytesWritten <= 0) throw new Error("launcher update installer could not be written");
        chunkOffset += bytesWritten;
      }

      hash.update(value);
      if (executableHeader.byteLength < 2) {
        executableHeader = Buffer.concat([executableHeader, Buffer.from(value.subarray(0, 2 - executableHeader.byteLength))]);
      }
      downloadedBytes += value.byteLength;
      onProgress?.({
        downloadedBytes,
        totalBytes,
        progress: totalBytes === null ? 0 : Math.min(downloadedBytes / totalBytes, 1),
      });
    }

    if (totalBytes !== null && downloadedBytes !== totalBytes) throw new Error("launcher update download was incomplete");
    if (executableHeader.toString("ascii") !== "MZ") throw new Error("launcher update asset is not a Windows installer");
    if (expectedDigest && hash.digest("hex") !== expectedDigest) throw new Error("launcher update installer failed SHA-256 verification");
  } catch (error) {
    await reader.cancel().catch(() => undefined);
    await file.close().catch(() => undefined);
    await fs.rm(destination, { force: true }).catch(() => undefined);
    throw error;
  }

  await file.close();
}
