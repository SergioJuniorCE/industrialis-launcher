# Open VSX themes

Open **Settings > Appearance > Browse Open VSX themes**. Search by a theme or
publisher name (Enter or Search), choose Install, then Apply a downloaded variant.
An empty search shows popular extensions. Load more retrieves further pages.
Details opens the extension's public page, including its publisher/license context.

A package can contribute several variants. Each becomes a Saved theme, with its
own dark/light mode. Applying a variant selects that mode. Saved themes remain
usable offline and after restarting. Select, edit, clone and delete use the existing
custom-theme controls. Existing preferences and unknown fields are preserved.
Package updates are explicit: repeat a search, then choose Update when the registry
reports a new version. Updating replaces matching variants without adding another
copy per version, preserving unrelated saved-theme extension fields. Variants no
longer contributed by an updated package remain saved until explicitly deleted.

## Native theme conversion

VS Code workbench colors map to native launcher surfaces, buttons, borders,
selection and text. Missing or invalid colors use the corresponding built-in
Monochrome mode. Hex RGB/RGBA and short hex colors are supported; transparency is
composited over the corresponding surface. The launcher keeps its own layout,
controls and window material preference.

The file editor uses the imported editor background and TextMate `tokenColors`
with its existing Syntect grammars. Scoped colors, bold, italic and underline are
supported. Low-contrast syntax text uses the editor's existing readability guard.
Theme changes replace the highlighter without replacing the editor or losing its
text/undo history. Switching to the opposite brightness uses Monochrome colors;
choose the package's matching variant for that brightness.

JSON/JSONC includes and referenced XML `.tmTheme` files are resolved within the
package. Icon themes, grammar extensions, semantic-token colors requiring a
language server, extension commands and runtime-generated themes are outside this
static color-theme integration. Theme packs without declared static color-theme
contributions show an error.

## Downloads and storage

The backend uses the public Open VSX API and pins the search result's version.
HTTPS downloads follow the registry's content host, and the VSIX must match the
registry's SHA-256 before theme parsing. ZIP paths, duplicate entries, links,
include cycles, item counts and sizes are checked. Extension code is never run.
Downloads stay in memory; the VSIX is not extracted or retained on disk.

Only resolved theme JSON, native colors and provenance (publisher, extension,
version, source page, registry checksum and license identifier) are stored in the
existing `launcher-settings.json` custom presets. The importer bounds a package
to 16 MiB compressed/64 MiB declared expanded data, each selected theme file to
2 MiB, and one converted bundle to 4 MiB. The browser rejects imports taking all
saved themes above 8 MiB. Fetch/parse failures leave saved settings unchanged;
preference-write failures retain the existing Retry/Discard workflow.

Implementation: `core/src/vscode_theme.rs`, `backend/src/openvsx.rs`,
`backend/src/theme_archive.rs`, `app/src/views/vsx_themes.rs`, and
`app/src/code_highlighting.rs` under `crates/`.

Sources: [Open VSX Registry API](https://github.com/eclipse-openvsx/openvsx/wiki/Registry-API)
and [VS Code color themes](https://code.visualstudio.com/api/extension-guides/color-theme).

## Verification

The provider/archive tests cover JSONC inheritance, TextMate file references,
light/dark variants, stable update identities, unsafe paths, cycles, mismatched
packages and non-color extensions. Native GPUI interaction tests cover import,
Apply, Saved theme selection, updates, profile reload and extension-field
preservation. Editor tests check scoped colors/styles and theme switching with
undo preserved. These GPUI tests use a virtual test context, never the host desktop.

The opt-in online smoke writes a new temporary profile and removes it on success:

```sh
cargo run -p industrialis-backend --example openvsx_theme_smoke --locked --jobs 1
```

It exercises public search, a real pinned Dracula package with checksum validation,
all contributed variants, native color validity, syntax retention, save/reload and
cleanup. A separate opt-in GPUI test dispatches the actual Install, Apply and Search
controls against the public registry:

```sh
cargo test -p industrialis-app --bin industrialis --locked --jobs 1 --features stable-updates online_install_and_search_dispatch_from_real_native_controls -- --ignored --test-threads=1
```

Both public-registry checks passed on 2026-10-14. Dracula 2.25.1 contributed two
dark variants, each with 85 syntax rules; their persisted theme data occupied
51,139 bytes. All writes used disposable profiles and those profiles were removed.
Results are in `target/live-acceptance/openvsx-20261014/`. A virtual test-context
pass does not establish rendered Windows/macOS/Linux desktop acceptance; no host
desktop input or personal profile was used.

The final Windows suites passed 50 core, 144 backend and 51 app tests. The opt-in
public-network GPUI test passed separately; the backend NSIS scenario was not run.
