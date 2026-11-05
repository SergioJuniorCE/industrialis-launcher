//! Resolve the Minecraft version and mod loader declared by an instance.
//!
//! The catalog must follow the instance rather than a hard-coded pack version,
//! so the same browser can serve GTNH, other 1.7.10 packs, or any later
//! Forge/Fabric/NeoForge/Quilt/LiteLoader pack.

use anyhow::{ensure, Context};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModLoader {
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    LiteLoader,
}
impl ModLoader {
    pub fn modrinth_name(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::LiteLoader => "liteloader",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::LiteLoader => "LiteLoader",
        }
    }
    fn from_uid(uid: &str) -> Option<Self> {
        match uid {
            "net.minecraftforge" => Some(Self::Forge),
            "net.neoforged" => Some(Self::NeoForge),
            "net.fabricmc.fabric-loader" => Some(Self::Fabric),
            "org.quiltmc.quilt-loader" => Some(Self::Quilt),
            "com.mumfrey.liteloader" => Some(Self::LiteLoader),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameTarget {
    pub game_version: String,
    pub loaders: Vec<ModLoader>,
}
impl GameTarget {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            valid_game_version(&self.game_version),
            "The instance declares an invalid Minecraft version"
        );
        ensure!(!self.loaders.is_empty(), "The instance has no mod loader");
        Ok(())
    }
    pub fn summary(&self) -> String {
        let loaders = self
            .loaders
            .iter()
            .map(|loader| loader.label())
            .collect::<Vec<_>>()
            .join(" + ");
        format!("Minecraft {} / {}", self.game_version, loaders)
    }
    pub fn modrinth_loaders(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.loaders.iter().map(|loader| loader.modrinth_name())
    }
}

pub async fn load(instance: &Path) -> anyhow::Result<GameTarget> {
    let path = instance.join("mmc-pack.json");
    let raw = tokio::fs::read(&path)
        .await
        .with_context(|| format!("Could not read {}", path.display()))?;
    parse(&raw).with_context(|| format!("Invalid pack metadata in {}", path.display()))
}

fn valid_game_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub fn parse(raw: &[u8]) -> anyhow::Result<GameTarget> {
    let pack: MmcPack = serde_json::from_slice(raw).context("Invalid pack metadata")?;
    let game_version = pack
        .components
        .iter()
        .find(|component| component.uid == "net.minecraft")
        .map(Component::effective_version)
        .filter(|version| !version.is_empty())
        .context("The instance does not declare a Minecraft version")?;
    ensure!(
        valid_game_version(game_version),
        "The instance declares an invalid Minecraft version"
    );
    let mut loaders = Vec::new();
    for component in &pack.components {
        if let Some(loader) = ModLoader::from_uid(&component.uid) {
            if !loaders.contains(&loader) {
                loaders.push(loader);
            }
        }
    }
    ensure!(
        !loaders.is_empty(),
        "This instance has no supported mod loader"
    );
    // Canonical order keeps plan/install equality stable if component order changes.
    loaders.sort_by_key(|loader| loader.modrinth_name());
    Ok(GameTarget {
        game_version: game_version.to_owned(),
        loaders,
    })
}

#[derive(Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<Component>,
}
#[derive(Deserialize)]
struct Component {
    uid: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "cachedVersion")]
    cached_version: Option<String>,
}
impl Component {
    fn effective_version(&self) -> &str {
        self.version
            .as_deref()
            .or(self.cached_version.as_deref())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(components: &str) -> Vec<u8> {
        format!(r#"{{"components":[{components}]}}"#).into_bytes()
    }

    #[test]
    fn reads_the_minecraft_version_and_each_supported_loader() {
        for (uid, loader) in [
            ("net.minecraftforge", ModLoader::Forge),
            ("net.neoforged", ModLoader::NeoForge),
            ("net.fabricmc.fabric-loader", ModLoader::Fabric),
            ("org.quiltmc.quilt-loader", ModLoader::Quilt),
            ("com.mumfrey.liteloader", ModLoader::LiteLoader),
        ] {
            let raw = pack(&format!(
                r#"{{"uid":"net.minecraft","version":"1.20.1"}},{{"uid":"{uid}","version":"1"}}"#
            ));
            let target = parse(&raw).unwrap();
            assert_eq!(target.game_version, "1.20.1");
            assert_eq!(target.loaders, vec![loader]);
            assert!(target.summary().contains(loader.label()));
        }
    }

    #[test]
    fn keeps_every_declared_loader_for_multi_loader_packs() {
        let target = parse(&pack(
            r#"{"uid":"net.minecraft","version":"1.7.10"},{"uid":"net.minecraftforge"},{"uid":"com.mumfrey.liteloader"}"#,
        ))
        .unwrap();
        assert_eq!(
            target.loaders,
            vec![ModLoader::Forge, ModLoader::LiteLoader]
        );
        assert_eq!(target.summary(), "Minecraft 1.7.10 / Forge + LiteLoader");
        assert_eq!(
            target.modrinth_loaders().collect::<Vec<_>>(),
            vec!["forge", "liteloader"]
        );
    }

    #[test]
    fn cached_version_is_a_fallback_and_invalid_packs_fail_closed() {
        let target = parse(&pack(
            r#"{"uid":"net.minecraft","cachedVersion":"1.12.2"},{"uid":"net.minecraftforge"}"#,
        ))
        .unwrap();
        assert_eq!(target.game_version, "1.12.2");

        assert!(parse(b"{}").is_err());
        assert!(parse(&pack(r#"{"uid":"net.minecraftforge"}"#)).is_err());
        assert!(parse(&pack(r#"{"uid":"net.minecraft","version":"1.7.10"}"#)).is_err());
        assert!(parse(&pack(
            r#"{"uid":"net.minecraft","version":"../1.7.10"},{"uid":"net.minecraftforge"}"#
        ))
        .is_err());
    }

    #[tokio::test]
    async fn reads_mmc_pack_from_the_instance_root() {
        let root =
            std::env::temp_dir().join(format!("industrialis-target-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(
            root.join("mmc-pack.json"),
            pack(r#"{"uid":"net.minecraft","version":"1.7.10"},{"uid":"net.minecraftforge","version":"10.13.4.1614"}"#),
        )
        .await
        .unwrap();
        let target = load(&root).await.unwrap();
        assert_eq!(target.game_version, "1.7.10");
        assert_eq!(target.loaders, vec![ModLoader::Forge]);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
