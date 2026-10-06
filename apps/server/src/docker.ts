import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import Docker from "dockerode";
import type {
  CreateServerInput,
  GtnhServer,
  ServerStatus,
  ServerVersionDetail,
  UpdateServerInput,
  UpdateServerResourcesInput,
} from "@industrialis/server-contracts";
import { DEFAULT_SERVER_MEMORY_MB, DEFAULT_SERVER_PORT, DEFAULT_SERVER_VERSION } from "@industrialis/server-contracts";
import type { ServerConfig } from "./config.js";
import { decodeDockerLogs } from "./docker-logs.js";
import { ServerRegistry } from "./registry.js";
import { getImageVersions, isValidImageTag } from "./image-versions.js";
import { fetchPackVersions } from "./pack-versions.js";
import { buildVersionDetails } from "./version-details.js";
import { ServerFiles } from "./server-files.js";

export { decodeDockerLogs } from "./docker-logs.js";

function slugify(value: string): string {
  const slug = value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return slug || "gtnh-server";
}

function dockerStatus(state: Docker.ContainerInspectInfo["State"]): ServerStatus {
  if (state.Running) return "running";
  if (state.Restarting) return "starting";
  if (state.Status === "created" || state.Status === "exited") return "stopped";
  if (state.Status === "dead") return "error";
  return "stopped";
}

const DOCKER_STOP_TIMEOUT_SECONDS = 120;
const SERVER_STARTUP_TIMEOUT_MS = 10 * 60_000;
const SERVER_STARTUP_POLL_INTERVAL_MS = 2_000;
const SERVER_READY_MESSAGE = /Done \([^)]*\)! For help, type/;

export class DockerServerManager {
  private readonly docker: Docker;
  private readonly registry: ServerRegistry;
  private readonly files: ServerFiles;
  private createQueue: Promise<unknown> = Promise.resolve();
  private readonly serverQueues = new Map<string, Promise<void>>();

  constructor(
    private readonly config: ServerConfig,
    docker?: Docker,
  ) {
    this.docker = docker ?? new Docker({ socketPath: config.dockerSocket });
    this.registry = new ServerRegistry(config.dataDir);
    this.files = new ServerFiles(this.docker, config);
  }

  async checkDocker(): Promise<void> {
    try {
      await this.docker.ping();
    } catch {
      throw new Error(`Docker Engine is unavailable at ${this.config.dockerSocket}. Verify the daemon is running and this user can access the socket.`);
    }
  }

  async list(): Promise<GtnhServer[]> {
    const servers = await this.registry.list();
    return Promise.all(servers.map((server) => this.refresh(server)));
  }

  async get(id: string): Promise<GtnhServer> {
    const server = (await this.registry.list()).find((candidate) => candidate.id === id);
    if (!server) throw new Error(`Server ${id} was not found`);
    return this.refresh(server);
  }

