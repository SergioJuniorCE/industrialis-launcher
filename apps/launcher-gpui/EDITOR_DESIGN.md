# Files editor design and acceptance

## Easy mode responsiveness, 2026-10-15

Easy forms use GPUI's variable-height virtual list, retaining each visible input's
native entity, draft value, and undo history while constructing only viewport rows.
The parsed Forge document is cached against the exact current source. Source changes
invalidate parsed data and list measurements without moving the scroll anchor.
Descriptions still wrap and multiline settings retain their native text areas.
Save controls sit outside the scrolling form, including on narrow panels.

The 500-setting hover regression measured 502.29 ms per CPU redraw before this
change and 4.44 ms after virtualization and parse caching (virtual GPUI test, not
GPU FPS). The test also reaches and edits the last field. Native Windows acceptance:
`scripts/test-gpui-windows-caption-sandbox.ps1 -Scenario easy`.

## Explorer performance, 2026-10-15

The native Explorer includes ranked fuzzy filename/path search. Each whitespace
separated word must match the relative path: `server ranks` finds
`serverutilities/server/ranks.txt`. Abbreviated character sequences such as
`srvr rnks` also match. Exact names and contiguous filename/component matches rank
above weaker subsequences; this is not a file-content search or typo correction.

The first query builds a per-instance, in-memory path index on a backend worker.
Later queries reuse that index, checking directory modification timestamps for
added, removed, or renamed entries. Refresh explicitly invalidates it. Matched-file
metadata is checked at query time for deletions, current sizes, and persistent
override indicators. Nothing is indexed on disk and file contents are not read.
The debounce is 60 ms; superseded requests are cancelled. The 2,001-file regression
measured 1.78 ms per indexed query on the development machine. These are backend
timings, excluding debounce and the initial filesystem scan.

Clearing search preserves expanded folders and the open draft. Results include
instance-relative paths. Broad searches show the best 2,000 matches with a request
to narrow the query; unreadable folders are reported and links are not followed.

Right-click or Shift+F10 on an Explorer row offers an external editor, relative path
copy (from `.minecraft`), absolute path copy, and opening the file's location. These
actions target the clicked row even when another file is open. External opening
saves the current valid draft first and uses the existing persistent-edit mirror;
noneditable/executable paths retain backend guards. Instance-folder actions open
the directory rather than revealing its entry inside the parent folder.

Pop-out moves the saved draft to its own native window. Old pane mouse callbacks
tolerate the moved draft through the rest of that pointer event; otherwise bubbling
mouse-up could abort the native launcher. Native file action and pop-out acceptance:
`scripts/test-gpui-windows-caption-sandbox.ps1 -Scenario files`.

Keep the editor native in GPUI, as requested. The Explorer now uses GPUI's
virtualized uniform list: only viewport rows become UI elements, while the full
directory data and stable row IDs remain available for navigation. Folder loading,
drafts, Easy/Advanced editing, and file persistence keep their existing paths.

The `large_folder_hover_work_is_bounded_by_the_viewport` regression drives pointer
movement over actual rendered rows. With 5,000 cached entries, CPU hover redraw
time fell from 524.75 ms to 4.42 ms on the development machine. This virtual GPUI
measurement is not a native GPU frame-rate claim. Native Windows acceptance runs
with `scripts/test-gpui-windows-caption-sandbox.ps1 -Scenario explorer`, using an
isolated 5,000-file profile and checking that End reaches the last editable file.

Brief: repair the native launcher's contrast, expose every directory entry, retain
Easy configuration controls and provide an actual highlighted code editor.
This is a targeted product UI repair, preserving navigation, icons, logo and the
shared profile. Design variance 3, motion intensity 1, visual density 7.

The supplied screenshot uses Sandstone: bronze actions, warm near-black surfaces,
small system text, a fixed file-list height and almost indistinguishable panels.
The previous GPUI editor already uses gpui-base's native EditorState, with line
numbers, a search engine, selection and undo; it never installs a syntax highlighter
or renders the search input.
Forge's lossless Easy editor exists, but a single conditional mode-switch button
makes it hard to find. Directory entries come from the existing file service;
the layout lets rows shrink and spill outside the usable scrolling area.

Use the design-taste-frontend skill's audit, contrast and preservation guidance.
Its marketing layouts and web editor recommendations do not apply to this native
product. Keep the pinned GPUI/gpui-base editing engine; Syntect supplies maintained
grammars through the engine's supported InputHighlighter seam. Forge gets a small
bundled syntax definition for its property types, comments, values and sections.
This is real native editing with syntax-colored spans, not an overlay over a
textarea or a webview. Unknown languages retain plain-text editing.

