import fs from "node:fs/promises";
import path from "node:path";

export interface ConsoleLogEntry {
  stream: string;
  line: string;
}

export class ConsoleLogWriter {
  private readonly pending = new Map<string, Promise<void>>();

  constructor(private readonly pathForId: (id: string) => string) {}

  append(id: string, entry: ConsoleLogEntry): void {
    const filePath = this.pathForId(id);
    const previous = this.pending.get(id) ?? Promise.resolve();
    const write = previous
      .then(async () => {
        await fs.mkdir(path.dirname(filePath), { recursive: true });
        await fs.appendFile(filePath, `${JSON.stringify(entry)}\n`, "utf8");
      })
      .catch(() => undefined);
    this.pending.set(id, write);
    void write.then(() => {
      if (this.pending.get(id) === write) this.pending.delete(id);
    });
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
