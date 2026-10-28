# Electron UI parity

Functional gaps and the criteria for removing Electron are tracked in
[MIGRATION_TRACKER.md](MIGRATION_TRACKER.md). Prioritize that checklist;
aesthetic matching can follow later.

The launcher migration should preserve the Electron launcher's interface. This is a
work list, not a declaration of pixel parity. Implemented dimensions were checked
against the actual React components and their computed browser styles using
isolated fictional instance data. Native screenshot comparison remains pending.

## Reference

Use the frozen [Electron 0.3.0 source](https://github.com/SergioJuniorCE/industrialis-launcher/tree/launcher-v0.3.0/apps/launcher/src):
`App.tsx`, the final overrides in `App.css`, and the shared `components/` directory.
The relevant components include
`InstanceGridCard`, `InstanceAvatar`, `AccountsTab`, `AccountSwitcher`,
`InstanceSettingsPanel`, `InstanceSettingsTabs`, `ProcessesTab`,
`ProcessesDropdown`, `VirtualizedLogList`, `WindowControls`, and `ui/button`.
Use the same saved settings and fixture data in both runtimes.

The 2026-10-12 library update removes ordinary pack-check buttons in favor of
startup/hourly polling and a manual context-menu item. Reordering shows a stable
rounded dashed landing slot matching the eventual drop. Native dark/light
screenshots were inspected on a private Xvfb; physical/platform comparison remains
open.

Top navigation uses transparent backgrounds and a 2px underline on the selected
tab, following the latest user correction. Hover adds a subtle theme tint. Other
panel tabs retain black for the selected tab and grey for unselected tabs.

## Implemented reference geometry

| Area                | Reference applied in Launcher                                                                                      |
| ------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Window              | Minimum 800×600; saved dimensions and maximized preference                                                         |
| Shell               | Toolbar 44px; footer 24px and 11px text; Windows controls 46×44px                                                  |
| Toolbar             | EBF image; original Lucide assets; 28px Add; 24px navigation; responsive text hiding                               |
| Buttons             | 32px default, 28px small, 32px icon, 6px corner radius                                                             |
| Icon alignment      | Navigation uses 16px icons and 16px label line height; icon-only buttons have one child and no flex gap            |
| Library             | 8px outer spacing; 144px tiles; 8px gaps; 56px avatars; 12px names and 10px metadata                               |
| Workspace           | Initial 54% library; 12px separator; library minimum 260px and maximum 68%; details minimum 320px                  |
| Detail header       | 16px horizontal / 12px vertical padding; 40px avatar; 16px title and 12px metadata                                 |
| Detail info/actions | Two-column horizontal metadata rows; 32px launch and icon controls; 16px horizontal / 12px vertical action padding |
| Accounts            | Centered 672px maximum; 24px row avatars and default stars; compact toolbar picker                                 |
| Preferences         | 1024px maximum; 192px sidebar; 24px content gap; Java/Appearance/Backups/About sections                            |
| Processes           | 280px sidebar capped at 32%; 12px titles; 10px badges; 288px toolbar popup with summary                            |
| Logs                | Consolas 11px; console row 18px; process row 20px; small copy/clear controls                                       |

The Industrialis preset retains the original theme colors. Shared controls use
the existing theme tokens, including primary hover tints and translucent selected
states. Native text metrics, image rendering, shadows, scrollbars, hover/focus
states, and custom titlebar behavior still require visual comparison.

Top-bar hover transitions have a launcher rendering regression test covering all
eleven Windows controls, entry and exit, light/dark modes, Monochrome/Industrialis,
and compact/default widths. Stable IDs are required for Launcher to retain hover
state and schedule redraws; retain those IDs when adjusting the toolbar.

## Remaining work

The user's 2026-10-12 contrast/editor request intentionally updates Sandstone
surfaces and the Files workspace. Bronze branding, navigation and icons remain.
Files now use independent bounded scrolling regions, a sidebar on wide panels,
stacked regions on narrow panels and persistent Easy/Advanced mode controls.
The native highlighted code editor has readable normal/active lines and a visible
Find bar. Dark/light screenshots were inspected on an owned disposable Xvfb;
see [EDITOR_DESIGN.md](EDITOR_DESIGN.md). Exact physical-platform pixel parity
remains pending. The initial audit table below describes visual comparisons;
current functional status belongs in the migration tracker.

| Area                    | Difference to resolve                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------- |
| Detail tabs             | Files, Mods and Backups are ported; exact panel geometry still needs platform comparison                      |
| Library management      | Management workflows are ported; compare drag indicators, menus and picker geometry                           |
| Detail actions          | Compare footer actions and group-change access against the Electron reference                                 |
| Preferences             | Java, themes and backup workflows are ported; compare control geometry and visual states                      |
| Settings panels         | Presets, multiline fields and backup UI are ported; exact control arrangement needs comparison                |
| Dialogs                 | Existing install/copy/update/reinstall/delete workflows work but need reference layout and state comparison   |
| Footer                  | Updater/status UI is ported; compare actual update and error states                                           |
| Input/accessibility     | Native editing and keyboard regressions pass; physical IME/font and platform accessibility checks remain open |
| Accounts/processes/logs | Source geometry is aligned; interaction and visual states require native comparison                           |

## Validation boundary

The Rust workspace tests cover backend workflows, split limits at minimum and
larger widths, and protection against stale asynchronous workspace saves.
The normal app and an isolated fixture build successfully. Fixture startup checks
exercise the library/details, console, instance settings, accounts, preferences,
process history, and popup views without operating on real launcher data. Ten
fixture cases stayed alive without panics, including 800×600 and light-mode Info.

The initial native capture attempt did not expose the fixture window. A later
fixture launch without the subprocess `windowsHide` flag made it targetable.
Native inspection confirms the icons render, and an actual topbar drag moved the
window by 70px horizontally and vertically. A Windows `WM_NCHITTEST` probe
reproduced the old toolbar returning `HTCLIENT`, then confirmed `HTCAPTION` after
the fix, with Add, navigation, and Close remaining client controls. Icon tests
also check inherited colors and rasterize every embedded SVG through the framework.
Native layout probes confirm the three-dot SVG shares the exact center of its
28×28 button and navigation icons share a vertical center with their labels.
These focused checks do not certify visual or interaction parity for every screen.

Before declaring 1:1 parity, compare both apps with the same fixture at 1280×800,
1100×750, and 800×600 in dark and light modes. Check empty, selected, running,
starting, failed, and completed states; menu/dialog placement; hover/focus;
scrolling; split resizing; and titlebar behavior. Resolve the pending features
above and compare their complete user flows as well.
