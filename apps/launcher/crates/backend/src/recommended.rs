//! Curated community add-ons the pack does not ship. Each entry pins one stable
//! GitHub release per supported pack series, verifies the published digest, and
//! installs the mod together with the companion libraries it needs in one batch.
//! Libraries the pack already ships are never added.
use crate::{
    content::{self, ContentKind, PendingContent, SourceRecord},
    modernity,
    paths::LauncherPaths,
};
use anyhow::{ensure, Context};
use serde::Deserialize;
use std::collections::HashSet;

pub struct AddOn {
    pub name: &'static str,
    pub description: &'static str,
    pub repo: &'static str,
    pub prefix: &'static str,
    /// Stable release tag pinned per GTNH series. Upstream tags do not always
    /// encode the series, and some projects publish rolling or daily releases
    /// with no flag that marks them experimental, so the mapping is explicit.
    pub releases: &'static [(&'static str, &'static str)],
    pub companions: &'static [Companion],
}
pub struct Companion {
    pub repo: &'static str,
    pub prefix: &'static str,
    pub tag: &'static str,
}
pub const CATALOG: &[AddOn] = &[
    AddOn {
        name: "GTNotLeisure",
        description: "Adds machines and multiblocks to GTNH 2.9.",
        repo: "GTNotLeisure/GT-Not-Leisure",
        prefix: "sciencenotleisure",
        // 2.8.4 has an EOL build upstream, but its GTNLLib version is not
        // verified; only the 2.9 pairing this pack ships is offered.
        releases: &[("2.9", "0.2.7")],
        companions: &[
            Companion {
                repo: "GTNotLeisure/GTNLLib",
                prefix: "gtnllib",
                tag: "1.1.8",
            },
            Companion {
                repo: "GTNotLeisure/BetterQuestingAPI",
                prefix: "bqapi",
                tag: "1.1.4",
            },
        ],
    },
    AddOn {
        name: "GTNHRates",
        description: "Shows machine and recipe rates in tooltips.",
        repo: "Sladki/GTNHRates",
        prefix: "gtnhrates",
        releases: &[("2.9", "1.12.0-2.9"), ("2.8", "1.11.0-2.8.4")],
        companions: &[],
    },
    AddOn {
        name: "Bandit",
        description: "Vein-mines a whole ore vein with one key.",
        repo: "ElytraServers/Bandit-Legacy",
        prefix: "bandit",
        // No 2.9 release exists yet. Keep the repo line alone swappable for the
        // owner's fork rather than assuming upstream is the only source.
        releases: &[("2.8", "1.1.15")],
        companions: &[],
    },
];

