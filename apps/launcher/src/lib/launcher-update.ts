export interface LauncherUpdateState {
  status: "idle" | "checking" | "available" | "downloading" | "deferred" | "manual" | "installing" | "up-to-date" | "disabled" | "failed";
  current_version: string;
  version?: string;
  body?: string;
  progress?: number;
  error?: string;
  download_url?: string;
  sha256?: string;
  release_url?: string;
}
