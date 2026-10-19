// Kept in sync with launcher-gpui/backend/profile_owner.rs. This is an
// ownership/activation endpoint only: no credentials or profile payloads cross it.
import { createHash } from "node:crypto";
import { mkdir, realpath } from "node:fs/promises";
import { createConnection, createServer } from "node:net";

const PREFIX = "industrialis-profile-v1";
const TIMEOUT = 500;

export async function profileEndpoint(directory) {
  await mkdir(directory, { recursive: true });
  let name = await realpath(directory);
  if (process.platform === "win32") {
    name = name
      .replace(/^\\\\\?\\/u, "")
      .replaceAll("\\", "/")
      .toLowerCase();
    if (name.startsWith("unc/")) name = `//${name.slice(4)}`;
  }
  const digest = createHash("sha256").update(name).digest();
  return { key: digest.toString("hex"), port: 20000 + (digest.readUInt16BE(0) % 20000) };
}

function signalOwner({ key, port }) {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: "127.0.0.1", port });
    const timer = setTimeout(() => socket.destroy(new Error("Profile ownership response timed out")), TIMEOUT);
    let response = "";
    socket.setEncoding("utf8");
    socket.once("connect", () => socket.write(`${PREFIX} ${key} focus\n`));
    socket.on("data", (chunk) => {
      response += chunk;
      if (response.includes("\n")) {
        const fields = response.replace(/\n$/u, "").split(" ");
        if (
          response.endsWith("\n") &&
          fields.length === 4 &&
          fields[0] === PREFIX &&
          fields[1] === key &&
          ["launcher", "operation"].includes(fields[2]) &&
          /^[1-9]\d*$/u.test(fields[3]) &&
          Number(fields[3]) <= 0xffffffff
        )
          resolve(fields[2]);
        else reject(new Error("The occupied ownership endpoint did not identify this profile"));
        socket.destroy();
      } else if (response.length > 128) socket.destroy(new Error("Invalid ownership response"));
    });
    socket.once("error", reject);
    socket.once("end", () => reject(new Error("Incomplete ownership response")));
    socket.once("close", () => clearTimeout(timer));
  });
}

/** @returns {Promise<{primary: true, close: () => Promise<void>} | {primary: false, owner: string}>} */
export async function acquireProfileOwnership(directory, activate) {
  const endpoint = await profileEndpoint(directory);
  const { key, port } = endpoint;
  const sockets = new Set();
  const server = createServer((socket) => {
    sockets.add(socket);
    const timer = setTimeout(() => socket.destroy(), TIMEOUT);
    socket.once("close", () => {
      clearTimeout(timer);
      sockets.delete(socket);
    });
    socket.on("error", () => {});
    socket.setEncoding("utf8");
    let request = "";
    socket.on("data", (chunk) => {
      request += chunk;
      if (request.length > 128) return socket.destroy();
      if (!request.includes("\n")) return;
      const focus = request === `${PREFIX} ${key} focus\n`;
      if (!focus && request !== `${PREFIX} ${key} probe\n`) return socket.destroy();
      socket.removeAllListeners("data");
      if (focus) activate();
      socket.end(`${PREFIX} ${key} launcher ${process.pid}\n`);
    });
  });
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen({ host: "127.0.0.1", port, exclusive: true }, () => {
        server.removeListener("error", reject);
        resolve();
      });
    });
  } catch (error) {
    if (error.code !== "EADDRINUSE" && error.code !== "EACCES") throw error;
    return { primary: false, owner: await signalOwner(endpoint) };
  }
  return {
    primary: true,
    close: () =>
      new Promise((resolve, reject) => {
        for (const socket of sockets) socket.destroy();
        server.close((error) => (error ? reject(error) : resolve()));
      }),
  };
}
