import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import Docker from "dockerode";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DockerServerManager } from "./docker.js";
import { getConfig } from "./config.js";

type CreateOptions = Parameters<Docker["createContainer"]>[0];

class FakeContainer {
  running = false;
  removed = false;
  currentName: string;
  starts = 0;
  execCalls = 0;
  stopTimeoutSeconds: number | undefined;
  exitCode = 0;
  ready = true;
  historicalReady = false;
  startedAt: string | undefined;

  constructor(
    readonly id: string,
    name: string,
    private readonly helperOutput = "",
    private readonly failStart = false,
    private readonly fileHelper = false,
  ) {
    this.currentName = name;
  }

  async start(): Promise<void> {
    this.starts += 1;
    if (this.failStart) throw new Error("simulated new image startup failure");
    this.running = true;
    this.startedAt = new Date().toISOString();
  }

  async stop(options?: { t?: number }): Promise<void> {
    this.stopTimeoutSeconds = options?.t;
    this.running = false;
  }

  async wait(): Promise<{ StatusCode: number }> {
    this.running = false;
    return { StatusCode: this.exitCode };
  }

  async inspect(): Promise<unknown> {
    return {
      State: {
        Running: this.running,
        Restarting: false,
        Status: this.running ? "running" : "exited",
        StartedAt: this.startedAt,
        ExitCode: this.exitCode,
        OOMKilled: false,
      },
    };
  }

  async exec(): Promise<{ start: () => Promise<void> }> {
    this.execCalls += 1;
    return { start: async () => undefined };
  }

  async logs(options?: { timestamps?: boolean }): Promise<Buffer> {
    if (this.fileHelper) return Buffer.from(this.helperOutput);
    const readyLine = 'Done (1.0s)! For help, type "help" or "?"';
    const lines = [
      ...(this.historicalReady ? [`2020-01-01T00:00:00.000Z ${readyLine}`] : []),
      ...(this.ready ? [`${this.startedAt ?? "2026-01-01T00:00:00.000Z"} ${readyLine}`] : []),
    ];
    return Buffer.from(options?.timestamps ? lines.join("\n") : lines.map((line) => line.slice(line.indexOf(" ") + 1)).join("\n"));
  }

  async rename({ name }: { name: string }): Promise<void> {
    this.currentName = name;
  }

  async remove(): Promise<void> {
    this.removed = true;
    this.running = false;
  }
}

function fakeDocker() {
  const containers = new Map<string, FakeContainer>();
  const createSpecs: CreateOptions[] = [];
  let failStartImage: string | null = null;
  let containerNumber = 0;
  const docker = {
    ping: vi.fn(async () => undefined),
    pull: vi.fn(async () => ({})),
    modem: { followProgress: (_stream: unknown, callback: (error?: Error | null) => void) => callback(null) },
    createVolume: vi.fn(async () => undefined),
    createContainer: vi.fn(async (options: CreateOptions) => {
      createSpecs.push(options);
      containerNumber += 1;
      const id = `container-${containerNumber}`;
      const label = options.Labels?.["dev.industrialis.operation"];
      const command = options.Cmd?.[0] ?? "";
      const output =
        label === "volume-file"
          ? command.includes("find /app/server/config")
            ? "/app/server/config/GT5U.cfg\n/app/server/server.properties\n/app/server/config/mod.jar\n"
            : command.includes("base64 -d")
              ? ""
              : "motd=Assembly Line\n"
          : "";
      const container = new FakeContainer(id, options.name ?? id, output, failStartImage === options.Image, label === "volume-file");
      containers.set(id, container);
      return container as unknown as Docker.Container;
    }),
    getContainer: vi.fn((id: string) => {
      const container = containers.get(id);
      if (!container) throw new Error(`Unknown fake container ${id}`);
      return container as unknown as Docker.Container;
    }),
  };
  return {
    docker: docker as unknown as Docker,
    createSpecs,
    containers,
    failStartForImage(image: string) {
      failStartImage = image;
    },
  };
}

