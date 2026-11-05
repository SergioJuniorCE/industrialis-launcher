# Third-party mods and resource packs

The launcher instance **Add-ons** tab contains **Mods**, **Browse mods**, and
**Resource packs**. Existing custom mod imports remain available. Resource packs
can be imported from ZIP files or installed individually from Modernity's GitHub
release assets. Mods can be found on Modrinth and installed as a reviewed set.

## Modrinth

**Browse mods** follows the instance's own `mmc-pack.json`: the Minecraft version
and mod loader are read from its components, not hard-coded to GTNH. Supported
loaders are Forge, NeoForge, Fabric, Quilt, and LiteLoader. A pack that declares
several supported loaders is browsed with all of them (for example Forge +
LiteLoader). The search catalog is public and needs no API key. Choose a
project, choose one of its compatible versions, and add it to the install list.
The list accepts one entry per project; choosing a different version replaces the
previous entry. A vanilla instance or an instance with no recognized loader
shows an error instead of browsing.

The browser uses a two-pane layout: search results with icon tiles and selection
boxes on the left, and the selected project's description, external links,
versions and install review on the right. A persistent action bar shows the
selected count and the review/install actions.

**Review install** resolves every selected mod's required dependencies
recursively. Dependencies already installed, already selected, or already queued
are skipped. Optional and embedded dependencies are not installed. A required
dependency with no file for the instance's Minecraft version and loader stops
the review with an error instead of producing a broken set. An `incompatible`
dependency that targets a selected, planned or already installed project is
shown as a conflict and blocks install until the conflicting selection is
removed. Dependencies that were added automatically show which mods require
them; the review also lists already-installed projects that were skipped.

The plan is bounded to 64 files and 512 MB, and it rejects duplicate mod
identities before any download. This keeps a hostile or broken dependency graph
from turning into an unbounded download.

Only after the review is confirmed does **Install** begin. Every file is
downloaded and verified first (exact size plus Modrinth's SHA-512, falling back
to SHA-1). Only then are all files and receipts written in one content
transaction. Any validation, download, or write failure removes every file from
that batch and restores previous receipts, so no partial install survives.
Installation requires a stopped instance and holds the same instance reservation
as pack updates and other add-on changes.

Receipts record the provider (`modrinth`), project id, version id, page URL,
version number, exact download URL and the installed archive SHA-256. Pack-owned
mods remain protected: a selected file whose mod identity matches an existing
file is rejected or skipped rather than overwritten. Replacing an installed
version and update notifications are not implemented yet; remove the old file in
the **Mods** tab first. CurseForge is not included: it requires an API key and
its author-controlled third-party download rules need a separate policy.

## Modernity

The release browser fetches the newest published weekly release matching the
instance's GTNH **2.8** or **2.9** series. It displays the main pack, optional UI
styles, texture add-ons and each MyCTMLib JAR offered by that release. It does not
install optional extras automatically. Both MyCTMLib versions may be offered;
only one mod identity can be installed. Pack-owned versions remain protected.
Choose one UI style and one bee texture style. Extras for mods such as GTNL only
have an effect when that mod is installed; the launcher does not add their other
mod dependencies.

