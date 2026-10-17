//! GitHub release provider. Asset identity, compatibility and checksums are resolved
//! before installation; third-party downloads never go through the pack updater.
use crate::{
    content::{self, ContentKind, SourceRecord},
    paths::LauncherPaths,
};
use anyhow::{ensure, Context};
use serde::Deserialize;
use std::time::Duration;
pub const PROJECT_URL: &str = "https://github.com/ModernityGTNH/Modernity-GTNH";
const RELEASES_API: &str =
    "https://api.github.com/repos/ModernityGTNH/Modernity-GTNH/releases?per_page=30";

#[derive(Clone, Debug)]
pub struct ReleaseAsset {
    pub name: String,
    pub kind: ContentKind,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub release: String,
    pub series: String,
}
#[derive(Clone, Debug)]
pub struct ReleaseCatalog {
    pub tag: String,
    pub page: String,
    pub assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

pub fn pack_series(version: &str) -> anyhow::Result<String> {
    let pieces: Vec<_> = version.split('.').collect();
    ensure!(pieces.len() >= 2 && pieces[0] == "2" && ["8", "9"].contains(&pieces[1]), "Modernity weekly builds are available for GTNH 2.8 and 2.9. Set the instance pack version before browsing.");
    Ok(format!("2.{}", pieces[1]))
}
fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("IndustrialisLauncher/third-party-content")
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or_default();
            if attempt.previous().len() >= 5
                || attempt.url().scheme() != "https"
                || ![
                    "github.com",
                    "api.github.com",
                    "release-assets.githubusercontent.com",
                    "objects.githubusercontent.com",
                ]
                .contains(&host)
            {
                attempt.error("Unexpected third-party download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}
async fn bounded(mut response: reqwest::Response, limit: u64) -> anyhow::Result<Vec<u8>> {
    ensure!(
        response.status().is_success(),
        "GitHub returned HTTP {}. Try again later or import a downloaded file.",
        response.status()
    );
    ensure!(
        response.content_length().is_none_or(|s| s <= limit),
        "Download exceeds its size limit"
    );
    let mut raw = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            raw.len() as u64 + chunk.len() as u64 <= limit,
            "Download exceeds its size limit"
        );
        raw.extend_from_slice(&chunk);
    }
    Ok(raw)
}
pub async fn catalog(pack_version: &str) -> anyhow::Result<ReleaseCatalog> {
    let series = pack_series(pack_version)?;
    let raw = bounded(client()?.get(RELEASES_API).send().await?, 4 * 1024 * 1024).await?;
    parse_catalog(&raw, &series)
}
fn parse_catalog(raw: &[u8], series: &str) -> anyhow::Result<ReleaseCatalog> {
    let releases: Vec<Release> =
        serde_json::from_slice(raw).context("Invalid GitHub release catalog")?;
    let prefix = format!("weekly-{series}.x-");
    let release = releases
        .into_iter()
        .filter(|r| !r.draft && r.tag_name.starts_with(&prefix) && r.published_at.is_some())
        .max_by(|a, b| a.published_at.cmp(&b.published_at))
        .context("No Modernity release matches this GTNH version")?;
    ensure!(
        release
            .tag_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
        "Invalid release tag"
    );
    let mut assets = Vec::new();
    for asset in release.assets {
        let kind = if asset.name.starts_with("Modernity-") && asset.name.ends_with(".zip") {
            ContentKind::ResourcePack
        } else if asset.name.starts_with("MyCTMLib-") && asset.name.ends_with(".jar") {
            ContentKind::Mod
        } else {
            continue;
        };
        let digest = asset
            .digest
            .and_then(|s| s.strip_prefix("sha256:").map(str::to_owned));
        let Some(sha256) =
            digest.filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            continue;
        };
        let item = ReleaseAsset {
            name: asset.name,
            kind,
            url: asset.browser_download_url,
            sha256,
            size: asset.size,
            release: release.tag_name.clone(),
            series: series.to_owned(),
        };
        validate_asset(&item)?;
        assets.push(item);
    }
    ensure!(
        !assets.is_empty(),
        "No checksum-verified Modernity assets are available"
    );
    Ok(ReleaseCatalog {
        page: format!("{PROJECT_URL}/releases/tag/{}", release.tag_name),
        tag: release.tag_name,
        assets,
    })
}
fn validate_asset(asset: &ReleaseAsset) -> anyhow::Result<()> {
    ensure!(
        asset
            .release
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || [b'-', b'.'].contains(&b)),
        "Invalid release tag"
    );
    let recognized = match asset.kind {
        ContentKind::ResourcePack => {
            asset.name.starts_with("Modernity-") && asset.name.ends_with(".zip")
        }
        ContentKind::Mod => asset.name.starts_with("MyCTMLib-") && asset.name.ends_with(".jar"),
    };
    ensure!(recognized, "Release asset does not match its content type");
    ensure!(
        ["2.8", "2.9"].contains(&asset.series.as_str())
            && asset
                .release
                .starts_with(&format!("weekly-{}.x-", asset.series)),
        "Release does not match the instance pack series"
    );
    ensure!(
        asset
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || [b'-', b'_', b'.'].contains(&b)),
        "Invalid release asset filename"
    );
    ensure!(
        asset.url
            == format!(
                "{PROJECT_URL}/releases/download/{}/{}",
                asset.release, asset.name
            ),
        "Unexpected release asset URL"
    );
    ensure!(
        asset.size > 0
            && asset.size <= crate::content_archive::MAX_ARCHIVE
            && asset.sha256.len() == 64
            && asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid release checksum or size"
    );
    Ok(())
}
pub async fn install(paths: &LauncherPaths, id: &str, asset: ReleaseAsset) -> anyhow::Result<()> {
    validate_asset(&asset)?;
    let raw = bounded(client()?.get(&asset.url).send().await?, asset.size).await?;
    verify_download(&asset, &raw)?;
    content::install_bytes(
        paths,
        id,
        asset.kind,
        asset.name,
        raw,
        Some(SourceRecord {
            project: PROJECT_URL.into(),
            release: asset.release,
            url: asset.url,
            archive_sha256: asset.sha256,
            pack_series: Some(asset.series),
        }),
    )
    .await
}

