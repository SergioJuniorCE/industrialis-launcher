//! Explicit opt-in online acceptance using a new disposable profile only.
use industrialis_backend::{
    content::{self, ContentKind},
    modernity,
    paths::LauncherPaths,
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "industrialis-content-online-{}",
        uuid::Uuid::new_v4()
    ));
    let paths = LauncherPaths::new(root.clone());
    std::fs::create_dir_all(paths.instance_dir("fixture")?.join(".minecraft"))?;
    std::fs::write(
        paths.settings_path("fixture")?,
        br#"{"pack_version":"2.9.0-RC-2","preservedExtension":true}"#,
    )?;
    let catalog = modernity::catalog("2.9.0-RC-2").await?;
    println!("Release: {} / {} assets", catalog.tag, catalog.assets.len());
    let names = [
        catalog
            .assets
            .iter()
            .find(|a| a.name.starts_with("Modernity-2.9.x-"))
            .unwrap()
            .name
            .clone(),
        "Modernity-GTNHAddon-OutlinedOres.zip".into(),
        "MyCTMLib-1.4.2.jar".into(),
    ];
    for name in names {
        let asset = catalog
            .assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| anyhow::anyhow!("Acceptance asset not in release: {name}"))?
            .clone();
        modernity::install(&paths, "fixture", asset).await?;
        println!("Installed and verified: {name}");
    }
    let packs = content::list(&paths, "fixture", ContentKind::ResourcePack).await?;
    anyhow::ensure!(
        packs.len() == 2 && packs.iter().all(|p| p.managed && p.source.is_some()),
        "Expected managed packs and provenance"
    );
    anyhow::ensure!(
        content::list(&paths, "fixture", ContentKind::Mod)
            .await?
            .len()
            == 1,
        "Expected one optional mod"
    );
    let instance = paths.instance_dir("fixture")?;
    std::fs::remove_dir_all(instance.join(".minecraft"))?;
    industrialis_backend::pack_prep::apply_persistent_custom_mods(&instance).await?;
    industrialis_backend::pack_prep::apply_persistent_minecraft(&instance).await?;
    for p in packs {
        anyhow::ensure!(
            instance
                .join(".minecraft/resourcepacks")
                .join(&p.filename)
                .is_file(),
            "Resource pack did not survive restoration"
        );
        content::remove(&paths, "fixture", ContentKind::ResourcePack, &p.filename).await?;
    }
    industrialis_backend::custom_mods::remove(&paths, "fixture", "myctmlib").await?;
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(paths.settings_path("fixture")?)?)?;
    anyhow::ensure!(
        settings["preservedExtension"] == true,
        "Instance settings changed"
    );
    std::fs::remove_dir_all(root)?;
    println!("PASS: upstream checksums, main/add-on/mod install, overlay restore, removal and settings preservation");
    Ok(())
}
