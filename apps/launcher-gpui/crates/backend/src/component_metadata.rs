//! Resolve reference-only MultiMC components without replacing bundled patches.
use anyhow::{ensure, Context};
use std::path::Path;

const ROOT: &str = "https://meta.prismlauncher.org/v1";
const LIMIT: usize = 2 * 1024 * 1024;

fn segment(value: &str) -> anyhow::Result<()> {
    ensure!(
        !value.is_empty()
            && value != "."
            && value != ".."
            && value.len() <= 200
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Invalid component metadata identifier"
    );
    Ok(())
}

fn validate(raw: &[u8], uid: &str, version: &str) -> anyhow::Result<()> {
    ensure!(raw.len() <= LIMIT, "Component metadata exceeds 2 MB");
    let parsed: serde_json::Value = serde_json::from_slice(raw)?;
    ensure!(
        parsed.get("uid").and_then(|v| v.as_str()) == Some(uid)
            && parsed.get("version").and_then(|v| v.as_str()) == Some(version),
        "Component metadata identity does not match {uid} {version}"
    );
    Ok(())
}

pub(crate) async fn load(
    client: &reqwest::Client,
    instance: &Path,
    uid: &str,
    version: &str,
) -> anyhow::Result<Vec<u8>> {
    segment(uid)?;
    let bundled = instance.join("patches").join(format!("{uid}.json"));
    match tokio::fs::read(&bundled).await {
        Ok(raw) => {
            ensure!(raw.len() <= LIMIT, "Bundled component patch exceeds 2 MB");
            return Ok(raw);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Read bundled component patch"),
    }
    segment(version)?;
    ensure!(["net.minecraft", "net.minecraftforge", "org.lwjgl", "org.lwjgl3"].contains(&uid),
        "Pack component {uid} {version} has no bundled patch and is not supported by the metadata service");
    let cache = instance
        .join("component-cache")
        .join(uid)
        .join(format!("{version}.json"));
    match tokio::fs::read(&cache).await {
        Ok(raw) => {
            validate(&raw, uid, version)?;
            return Ok(raw);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Read component metadata cache"),
    }
    let url = format!("{ROOT}/{uid}/{version}.json");
    let mut response = client.get(&url).send().await?.error_for_status()?;
    let mut raw = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            raw.len() + chunk.len() <= LIMIT,
            "Component metadata exceeds 2 MB"
        );
        raw.extend_from_slice(&chunk);
    }
    validate(&raw, uid, version)?;
    tokio::task::spawn_blocking(move || crate::fs_utils::write_atomic(&cache, &raw).map(|_| raw))
        .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_identity_and_paths_are_checked_before_use() {
        for invalid in ["", ".", "..", "../net.minecraft", "a/b", "a\\b", "a?b"] {
            assert!(segment(invalid).is_err());
        }
        let valid = br#"{"uid":"net.minecraft","version":"1.7.10"}"#;
        assert!(validate(valid, "net.minecraft", "1.7.10").is_ok());
        assert!(validate(valid, "net.minecraftforge", "1.7.10").is_err());
        assert!(validate(valid, "net.minecraft", "other").is_err());
        assert!(validate(&vec![b' '; LIMIT + 1], "net.minecraft", "1.7.10").is_err());
    }

    #[tokio::test]
    async fn bundled_custom_patch_is_authoritative_and_missing_custom_components_fail_closed() {
        let root =
            std::env::temp_dir().join(format!("gpui-metadata-bundled-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(root.join("patches"))
            .await
            .unwrap();
        let raw = br#"{"mainClass":"custom.Launch","libraries":[]}"#;
        tokio::fs::write(root.join("patches/custom.runtime.json"), raw)
            .await
            .unwrap();
        let client = reqwest::Client::new();
        assert_eq!(
            load(&client, &root, "custom.runtime", "").await.unwrap(),
            raw
        );
        assert!(load(&client, &root, "missing.runtime", "1")
            .await
            .unwrap_err()
            .to_string()
            .contains("not supported"));
        assert!(!root.join("component-cache").exists());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
