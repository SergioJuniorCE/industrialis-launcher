//! Read only declared static color themes from a bounded VSIX, without extraction.
use anyhow::{ensure, Context};
use industrialis_core::{
    theme::{resolve_preset_or_default, BackgroundEffect},
    vscode_theme,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
};

fn relative(base: &str, path: &str) -> anyhow::Result<String> {
    ensure!(
        !path.starts_with('/') && !path.contains(['\\', ':', '\0']),
        "Invalid theme file path"
    );
    let mut parts: Vec<_> = base
        .split('/')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                ensure!(parts.len() > 1, "Theme path escapes the extension");
                parts.pop();
            }
            _ => parts.push(part.to_owned()),
        }
    }
    ensure!(
        parts.first().is_some_and(|p| p == "extension"),
        "Theme path escapes the extension"
    );
    Ok(parts.join("/"))
}
struct Package<'a> {
    zip: zip::ZipArchive<Cursor<&'a [u8]>>,
    names: HashMap<String, usize>,
}
impl<'a> Package<'a> {
    fn new(bytes: &'a [u8]) -> anyhow::Result<Self> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        ensure!(zip.len() <= 5000, "Theme package has too many entries");
        let mut names = HashMap::new();
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for index in 0..zip.len() {
            let entry = zip.by_index(index)?;
            let name = entry.name().trim_end_matches('/');
            ensure!(
                !name.is_empty()
                    && !name.contains('\\')
                    && crate::minecraft_files::sanitize_rel_path(name)? == name,
                "Invalid theme archive path"
            );
            ensure!(
                seen.insert(name.to_ascii_lowercase()),
                "Duplicate theme archive path"
            );
            ensure!(
                entry
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 != 0o120000),
                "Linked theme assets are unsupported"
            );
            total = total
                .checked_add(entry.size())
                .context("Theme archive size overflow")?;
            ensure!(
                total <= 64 * 1024 * 1024,
                "Theme package exceeds the expanded size limit"
            );
            if !entry.is_dir() {
                names.insert(name.to_owned(), index);
            }
        }
        Ok(Self { zip, names })
    }
    fn text(&mut self, path: &str) -> anyhow::Result<String> {
        let index = *self
            .names
            .get(path)
            .with_context(|| format!("Theme asset is missing: {path}"))?;
        let file = self.zip.by_index(index)?;
        ensure!(
            file.size() <= 2 * 1024 * 1024,
            "Theme asset exceeds the 2 MB limit"
        );
        let mut raw = Vec::new();
        file.take(2 * 1024 * 1024 + 1).read_to_end(&mut raw)?;
        ensure!(
            raw.len() <= 2 * 1024 * 1024,
            "Theme asset exceeds the 2 MB limit"
        );
        Ok(String::from_utf8(raw)?
            .trim_start_matches('\u{feff}')
            .to_owned())
    }
    fn theme(&mut self, path: &str, chain: &mut HashSet<String>) -> anyhow::Result<Value> {
        ensure!(
            chain.len() < 16 && chain.insert(path.to_owned()),
            "Cyclic or deeply nested theme includes"
        );
        let text = self.text(path)?;
        let mut theme: Value = if path.to_ascii_lowercase().ends_with(".tmtheme") {
            tm_theme(&text)?
        } else {
            json5::from_str(&text).with_context(|| format!("Invalid theme JSON: {path}"))?
        };
        ensure!(theme.is_object(), "Theme definition must be an object");
        let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("extension");
        if let Some(token_file) = theme["tokenColors"].as_str() {
            let path = relative(parent, token_file)?;
            theme["tokenColors"] = tm_theme(&self.text(&path)?)?["tokenColors"].clone();
        }
        if let Some(include) = theme["include"].as_str() {
            let included = relative(parent, include)?;
            let mut base = self.theme(&included, chain)?;
            let base_colors = base["colors"].as_object_mut();
            if let (Some(target), Some(colors)) = (base_colors, theme["colors"].as_object()) {
                target.extend(colors.clone());
            } else if theme["colors"].is_object() {
                base["colors"] = theme["colors"].clone();
            }
            let mut rules = base["tokenColors"].as_array().cloned().unwrap_or_default();
            rules.extend(
                theme["tokenColors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            theme["colors"] = base["colors"].clone();
            theme["tokenColors"] = rules.into();
        }
        ensure!(
            theme["tokenColors"]
                .as_array()
                .is_none_or(|rules| rules.len() <= 4096),
            "Theme has too many syntax rules"
        );
        chain.remove(path);
        Ok(theme)
    }
}
fn tm_theme(text: &str) -> anyhow::Result<Value> {
    let plist = plist::Value::from_reader(Cursor::new(text.as_bytes()))?;
    let raw = serde_json::to_value(plist)?;
    let settings = raw["settings"]
        .as_array()
        .context("Invalid TextMate theme")?;
    let defaults = settings
        .iter()
        .find(|r| r["scope"].is_null())
        .map(|r| &r["settings"]);
    Ok(
        serde_json::json!({"colors":{"editor.background":defaults.and_then(|v|v["background"].as_str()),"editor.foreground":defaults.and_then(|v|v["foreground"].as_str())},"tokenColors":settings}),
    )
}
pub(crate) fn import(bytes: &[u8], source: Value) -> anyhow::Result<Vec<Value>> {
    let mut package = Package::new(bytes)?;
    let manifest: Value = json5::from_str(&package.text("extension/package.json")?)?;
    ensure!(
        manifest["publisher"]
            .as_str()
            .zip(source["namespace"].as_str())
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            && manifest["name"]
                .as_str()
                .zip(source["extension"].as_str())
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            && manifest["version"] == source["version"],
        "Package manifest does not match the registry identity"
    );
    let contributions = manifest["contributes"]["themes"]
        .as_array()
        .context("This extension does not contribute color themes (icon themes are separate)")?;
    ensure!(
        !contributions.is_empty() && contributions.len() <= 64,
        "Unsupported number of theme variants"
    );
    let mut result = Vec::new();
    let mut ids = HashSet::new();
    for contribution in contributions {
        let declared = contribution["path"]
            .as_str()
            .context("Theme contribution has no path")?;
        let path = relative("extension", declared)?;
        let document = package.theme(&path, &mut HashSet::new())?;
        let name = contribution["label"]
            .as_str()
            .or_else(|| contribution["id"].as_str())
            .unwrap_or(declared);
        ensure!(name.len() <= 256, "Theme label is too long");
        let mode = contribution["uiTheme"]
            .as_str()
            .or_else(|| document["type"].as_str())
            .unwrap_or("vs-dark");
        let dark = vscode_theme::dark_mode(mode);
        use sha2::Digest;
        let identity = format!(
            "{:x}",
            sha2::Sha256::digest(format!("{path}\0{name}").as_bytes())
        );
        let mut preset = resolve_preset_or_default("monochrome");
        preset.id = format!(
            "openvsx:{}.{}:{}",
            source["namespace"].as_str().unwrap_or_default(),
            source["extension"].as_str().unwrap_or_default(),
            &identity[..16]
        );
        ensure!(
            ids.insert(preset.id.clone()),
            "Duplicate theme contribution"
        );
        preset.name = name.into();
        preset.description = format!(
            "{} / Open VSX {}",
            manifest["displayName"].as_str().unwrap_or(name),
            source["version"].as_str().unwrap_or_default()
        );
        preset.builtin = false;
        preset.background_effect = BackgroundEffect::None;
        if dark {
            preset.dark = vscode_theme::convert(&document, true);
        } else {
            preset.light = vscode_theme::convert(&document, false);
        }
        let mut raw = serde_json::to_value(preset)?;
        raw["openvsx"] = source.clone();
        raw["openvsx"]["mode"] = if dark { "dark" } else { "light" }.into();
        raw["openvsx"]["themePath"] = path.into();
        raw["editorTheme"] = document;
        result.push(raw);
    }
    ensure!(
        serde_json::to_vec(&result)?.len() <= 4 * 1024 * 1024,
        "Converted themes exceed the storage limit"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn package(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn source() -> Value {
        serde_json::json!({"namespace":"publisher","extension":"theme","version":"1.0.0","sha256":"test"})
    }
    const MANIFEST: &str = r#"{"publisher":"publisher","name":"theme","version":"1.0.0","contributes":{"themes":[{"label":"Test","uiTheme":"vs-dark","path":"./themes/dark.json"}]}}"#;
    #[test]
    fn jsonc_inheritance_and_syntax_survive_without_extracting_extension_code() {
        let bytes = package(&[
            ("extension/package.json", MANIFEST),
            (
                "extension/themes/dark.json",
                r##"{// comment
            "include":"../base.json","colors":{"button.background":"#abc",},"tokenColors":[{"scope":"string","settings":{"foreground":"#00ff00"}}],}"##,
            ),
            (
                "extension/base.json",
                r##"{"colors":{"editor.background":"#123456"},"tokenColors":[{"scope":"comment","settings":{"foreground":"#aaaaaa"}}]}"##,
            ),
            ("extension/main.js", "throw new Error('Never run');"),
        ]);
        let themes = import(&bytes, source()).unwrap();
        assert_eq!(themes[0]["dark"]["background"], "#123456");
        assert_eq!(themes[0]["dark"]["primary"], "#aabbcc");
        assert_eq!(
            themes[0]["editorTheme"]["tokenColors"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(themes[0]["openvsx"]["mode"], "dark");
    }
    #[test]
    fn textmate_files_light_variants_and_version_independent_ids_are_supported() {
        let manifest = r#"{"publisher":"Publisher","name":"theme","version":"1.0.0","contributes":{"themes":[{"label":"Dark","uiTheme":"vs-dark","path":"themes/dark.json"},{"label":"Light","uiTheme":"vs","path":"themes/light.tmTheme"}]}}"#;
        let textmate = r##"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>settings</key><array><dict><key>settings</key><dict><key>foreground</key><string>#123456</string><key>background</key><string>#ffffff</string></dict></dict><dict><key>scope</key><string>comment</string><key>settings</key><dict><key>foreground</key><string>#009900</string><key>fontStyle</key><string>italic</string></dict></dict></array></dict></plist>"##;
        let files = [
            ("extension/package.json", manifest),
            (
                "extension/themes/dark.json",
                r##"{"include":"../base.json","tokenColors":"light.tmTheme","colors":{"editor.background":"#112233"}}"##,
            ),
            (
                "extension/base.json",
                r##"{"tokenColors":[{"scope":"string","settings":{"foreground":"#abcdef"}}]}"##,
            ),
            ("extension/themes/light.tmTheme", textmate),
        ];
        let result = import(&package(&files), source()).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["dark"]["background"], "#112233");
        assert_eq!(
            result[0]["editorTheme"]["tokenColors"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(result[1]["openvsx"]["mode"], "light");
        assert_eq!(result[1]["light"]["background"], "#ffffff");
        assert_eq!(result[1]["light"]["foreground"], "#123456");
        let updated_manifest = manifest.replace("1.0.0", "1.1.0");
        let mut updated_files = files;
        updated_files[0].1 = &updated_manifest;
        let mut updated_source = source();
        updated_source["version"] = "1.1.0".into();
        let updated = import(&package(&updated_files), updated_source).unwrap();
        assert_eq!(result[0]["id"], updated[0]["id"]);
        assert_eq!(result[1]["id"], updated[1]["id"]);
    }
    #[test]
    fn unsafe_paths_cycles_wrong_identity_and_non_color_extensions_are_rejected() {
        let cyclic = package(&[
            ("extension/package.json", MANIFEST),
            ("extension/themes/dark.json", r#"{"include":"dark.json"}"#),
        ]);
        assert!(import(&cyclic, source()).is_err());
        let escape = package(&[
            ("extension/package.json", MANIFEST),
            (
                "extension/themes/dark.json",
                r#"{"include":"../../outside.json"}"#,
            ),
        ]);
        assert!(import(&escape, source()).is_err());
        let bad = package(&[("../escape", "bad"), ("extension/package.json", MANIFEST)]);
        assert!(import(&bad, source()).is_err());
        let empty = package(&[(
            "extension/package.json",
            r#"{"publisher":"publisher","name":"theme","version":"1.0.0"}"#,
        )]);
        assert!(import(&empty, source()).is_err());
        let mut wrong = source();
        wrong["namespace"] = "other".into();
        assert!(import(&cyclic, wrong).is_err());
    }
}