#[derive(Clone)]
pub struct Asset {
    pub repo: &'static str,
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub release: String,
}
#[derive(Clone)]
pub struct Plan {
    pub addon: &'static AddOn,
    pub series: String,
    pub page: String,
    pub assets: Vec<Asset>,
}
impl Plan {
    pub fn mod_asset(&self) -> &Asset {
        &self.assets[0]
    }
    pub fn companions(&self) -> &[Asset] {
        &self.assets[1..]
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

/// Resolve every catalog entry against the instance's pack series. One failing
/// entry never hides the others; results stay aligned with [`CATALOG`].
pub async fn plans(pack_version: &str) -> Vec<Result<Plan, String>> {
    let series = match modernity::pack_series(pack_version) {
        Ok(series) => series,
        Err(_) => {
            return CATALOG
                .iter()
                .map(|_| Err("Recommended add-ons target GTNH 2.8 and 2.9 instances.".into()))
                .collect()
        }
    };
    let client = match modernity::client() {
        Ok(client) => client,
        Err(error) => return CATALOG.iter().map(|_| Err(format!("{error:#}"))).collect(),
    };
    let mut plans = Vec::with_capacity(CATALOG.len());
    for addon in CATALOG {
        plans.push(
            plan(&client, addon, &series)
                .await
                .map_err(|error| format!("{error:#}")),
        );
    }
    plans
}

fn release_tag(addon: &AddOn, series: &str) -> Option<&'static str> {
    addon
        .releases
        .iter()
        .find(|(supported, _)| *supported == series)
        .map(|(_, tag)| *tag)
}

async fn plan(
    client: &reqwest::Client,
    addon: &'static AddOn,
    series: &str,
) -> anyhow::Result<Plan> {
    let tag = release_tag(addon, series).with_context(|| {
        format!(
            "{} is not offered for GTNH {series}. Available for {}.",
            addon.name,
            addon
                .releases
                .iter()
                .map(|(series, _)| *series)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let release = tagged_release(client, addon.repo, tag).await?;
    let mut assets = vec![asset(addon.repo, addon.prefix, tag, &release)?];
    for companion in addon.companions {
        let release = tagged_release(client, companion.repo, companion.tag).await?;
        assets.push(asset(
            companion.repo,
            companion.prefix,
            companion.tag,
            &release,
        )?);
    }
    Ok(Plan {
        addon,
        series: series.to_owned(),
        page: format!("https://github.com/{}/releases/tag/{tag}", addon.repo),
        assets,
    })
}

async fn tagged_release(
    client: &reqwest::Client,
    repo: &str,
    tag: &str,
) -> anyhow::Result<Release> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let raw = modernity::bounded(client.get(url).send().await?, 1024 * 1024).await?;
    serde_json::from_slice(&raw).context("Invalid GitHub release")
}

fn asset(
    repo: &'static str,
    prefix: &str,
    tag: &str,
    release: &Release,
) -> anyhow::Result<Asset> {
    ensure!(
        release.tag_name == tag,
        "Release tag changed while resolving {repo}."
    );
    let name = format!("{prefix}-{tag}.jar");
    let found = release
        .assets
        .iter()
        .find(|candidate| candidate.name == name)
        .with_context(|| format!("{repo} {tag} does not publish {name}."))?;
    let asset = Asset {
        repo,
        name,
        url: found.browser_download_url.clone(),
        sha256: found
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .unwrap_or_default()
            .to_owned(),
        size: found.size,
        release: tag.to_owned(),
    };
    validate(&asset)?;
    Ok(asset)
}

fn validate(asset: &Asset) -> anyhow::Result<()> {
    ensure!(
        asset
            .release
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || [b'-', b'.'].contains(&b)),
        "Invalid release tag"
    );
    ensure!(
        asset.name.ends_with(".jar")
            && asset
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || [b'-', b'_', b'.'].contains(&b)),
        "Invalid add-on filename"
    );
    ensure!(
        asset.url
            == format!(
                "https://github.com/{}/releases/download/{}/{}",
                asset.repo, asset.release, asset.name
            ),
        "Unexpected add-on download URL"
    );
    ensure!(
        asset.size > 0
            && asset.size <= crate::content_archive::MAX_ARCHIVE
            && asset.sha256.len() == 64
            && asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid add-on checksum or size"
    );
    Ok(())
}

/// Whether `filename` belongs to the same mod as `candidate` even when the
/// version differs. The pack's identity parser drops trailing version segments,
/// but a build suffix such as `gtnhrates-1.11.0-2.9-daily523.jar` stops it
/// early, so the name before the first separator is compared as well.
pub fn same_project(filename: &str, candidate: &str) -> bool {
    let identity = crate::pack_files::mod_identity_from_filename;
    identity(filename) == identity(candidate)
        || project_prefix(filename).eq_ignore_ascii_case(project_prefix(candidate))
}

fn project_prefix(filename: &str) -> &str {
    filename.split(['-', '_']).next().unwrap_or(filename)
}

/// Install the plan's mod and companions that are not already present. Assets
/// already in either mods folder are skipped, so a retry only fills the gaps; a
/// different version of the add-on itself is refused rather than added beside it.
pub async fn install(
    paths: &LauncherPaths,
    id: &str,
    plan: &Plan,
) -> anyhow::Result<usize> {
    let installed: HashSet<String> = content::list(paths, id, ContentKind::Mod)
        .await?
        .into_iter()
        .map(|entry| entry.filename.to_lowercase())
        .collect();
    let client = modernity::client()?;
    let mut pending = Vec::new();
    for (index, asset) in plan.assets.iter().enumerate() {
        if installed.contains(&asset.name.to_lowercase()) {
            continue;
        }
        if let Some(existing) = installed.iter().find(|name| same_project(name, &asset.name)) {
            // The first asset is the add-on itself; companions are libraries the
            // instance may already carry at another version.
            ensure!(
                index != 0,
                "A different version of {} is already installed ({existing}). Remove it first.",
                plan.addon.name
            );
            continue;
        }
        validate(asset)?;
        let raw = modernity::bounded(client.get(&asset.url).send().await?, asset.size).await?;
        ensure!(
            raw.len() as u64 == asset.size
                && content::digest(&raw).eq_ignore_ascii_case(&asset.sha256),
            "Download checksum mismatch; nothing was installed"
        );
        pending.push(PendingContent {
            name: asset.name.clone(),
            raw,
            source: SourceRecord {
                project: format!("https://github.com/{}", asset.repo),
                release: asset.release.clone(),
                url: asset.url.clone(),
                archive_sha256: asset.sha256.clone(),
                pack_series: Some(plan.series.clone()),
                provider: Some("recommended".into()),
                project_id: None,
                version_id: None,
            },
        });
    }
    if pending.is_empty() {
        return Ok(0);
    }
    content::install_batch(paths, id, ContentKind::Mod, pending).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(raw: serde_json::Value) -> Release {
        serde_json::from_value(raw).unwrap()
    }

    #[test]
    fn catalog_pins_one_stable_release_per_series() {
        assert_eq!(release_tag(&CATALOG[0], "2.9"), Some("0.2.7"));
        assert_eq!(release_tag(&CATALOG[0], "2.8"), None);
        assert_eq!(release_tag(&CATALOG[1], "2.9"), Some("1.12.0-2.9"));
        assert_eq!(release_tag(&CATALOG[1], "2.8"), Some("1.11.0-2.8.4"));
        assert_eq!(release_tag(&CATALOG[2], "2.8"), Some("1.1.15"));
        assert_eq!(release_tag(&CATALOG[2], "2.9"), None);
        for addon in CATALOG {
            for (series, tag) in addon.releases {
                assert!(["2.8", "2.9"].contains(series));
                assert!(!tag.is_empty());
                assert!(
                    !tag.contains("daily") && !tag.contains("dev"),
                    "{} pins a rolling build: {tag}",
                    addon.name
                );
            }
        }
    }

    #[test]
    fn assets_require_the_exact_jar_and_a_published_checksum() {
        let digest = "a".repeat(64);
        let good = release(serde_json::json!({
            "tag_name": "0.2.7",
            "assets": [{
                "name": "sciencenotleisure-0.2.7.jar",
                "browser_download_url": "https://github.com/GTNotLeisure/GT-Not-Leisure/releases/download/0.2.7/sciencenotleisure-0.2.7.jar",
                "size": 21,
                "digest": format!("sha256:{digest}")
            }]
        }));
        let resolved = asset(
            "GTNotLeisure/GT-Not-Leisure",
            "sciencenotleisure",
            "0.2.7",
            &good,
        )
        .unwrap();
        assert_eq!(resolved.name, "sciencenotleisure-0.2.7.jar");
        let mut changed = good;
        changed.assets[0].browser_download_url = "https://example.com/x.jar".into();
        assert!(asset(
            "GTNotLeisure/GT-Not-Leisure",
            "sciencenotleisure",
            "0.2.7",
            &changed
        )
        .is_err());
        let mut unchecked = release(serde_json::json!({
            "tag_name": "0.2.7",
            "assets": [{
                "name": "sciencenotleisure-0.2.7.jar",
                "browser_download_url": "https://github.com/GTNotLeisure/GT-Not-Leisure/releases/download/0.2.7/sciencenotleisure-0.2.7.jar",
                "size": 21
            }]
        }));
        assert!(asset(
            "GTNotLeisure/GT-Not-Leisure",
            "sciencenotleisure",
            "0.2.7",
            &unchecked
        )
        .is_err());
        unchecked.assets.clear();
        assert!(asset(
            "GTNotLeisure/GT-Not-Leisure",
            "sciencenotleisure",
            "0.2.7",
            &unchecked
        )
        .is_err());
    }

    #[test]
    fn same_project_catches_builds_the_identity_parser_keeps_apart() {
        assert!(same_project(
            "gtnhrates-1.11.0-2.9-daily523.jar",
            "gtnhrates-1.12.0-2.9.jar"
        ));
        assert!(same_project(
            "sciencenotleisure-0.2.7.jar",
            "sciencenotleisure-0.2.7.jar"
        ));
        assert!(same_project("bandit-1.1.15.jar", "bandit-1.1.14-pre.jar"));
        assert!(!same_project("gtnhlib-0.11.58.jar", "gtnllib-1.1.8.jar"));
        assert!(!same_project(
            "BetterQuesting-3.8.94-GTNH.jar",
            "bqapi-1.1.4.jar"
        ));
    }

    #[test]
    fn catalog_entries_name_a_release_and_their_series() {
        for addon in CATALOG {
            assert!(!addon.name.is_empty() && !addon.description.is_empty());
            assert!(!addon.repo.is_empty() && !addon.prefix.is_empty());
            assert!(!addon.releases.is_empty());
            for companion in addon.companions {
                assert!(!companion.repo.is_empty() && !companion.prefix.is_empty());
                assert!(!companion.tag.is_empty());
            }
        }
    }
}
