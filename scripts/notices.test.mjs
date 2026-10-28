import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { collectNotices, collectDistributionNotices } from "./notices.mjs";
test("artifact notices retain license text for the resolved graph and exclude unrelated packages", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "industrialis-notices-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const packages = [];
  for (const name of ["industrialis-app", "library", "unrelated"]) {
    const folder = path.join(root, name);
    await mkdir(folder);
    packages.push({
      id: name,
      name,
      version: "1.0.0",
      license: "MIT",
      repository: `https://example.test/${name}`,
      manifest_path: path.join(folder, "Cargo.toml"),
    });
  }
  await writeFile(path.join(root, "library", "LICENSE"), "Copyright retained\nPermission granted\n");
  const metadata = {
    packages,
    resolve: {
      nodes: [
        { id: "industrialis-app", dependencies: ["library"] },
        { id: "library", dependencies: [] },
        { id: "unrelated", dependencies: [] },
      ],
    },
  };
  const report = await collectNotices(metadata);
  assert.match(report, /Copyright retained\nPermission granted/);
  assert.match(report, /library 1.0.0\nDeclared license: MIT/);
  assert.ok(!report.includes("unrelated"));
  assert.ok(!report.includes(root));
  await mkdir(path.join(root, "crates/app/assets"), { recursive: true });
  const artwork = "Copyright original artwork\nComplete permission and disclaimer\n";
  await writeFile(path.join(root, "crates/app/assets/LICENSE-LUCIDE"), artwork);
  const distribution = await collectDistributionNotices(metadata, root);
  assert.ok(distribution.includes(artwork));
  assert.ok(distribution.includes("https://github.com/lucide-icons/lucide"));
  packages[1].license_file = "../outside.txt";
  await writeFile(path.join(root, "outside.txt"), "Must not be distributed");
  await assert.rejects(collectNotices(metadata), /escapes its package directory/);
});
