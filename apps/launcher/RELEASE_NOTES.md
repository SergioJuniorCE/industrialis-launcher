<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.4.3

This release makes GTNH pack updates atomic and recoverable, preserves user-modified configs and scripts, and expands launcher update visibility with a status-bar indicator and changelog viewer. It builds on the 0.4.2 native stable release without changing launcher data or user settings.

## Features

- Pack updates now build the new version beside the live instance and swap it in only after it completes. A verified rollback copy restores the previous instance if any step fails.
- If the launcher closes mid-update, the next start recovers the previous instance automatically and reports it.
- User-modified and user-created files under `config` and `scripts` are carried into the updated pack. Files that still match the old pack are replaced by the new pack's version.
- The update and clean-reinstall dialog has been redesigned with pack mod change counts, custom mod selection, live analysis progress, searchable Stable/Beta version lists, and an optional instance copy before the operation.
- Process cards show animated spinners and progress rings while pack operations run.
- The status bar reports launcher update state and opens the update dialog. Settings → About and the update dialog include a changelog viewer with recent published stable release notes.
- The retired internal codename no longer appears in scripts, packaging paths, or the Windows install marker. Existing native installations that still use `gpui-install.json` upgrade in place.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