const directories: string[] = [];

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

async function managerFixture() {
  const dataDir = await mkdtemp(join(tmpdir(), "industrialis-docker-manager-"));
  directories.push(dataDir);
  const harness = fakeDocker();
  return {
    manager: new DockerServerManager(getConfig({ dataDir }), harness.docker),
    dataDir,
    ...harness,
  };
}

describe("DockerServerManager server lifecycle", () => {
  it("creates a pinned server and replaces its image without losing the named volume", async () => {
    const { manager, createSpecs, containers } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "stable-latest", port: 25565, memoryMb: 6144 });

    expect(created.status).toBe("stopped");
    expect(createSpecs[0]).toMatchObject({
      Entrypoint: ["java"],
      Cmd: ["-Xms5120M", "-Xmx5120M", "-Dfml.readTimeout=180", "@java9args.txt", "-jar", "lwjgl3ify-forgePatches.jar", "nogui"],
      WorkingDir: "/app/server",
    });
    const updated = await manager.update(created.id, { version: "2.9.0-RC-2", createBackup: false });

    expect(updated.version).toBe("2.9.0-RC-2");
    expect(updated.image).toBe("ghcr.io/debuas/gtnhserverdocker:2.9.0-RC-2");
    expect(updated.volumeName).toBe(created.volumeName);
    expect(updated.containerId).toBe("container-2");
    expect(containers.get("container-1")?.removed).toBe(true);
    expect(createSpecs[1]?.Image).toBe("ghcr.io/debuas/gtnhserverdocker:2.9.0-RC-2");
    expect(createSpecs[1]?.HostConfig?.Mounts?.[0]?.Source).toBe(created.volumeName);
  });

  it("stops the Java server through Docker's graceful shutdown timeout before updating", async () => {
    const { manager, containers } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "2.8.4", port: 25565, memoryMb: 6144 });
    await manager.start(created.id);
    const originalContainer = containers.get(created.containerId!)!;

    const updated = await manager.update(created.id, { version: "2.9.0-RC-2", createBackup: false });

    expect(originalContainer.stopTimeoutSeconds).toBe(120);
    expect(originalContainer.execCalls).toBe(0);
    expect(originalContainer.running).toBe(false);
    expect(updated.status).toBe("running");
  });

  it("keeps a newly started server in starting state until Minecraft reports ready", async () => {
    const { manager, containers } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "stable-latest", port: 25565, memoryMb: 6144 });
    const container = containers.get(created.containerId!)!;
    container.ready = false;
    container.historicalReady = true;

    const starting = manager.start(created.id);
    try {
      await vi.waitFor(async () => {
        await expect(manager.get(created.id)).resolves.toMatchObject({ status: "starting" });
      });
    } finally {
      container.ready = true;
      await starting.catch(() => undefined);
    }
    await expect(starting).resolves.toMatchObject({ status: "running" });
  });

  it("clears a stuck stopping state when Docker has to kill a server", async () => {
    const { manager, containers } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "stable-latest", port: 25565, memoryMb: 6144 });
    await manager.start(created.id);
    containers.get(created.containerId!)!.exitCode = 137;

    await expect(manager.stop(created.id)).rejects.toThrow("world may not have saved cleanly");

    await expect(manager.get(created.id)).resolves.toMatchObject({ status: "stopped", error: undefined });
  });

  it("applies port and memory changes by replacing the container with the same server data", async () => {
    const { manager, createSpecs } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "2.8.4", port: 25565, memoryMb: 6144 });

    const updated = await manager.updateResources(created.id, { port: 25600, memoryMb: 8192 });

    expect(updated.port).toBe(25600);
    expect(updated.memoryMb).toBe(8192);
    expect(updated.version).toBe("2.8.4");
    expect(createSpecs[1]?.HostConfig?.Memory).toBe(8192 * 1024 * 1024);
    expect(createSpecs[1]?.HostConfig?.PortBindings?.["25565/tcp"]?.[0]?.HostPort).toBe("25600");
    expect(createSpecs[1]?.HostConfig?.Mounts?.[0]?.Source).toBe(created.volumeName);
  });

  it("does not back up or replace a server after Docker had to kill it forcefully", async () => {
    const { manager, createSpecs, containers } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "2.8.4", port: 25565, memoryMb: 6144 });
    await manager.start(created.id);
    containers.get(created.containerId!)!.exitCode = 137;

    await expect(manager.update(created.id, { version: "2.9.0-RC-2", createBackup: true })).rejects.toThrow("world may not have saved cleanly");

    const stopped = await manager.get(created.id);
    expect(stopped.version).toBe("2.8.4");
    expect(stopped.containerId).toBe(created.containerId);
    expect(stopped.status).toBe("stopped");
    expect(containers.get(created.containerId!)?.removed).toBe(false);
    expect(createSpecs).toHaveLength(1);
  });

  it("rejects a resource port already allocated to another managed server", async () => {
    const { manager, createSpecs } = await managerFixture();
    await manager.create({ name: "Alpha", version: "stable-latest", port: 25565, memoryMb: 6144 });
    const beta = await manager.create({ name: "Beta", version: "stable-latest", port: 25566, memoryMb: 6144 });

    await expect(manager.updateResources("alpha", { port: beta.port })).rejects.toThrow("Port 25566 is already assigned");
    expect(createSpecs).toHaveLength(2);
  });

  it("restores the old image, container, and files when an updated container will not start", async () => {
    const { manager, createSpecs, containers, dataDir, failStartForImage } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "2.8.4", port: 25565, memoryMb: 6144 });
    await manager.start(created.id);
    failStartForImage("ghcr.io/debuas/gtnhserverdocker:2.9.0-RC-2");

    await expect(manager.update(created.id, { version: "2.9.0-RC-2", createBackup: true })).rejects.toThrow("simulated new image startup failure");

    const restored = await manager.get(created.id);
    expect(restored.version).toBe("2.8.4");
    expect(restored.containerId).toBe("container-1");
    expect(restored.status).toBe("running");
    expect(containers.get("container-1")?.currentName).toBe("industrialis-gtnh-assembly-line");
    expect(containers.get("container-1")?.running).toBe(true);
    expect(createSpecs.some((spec) => spec.Cmd?.[0]?.includes("tar -xzf /backup/server-volume.tar.gz"))).toBe(true);
    const backupRoot = join(dataDir, "assembly-line", "backups");
    const [backupId] = await readdir(backupRoot);
    expect(backupId).toBeDefined();
    const backupMetadata = await readFile(join(backupRoot, backupId!, "backup.json"), "utf8");
    expect(JSON.parse(backupMetadata).version).toBe("2.8.4");
  });

  it("lists and edits only stopped-server config files through an isolated helper container", async () => {
    const { manager, createSpecs } = await managerFixture();
    const created = await manager.create({ name: "Assembly Line", version: "stable-latest", port: 25565, memoryMb: 6144 });

    await expect(manager.listConfigFiles(created.id)).resolves.toEqual(["config/GT5U.cfg", "server.properties"]);
    await expect(manager.readConfigFile(created.id, "config/GT5U.cfg")).resolves.toBe("motd=Assembly Line\n");
    await manager.writeConfigFile(created.id, "config/GT5U.cfg", "motd=Changed\n");
    await expect(manager.readConfigFile(created.id, "../../etc/passwd")).rejects.toThrow("config folder");

    const writeCommand = createSpecs.find((spec) => spec.Labels?.["dev.industrialis.operation"] === "volume-file" && spec.Cmd?.[0]?.includes("base64 -d"));
    expect(writeCommand?.Cmd?.at(-2)).toBe(Buffer.from("motd=Changed\n").toString("base64"));
    expect(writeCommand?.Cmd?.at(-1)).toBe("config/GT5U.cfg");
  });
});
