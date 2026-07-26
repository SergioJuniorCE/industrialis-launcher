import { randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ServerConfig } from "./config.js";

export async function resolveApiToken(config: ServerConfig): Promise<string> {
  if (config.apiToken) return config.apiToken;
  const tokenPath = join(config.dataDir, ".api-token");
  try {
    return (await readFile(tokenPath, "utf8")).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
  }
  const token = randomBytes(32).toString("hex");
  await mkdir(config.dataDir, { recursive: true });
  await writeFile(tokenPath, `${token}\n`, { encoding: "utf8", mode: 0o600 });
  return token;
}
