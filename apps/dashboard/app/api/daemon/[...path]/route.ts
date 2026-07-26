import { homedir } from "node:os";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { NextRequest } from "next/server";

const DAEMON_URL = process.env.INDUSTRIALIS_API_URL ?? "http://127.0.0.1:4310";

async function apiToken(): Promise<string> {
  if (process.env.INDUSTRIALIS_API_TOKEN) return process.env.INDUSTRIALIS_API_TOKEN;
  const dataDir = process.env.INDUSTRIALIS_SERVER_DATA
    ? resolve(/* turbopackIgnore: true */ process.env.INDUSTRIALIS_SERVER_DATA)
    : resolve(/* turbopackIgnore: true */ homedir(), ".industrialis", "servers");
  return (await readFile(join(dataDir, ".api-token"), "utf8")).trim();
}

async function proxy(request: NextRequest, context: { params: Promise<{ path: string[] }> }) {
  const { path } = await context.params;
  const target = new URL(`/api/${path.map(encodeURIComponent).join("/")}`, DAEMON_URL);
  target.search = request.nextUrl.search;
  const body = request.method === "GET" || request.method === "HEAD" ? undefined : await request.text();
  const response = await fetch(target, {
    method: request.method,
    body,
    cache: "no-store",
    headers: {
      authorization: `Bearer ${await apiToken()}`,
      "content-type": request.headers.get("content-type") ?? "application/json",
    },
  });
  return new Response(response.body, {
    status: response.status,
    headers: { "content-type": response.headers.get("content-type") ?? "application/json" },
  });
}

export const GET = proxy;
export const POST = proxy;
export const DELETE = proxy;
