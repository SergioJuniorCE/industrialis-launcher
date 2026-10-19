//! Opt-in public registry smoke; writes only a new temporary profile.
use industrialis_backend::{
    openvsx::{self, Extension},
    paths::LauncherPaths,
    settings,
};
use industrialis_core::{
    launcher_settings::{LauncherSettingsData, ThemeMode},
    theme,
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let results = openvsx::search("dracula", 0).await?;
    anyhow::ensure!(!results.extensions.is_empty(), "No real search results");
    println!(
        "Search: {} total matches, {} returned",
        results.total_size,
        results.extensions.len()
    );
    let extension = Extension {
        namespace: "dracula-theme".into(),
        name: "theme-dracula".into(),
        version: "2.25.1".into(),
        display_name: None,
        description: None,
    };
    let bundle = openvsx::download(extension).await?;
    anyhow::ensure!(!bundle.themes.is_empty(), "No variants imported");
    for raw in &bundle.themes {
        let preset: theme::ThemePreset = serde_json::from_value(raw.clone())?;
        anyhow::ensure!(
            theme::is_valid_theme_tokens(&preset.dark)
                && theme::is_valid_theme_tokens(&preset.light),
            "Invalid native colors"
        );
        anyhow::ensure!(
            raw["editorTheme"]["tokenColors"]
                .as_array()
                .is_some_and(|rules| !rules.is_empty()),
            "Missing syntax colors"
        );
        println!(
            "Imported: {} / {} / {} syntax rules",
            preset.name,
            raw["openvsx"]["mode"],
            raw["editorTheme"]["tokenColors"].as_array().unwrap().len()
        );
    }
    let root = std::env::temp_dir().join(format!(
        "industrialis-openvsx-smoke-{}",
        uuid::Uuid::new_v4()
    ));
    let paths = LauncherPaths::new(root.clone());
    let mut prefs = LauncherSettingsData::default();
    prefs.theme_preset = bundle.themes[0]["id"].as_str().unwrap().into();
    prefs.theme_mode = ThemeMode::Dark;
    prefs.custom_theme_presets = bundle.themes.clone();
    prefs
        .extra_fields
        .insert("preservedExtension".into(), true.into());
    settings::save_launcher_preferences(&paths, &prefs).await?;
    let loaded = settings::load_launcher_settings(&paths).await?;
    anyhow::ensure!(
        loaded.custom_theme_presets == bundle.themes
            && loaded.extra_fields["preservedExtension"] == true,
        "Saved theme data changed"
    );
    println!(
        "Persisted theme data: {} bytes",
        serde_json::to_vec(&loaded.custom_theme_presets)?.len()
    );
    std::fs::remove_dir_all(root)?;
    println!("PASS: public search, pinned checksum download, static color/syntax import, save/reload and cleanup");
    Ok(())
}
