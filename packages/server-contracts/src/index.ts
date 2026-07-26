export type ServerStatus =
  | "creating"
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "error"
  | "missing";

export interface GtnhServer {
  id: string;
  name: string;
  version: string;
  image: string;
  port: number;
  memoryMb: number;
  status: ServerStatus;
  containerId: string | null;
  volumeName: string;
  createdAt: string;
  error?: string;
}

export interface CreateServerInput {
  name: string;
  version?: string;
  port?: number;
  memoryMb?: number;
}

export interface ServerLog {
  lines: string;
}

export interface ApiError {
  error: string;
}

export const DEFAULT_SERVER_VERSION = "stable-latest";
export const DEFAULT_SERVER_PORT = 25565;
export const DEFAULT_SERVER_MEMORY_MB = 6144;
