# Third-party mods and resource packs

The GPUI instance **Add-ons** tab contains **Mods** and **Resource packs**.
Existing custom mod imports remain available. Resource packs can be imported from
ZIP files or installed individually from Modernity's GitHub release assets.

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
validated byte installation, safe local resource-pack imports and removal.
`content_archive.rs` owns archive validation/normalization. `modernity.rs` resolves
upstream releases into typed `ReleaseAsset` values and verifies downloads before
calling the content service. UI state and asynchronous dispatch live in
`views/content.rs`; existing local mod behavior stays in `custom_mods.rs`.

To add a provider, implement a separate catalog/resolver using the shared kind,
provenance and installation contract. A provider must define its supported game/
loader versions, exact artifact identity, source allowlist and trusted checksum
before exposing an Install action. Keep library/API credentials out of UI state.
Do not treat a ZIP extension alone as evidence that an artifact is a resource
pack. New source catalogs should not change the GTNH updater or account store.

Follow-up work for a general marketplace (separate from Electron parity):

- [ ] Add another provider only after choosing its catalog and API/license policy
      (for example Modrinth or CurseForge); no provider credentials are required here.
- [ ] Add explicit dependency/conflict metadata and a reviewable multi-item plan.
- [ ] Add opt-in third-party update checks and replacement/rollback per project.
- [ ] Add in-launcher resource-pack activation and load-order management if wanted;
      current installation leaves Minecraft's existing selection untouched.

## Verification

Automated GPUI tests exercise ZIP picker cancellation, valid imports, invalid
archives, real paired files, confirmed removal and maintenance guards. Backend
checks cover wrapper normalization, invalid archives, identity collisions,
source receipts, restoration, safe removal and compatibility changes. The
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
GPUI_ISOLATED_CONTENT_CONTAINER=1 sh scripts/test-gpui-linux-content.sh \
  /target/debug/examples/profile_ownership_smoke /output
```

The runner rejects an unowned/non-container desktop before creating a window.
Receipts, binary/source hashes, test logs and inspected screenshots are under
`target/live-acceptance/content-20261012/`. The migration tracker records the
latest results. No physical desktop or original user profile is used.
