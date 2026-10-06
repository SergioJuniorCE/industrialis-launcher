import { afterEach, describe, expect, it, vi } from "vitest";
import type { GtnhServer } from "@industrialis/server-contracts";
import { createApi } from "./api.js";
import { getConfig } from "./config.js";

const server: GtnhServer = {
  id: "assembly-line",
  name: "Assembly Line",
  version: "2.9.0-RC-2",
  image: "ghcr.io/debuas/gtnhserverdocker:2.9.0-RC-2",
  port: 25565,
  memoryMb: 6144,
  status: "stopped",
  containerId: "container-1",
  volumeName: "industrialis-gtnh-assembly-line-data",
  createdAt: new Date(0).toISOString(),
};

const apps: Array<Awaited<ReturnType<typeof createApi>>> = [];

afterEach(async () => {
  await Promise.all(apps.splice(0).map((app) => app.close()));
});

async function createTestApi() {
  const manager = {
    checkDocker: vi.fn(async () => undefined),
    list: vi.fn(async () => [server]),
    get: vi.fn(async () => server),
    create: vi.fn(async () => server),
    start: vi.fn(async () => server),
    stop: vi.fn(async () => server),
    restart: vi.fn(async () => server),
    remove: vi.fn(async () => undefined),
    logs: vi.fn(async () => "server ready\n"),
    versions: vi.fn(async () => ["stable-latest", "2.9.0-RC-2"]),
    versionDetails: vi.fn(async () => [
      { tag: "stable-latest", packVersion: "stable-latest", channel: "stable", title: "Stable release", releaseDate: null, maxJavaVersion: null },
      {
        tag: "2.9.0-RC-2",
        packVersion: "2.9.0-RC-2",
        channel: "beta",
        title: "Beta release",
        releaseDate: "2026/10/04",
        maxJavaVersion: 26,
      },
    ]),
    update: vi.fn(async () => server),
    updateResources: vi.fn(async () => server),
    listConfigFiles: vi.fn(async () => ["server.properties", "config/GT5U.cfg"]),
    readConfigFile: vi.fn(async () => "motd=Assembly Line\n"),
    writeConfigFile: vi.fn(async () => undefined),
  };
  const app = await createApi(getConfig({ host: "127.0.0.1", port: 0, dataDir: "test-data" }), {
    manager: manager as never,
    apiToken: "test-token",
    logger: false,
  });
  apps.push(app);
  return { app, manager };
}

describe("server management API", () => {
  it("requires authentication for the live GTNH version catalog", async () => {
    const { app } = await createTestApi();
    const unauthorized = await app.inject({ method: "GET", url: "/api/versions" });
    expect(unauthorized.statusCode).toBe(401);
    const unauthorizedDetails = await app.inject({ method: "GET", url: "/api/versions/details" });
    expect(unauthorizedDetails.statusCode).toBe(401);

    const response = await app.inject({
      method: "GET",
      url: "/api/versions",
      headers: { authorization: "Bearer test-token" },
    });
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual(["stable-latest", "2.9.0-RC-2"]);

    const details = await app.inject({
      method: "GET",
      url: "/api/versions/details",
      headers: { authorization: "Bearer test-token" },
    });
    expect(details.statusCode).toBe(200);
    expect(details.json()).toEqual([
      { tag: "stable-latest", packVersion: "stable-latest", channel: "stable", title: "Stable release", releaseDate: null, maxJavaVersion: null },
      {
        tag: "2.9.0-RC-2",
        packVersion: "2.9.0-RC-2",
        channel: "beta",
        title: "Beta release",
        releaseDate: "2026/10/04",
        maxJavaVersion: 26,
      },
    ]);
  });

  it("creates a server with the chosen release candidate, port, and memory", async () => {
    const { app, manager } = await createTestApi();
    const response = await app.inject({
      method: "POST",
      url: "/api/servers",
      headers: { authorization: "Bearer test-token" },
      payload: { name: "Assembly Line", version: "2.9.0-RC-2", port: 25565, memoryMb: 8192 },
    });

    expect(response.statusCode).toBe(201);
    expect(manager.create).toHaveBeenCalledWith({
      name: "Assembly Line",
      version: "2.9.0-RC-2",
      port: 25565,
      memoryMb: 8192,
    });
  });

  it("routes authenticated start, stop, restart, and remove operations", async () => {
    const { app, manager } = await createTestApi();
    const headers = { authorization: "Bearer test-token" };
    for (const action of ["start", "stop", "restart"] as const) {
      const response = await app.inject({ method: "POST", url: `/api/servers/assembly-line/${action}`, headers });
      expect(response.statusCode).toBe(200);
    }
    const removed = await app.inject({ method: "DELETE", url: "/api/servers/assembly-line", headers });
    expect(removed.statusCode).toBe(204);
    expect(manager.start).toHaveBeenCalledWith("assembly-line");
    expect(manager.stop).toHaveBeenCalledWith("assembly-line");
    expect(manager.restart).toHaveBeenCalledWith("assembly-line");
    expect(manager.remove).toHaveBeenCalledWith("assembly-line");
  });

  it("updates a server, applies resource settings, and reads and writes config files", async () => {
    const { app, manager } = await createTestApi();
    const headers = { authorization: "Bearer test-token" };

    const update = await app.inject({
      method: "POST",
      url: "/api/servers/assembly-line/update",
      headers,
      payload: { version: "2.9.0-RC-2", createBackup: true },
    });
    expect(update.statusCode).toBe(200);
    expect(manager.update).toHaveBeenCalledWith("assembly-line", {
      version: "2.9.0-RC-2",
      createBackup: true,
    });

    const resources = await app.inject({
      method: "PUT",
      url: "/api/servers/assembly-line/resources",
      headers,
      payload: { port: 25566, memoryMb: 8192 },
    });
    expect(resources.statusCode).toBe(200);
    expect(manager.updateResources).toHaveBeenCalledWith("assembly-line", {
      port: 25566,
      memoryMb: 8192,
    });

    const files = await app.inject({ method: "GET", url: "/api/servers/assembly-line/files", headers });
    expect(files.statusCode).toBe(200);
    expect(files.json()).toEqual(["server.properties", "config/GT5U.cfg"]);

    const file = await app.inject({
      method: "GET",
      url: "/api/servers/assembly-line/file?path=config%2FGT5U.cfg",
      headers,
    });
    expect(file.statusCode).toBe(200);
    expect(file.json()).toEqual({ path: "config/GT5U.cfg", content: "motd=Assembly Line\n" });

    const save = await app.inject({
      method: "PUT",
      url: "/api/servers/assembly-line/file",
      headers,
      payload: { path: "config/GT5U.cfg", content: "motd=New MOTD\n" },
    });
    expect(save.statusCode).toBe(204);
    expect(manager.writeConfigFile).toHaveBeenCalledWith("assembly-line", "config/GT5U.cfg", "motd=New MOTD\n");
  });

  it("rejects config paths outside the managed server config area", async () => {
    const { app, manager } = await createTestApi();
    const response = await app.inject({
      method: "GET",
      url: "/api/servers/assembly-line/file?path=../../etc/passwd",
      headers: { authorization: "Bearer test-token" },
    });

    expect(response.statusCode).toBe(400);
    expect(manager.readConfigFile).not.toHaveBeenCalled();
  });
});
