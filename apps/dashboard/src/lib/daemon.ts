import { homedir } from "node:os";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";

export const DAEMON_URL = process.env.INDUSTRIALIS_API_URL ?? "http://127.0.0.1:4310";

export async function resolveDaemonToken(): Promise<string> {
  if (process.env.INDUSTRIALIS_API_TOKEN) return process.env.INDUSTRIALIS_API_TOKEN;
  const dataDir = process.env.INDUSTRIALIS_SERVER_DATA
    ? resolve(process.env.INDUSTRIALIS_SERVER_DATA)
    : resolve(homedir(), ".industrialis", "servers");
  return (await readFile(join(dataDir, ".api-token"), "utf8")).trim();
}

export async function proxyToDaemon(request: Request, pathSegments: string[]): Promise<Response> {
  const target = new URL(`/api/${pathSegments.map(encodeURIComponent).join("/")}`, DAEMON_URL);
  target.search = new URL(request.url).search;
  const body = request.method === "GET" || request.method === "HEAD" ? undefined : await request.text();
  const response = await fetch(target, {
    method: request.method,
    body,
    cache: "no-store",
    headers: {
      authorization: `Bearer ${await resolveDaemonToken()}`,
      "content-type": request.headers.get("content-type") ?? "application/json",
    },
  });
  return new Response(response.body, {
    status: response.status,
    headers: { "content-type": response.headers.get("content-type") ?? "application/json" },
  });
}
