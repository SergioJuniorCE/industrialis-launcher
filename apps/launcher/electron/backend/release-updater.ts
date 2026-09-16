import fs from "node:fs/promises";
import path from "node:path";
import { spawn as spawnChildProcess } from "node:child_process";
import { app, shell } from "electron";
import { downloadLauncherInstaller, isTrustedLauncherDownloadUrl } from "./launcher-updater";
import type { LauncherUpdateState } from "./types";
import type { BackendContext } from "./backend-context";

export class ReleaseUpdater {
  private request: Promise<LauncherUpdateState> | null = null;
  private result: LauncherUpdateState | null = null;

  constructor(private readonly ctx: Pick<BackendContext, "emit" | "waitForInstanceOperations">) {}

  async checkForUpdate(): Promise<LauncherUpdateState> {
    const current_version = app.getVersion();
    if (!app.isPackaged) return { status: "disabled", current_version };
    try {
      const response = await fetch("https://api.github.com/repos/SergioJuniorCE/industrialis-launcher/releases/latest", {
        headers: { Accept: "application/vnd.github+json", "User-Agent": "industrialis-launcher" },
        signal: AbortSignal.timeout(10_000),
      });
      if (!response.ok) throw new Error(`GitHub release lookup failed: HTTP ${response.status}`);
      const release = (await response.json()) as {
        tag_name?: string;
        body?: string;
        draft?: boolean;
        prerelease?: boolean;
        html_url?: string;
        assets?: Array<{ name?: string; browser_download_url?: string; digest?: string }>;
      };
      const version = release.tag_name?.replace(/^launcher-v/u, "");
      if (!version || release.draft || release.prerelease || !isNewerVersion(version, current_version)) return { status: "up-to-date", current_version };
      const asset = release.assets?.find((entry) => {
        const name = entry.name?.toLowerCase() ?? "";
        return process.platform === "win32"
          ? name.endsWith("setup.exe")
          : process.platform === "darwin"
            ? name.endsWith(".dmg")
            : name.endsWith(".deb") || name.endsWith(".rpm");
      });
      const state = {
        status: "available",
        current_version,
        version,
        body: release.body ?? "",
        release_url: release.html_url ?? "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest",
        ...(asset?.browser_download_url ? { download_url: asset.browser_download_url } : {}),
        ...(asset?.digest ? { sha256: asset.digest } : {}),
      } satisfies LauncherUpdateState;
      this.ctx.emit("launcher-update", state);
      return state;
    } catch (error) {
      return { status: "failed", current_version, error: String(error) };
    }
  }

  installUpdate(): Promise<LauncherUpdateState> {
    if (this.request) return this.request;
    if (this.result) return Promise.resolve(this.result);
    const request = this.installCore();
    this.request = request.finally(() => {
      this.request = null;
    });
    return this.request;
  }

  private async installCore(): Promise<LauncherUpdateState> {
    const state = await this.checkForUpdate();
    if (state.status === "available") {
      if (process.platform === "win32" && state.download_url && isTrustedLauncherDownloadUrl(state.download_url)) {
        const updateDirectory = await fs.mkdtemp(path.join(app.getPath("temp"), "industrialis-launcher-update-"));
        const installer = path.join(updateDirectory, "IndustrialisLauncherSetup.exe");
        try {
          await downloadLauncherInstaller({
            url: state.download_url,
            destination: installer,
            expectedSha256: state.sha256,
            onProgress: ({ progress }) => {
              const downloading = { ...state, status: "downloading", progress } satisfies LauncherUpdateState;
              this.ctx.emit("launcher-update", downloading);
            },
          });

          const child = spawnChildProcess(installer, [], { detached: true, stdio: "ignore", windowsHide: true });
          await new Promise<void>((resolve, reject) => {
            child.once("spawn", resolve);
            child.once("error", reject);
          });
          child.unref();
          const installing = { ...state, status: "installing", progress: 1 } satisfies LauncherUpdateState;
          this.result = installing;
          this.ctx.emit("launcher-update", installing);
          setTimeout(() => {
            void this.ctx.waitForInstanceOperations().then(() => app.quit());
          }, 300);
          return installing;
        } catch (error) {
          await fs.rm(updateDirectory, { recursive: true, force: true }).catch(() => undefined);
          throw error;
        }
      }

      const releaseUrl = state.release_url ?? "https://github.com/SergioJuniorCE/industrialis-launcher/releases/latest";
      await shell.openExternal(releaseUrl);
      const manual = { ...state, status: "manual" } satisfies LauncherUpdateState;
      this.ctx.emit("launcher-update", manual);
      return manual;
    }
    return state;
  }
}

function isNewerVersion(candidate: string, current: string): boolean {
  const parse = (value: string) => value.split(/[.-]/u).map((part) => Number(part.replace(/\D.*$/u, "")) || 0);
  const a = parse(candidate);
  const b = parse(current);
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  return false;
}
