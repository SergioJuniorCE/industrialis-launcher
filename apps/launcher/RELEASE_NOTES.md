<!-- industrialis-launcher-release-v1 -->

# Industrialis Launcher 0.5.0

This release adds a built-in Modrinth mod browser, mod enable/disable toggles that survive updates, and configurable keyboard shortcuts for the first nine library slots. It also stops repeated sign-ins from tripping authentication rate limits. It builds on the 0.4.4 native stable release without changing launcher data or user settings.

## Features

- A new **Browse mods** tab in an instance's Add-ons searches Modrinth for mods compatible with that instance's game version and loader. A two-pane view shows project details, links and available versions, and each install resolves required dependencies, validates conflicts and verifies downloads before applying the set atomically with rollback.
- Pack mods can now be enabled or disabled from the Mods tab. Disabled mods are renamed to `.disabled` on disk and remain disabled across pack updates, reinstalls and launches.
- Preferences → Shortcuts binds configurable keys to the first nine library slots. Shortcut hints appear on instance cards, and shortcuts can be changed, cleared or reset; they work while the launcher is focused and are ignored in dialogs.

## Fixes

- Launching a game reuses an existing game token when at least one hour of validity remains. This avoids a fresh sign-in on every launch and the HTTP 429 rate limits that came with it.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisLauncher-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisLauncher-macos-arm64.dmg**.
- Linux x64: **IndustrialisLauncher-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations

The limitations documented for 0.4.0 still apply, including unsigned Windows binaries, ad-hoc-signed and non-notarized macOS builds, and incomplete physical IME/accessibility, macOS lifecycle, Wayland, and older-OS acceptance. Google Drive still needs a configured desktop OAuth client.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
