import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import Docker from "dockerode";
import type { CreateServerInput, GtnhServer, ServerStatus } from "@industrialis/server-contracts";
import {
  DEFAULT_SERVER_MEMORY_MB,
  DEFAULT_SERVER_PORT,
  DEFAULT_SERVER_VERSION,
} from "@industrialis/server-contracts";
import type { ServerConfig } from "./config.js";
import { ServerRegistry } from "./registry.js";

function slugify(value: string): string {
  const slug = value.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return slug || "gtnh-server";
}

function dockerStatus(state: Docker.ContainerInspectInfo["State"]): ServerStatus {
  if (state.Running) return "running";
  if (state.Restarting) return "starting";
  if (state.Status === "created" || state.Status === "exited") return "stopped";
  if (state.Status === "dead") return "error";
  return "stopped";
}

export function decodeDockerLogs(output: Buffer): string {
  const chunks: Buffer[] = [];
  let offset = 0;

  while (offset + 8 <= output.length) {
    const streamType = output[offset];
    const length = output.readUInt32BE(offset + 4);
    const payloadStart = offset + 8;
    const payloadEnd = payloadStart + length;
    if ((streamType !== 1 && streamType !== 2) || payloadEnd > output.length) {
      return output.toString("utf8");
    }
    chunks.push(output.subarray(payloadStart, payloadEnd));
    offset = payloadEnd;
  }

  return offset === output.length ? Buffer.concat(chunks).toString("utf8") : output.toString("utf8");
}

export class DockerServerManager {
  private readonly docker = new Docker();
  private readonly registry: ServerRegistry;
  private createQueue: Promise<unknown> = Promise.resolve();

  constructor(private readonly config: ServerConfig) {
    this.registry = new ServerRegistry(config.dataDir);
  }

  async checkDocker(): Promise<void> {
    try {
      await this.docker.ping();
    } catch {
      throw new Error("Docker is unavailable. Start Docker and verify the current user can access it.");
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
    this.createQueue = result.then(() => undefined, () => undefined);
    return result;
  }

  private async createServer(input: CreateServerInput): Promise<GtnhServer> {
    await this.checkDocker();
    const servers = await this.registry.list();
    const baseId = slugify(input.name);
    let id = baseId;
    let suffix = 2;
    while (servers.some((server) => server.id === id)) id = `${baseId}-${suffix++}`;

    const version = input.version ?? DEFAULT_SERVER_VERSION;
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
      await Promise.all([
        mkdir(worldDir, { recursive: true }),
        mkdir(backupsDir, { recursive: true }),
        mkdir(logsDir, { recursive: true }),
      ]);
      await this.docker.createVolume({
        Name: volumeName,
        Labels: { "dev.industrialis.managed": "true", "dev.industrialis.server-id": id },
      });

      const heapMb = memoryMb - 1024;

      const container = await this.docker.createContainer({
        name: `industrialis-gtnh-${id}`,
        Image: image,
        Labels: {
          "dev.industrialis.managed": "true",
          "dev.industrialis.server-id": id,
        },
        Entrypoint: ["/bin/sh", "-c"],
        Cmd: [
          `sed -E -i 's/-Xms[^ ]+/-Xms${heapMb}M/g; s/-Xmx[^ ]+/-Xmx${heapMb}M/g' /app/server/startserver-java9.sh && exec /bin/sh /app/server/startserver-java9.sh`,
        ],
        ExposedPorts: { "25565/tcp": {} },
        HostConfig: {
          Memory: memoryMb * 1024 * 1024,
          PortBindings: { "25565/tcp": [{ HostPort: String(port) }] },
          RestartPolicy: { Name: "unless-stopped", MaximumRetryCount: 0 },
          Mounts: [
            { Type: "volume", Source: volumeName, Target: "/app/server" },
            { Type: "bind", Source: worldDir, Target: "/app/server/World" },
            { Type: "bind", Source: backupsDir, Target: "/app/server/backups" },
            { Type: "bind", Source: logsDir, Target: "/app/server/logs" },
          ],
        },
      });
      return await this.registry.update(id, { containerId: container.id, status: "stopped" });
    } catch (error) {
      await this.registry.update(id, { status: "error", error: String(error) });
      throw error;
    }
  }

  async start(id: string): Promise<GtnhServer> {
    const server = await this.get(id);
    const container = this.requireContainer(server);
    await this.registry.update(id, { status: "starting", error: undefined });
    await container.start();
    return this.registry.update(id, { status: "running" });
  }

  async stop(id: string): Promise<GtnhServer> {
    const server = await this.get(id);
    const container = this.requireContainer(server);
    await this.registry.update(id, { status: "stopping" });
    await this.gracefulStop(container);
    return this.registry.update(id, { status: "stopped" });
  }

  async restart(id: string): Promise<GtnhServer> {
    const server = await this.get(id);
    const container = this.requireContainer(server);
    await this.gracefulStop(container);
    await container.start();
    return this.registry.update(id, { status: "running", error: undefined });
  }

  async remove(id: string): Promise<void> {
    const server = await this.get(id);
    if (server.containerId) {
      const container = this.docker.getContainer(server.containerId);
      const info = await container.inspect().catch(() => null);
      if (info?.State.Running) await this.gracefulStop(container);
      if (info) await container.remove();
    }
    await this.registry.remove(id);
  }

  async logs(id: string, tail = 200): Promise<string> {
    const server = await this.get(id);
    const output = await this.requireContainer(server).logs({
      stdout: true,
      stderr: true,
      tail,
      timestamps: true,
    });
    return decodeDockerLogs(output).replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "");
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
    const exec = await container.exec({
      AttachStdout: true,
      AttachStderr: true,
      Cmd: [
        "/bin/sh",
        "-c",
        "for stat in /proc/[0-9]*/comm; do if [ \"$(cat \"$stat\")\" = java ]; then pid=${stat%/comm}; kill -TERM ${pid#/proc/}; exit 0; fi; done; exit 1",
      ],
    });
    await exec.start({ Detach: false }).catch(() => undefined);
    const exited = container.wait().then(() => true).catch(() => false);
    const timedOut = new Promise<false>((resolve) => setTimeout(() => resolve(false), 35_000));
    if (!(await Promise.race([exited, timedOut]))) await container.stop({ t: 15 });
  }
}
