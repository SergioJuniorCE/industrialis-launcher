<!-- industrialis-gpui-release-v1 -->

# Industrialis Launcher 0.4.0

The native Rust/GPUI launcher replaces the Electron launcher.

- Existing instances, accounts, groups, settings and persistent file/mod/resource-pack overrides use the same data directory. No import or data migration is required.
- Native file explorer with fuzzy path search, file context actions, Easy configuration controls, syntax highlighting and separate editor windows.
- Modernity resource packs and optional release mods, local add-on management and Open VSX color themes.
- Hourly pack checks, automatic instance sizes, group context menus and drag insertion previews.
- Windows stable installer replaces the old Squirrel application while preserving launcher data. Close the old launcher before installing.

## Downloads

- Windows x64 installer: **IndustrialisLauncherSetup.exe**. Windows portable: **IndustrialisGPUI-windows-x64.zip**.
- macOS Apple Silicon: **IndustrialisGPUI-macos-arm64.dmg**.
- Linux x64: **IndustrialisGPUI-linux-x64.tar.gz**. Requires a graphical desktop, Vulkan or OpenGL drivers, Fontconfig and X11/Wayland libraries.
- Windows installed builds support in-app updates. Portable, Linux and macOS updates are manual.

## Known limitations accepted for this release

- Google Drive needs a configured desktop OAuth client. Live Google backup upload/restore/retry acceptance is pending.
- Microsoft sign-in and refresh passed with a newly authorized test account; existing original-account refresh and macOS Keychain acceptance are not fully verified.
- Immediate Forge server auto-join failed in the software-rendered test fixture. Manual joining remains the alternative; general server/gameplay acceptance is incomplete.
- Physical IME/accessibility, macOS lifecycle, Wayland and older supported OS acceptance are incomplete. Linux launcher hiding uses minimize.
- Mesa 23's Vulkan software renderer showed a black window on Xvfb. OpenGL software rendering passed the same reproduction. For an affected Linux setup, try `VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null LIBGL_ALWAYS_SOFTWARE=1 ./industrialis`. Physical GPU/Vulkan acceptance remains incomplete.
- This is the first native stable release. An actual published native N-to-N+1 self-update has not yet been certified.
- Windows binaries are not publisher-signed. macOS is ad-hoc signed and not notarized; operating-system trust prompts may appear.
- The copy completion cue uses the OS sound; choosing a custom sound is a follow-up feature.

The previous Electron release remains available as [launcher-v0.3.0](https://github.com/SergioJuniorCE/industrialis-launcher/releases/tag/launcher-v0.3.0). For rollback, close the native launcher, back up the existing launcher data directory, uninstall only the native application, then reinstall 0.3.0. Do not delete the data directory or run both launchers against it.
