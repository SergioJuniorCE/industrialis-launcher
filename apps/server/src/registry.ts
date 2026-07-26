import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { GtnhServer } from "@industrialis/server-contracts";

export class ServerRegistry {
  private readonly statePath: string;
  private queue: Promise<unknown> = Promise.resolve();

  constructor(private readonly dataDir: string) {
    this.statePath = join(dataDir, "servers.json");
  }

  async list(): Promise<GtnhServer[]> {
    return this.exclusive(() => this.read());
  }

  async add(server: GtnhServer): Promise<void> {
    await this.exclusive(async () => {
      const servers = await this.read();
      if (servers.some((candidate) => candidate.id === server.id)) {
        throw new Error(`Server ${server.id} already exists`);
      }
      if (servers.some((candidate) => candidate.port === server.port)) {
        throw new Error(`Port ${server.port} is already assigned`);
      }
      await this.write([...servers, server]);
    });
  }

  async save(servers: GtnhServer[]): Promise<void> {
    await this.exclusive(() => this.write(servers));
  }

  async update(id: string, update: Partial<GtnhServer>): Promise<GtnhServer> {
    return this.exclusive(async () => {
      const servers = await this.read();
      const index = servers.findIndex((server) => server.id === id);
      if (index < 0) throw new Error(`Server ${id} was not found`);
      const server = { ...servers[index]!, ...update };
      servers[index] = server;
      await this.write(servers);
      return server;
    });
  }

  async remove(id: string): Promise<void> {
    await this.exclusive(async () => {
      const servers = await this.read();
      await this.write(servers.filter((server) => server.id !== id));
    });
  }

  private async read(): Promise<GtnhServer[]> {
    try {
      return JSON.parse(await readFile(this.statePath, "utf8")) as GtnhServer[];
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return [];
      throw error;
    }
  }

  private async write(servers: GtnhServer[]): Promise<void> {
    await mkdir(this.dataDir, { recursive: true });
    const temporaryPath = `${this.statePath}.tmp`;
    await writeFile(temporaryPath, `${JSON.stringify(servers, null, 2)}\n`, "utf8");
    await rename(temporaryPath, this.statePath);
  }

  private exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.queue.then(operation, operation);
    this.queue = result.then(() => undefined, () => undefined);
    return result;
  }
}
