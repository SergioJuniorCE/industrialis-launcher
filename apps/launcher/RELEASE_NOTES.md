<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.7.0

This release refreshes the native launcher with a graphite-and-amber Industrialis theme, clearer navigation, and more compact instance details.

- Instance cards center the pack icon and overlay update and progress badges. Hover for the right-click hint, or press Shift+F10 on a focused card for the same menu.
- The Info tab groups pack version, play time, Java, memory, and other instance details in a two-column layout.
- Accounts, Settings, Add-ons, and Processes use updated layouts and controls.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- The installed Windows launcher supports in-app updates and channel switches. Download the matching build manually on portable Windows, Linux, and macOS.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
