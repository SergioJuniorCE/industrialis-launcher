//! Public Open VSX catalog and verified data-only color-theme imports.
use anyhow::{ensure, Context};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
pub const REGISTRY: &str = "https://open-vsx.org";
const MAX_PACKAGE: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub namespace: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}
impl Extension {
    pub fn id(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
    pub fn page(&self) -> String {
        format!("{REGISTRY}/extension/{}/{}", self.namespace, self.name)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        for part in [&self.namespace, &self.name, &self.version] {
            ensure!(!part.is_empty() && part.len() <= 128 && part.bytes().all(|b| b.is_ascii_alphanumeric() || [b'-',b'_',b'.',b'+'].contains(&b)), "Invalid Open VSX extension identity");
            ensure!(
                part != "." && part != "..",
                "Invalid Open VSX extension identity"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub extensions: Vec<Extension>,
    pub total_size: usize,
}
#[derive(Deserialize)]
struct Metadata {
    #[serde(flatten)]
    extension: Extension,
    files: HashMap<String, String>,
    #[serde(default)]
    license: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ThemeBundle {
    pub extension: Extension,
    pub themes: Vec<Value>,
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("IndustrialisLauncher/OpenVSX")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || attempt.url().scheme() != "https"
                || !["open-vsx.org", "openvsx.eclipsecontent.org"]
                    .contains(&attempt.url().host_str().unwrap_or_default())
            {
                attempt.error("Unexpected Open VSX redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}
async fn bounded(mut response: reqwest::Response, limit: u64) -> anyhow::Result<Vec<u8>> {
    ensure!(
        response.status().is_success(),
        "Open VSX returned HTTP {}. Try again later.",
        response.status()
    );
    ensure!(
        response.content_length().is_none_or(|size| size <= limit),
        "Open VSX response exceeds its size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() as u64 + chunk.len() as u64 <= limit,
            "Open VSX response exceeds its size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub async fn search(query: &str, offset: usize) -> anyhow::Result<Catalog> {
    ensure!(
        query.len() <= 256 && offset <= 2000,
        "Theme search is too long or exceeds the page limit"
    );
    let raw = bounded(
        client()?
            .get(format!("{REGISTRY}/api/-/search"))
            .query(&[
                ("query", query),
                ("category", "Themes"),
                ("size", "20"),
                ("offset", &offset.to_string()),
                (
                    "sortBy",
                    if query.trim().is_empty() {
                        "downloadCount"
                    } else {
                        "relevance"
                    },
                ),
            ])
            .send()
            .await?,
        2 * 1024 * 1024,
    )
    .await?;
    let catalog: Catalog = serde_json::from_slice(&raw).context("Invalid Open VSX catalog")?;
    ensure!(catalog.extensions.len() <= 20, "Invalid Open VSX page size");
    for extension in &catalog.extensions {
        extension.validate()?;
    }
    Ok(catalog)
}
fn file_url(extension: &Extension, raw: &str) -> anyhow::Result<String> {
    let url = reqwest::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("open-vsx.org")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "Unexpected Open VSX artifact source"
    );
    ensure!(
        url.path().starts_with(&format!(
            "/api/{}/{}/{}/file/",
            extension.namespace, extension.name, extension.version
        )),
        "Artifact does not belong to the selected extension version"
    );
    Ok(url.to_string())
}
pub async fn download(extension: Extension) -> anyhow::Result<ThemeBundle> {
    extension.validate()?;
    let client = client()?;
    let raw = bounded(
        client
            .get(format!(
                "{REGISTRY}/api/{}/{}/{}",
                extension.namespace, extension.name, extension.version
            ))
            .send()
            .await?,
        2 * 1024 * 1024,
    )
    .await?;
    let metadata: Metadata =
        serde_json::from_slice(&raw).context("Invalid Open VSX extension metadata")?;
    ensure!(
        metadata.extension.id() == extension.id()
            && metadata.extension.version == extension.version,
        "Open VSX returned a different extension version"
    );
    let download = file_url(
        &extension,
        metadata
            .files
            .get("download")
            .context("No VSIX download is available")?,
    )?;
    let checksum = file_url(
        &extension,
        metadata
            .files
            .get("sha256")
            .context("No SHA-256 is available for this extension")?,
    )?;
    let digest = bounded(client.get(checksum).send().await?, 256).await?;
    let digest = std::str::from_utf8(&digest)?.trim().to_ascii_lowercase();
    ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid extension checksum"
    );
    let bytes = bounded(client.get(download).send().await?, MAX_PACKAGE).await?;
    use sha2::Digest;
    ensure!(
        format!("{:x}", sha2::Sha256::digest(&bytes)) == digest,
        "Extension checksum mismatch; no theme was installed"
    );
    let source = serde_json::json!({"registry":"Open VSX","namespace":extension.namespace,"extension":extension.name,"version":extension.version,"page":extension.page(),"sha256":digest,"license":metadata.license});
    let themes =
        tokio::task::spawn_blocking(move || crate::theme_archive::import(&bytes, source)).await??;
    Ok(ThemeBundle { extension, themes })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_registry_identity_and_artifact_origin_are_required() {
        let extension = Extension {
            namespace: "publisher".into(),
            name: "theme".into(),
            version: "1.0.0".into(),
            display_name: None,
            description: None,
        };
        assert!(extension.validate().is_ok());
        assert!(file_url(&extension, "https://evil.example/theme.vsix").is_err());
        assert!(file_url(
            &extension,
            "https://open-vsx.org/api/other/theme/1.0.0/file/a.vsix"
        )
        .is_err());
        assert!(file_url(
            &extension,
            "https://open-vsx.org/api/publisher/theme/1.0.0/file/a.vsix"
        )
        .is_ok());
        let mut invalid = extension;
        invalid.namespace = "../publisher".into();
        assert!(invalid.validate().is_err());
    }
}
