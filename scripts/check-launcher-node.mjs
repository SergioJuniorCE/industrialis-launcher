const majorVersion = Number(process.versions.node.split(".")[0]);

if (majorVersion !== 24) {
  console.error(`Launcher packaging requires Node.js 24.x; found ${process.versions.node}.`);
  process.exit(1);
}
