#!/usr/bin/env node
/**
 * Build a portable Industrialis server + dashboard tarball for Linux hosts.
 *
 * Output: artifacts/server/industrialis-server-linux-x64.tar.gz
 */
import { cp, mkdir, rm, writeFile, readFile, chmod, access } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outRoot = join(root, "artifacts", "server");
const stageDir = join(outRoot, "stage", "industrialis");
const tarball = join(outRoot, "industrialis-server-linux-x64.tar.gz");

function run(cmd, args, cwd = root) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(cmd, args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
    child.on("exit", (code) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${cmd} ${args.join(" ")} exited ${code}`));
    });
  });
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function main() {
  console.log("Building contracts, server, dashboard…");
  await run("pnpm", ["--filter", "@industrialis/server-contracts", "build"]);
  await run("pnpm", ["--filter", "@industrialis/server", "build"]);
  await run("pnpm", ["--filter", "@industrialis/dashboard", "build"]);

  await rm(join(outRoot, "stage"), { recursive: true, force: true });
  await mkdir(stageDir, { recursive: true });

  // Deploy production package for the server (includes node_modules).
  const deployServer = join(outRoot, "deploy-server");
  await rm(deployServer, { recursive: true, force: true });
  await run("pnpm", ["--filter", "@industrialis/server", "deploy", "--prod", deployServer]);
  await cp(deployServer, stageDir, { recursive: true });

  // Deploy dashboard package, then ensure built dist is present.
  const deployDashboard = join(outRoot, "deploy-dashboard");
  await rm(deployDashboard, { recursive: true, force: true });
  await run("pnpm", ["--filter", "@industrialis/dashboard", "deploy", "--prod", deployDashboard]);
  const builtDashboardDist = join(root, "apps", "dashboard", "dist");
  if (!(await exists(builtDashboardDist))) {
    throw new Error("Dashboard dist missing after build");
  }
  // Prefer freshly built dist over any deploy-time residue.
  await rm(join(deployDashboard, "dist"), { recursive: true, force: true });
  await cp(builtDashboardDist, join(deployDashboard, "dist"), { recursive: true });
  // Install layout expects dashboard/server/entry.mjs (dist contents).
  await cp(join(deployDashboard, "dist"), join(stageDir, "dashboard"), { recursive: true });
  // Keep production node_modules for any non-bundled runtime deps.
  if (await exists(join(deployDashboard, "node_modules"))) {
    await cp(join(deployDashboard, "node_modules"), join(stageDir, "dashboard", "node_modules"), {
      recursive: true,
    });
  }

  const pkgPath = join(stageDir, "package.json");
  const pkg = JSON.parse(await readFile(pkgPath, "utf8"));
  pkg.bin = { industrialis: "./dist/cli.js" };
  pkg.name = "industrialis";
  await writeFile(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);

  await mkdir(join(stageDir, "bin"), { recursive: true });
  const shim = `#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export INDUSTRIALIS_DASHBOARD_DIR="\${INDUSTRIALIS_DASHBOARD_DIR:-\$ROOT/dashboard}"
exec node "\$ROOT/dist/cli.js" "$@"
`;
  const shimPath = join(stageDir, "bin", "industrialis");
  await writeFile(shimPath, shim, { encoding: "utf8", mode: 0o755 });
  try {
    await chmod(shimPath, 0o755);
  } catch {
    // Windows may ignore mode
  }

  await writeFile(
    join(stageDir, "INSTALL.json"),
    `${JSON.stringify(
      {
        name: "industrialis",
        version: pkg.version ?? "0.1.0",
        dashboard: "dashboard",
        cli: "dist/cli.js",
      },
      null,
      2,
    )}\n`,
  );

  await mkdir(outRoot, { recursive: true });
  await rm(tarball, { force: true });

  console.log("Creating tarball…");
  await run("tar", ["-czf", tarball, "-C", join(outRoot, "stage"), "industrialis"]);

  console.log(`Wrote ${tarball}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
