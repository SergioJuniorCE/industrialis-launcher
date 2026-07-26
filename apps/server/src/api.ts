import Fastify from "fastify";
import { z } from "zod";
import { resolveApiToken } from "./auth.js";
import type { ServerConfig } from "./config.js";
import { DockerServerManager } from "./docker.js";

const createServerSchema = z.object({
  name: z.string().trim().min(1).max(64),
  version: z.string().trim().regex(/^[a-zA-Z0-9._-]+$/).optional(),
  port: z.number().int().min(1024).max(65535).optional(),
  memoryMb: z.number().int().min(4096).max(131072).optional(),
});

const logsQuerySchema = z.object({ tail: z.coerce.number().int().min(1).max(1000).default(200) });

export async function startApi(config: ServerConfig): Promise<void> {
  const app = Fastify({ logger: true });
  const manager = new DockerServerManager(config);
  const apiToken = await resolveApiToken(config);

  app.addHook("onRequest", async (request, reply) => {
    if (!request.url.startsWith("/api/")) return;
    if (request.headers.authorization !== `Bearer ${apiToken}`) {
      return reply.status(401).send({ error: "Unauthorized" });
    }
  });

  app.setErrorHandler((error, _request, reply) => {
    const statusCode =
      error instanceof z.ZodError
        ? 400
        : typeof error === "object" && error !== null && "statusCode" in error
          ? Number(error.statusCode)
          : 500;
    const message = error instanceof Error ? error.message : String(error);
    void reply.status(statusCode).send({ error: message });
  });

  app.get("/health", async () => {
    await manager.checkDocker();
    return { status: "ok" };
  });
  app.get("/api/servers", () => manager.list());
  app.get<{ Params: { id: string } }>("/api/servers/:id", ({ params }) => manager.get(params.id));
  app.post("/api/servers", async (request, reply) => {
    const server = await manager.create(createServerSchema.parse(request.body));
    return reply.status(201).send(server);
  });
  app.post<{ Params: { id: string } }>("/api/servers/:id/start", ({ params }) => manager.start(params.id));
  app.post<{ Params: { id: string } }>("/api/servers/:id/stop", ({ params }) => manager.stop(params.id));
  app.post<{ Params: { id: string } }>("/api/servers/:id/restart", ({ params }) => manager.restart(params.id));
  app.delete<{ Params: { id: string } }>("/api/servers/:id", async ({ params }, reply) => {
    await manager.remove(params.id);
    return reply.status(204).send();
  });
  app.get<{ Params: { id: string }; Querystring: { tail?: string } }>(
    "/api/servers/:id/logs",
    async ({ params, query }) => ({ lines: await manager.logs(params.id, logsQuerySchema.parse(query).tail) }),
  );

  await app.listen({ host: config.host, port: config.port });
}
