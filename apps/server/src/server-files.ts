import { randomUUID } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import Docker from "dockerode";
import type { GtnhServer } from "@industrialis/server-contracts";
import type { ServerConfig } from "./config.js";
import { decodeDockerLogs } from "./docker-logs.js";

const MAX_CONFIG_FILE_BYTES = 1024 * 1024;
const CONFIG_FILE_EXTENSIONS = new Set(["cfg", "conf", "ini", "json", "properties", "toml", "txt", "xml", "yaml", "yml"]);

export function isManagedConfigFile(path: string): boolean {
  if (path === "server.properties") return true;
  if (!path.startsWith("config/") || path.includes("\\")) return false;
  const segments = path.split("/");
  if (segments.some((segment) => !segment || segment === "." || segment === "..")) return false;
  const fileName = segments.at(-1) ?? "";
  const extension = fileName.split(".").at(-1)?.toLowerCase();
  return Boolean(extension && CONFIG_FILE_EXTENSIONS.has(extension));
}

export class ServerFiles {
  constructor(
    private readonly docker: Docker,
    private readonly config: ServerConfig,
  ) {}

  async list(server: GtnhServer): Promise<string[]> {
    const output = await this.runVolumeCommand(
      server,
      "{ find /app/server/config -type f -print 2>/dev/null || true; if [ -f /app/server/server.properties ] && [ ! -L /app/server/server.properties ]; then printf '%s\\n' /app/server/server.properties; fi; }",
    );
    return [
      ...new Set(
        output
          .split(/\r?\n/)
          .map((entry) => entry.replace(/^\/app\/server\//, ""))
          .filter((entry) => isManagedConfigFile(entry)),
      ),
    ].sort((left, right) => left.localeCompare(right, undefined, { sensitivity: "base" }));
  }

  async read(server: GtnhServer, path: string): Promise<string> {
    this.assertConfigPath(path);
    const output = await this.runVolumeCommand(
      server,
      'set -eu; case "$1" in server.properties) target=/app/server/server.properties ;; config/*) target="/app/server/$1" ;; *) exit 2 ;; esac; test -f "$target"; test ! -L "$target"; resolved=$(realpath "$target"); case "$resolved" in /app/server/config/*|/app/server/server.properties) ;; *) exit 2 ;; esac; size=$(wc -c < "$resolved"); test "$size" -le 1048576 || { echo "Config file is larger than 1 MiB" >&2; exit 3; }; cat "$resolved"',
      [path],
    );
    return output;
  }

  async write(server: GtnhServer, path: string, content: string): Promise<void> {
    this.assertConfigPath(path);
    if (Buffer.byteLength(content, "utf8") > MAX_CONFIG_FILE_BYTES) {
      throw new Error("Config files must be 1 MiB or smaller");
    }
    if (content.includes("\u0000")) throw new Error("Config files cannot contain null bytes");
    const encoded = Buffer.from(content, "utf8").toString("base64");
    await this.runVolumeCommand(
      server,
      'set -eu; case "$2" in server.properties) target=/app/server/server.properties ;; config/*) target="/app/server/$2" ;; *) exit 2 ;; esac; test -f "$target"; test ! -L "$target"; resolved=$(realpath "$target"); case "$resolved" in /app/server/config/*|/app/server/server.properties) ;; *) exit 2 ;; esac; temporary="$target.industrialis-tmp"; trap \'rm -f "$temporary"\' EXIT; umask 077; printf "%s" "$1" | base64 -d > "$temporary"; mv -f "$temporary" "$target"',
      [encoded, path],
    );
  }

  async createUpdateBackup(server: GtnhServer): Promise<string> {
    const backupId = `pre-update-${new Date().toISOString().replace(/[:.]/g, "-")}`;
    const serverDir = join(this.config.dataDir, server.id);
    const backupDir = join(serverDir, "backups", backupId);
    await mkdir(backupDir, { recursive: false });
    try {
      await cp(join(serverDir, "world"), join(backupDir, "world"), { recursive: true, errorOnExist: true, force: false });
      await this.runVolumeCommand(
        server,
        "tar -czf /backup/server-volume.tar.gz --exclude=./World --exclude=./backups --exclude=./logs -C /app/server .",
        [],
        backupDir,
      );
      const metadata = {
        id: backupId,
        serverId: server.id,
        version: server.version,
        image: server.image,
        createdAt: new Date().toISOString(),
      };
      await writeFile(join(backupDir, "backup.json"), `${JSON.stringify(metadata, null, 2)}\n`, "utf8");
      return backupId;
    } catch (error) {
      await rm(backupDir, { recursive: true, force: true });
      throw error;
    }
  }

  async restoreUpdateBackup(server: GtnhServer, backupId: string): Promise<void> {
    if (!/^pre-update-[0-9T-Z-]+$/.test(backupId)) throw new Error("Invalid pre-update backup identifier");
    const serverDir = join(this.config.dataDir, server.id);
    const backupDir = join(serverDir, "backups", backupId);
    const metadata = JSON.parse(await readFile(join(backupDir, "backup.json"), "utf8")) as {
      id?: unknown;
      serverId?: unknown;
    };
    if (metadata.id !== backupId || metadata.serverId !== server.id) {
      throw new Error("Pre-update backup does not belong to this server");
    }
    await this.runVolumeCommand(
      server,
      "set -eu; find /app/server -mindepth 1 -maxdepth 1 ! -name World ! -name backups ! -name logs -exec rm -rf -- {} +; tar -xzf /backup/server-volume.tar.gz -C /app/server",
      [],
      backupDir,
    );
    const worldDir = join(serverDir, "world");
    await rm(worldDir, { recursive: true, force: true });
    await cp(join(backupDir, "world"), worldDir, { recursive: true, errorOnExist: true, force: false });
  }

  private assertConfigPath(path: string): void {
    if (!isManagedConfigFile(path) || path.length > 256) {
      throw new Error("Only files in the server root or config folder can be managed");
    }
  }

  private async runVolumeCommand(server: GtnhServer, script: string, args: string[] = [], backupDir?: string): Promise<string> {
    const helper = await this.docker.createContainer({
      name: `industrialis-gtnh-${server.id}-file-${randomUUID()}`,
      Image: server.image,
      Entrypoint: ["/bin/sh", "-c"],
      Cmd: [script, "industrialis", ...args],
      Labels: {
        "dev.industrialis.managed": "true",
        "dev.industrialis.server-id": server.id,
        "dev.industrialis.operation": "volume-file",
      },
      HostConfig: {
        NetworkMode: "none",
        Mounts: [
          { Type: "volume", Source: server.volumeName, Target: "/app/server" },
          ...(backupDir ? [{ Type: "bind" as const, Source: backupDir, Target: "/backup" }] : []),
        ],
      },
      Tty: false,
    });

    try {
      await helper.start();
      const result = await helper.wait();
      const output = decodeDockerLogs(await helper.logs({ stdout: true, stderr: true, timestamps: false }));
      if (result.StatusCode !== 0) throw new Error(output.trim() || "Server file operation failed");
      return output;
    } finally {
      await helper.remove({ force: true }).catch(() => undefined);
    }
  }
}
