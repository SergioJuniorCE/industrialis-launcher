import { readdir, readFile, realpath } from "node:fs/promises";
import path from "node:path";

export async function collectDistributionNotices(metadata, appDirectory) {
  const artwork = await readFile(path.join(appDirectory, "crates/app/assets/LICENSE-LUCIDE"), "utf8");
  return `${await collectNotices(metadata)}\n${"=".repeat(72)}\n\nLucide SVG artwork\nSource: https://github.com/lucide-icons/lucide\n\n${artwork}\n`;
}

// Follow the actual resolved app graph, including build dependencies. Never
// include profile paths or unrelated workspace packages in the distributed report.
export async function collectNotices(metadata, rootName = "industrialis-app") {
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const root = metadata.packages.find((pkg) => pkg.name === rootName);
  if (!root) throw new Error(`Missing notice root ${rootName}`);
  const visited = new Set(),
    pending = [root.id];
  while (pending.length) {
    const id = pending.pop();
    if (visited.has(id)) continue;
    visited.add(id);
    const node = nodes.get(id);
    if (!node) throw new Error(`Missing resolved dependency ${id}`);
    pending.push(...node.dependencies);
  }
  const entries = [];
  for (const id of visited) {
    const pkg = packages.get(id);
    if (!pkg) throw new Error(`Missing package metadata ${id}`);
    const folder = path.dirname(pkg.manifest_path);
    const filenames = (await readdir(folder)).filter((name) => /^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-].*)?$/i.test(name));
    if (pkg.license_file && !filenames.includes(pkg.license_file)) filenames.push(pkg.license_file);
    const texts = [];
    const realFolder = await realpath(folder);
    for (const filename of filenames.sort()) {
      const target = await realpath(path.resolve(folder, filename));
      const relative = path.relative(realFolder, target);
      if (relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
        throw new Error(`Notice for ${pkg.name} escapes its package directory`);
      }
      texts.push(`--- ${filename} ---\n${await readFile(target, "utf8")}`);
    }
    entries.push({
      name: pkg.name,
      version: pkg.version,
      text: `${pkg.name} ${pkg.version}\nDeclared license: ${pkg.license ?? "Not declared"}\n${pkg.repository ? `Source: ${pkg.repository}\n` : ""}${texts.length ? texts.join("\n\n") : "No standalone license/notice file supplied in the package."}`,
    });
  }
  entries.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  return `Industrialis GPUI dependency notices\n\nGenerated from the locked Cargo app dependency graph. Declared license expressions and supplied notices are reproduced below. This inventory does not determine redistribution obligations.\n\n${entries.map((entry) => entry.text).join("\n\n" + "=".repeat(72) + "\n\n")}\n`;
}
