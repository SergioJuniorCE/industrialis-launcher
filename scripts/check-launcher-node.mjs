const majorVersion = Number(process.versions.node.split(".")[0]);

if (majorVersion !== 22) {
  console.error(
    `Launcher packaging requires Node.js 22.x; found ${process.versions.node}.`,
  );
  process.exit(1);
}
