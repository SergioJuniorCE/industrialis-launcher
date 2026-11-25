<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.6.0

This release adds selectable launcher update channels, world backups for in-place pack updates, a Recommended add-ons tab for GT New Horizons, and reusable config profiles.

## Launcher update channels

- Choose **Stable**, **Nightly**, or **Preview** in **Settings → Updates**. Stable is the default for this release. Nightly offers builds from recent changes and may contain regressions.
- When you switch channels, the next update check offers the newest build in the selected channel, even if its version number matches or is older than your current build. The Updates page shows which build you're running.

## Pack updates and world backups

- In-place pack updates back up your worlds before replacing the instance. The launcher keeps the two most recent update backups per instance and shows the backup path in the operation log.
- Select **Install as a new instance** in the update dialog to keep playing the original instance on its current pack version. The new instance carries over your saves, configs, selected custom mods, and launch settings.
- The update dialog warns when you change pack series and explains the recommended upgrade path.

## Recommended add-ons

- Open **Add-ons → Recommended** to browse GTNotLeisure, GTNHRates, and Bandit. The tab shows which add-ons are available for your pack series and installs verified releases with any required companion libraries.

## Config profiles

- Create and edit reusable sets of config overrides in **Settings → Config profiles**. Choose a profile when creating an instance or apply one from an existing instance’s settings while the game is closed.
- The included **GTNH quiet defaults** profile adjusts pollution, underground dirt and gravel generation, cave generation, and chunk claiming. The launcher reports any config files or keys it skipped.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- The installed Windows launcher supports in-app updates and channel switches. Download the matching build manually on portable Windows, Linux, and macOS.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
