<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.4.4

This patch fixes launcher self-updates: installed builds can update while a game keeps running, and installations still using the retired `Industrialis GPUI` folder can move to the current directory without aborting. It builds on the 0.4.3 native stable release without changing launcher data or user settings.

## Fixes

- Running games no longer block an in-app launcher update. Only a game launch that is still starting blocks, because the installer handoff waits for the old launcher to exit.
- A self-update from `%LOCALAPPDATA%\Programs\Industrialis GPUI` now recognizes the old install marker, moves the files to `Industrialis Launcher`, restarts, and preserves unrelated files.
- The detached game broker is no longer mistaken for a second launcher window during old-Electron retirement.
- A rollback executable still mapped by a running game no longer blocks a later update; the installer switches to a unique rollback name.
- Installer smoke tests no longer read or migrate a real developer installation.

## Upgrading from 0.4.2 or 0.4.3

Close running games once for this update; the older launcher still enforces that check. After 0.4.4 is installed, later launcher updates run with games open.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