The file sidebar and editor have distinct bordered surfaces. Wide panels place
them side by side; narrow panels put a bounded browser above the editor. Each
region owns its scroll; file rows retain their height. Save controls stay outside
the code/Easy scrolling region. Easy and Advanced are persistent mode tabs after
opening a file, with an explanation when Easy is unavailable for its format.

Sandstone keeps its bronze accent with neutral charcoal surfaces in dark mode
and cool off-white surfaces in light mode. Keep the existing radius and icon
family. Primary button text and code token colors target at least 4.5:1 contrast;
Sandstone secondary text targets 4.5:1 and control boundaries 3:1. Syntax colors
carry token meaning, while bronze remains the product's action/selection accent.
Stored custom theme overrides are retained.

Use real platform font family names for code: Consolas on Windows, Menlo on
macOS and the Fontconfig monospace alias on Linux. The pinned Windows DirectWrite
resolver only special-cases `.SystemUIFont`; a literal `monospace` name can fall
through to GPUI's proportional fallback stack.

Acceptance must cover the last entry in a directory with more than one screen of
files; Easy scalar/list edits; Advanced Unicode edits, highlighting, clipboard,
undo and saved game/overlay bytes; bounded layouts and visible controls at small
and large windows; both themes; and the existing conflicts/pop-out/shared drafts.
Native screenshots and input tests run only on disposable owned desktops.

## Verified repair, 2026-10-12

The GPUI regression reaches file 120 in a bounded scrolling directory and saves
an Easy edit with its comments intact in both game and persistent overlay files.
A second regression checks painted and clickable row bounds in a wide layout.
The existing Advanced edit/save/conflict/pop-out and Unicode/clipboard/undo tests
remain in the suite. Highlight tests check JSON and Forge ranges, Unicode byte
boundaries, incremental edits and token contrast over both normal and active lines.

Native acceptance exposed the missing Find presentation: Ctrl+F opened the engine
session but typing still edited the document. The wrapper now renders a focused
Find input, match count, Previous/Next and Close. Escape restores code focus.
Its regression verifies that a query finds two matches without changing the
document. Only changes to search presentation trigger wrapper redraws.

The actual app passes native Find, Easy boolean editing, matching game/overlay
bytes and normal close in both Sandstone modes on a private Linux Xvfb. Screenshots
must wait for actual source glyphs, rather than merely the selected Advanced tab
while a file read is pending. The container lacks a CJK font; Unicode source bytes
are preserved, but this does not certify physical font/IME behavior. Original
profiles and the user's running development executable remain untouched.

## Explorer tree redesign, 2026-10-13

The supplied VS Code Explorer screenshot is the reference for the file browser:
compact rows, chevrons, file-type icons, depth guides and a full-width selected row.
The main launcher navigation and native Easy/Advanced editor remain in place.
The sidebar is a flat Explorer surface rather than a card with a wrapped toolbar.
Refresh, Collapse all and Pop out use named icon controls; .minecraft is a
collapsible root. New File, New Folder and Git badges are not presented as
non-functional decorations. Persistent overrides retain their own real marker.

Folders expand in place and load only when opened. Their cached contents and
expansion state belong to the instance draft. Folder requests have their own
refresh generation, independently of document reads/saves. Expanding, collapsing
and refreshing never save, clear or replace a document draft. Opening another
file retains the existing save/conflict protection. Tree state stays separate
between instances and follows the existing pop-out ownership contract.

The wide sidebar starts at 280 pixels and can be resized with its divider. Its
maximum width leaves room for the editor. Narrow panels keep the bounded browser
above the editor, with controls inside the Files region above the launcher action
bar. Rows stay 28 pixels high, names truncate instead of wrapping, and every
visible branch scrolls within the browser. The selected file is revealed when
opened. Up/Down/Home/End select rows, Right expands or enters a branch, Left
collapses or moves to the parent, and Enter opens the selected file.

Folder/config/code/JSON/image/archive/text icons use original Lucide SVGs under
the existing license and are bundled through LauncherAssets. Dark mode uses
restrained file-type colors; light mode uses contrasting theme foreground colors.
Tree and TreeItem semantics expose selected and expanded state, with full names
and read-only/persistent status available to accessibility APIs.

The new regression expands nested folders, keeps an unsaved document intact
through folder and keyboard navigation, refreshes externally added files and
switches instances without moving the draft. Existing Easy/Advanced save,
external conflict, pop-out, last-file scrolling and visible-hit-bound tests are
retained. The compact-layout test also asserts that Save remains inside the
editor frame. Native evidence and commands are recorded in MIGRATION_TRACKER.md
and target/live-acceptance/explorer-20261013/.
