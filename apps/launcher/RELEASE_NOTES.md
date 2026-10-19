<!-- industrialis-gpui-release-v1 -->

# Industrialis Launcher 0.4.1

This patch release fixes the installer and application branding left over from the native launcher migration. It builds on the 0.4.0 native stable release without changing launcher data or user settings.

## Fixes

- The Windows installer title, wizard text, uninstall entry, Start menu shortcut, and executable file description now use **Industrialis Launcher** instead of **Industrialis Launcher (GPUI)**.
- Fresh Windows installations default to `%LOCALAPPDATA%\Programs\Industrialis Launcher`. Existing installations keep their registered folder and update in place.
- macOS, Linux, portable, and third-party notice surfaces use the same public product name.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisGPUI-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisGPUI-macos-arm64.dmg**.
- Linux x64: **IndustrialisGPUI-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
