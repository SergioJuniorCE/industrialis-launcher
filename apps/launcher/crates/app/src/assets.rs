//! Bundled brand artwork and the same Lucide vectors used by the Electron UI.
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
pub struct LauncherAssets;
impl AssetSource for LauncherAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/image.svg" => include_bytes!("../assets/icons/image.svg"),
            "icons/folder.svg" => include_bytes!("../assets/icons/folder.svg"),
            "icons/file-text.svg" => include_bytes!("../assets/icons/file-text.svg"),
            "icons/file-code.svg" => include_bytes!("../assets/icons/file-code.svg"),
            "icons/file-braces.svg" => include_bytes!("../assets/icons/file-braces.svg"),
            "icons/archive.svg" => include_bytes!("../assets/icons/archive.svg"),
            "icons/fold-vertical.svg" => include_bytes!("../assets/icons/fold-vertical.svg"),
            "icons/external-link.svg" => include_bytes!("../assets/icons/external-link.svg"),
            "icons/chevron-up.svg" => include_bytes!("../assets/icons/chevron-up.svg"),
            "icons/boxes.svg" => include_bytes!("../assets/icons/boxes.svg"),
            "icons/activity.svg" => include_bytes!("../assets/icons/activity.svg"),
            "icons/users.svg" => include_bytes!("../assets/icons/users.svg"),
            "icons/settings.svg" => include_bytes!("../assets/icons/settings.svg"),
            "icons/sun.svg" => include_bytes!("../assets/icons/sun.svg"),
            "icons/moon.svg" => include_bytes!("../assets/icons/moon.svg"),
            "icons/plus.svg" => include_bytes!("../assets/icons/plus.svg"),
            "icons/x.svg" => include_bytes!("../assets/icons/x.svg"),
            "icons/minus.svg" => include_bytes!("../assets/icons/minus.svg"),
            "icons/square.svg" => include_bytes!("../assets/icons/square.svg"),
            "icons/panels-top-left.svg" => include_bytes!("../assets/icons/panels-top-left.svg"),
            "icons/chevron-down.svg" => include_bytes!("../assets/icons/chevron-down.svg"),
            "icons/chevron-right.svg" => include_bytes!("../assets/icons/chevron-right.svg"),
            "icons/user-circle.svg" => include_bytes!("../assets/icons/user-circle.svg"),
            "icons/loader-circle.svg" => include_bytes!("../assets/icons/loader-circle.svg"),
            "icons/info.svg" => include_bytes!("../assets/icons/info.svg"),
            "icons/terminal.svg" => include_bytes!("../assets/icons/terminal.svg"),
            "icons/copy.svg" => include_bytes!("../assets/icons/copy.svg"),
            "icons/trash-2.svg" => include_bytes!("../assets/icons/trash-2.svg"),
            "icons/folder-open.svg" => include_bytes!("../assets/icons/folder-open.svg"),
            "icons/arrow-up-circle.svg" => include_bytes!("../assets/icons/arrow-up-circle.svg"),
            "icons/refresh-cw.svg" => include_bytes!("../assets/icons/refresh-cw.svg"),
            "icons/play.svg" => include_bytes!("../assets/icons/play.svg"),
            "icons/ellipsis.svg" => include_bytes!("../assets/icons/ellipsis.svg"),
            "icons/grip-vertical.svg" => include_bytes!("../assets/icons/grip-vertical.svg"),
            "icons/star.svg" => include_bytes!("../assets/icons/star.svg"),
            "icons/sliders-horizontal.svg" => {
                include_bytes!("../assets/icons/sliders-horizontal.svg")
            }
            "icons/check.svg" => include_bytes!("../assets/icons/check.svg"),
            "icons/star-filled.svg" => include_bytes!("../assets/icons/star-filled.svg"),
            "brand.png" => include_bytes!("../assets/brand.png"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }
    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, sync::Arc};

    #[test]
    fn every_bundled_icon_loads_and_rasterizes_visible_pixels() {
        let source = LauncherAssets;
        let renderer = gpui::SvgRenderer::new(Arc::new(LauncherAssets));
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "svg") {
                continue;
            }
            let key = format!("icons/{}", path.file_name().unwrap().to_str().unwrap());
            let bytes = source.load(&key).unwrap().expect("icon must be embedded");
            let image = renderer
                .render_single_frame(&bytes, 1.0)
                .expect("valid SVG");
            assert!(
                image
                    .as_bytes(0)
                    .unwrap()
                    .chunks_exact(4)
                    .any(|pixel| pixel[3] > 0),
                "{key} must have visible pixels in GPUI's rasterizer"
            );
        }
    }
}
