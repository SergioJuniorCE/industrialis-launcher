import type { APIRoute } from "astro";
import { proxyToDaemon } from "../../../lib/daemon";

export const prerender = false;

async function handle({
  params,
  request,
}: {
  params: { path?: string | string[] };
  request: Request;
}): Promise<Response> {
  const raw = params.path;
  const segments = Array.isArray(raw) ? raw : raw ? raw.split("/").filter(Boolean) : [];
  if (segments.length === 0) {
    return new Response(JSON.stringify({ error: "Missing daemon path" }), {
      status: 400,
      headers: { "content-type": "application/json" },
    });
  }
  try {
    return await proxyToDaemon(request, segments);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return new Response(JSON.stringify({ error: message }), {
      status: 502,
      headers: { "content-type": "application/json" },
    });
  }
}

export const GET: APIRoute = handle;
export const POST: APIRoute = handle;
export const DELETE: APIRoute = handle;
export const PUT: APIRoute = handle;
export const PATCH: APIRoute = handle;
