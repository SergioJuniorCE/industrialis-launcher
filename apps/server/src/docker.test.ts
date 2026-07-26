import { describe, expect, it } from "vitest";
import { decodeDockerLogs } from "./docker.js";

function frame(stream: 1 | 2, value: string): Buffer {
  const payload = Buffer.from(value);
  const header = Buffer.alloc(8);
  header[0] = stream;
  header.writeUInt32BE(payload.length, 4);
  return Buffer.concat([header, payload]);
}

describe("decodeDockerLogs", () => {
  it("combines multiplexed stdout and stderr frames", () => {
    const output = Buffer.concat([frame(1, "server ready\n"), frame(2, "warning\n")]);
    expect(decodeDockerLogs(output)).toBe("server ready\nwarning\n");
  });

  it("keeps plain TTY output intact", () => {
    expect(decodeDockerLogs(Buffer.from("server ready\n"))).toBe("server ready\n");
  });
});
