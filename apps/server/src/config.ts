import { homedir } from "node:os";
import { resolve } from "node:path";

export interface ServerConfig {
  host: string;
  port: number;
  dataDir: string;
  apiUrl: string;
  imageRepository: string;
  apiToken?: string;
}

export function getConfig(overrides: Partial<ServerConfig> = {}): ServerConfig {
  const host = overrides.host ?? process.env.INDUSTRIALIS_HOST ?? "127.0.0.1";
  const port = overrides.port ?? Number(process.env.INDUSTRIALIS_PORT ?? 4310);

  return {
    host,
    port,
    dataDir: resolve(
      overrides.dataDir ?? process.env.INDUSTRIALIS_SERVER_DATA ?? resolve(homedir(), ".industrialis", "servers"),
    ),
    apiUrl: overrides.apiUrl ?? process.env.INDUSTRIALIS_API_URL ?? `http://${host}:${port}`,
    imageRepository:
      overrides.imageRepository ??
      process.env.INDUSTRIALIS_GTNH_IMAGE ??
      "ghcr.io/debuas/gtnhserverdocker",
    apiToken: overrides.apiToken ?? process.env.INDUSTRIALIS_API_TOKEN,
  };
}
