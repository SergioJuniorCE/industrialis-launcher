<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.4.2

This release adds a Prism Launcher import so existing Prism instances can be copied into the native launcher. It builds on the 0.4.1 native stable release without changing launcher data or user settings.

## Features

- On first run with an empty library, Industrialis offers to copy instances from Prism Launcher. The same import is permanent from the toolbar Add menu and the library background menu.
- GTNH instances are pre-selected; other instances can be selected with a warning and are imported frozen.
- Imports are copies only: the Prism folder is never moved, registered, or written to. Accounts are not copied.
- Display names, icons, Java path, RAM, JVM arguments, and Prism groups are mapped. Name collisions receive a monotonic suffix.
- Imported GTNH copies stay frozen until a GTNH version is assigned, either during import or later from Info. Assigning a version enables update checks, updates, and clean reinstall.
- Imports run as cancellable Processes operations with per-instance failure isolation, and failed imports can be retried from the process card.
- Discovery checks the standard per-OS Prism data folders, honors a custom `InstanceDir`, and offers a manual folder fallback.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

Prism discovery on macOS, Linux, and Flatpak is implemented but was not exercised on those platforms. Real Prism profiles and assigning a version from the live catalog remain platform and live verification.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
