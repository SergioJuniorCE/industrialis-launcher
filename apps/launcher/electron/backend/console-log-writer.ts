import fs from "node:fs/promises";
import path from "node:path";

export interface ConsoleLogEntry {
  stream: string;
  line: string;
}

export const MAX_PERSISTED_CONSOLE_LOG_BYTES = 4 * 1024 * 1024;
const CONSOLE_LOG_COMPACTION_TARGET_BYTES = 3 * 1024 * 1024;

async function compactLogFile(filePath: string): Promise<void> {
  const stat = await fs.stat(filePath).catch(() => null);
  if (!stat?.isFile() || stat.size <= MAX_PERSISTED_CONSOLE_LOG_BYTES) return;

  const bytesToRead = Math.min(stat.size, CONSOLE_LOG_COMPACTION_TARGET_BYTES);
  const start = stat.size - bytesToRead;
  const file = await fs.open(filePath, "r").catch(() => null);
  if (!file) return;

  try {
    const buffer = Buffer.alloc(bytesToRead);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, start);
    const tail = buffer.subarray(0, bytesRead);
    const firstLineEnd = start > 0 ? tail.indexOf(0x0a) : -1;
    const retained = firstLineEnd >= 0 ? tail.subarray(firstLineEnd + 1) : start > 0 ? Buffer.alloc(0) : tail;
    await fs.writeFile(filePath, retained);
  } finally {
    await file.close();
  }
}

export class ConsoleLogWriter {
  private readonly pending = new Map<string, Promise<void>>();

  constructor(private readonly pathForId: (id: string) => string) {}

  append(id: string, entry: ConsoleLogEntry): void {
    const filePath = this.pathForId(id);
    const serialized = `${JSON.stringify(entry)}\n`;
    if (Buffer.byteLength(serialized, "utf8") > MAX_PERSISTED_CONSOLE_LOG_BYTES) return;

    void this.enqueue(id, async () => {
      await fs.mkdir(path.dirname(filePath), { recursive: true });
      await fs.appendFile(filePath, serialized, "utf8");
      await compactLogFile(filePath);
    });
  }

  compact(id: string): Promise<void> {
    return this.enqueue(id, () => compactLogFile(this.pathForId(id)));
  }

  private enqueue(id: string, operation: () => Promise<void>): Promise<void> {
    const previous = this.pending.get(id) ?? Promise.resolve();
    const current = previous.then(operation).catch(() => undefined);
    this.pending.set(id, current);
    void current.then(() => {
      if (this.pending.get(id) === current) this.pending.delete(id);
    });
    return current;
  }

  async flush(id: string): Promise<void> {
    while (true) {
      const pending = this.pending.get(id);
      if (!pending) return;
      await pending;
      if (this.pending.get(id) === pending) {
        this.pending.delete(id);
        return;
      }
    }
  }
}
