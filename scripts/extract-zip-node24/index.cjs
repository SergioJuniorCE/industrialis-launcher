// Forge 7's packager pins extract-zip 2.x, whose old extractor exits on Node 24.
// Keep the same promise-based API while using Electron's maintained extractor.
module.exports = async function extractZip(zipPath, options) {
  const { extract } = await import("@electron-internal/extract-zip");
  return extract(zipPath, options);
};