fn verify_download(asset: &ReleaseAsset, raw: &[u8]) -> anyhow::Result<()> {
    ensure!(
        raw.len() as u64 == asset.size && content::digest(raw).eq_ignore_ascii_case(&asset.sha256),
        "Download checksum mismatch; nothing was installed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_changed_asset_bytes_and_missing_checksums() {
        let raw = b"fixture";
        let asset = ReleaseAsset {
            name: "Modernity-2.9.x-abc.zip".into(),
            kind: ContentKind::ResourcePack,
            url: format!(
                "{PROJECT_URL}/releases/download/weekly-2.9.x-2026-10-12/Modernity-2.9.x-abc.zip"
            ),
            sha256: content::digest(raw),
            size: raw.len() as u64,
            release: "weekly-2.9.x-2026-10-12".into(),
            series: "2.9".into(),
        };
        assert!(verify_download(&asset, raw).is_ok());
        assert!(verify_download(&asset, b"changed").is_err());
        assert!(verify_download(&asset, b"truncated").is_err());
        let r = serde_json::json!([{"tag_name":asset.release,"draft":false,"published_at":"2026-10-12","assets":[{"name":asset.name,"browser_download_url":asset.url,"size":asset.size,"digest":null}]}]);
        assert!(parse_catalog(&serde_json::to_vec(&r).unwrap(), "2.9").is_err());
    }
    #[test]
    fn selects_matching_weekly_build_and_includes_optional_mods() {
        let mut r = serde_json::json!([{ "tag_name":"weekly-2.8.x-2026-10-12", "draft":false, "published_at":"2026-10-12", "assets":[] }, { "tag_name":"weekly-2.9.x-2026-10-05", "draft":false, "published_at":"2026-10-05", "assets":[] }]);
        for name in [
            "Modernity-2.9.x-abc.zip",
            "Modernity-GTNHAddon-OutlinedOres.zip",
            "MyCTMLib-1.4.2.jar",
            "unknown.exe",
        ] {
            r[1]["assets"].as_array_mut().unwrap().push(serde_json::json!({"name":name,"browser_download_url":format!("{PROJECT_URL}/releases/download/weekly-2.9.x-2026-10-05/{name}"),"size":100,"digest":format!("sha256:{}", "a".repeat(64))}));
        }
        let result = parse_catalog(&serde_json::to_vec(&r).unwrap(), "2.9").unwrap();
        assert_eq!(result.tag, "weekly-2.9.x-2026-10-05");
        assert_eq!(result.assets.len(), 3);
        assert_eq!(result.assets[2].kind, ContentKind::Mod);
        r[1]["assets"][0]["browser_download_url"] = "https://example.com/untrusted".into();
        assert!(parse_catalog(&serde_json::to_vec(&r).unwrap(), "2.9").is_err());
        assert_eq!(pack_series("2.9.0-RC-2").unwrap(), "2.9");
        assert!(pack_series("2.7.4").is_err());
    }
}
