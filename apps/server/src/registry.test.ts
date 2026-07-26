import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import type { GtnhServer } from "@industrialis/server-contracts";
import { ServerRegistry } from "./registry.js";

const directories: string[] = [];

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

function server(id: string, port: number): GtnhServer {
  return {
    id,
    name: id,
    version: "stable-latest",
    image: "example/gtnh:stable-latest",
    port,
    memoryMb: 6144,
    status: "stopped",
    containerId: null,
    volumeName: `${id}-data`,
    createdAt: new Date(0).toISOString(),
  };
}

describe("ServerRegistry", () => {
  it("serializes concurrent updates without losing either change", async () => {
    const directory = await mkdtemp(join(tmpdir(), "industrialis-registry-"));
    directories.push(directory);
    const registry = new ServerRegistry(directory);
    await registry.add(server("alpha", 25565));
    await registry.add(server("beta", 25566));

    await Promise.all([
      registry.update("alpha", { status: "running" }),
      registry.update("beta", { status: "error", error: "failed" }),
    ]);

    const servers = await registry.list();
    expect(servers.find(({ id }) => id === "alpha")?.status).toBe("running");
    expect(servers.find(({ id }) => id === "beta")?.status).toBe("error");
  });

  it("rejects duplicate host ports", async () => {
    const directory = await mkdtemp(join(tmpdir(), "industrialis-registry-"));
    directories.push(directory);
    const registry = new ServerRegistry(directory);
    await registry.add(server("alpha", 25565));
    await expect(registry.add(server("beta", 25565))).rejects.toThrow("already assigned");
  });
});
