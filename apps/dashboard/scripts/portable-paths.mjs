#!/usr/bin/env node
/**
 * Rewrite absolute file:// paths baked by @astrojs/node into import.meta.url
 * relatives so the dist tree is relocatable (install tarballs, other machines).
 */
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const distServer = fileURLToPath(new URL("../dist/server/", import.meta.url));

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(path);
    else if (entry.name.endsWith(".mjs") || entry.name.endsWith(".js")) yield path;
  }
}

function rewrite(source) {
  let next = source;

  // entry.mjs _args client/server bases
  next = next.replace(/"client":\s*"file:\/\/\/[^"]+\/dist\/client\/"/g, `"client": new URL("../client/", import.meta.url).href`);
  next = next.replace(/"server":\s*"file:\/\/\/[^"]+\/dist\/server\/"/g, `"server": new URL("./", import.meta.url).href`);

  // adapter chunk outDir / serverDir
  next = next.replace(/new URL\("file:\/\/\/[^"]+\/dist\/client\/"\)/g, `new URL("../client/", import.meta.url)`);
  next = next.replace(/new URL\("file:\/\/\/[^"]+\/dist\/server\/"\)/g, `new URL("./", import.meta.url)`);

  return next;
}

let changed = 0;
for await (const file of walk(distServer)) {
  const before = await readFile(file, "utf8");
  let after = rewrite(before);

  // Astro serializes build-machine paths into the server manifest. Keep the
  // manifest itself relocatable instead of replacing those URLs with
  // file:///./, which resolves to the filesystem root at runtime.
  if (after.includes("const manifest = deserializeManifest(")) {
    after = after.replace(/^import /, 'import { fileURLToPath } from "node:url";\nimport ');
    after = after.replace(
      "if (manifest.sessionConfig) manifest.sessionConfig.driverModule = () => import(",
      'const runtimeDist = new URL("../", import.meta.url).href;\nmanifest.hrefRoot = runtimeDist;\nmanifest.cacheDir = new URL("../node_modules/.astro/", import.meta.url).href;\nmanifest.outDir = runtimeDist;\nmanifest.srcDir = runtimeDist;\nmanifest.publicDir = runtimeDist;\nmanifest.buildClientDir = new URL("../client/", import.meta.url).href;\nmanifest.buildServerDir = new URL("./", import.meta.url).href;\nif (manifest.sessionConfig?.options) manifest.sessionConfig.options.base = fileURLToPath(new URL("../node_modules/.astro/sessions", import.meta.url));\nif (manifest.sessionConfig) manifest.sessionConfig.driverModule = () => import(',
    );
  }

  if (after !== before) {
    await writeFile(file, after);
    changed += 1;
  }
}

console.log(`portable-paths: rewrote ${changed} file(s) under dist/server`);
