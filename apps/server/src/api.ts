import Fastify from "fastify";
import { z } from "zod";
import { resolveApiToken } from "./auth.js";
import type { ServerConfig } from "./config.js";
import { DockerServerManager } from "./docker.js";
import { isManagedConfigFile } from "./server-files.js";

const versionSchema = z
  .string()
  .trim()
  .regex(/^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$/);
const createServerSchema = z.object({
  name: z.string().trim().min(1).max(64),
  version: versionSchema.optional(),
  port: z.number().int().min(1024).max(65535).optional(),
  memoryMb: z.number().int().min(4096).max(131072).optional(),
});

const logsQuerySchema = z.object({ tail: z.coerce.number().int().min(1).max(1000).default(200) });
const updateServerSchema = z.object({ version: versionSchema, createBackup: z.boolean().default(true) });
const updateResourcesSchema = z
  .object({
    port: z.number().int().min(1024).max(65535),
    memoryMb: z.number().int().min(4096).max(131072),
  })
  .partial()
  .refine((input) => input.port !== undefined || input.memoryMb !== undefined, "Provide a port or memory limit");
const configFilePathSchema = z.string().min(1).max(256).refine(isManagedConfigFile, "Config file path is outside the managed config area");
const configFileQuerySchema = z.object({ path: configFilePathSchema });
const configFileBodySchema = z.object({ path: configFilePathSchema, content: z.string().max(1_048_576) });

export async function createApi(config: ServerConfig, options: { manager?: DockerServerManager; apiToken?: string; logger?: boolean } = {}) {
  const app = Fastify({ logger: options.logger ?? true });
  const manager = options.manager ?? new DockerServerManager(config);
  const apiToken = options.apiToken ?? (await resolveApiToken(config));

  app.addHook("onRequest", async (request, reply) => {
    if (!request.url.startsWith("/api/")) return;
    if (request.headers.authorization !== `Bearer ${apiToken}`) {
      return reply.status(401).send({ error: "Unauthorized" });
    }
  });

  app.setErrorHandler((error, _request, reply) => {
    const errorMessage = error instanceof Error ? error.message : String(error);
    const statusCode =
      error instanceof z.ZodError
        ? 400
        : typeof error === "object" && error !== null && "statusCode" in error
          ? Number(error.statusCode)
          : /was not found/i.test(errorMessage)
            ? 404
            : /already assigned|must be stopped|wait for server|missing docker container/i.test(errorMessage)
              ? 409
              : 500;
    void reply.status(statusCode).send({ error: errorMessage });
  });

  app.get("/health", async () => {
    await manager.checkDocker();
    return { status: "ok" };
  });
  app.get("/api/versions", () => manager.versions());
  app.get("/api/versions/details", () => manager.versionDetails());
  app.get("/api/servers", () => manager.list());
  app.get<{ Params: { id: string } }>("/api/servers/:id", ({ params }) => manager.get(params.id));
  app.post("/api/servers", async (request, reply) => {
    const server = await manager.create(createServerSchema.parse(request.body));
    return reply.status(201).send(server);
  });
  app.post<{ Params: { id: string } }>("/api/servers/:id/start", ({ params }) => manager.start(params.id));
  app.post<{ Params: { id: string } }>("/api/servers/:id/stop", ({ params }) => manager.stop(params.id));
  app.post<{ Params: { id: string } }>("/api/servers/:id/restart", ({ params }) => manager.restart(params.id));
  app.post<{ Params: { id: string } }>("/api/servers/:id/update", ({ params, body }) => manager.update(params.id, updateServerSchema.parse(body)));
  app.put<{ Params: { id: string } }>("/api/servers/:id/resources", ({ params, body }) =>
    manager.updateResources(params.id, updateResourcesSchema.parse(body)),
  );
  app.get<{ Params: { id: string } }>("/api/servers/:id/files", ({ params }) => manager.listConfigFiles(params.id));
  app.get<{ Params: { id: string }; Querystring: { path?: string } }>("/api/servers/:id/file", async ({ params, query }) => {
    const { path } = configFileQuerySchema.parse(query);
    return { path, content: await manager.readConfigFile(params.id, path) };
  });
  app.put<{ Params: { id: string } }>("/api/servers/:id/file", async ({ params, body }, reply) => {
    const { path, content } = configFileBodySchema.parse(body);
    await manager.writeConfigFile(params.id, path, content);
    return reply.status(204).send();
  });
  app.delete<{ Params: { id: string } }>("/api/servers/:id", async ({ params }, reply) => {
    await manager.remove(params.id);
    return reply.status(204).send();
  });
  app.get<{ Params: { id: string }; Querystring: { tail?: string } }>("/api/servers/:id/logs", async ({ params, query }) => ({
    lines: await manager.logs(params.id, logsQuerySchema.parse(query).tail),
  }));

  return app;
}

export async function startApi(config: ServerConfig): Promise<void> {
  const app = await createApi(config);
  await app.listen({ host: config.host, port: config.port });
}