The repository README's download notice is older than the current release feed.
Use [the release page](https://github.com/ModernityGTNH/Modernity-GTNH/releases)
for current weekly assets, requirements and build-specific notes. The native
browser links the exact selected release. It downloads directly from upstream,
requires GitHub's SHA-256 digest and exact asset size, and verifies both before
any profile mutation. A weekly build is not a compatibility guarantee for every
individual GTNH mod version.

After installation, enable resource packs in Minecraft **Options > Resource
Packs**. Keep the main Modernity pack at the bottom of the selected packs, and
its UI/textures above it, as upstream directs. Existing selection/order in
options.txt is preserved. Removing a selected pack requires disabling it in-game
first. Resource packs and mod JARs have separate destinations and removal paths.
No third-party content is silently updated by the hourly GTNH pack-status poll.

## Storage and safety

- Original profile and instance directories are used in place; no data migration.
- Resource packs: `<instance>/persistent-minecraft/resourcepacks/<filename>.zip`,
  mirrored to `.minecraft/resourcepacks`.
- Mods: original `persistent-minecraft/mods`, mirrored to `.minecraft/mods`.
- Source receipts: `persistent-minecraft/.industrialis-content/<kind>/<filename>.json`.
  Receipts contain the project, release, exact upstream URL and original archive
  SHA-256. Local imports record the filename, not the user's full external path.
- Existing active packs/folders are listed. Only persistent ZIPs get a Remove
  control; active-only pack-owned/user files are not silently adopted or deleted.
- Installation requires a stopped instance and its shared operation reservation.
  Release compatibility is checked again after download, under that reservation.
- No overwrites: filename collisions block pack imports; mod identity collisions
  block duplicate versions, including mods already shipped by GTNH.
- ZIP/JAR validation checks names, case-insensitive duplicates, links, CRCs,
  file count and expanded-size limits. Resource packs require pack_format 1.
  A single repository wrapper is stripped so pack.mcmeta is at the ZIP root.
  Textures, licenses and credits are retained. No archive paths are extracted.
- Archive download/import limit: 512 MiB. Expanded limit: 1 GiB, 64 MiB per file,
  100,000 entries. Network requests and redirects are bounded.
- Writes run outside Render in the backend. Atomic file replacements plus failure
  rollback prevent partial imports. Removal stages every managed path before
  disposal; externally changed active resource packs are kept and reported.
- Original update/reinstall and launch overlay restoration preserve resource
  packs and receipts. Downloaded MyCTMLib is removable in the existing Mods UI.

## Provider foundation

`content.rs` owns the shared typed `ContentKind`, `SourceRecord`, inventory,
validated byte installation, safe local resource-pack imports, batch
installation and removal. `content_archive.rs` owns archive validation/
normalization. `modernity.rs` resolves upstream releases into typed
`ReleaseAsset` values and verifies downloads before calling the content service.
`modrinth.rs` owns Modrinth search, version selection, required-dependency
resolution, checksum verification and the reviewed install plan. UI state and
asynchronous dispatch live in `views/content.rs` and `views/mod_browser.rs`;
existing local mod behavior stays in `custom_mods.rs`.

To add a provider, implement a separate catalog/resolver using the shared kind,
provenance and installation contract. A provider must define its supported game/
loader versions, exact artifact identity, source allowlist and trusted checksum
before exposing an Install action. Keep library/API credentials out of UI state.
Do not treat a ZIP extension alone as evidence that an artifact is a resource
pack. New source catalogs should not change the GTNH updater or account store.

Follow-up work for a general marketplace (separate from UI parity):

- [x] Add another provider after choosing its catalog and API/license policy:
      Modrinth is implemented and needs no credentials.
- [x] Add explicit dependency metadata and a reviewable multi-item plan.
- [ ] Add CurseForge behind a build-time API key and an author-download policy.
- [ ] Add opt-in third-party update checks and replacement/rollback per project.
- [ ] Add in-launcher resource-pack activation and load-order management if wanted;
      current installation leaves Minecraft's existing selection untouched.

## Verification

Automated Launcher tests exercise ZIP picker cancellation, valid imports, invalid
archives, real paired files, confirmed removal and maintenance guards. Backend
checks cover wrapper normalization, invalid archives, identity collisions,
source receipts, restoration, safe removal, compatibility changes, Modrinth
catalog/URL/checksum validation and all-or-nothing batch rollback. The Modrinth
browser test adds and removes install-list entries without network access. The
existing staged GTNH update test now preserves a real imported resource pack
and its receipt through the actual maintenance pipeline.

The opt-in headless online acceptance uses a newly generated temporary profile:

```powershell
cargo run -p industrialis-backend --example third_party_content_smoke --locked --jobs 1
```

It downloads current upstream main/Outlined Ores/MyCTMLib assets, verifies their
checksums, installs, restores overlays, removes content and checks unrelated
instance metadata. It never uses the default/user profile. This tests installation,
not Minecraft rendering or connected textures on the user's game.

The native acceptance runner uses an explicitly owned Docker/Xvfb desktop and
the compiled `profile_ownership_smoke` app. It clicks Add-ons, Resource packs,
the release browser and the individual main/Outlined Ores/MyCTMLib Install
controls. It verifies active/persistent bytes, source receipts and unchanged
metadata, visits the existing Mods manager and closes the app normally:

```sh
LAUNCHER_ISOLATED_CONTENT_CONTAINER=1 sh scripts/test-linux-content.sh \
  /target/debug/examples/profile_ownership_smoke /output
```

The runner rejects an unowned/non-container desktop before creating a window.
Receipts, binary/source hashes, test logs and inspected screenshots are under
`target/live-acceptance/content-20261012/`. The migration tracker records the
latest results. No physical desktop or original user profile is used.
