import fs from "node:fs/promises";
import { spawnGameProcess } from "./process-manager";

const [statePath = "", helperReadyPath = "", launchSignalPath = "", marker = ""] = process.argv.slice(2);
if (!statePath || !helperReadyPath || !launchSignalPath || !marker) throw new Error("Expected state, readiness, launch signal, and marker arguments");

await fs.writeFile(helperReadyPath, String(process.pid), "utf8");
while (
  !(await fs.stat(launchSignalPath).then(
    () => true,
    () => false,
  ))
) {
  await new Promise((resolve) => setTimeout(resolve, 25));
}

const running = await spawnGameProcess(process.execPath, ["-e", "setInterval(() => {}, 1_000)"], process.cwd(), {}, () => undefined);

const pendingStatePath = `${statePath}.tmp`;
await fs.writeFile(pendingStatePath, JSON.stringify({ marker, gamePid: running.pid, helperPid: process.pid }), "utf8");
await fs.rename(pendingStatePath, statePath);
setInterval(() => undefined, 1_000);