  async create(input: CreateServerInput): Promise<GtnhServer> {
    const result = this.createQueue.then(() => this.createServer(input));
    this.createQueue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  versions(): Promise<string[]> {
    return getImageVersions(this.config.imageRepository);
  }

  async versionDetails(): Promise<ServerVersionDetail[]> {
    const tags = await getImageVersions(this.config.imageRepository);
    let catalog = null;
    try {
      catalog = await fetchPackVersions();
    } catch {
      catalog = null;
    }
    return buildVersionDetails(tags, catalog);
  }

  private async createServer(input: CreateServerInput): Promise<GtnhServer> {
    const version = input.version ?? DEFAULT_SERVER_VERSION;
    if (!isValidImageTag(version)) throw new Error("GTNH version must be a valid image tag");
    await this.checkDocker();
    const servers = await this.registry.list();
    const baseId = slugify(input.name);
    let id = baseId;
    let suffix = 2;
    while (servers.some((server) => server.id === id)) id = `${baseId}-${suffix++}`;

    const port = input.port ?? DEFAULT_SERVER_PORT;
    const memoryMb = input.memoryMb ?? DEFAULT_SERVER_MEMORY_MB;
    const image = `${this.config.imageRepository}:${version}`;
    const volumeName = `industrialis-gtnh-${id}-data`;
    const server: GtnhServer = {
      id,
      name: input.name.trim(),
      version,
      image,
      port,
      memoryMb,
      status: "creating",
      containerId: null,
      volumeName,
      createdAt: new Date().toISOString(),
    };
    await this.registry.add(server);

    try {
      await this.pullImage(image);
      const serverDir = join(this.config.dataDir, id);
      const worldDir = join(serverDir, "world");
      const backupsDir = join(serverDir, "backups");
      const logsDir = join(serverDir, "logs");
      await Promise.all([mkdir(worldDir, { recursive: true }), mkdir(backupsDir, { recursive: true }), mkdir(logsDir, { recursive: true })]);
      await this.docker.createVolume({
        Name: volumeName,
        Labels: { "dev.industrialis.managed": "true", "dev.industrialis.server-id": id },
      });
      const container = await this.createManagedContainer(server);
      return await this.registry.update(id, { containerId: container.id, status: "stopped" });
    } catch (error) {
      await this.registry.update(id, { status: "error", error: String(error) });
      throw error;
    }
  }

  async start(id: string): Promise<GtnhServer> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      if (server.status === "running") return server;
      const container = this.requireContainer(server);
      await this.registry.update(id, { status: "starting", error: undefined });
      try {
        await this.startAndConfirm(container);
        return await this.registry.update(id, { status: "running" });
      } catch (error) {
        await this.registry.update(id, { status: "error", error: String(error) });
        throw error;
      }
    });
  }

  async stop(id: string): Promise<GtnhServer> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      if (server.status === "starting") throw new Error("Wait for the server to finish starting before stopping it");
      if (server.status !== "running") return server;
      const container = this.requireContainer(server);
      await this.registry.update(id, { status: "stopping" });
      try {
        await this.gracefulStop(container);
      } catch (error) {
        await this.registry.update(id, { status: "error", error: String(error) });
        throw error;
      }
      return this.registry.update(id, { status: "stopped" });
    });
  }

  async restart(id: string): Promise<GtnhServer> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      const container = this.requireContainer(server);
      if (server.status === "running") await this.gracefulStop(container);
      try {
        await this.startAndConfirm(container);
        return await this.registry.update(id, { status: "running", error: undefined });
      } catch (error) {
        await this.registry.update(id, { status: "error", error: String(error) });
        throw error;
      }
    });
  }

  async update(id: string, input: UpdateServerInput): Promise<GtnhServer> {
    return this.exclusiveServer(id, async () => {
      if (!isValidImageTag(input.version)) throw new Error("GTNH version must be a valid image tag");
      const server = await this.get(id);
      this.assertReplaceable(server);
      if (input.version === server.version) return server;
      const image = `${this.config.imageRepository}:${input.version}`;
      await this.pullImage(image);
      return this.replaceContainer(server, { version: input.version, image }, input.createBackup);
    });
  }

  async updateResources(id: string, input: UpdateServerResourcesInput): Promise<GtnhServer> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      this.assertReplaceable(server);
      const port = input.port ?? server.port;
      const memoryMb = input.memoryMb ?? server.memoryMb;
      if (port < 1024 || port > 65535 || !Number.isInteger(port)) {
        throw new Error("Game port must be an integer between 1024 and 65535");
      }
      if (memoryMb < 4096 || memoryMb > 131072 || !Number.isInteger(memoryMb)) {
        throw new Error("Memory must be an integer between 4096 and 131072 MB");
      }
      const allocated = (await this.registry.list()).find((candidate) => candidate.id !== id && candidate.port === port);
      if (allocated) throw new Error(`Port ${port} is already assigned`);
      if (server.port === port && server.memoryMb === memoryMb) return server;
      return this.replaceContainer(server, { port, memoryMb }, false);
    });
  }

  async listConfigFiles(id: string): Promise<string[]> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      this.assertStopped(server);
      return this.files.list(server);
    });
  }

  async readConfigFile(id: string, path: string): Promise<string> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      this.assertStopped(server);
      return this.files.read(server, path);
    });
  }

  async writeConfigFile(id: string, path: string, content: string): Promise<void> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      this.assertStopped(server);
      await this.files.write(server, path, content);
    });
  }

  async remove(id: string): Promise<void> {
    return this.exclusiveServer(id, async () => {
      const server = await this.get(id);
      if (server.containerId) {
        const container = this.docker.getContainer(server.containerId);
        const info = await container.inspect().catch(() => null);
        if (info?.State.Running) await this.gracefulStop(container);
        if (info) await container.remove();
      }
      await this.registry.remove(id);
    });
  }

  async logs(id: string, tail = 200): Promise<string> {
    const server = await this.get(id);
    const output = await this.requireContainer(server).logs({
      stdout: true,
      stderr: true,
      tail,
      timestamps: true,
    });
    // oxlint-disable-next-line no-control-regex -- Docker multiplexed logs may contain these control bytes.
    return decodeDockerLogs(output).replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "");
  }

  private async replaceContainer(
    server: GtnhServer,
    changes: Pick<Partial<GtnhServer>, "version" | "image" | "port" | "memoryMb">,
    createBackup: boolean,
  ): Promise<GtnhServer> {
    const oldContainer = this.requireContainer(server);
    const inspection = await oldContainer.inspect();
    if (inspection.State.Restarting) throw new Error("Wait for the server to finish restarting before changing it");
    const wasRunning = inspection.State.Running;

    if (wasRunning) {
      await this.registry.update(server.id, { status: "stopping" });
      try {
        await this.gracefulStop(oldContainer);
      } catch (error) {
        await this.registry.update(server.id, { status: "error", error: String(error) });
        throw error;
      }
    }

    let backupId: string | null = null;
    if (createBackup) {
      try {
        backupId = await this.files.createUpdateBackup(server);
      } catch (error) {
        let restartError: unknown;
        if (wasRunning) {
          try {
            await this.startAndConfirm(oldContainer);
          } catch (startError) {
            restartError = startError;
          }
        }
        await this.registry.update(server.id, {
          status: restartError ? "error" : wasRunning ? "running" : "stopped",
          error: restartError ? `Backup failed and the previous server could not restart: ${String(restartError)}` : undefined,
        });
        throw new Error(`Update cancelled because its pre-update backup failed: ${String(error)}`);
      }
    }

    const originalName = `industrialis-gtnh-${server.id}`;
    const rollbackName = `${originalName}-rollback-${randomUUID().slice(0, 8)}`;
    let oldRenamed = false;
    let newContainer: Docker.Container | undefined;
    try {
      await oldContainer.rename({ name: rollbackName });
      oldRenamed = true;
      const target = { ...server, ...changes, status: "stopped" as const };
      newContainer = await this.createManagedContainer(target);
      if (wasRunning) await this.startAndConfirm(newContainer);

      const updated = await this.registry.update(server.id, {
        ...changes,
        containerId: newContainer.id,
        status: wasRunning ? "running" : "stopped",
        error: undefined,
      });
      await oldContainer.remove().catch((error: unknown) => {
        process.emitWarning(`Previous container ${server.containerId} needs manual cleanup: ${String(error)}`);
      });
      return updated;
    } catch (error) {
      if (newContainer) await newContainer.remove({ force: true }).catch(() => undefined);
      let rollbackError: unknown;
      if (createBackup && backupId) {
        try {
          await this.files.restoreUpdateBackup(server, backupId);
        } catch (restoreError) {
          rollbackError = restoreError;
        }
      }
      if (oldRenamed) {
        try {
          await oldContainer.rename({ name: originalName });
        } catch (renameError) {
          rollbackError ??= renameError;
        }
      }
      if (wasRunning && !rollbackError) {
        try {
          await this.startAndConfirm(oldContainer);
        } catch (startError) {
          rollbackError = startError;
        }
      }
      await this.registry.update(server.id, {
        containerId: server.containerId,
        status: rollbackError ? "error" : wasRunning ? "running" : "stopped",
        error: rollbackError ? `Update failed and the previous server could not be restored: ${String(rollbackError)}` : undefined,
      });
      if (rollbackError) {
        throw new Error(`Update failed: ${String(error)}. Previous server rollback failed: ${String(rollbackError)}. Backup: ${backupId ?? "not created"}`);
      }
      throw error;
    }
  }

  private async createManagedContainer(server: GtnhServer): Promise<Docker.Container> {
    const serverDir = join(this.config.dataDir, server.id);
    const heapMb = server.memoryMb - 1024;
    return this.docker.createContainer({
      name: `industrialis-gtnh-${server.id}`,
      Image: server.image,
      Labels: {
        "dev.industrialis.managed": "true",
        "dev.industrialis.server-id": server.id,
      },
      Entrypoint: ["java"],
      Cmd: [`-Xms${heapMb}M`, `-Xmx${heapMb}M`, "-Dfml.readTimeout=180", "@java9args.txt", "-jar", "lwjgl3ify-forgePatches.jar", "nogui"],
      WorkingDir: "/app/server",
      ExposedPorts: { "25565/tcp": {} },
      HostConfig: {
        Memory: server.memoryMb * 1024 * 1024,
        PortBindings: { "25565/tcp": [{ HostPort: String(server.port) }] },
        RestartPolicy: { Name: "unless-stopped", MaximumRetryCount: 0 },
        Mounts: [
          { Type: "volume", Source: server.volumeName, Target: "/app/server" },
          { Type: "bind", Source: join(serverDir, "world"), Target: "/app/server/World" },
          { Type: "bind", Source: join(serverDir, "backups"), Target: "/app/server/backups" },
          { Type: "bind", Source: join(serverDir, "logs"), Target: "/app/server/logs" },
        ],
      },
    });
  }

  private async startAndConfirm(container: Docker.Container): Promise<void> {
    const beforeStart = await container.inspect();
    if (!beforeStart.State.Running) await container.start();
    const inspection = await container.inspect();
    if (!inspection.State.Running) {
      throw new Error(`Server container did not stay running after start (Docker state: ${inspection.State.Status})`);
    }
    await this.waitForServerReady(container, inspection.State.StartedAt);
  }

  private async waitForServerReady(container: Docker.Container, startedAt?: string): Promise<void> {
    const startedAtMs = startedAt ? Date.parse(startedAt) : Number.NEGATIVE_INFINITY;
    const deadline = Date.now() + SERVER_STARTUP_TIMEOUT_MS;
    while (Date.now() < deadline) {
      const inspection = await container.inspect();
      if (!inspection.State.Running) {
        throw new Error(`Server stopped before Minecraft reported ready (Docker state: ${inspection.State.Status})`);
      }
      const output = decodeDockerLogs(await container.logs({ stdout: true, stderr: true, tail: 5000, timestamps: true }));
      const readyThisStart = output.split(/\r?\n/).some((line) => {
        const timestampEnd = line.indexOf(" ");
        if (timestampEnd < 0 || !SERVER_READY_MESSAGE.test(line)) return false;
        return Date.parse(line.slice(0, timestampEnd)) >= startedAtMs;
      });
      if (readyThisStart) return;
      await new Promise<void>((resolve) => setTimeout(resolve, SERVER_STARTUP_POLL_INTERVAL_MS));
    }
    throw new Error("GTNH server did not report ready within 10 minutes");
  }

  private assertReplaceable(server: GtnhServer): void {
    if (server.status === "missing") throw new Error("Restore the missing Docker container before changing this server");
    if (server.status === "starting") throw new Error("Wait for the server to finish starting before changing it");
    if (server.status === "stopping") throw new Error("Wait for the server to finish stopping before changing it");
    if (server.status === "creating" || !server.containerId) {
      throw new Error("Wait for server creation to finish before changing this server");
    }
  }

  private assertStopped(server: GtnhServer): void {
    if (server.status !== "stopped") throw new Error("Stop the server before editing its config files");
    if (!server.containerId) throw new Error("Server has no Docker container");
  }

  private exclusiveServer<T>(id: string, operation: () => Promise<T>): Promise<T> {
    const previous = this.serverQueues.get(id) ?? Promise.resolve();
    const result = previous.then(operation, operation);
    const queue = result.then(
      () => undefined,
      () => undefined,
    );
    this.serverQueues.set(id, queue);
    void queue.then(() => {
      if (this.serverQueues.get(id) === queue) this.serverQueues.delete(id);
    });
    return result;
  }

  private requireContainer(server: GtnhServer): Docker.Container {
    if (!server.containerId) throw new Error(`Server ${server.id} has no Docker container`);
    return this.docker.getContainer(server.containerId);
  }

  private async refresh(server: GtnhServer): Promise<GtnhServer> {
    if (!server.containerId) return server;
    try {
      const info = await this.docker.getContainer(server.containerId).inspect();
      const status = dockerStatus(info.State);
      if ((server.status === "starting" || server.status === "stopping") && (status === "running" || status === "stopped")) {
        return server;
      }
      if (status === server.status && !server.error) return server;
      return this.registry.update(server.id, { status, error: undefined });
    } catch (error) {
      if ((error as { statusCode?: number }).statusCode !== 404) return server;
      return this.registry.update(server.id, {
        status: "missing",
        error: "The Docker container no longer exists",
      });
    }
  }

  private async pullImage(image: string): Promise<void> {
    const stream = await this.docker.pull(image);
    await new Promise<void>((resolve, reject) => {
      this.docker.modem.followProgress(stream, (error) => (error ? reject(error) : resolve()));
    });
  }

  private async gracefulStop(container: Docker.Container): Promise<void> {
    let stopError: unknown;
    try {
      await container.stop({ t: DOCKER_STOP_TIMEOUT_SECONDS });
    } catch (error) {
      stopError = error;
    }

    let state: Docker.ContainerInspectInfo;
    try {
      state = await container.inspect();
    } catch (inspectionError) {
      throw new Error(`Could not confirm the server stopped; operation cancelled to protect its files. ${String(stopError ?? inspectionError)}`);
    }
    if (state.State.Running) {
      throw new Error(`Docker did not confirm the server stopped; operation cancelled to protect its files. ${String(stopError ?? "")}`);
    }
    if (state.State.ExitCode === 137 || state.State.OOMKilled) {
      throw new Error("Docker had to kill the server forcefully; operation cancelled because the world may not have saved cleanly");
    }
  }
}
