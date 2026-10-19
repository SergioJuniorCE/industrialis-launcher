//! Bounded extraction of legacy LWJGL/JInput platform libraries.
use anyhow::ensure;
use std::{io::Read, path::Path};

pub(crate) fn extract(jar: &Path, directory: &Path, excludes: &[String]) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name().replace('\\', "/");
        ensure!(
            !name.starts_with('/')
                && !name.contains(':')
                && !name.split('/').any(|part| part == ".."),
            "Native archive contains an unsafe path"
        );
        let relative = file
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("Native archive contains an unsafe path"))?;
        if file.is_dir()
            || name.starts_with("META-INF/")
            || excludes.iter().any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        ensure!(
            file.unix_mode()
                .map(|m| m & 0o170000 != 0o120000)
                .unwrap_or(true),
            "Native archive contains a symlink"
        );
        total = total
            .checked_add(file.size())
            .ok_or_else(|| anyhow::anyhow!("Native archive size overflow"))?;
        ensure!(total <= 64 * 1024 * 1024, "Native archive exceeds 64 MB");
        let target = directory.join(relative);
        // Existing symlinks must never turn a safe ZIP path into an external write.
        let mut ancestor = Some(target.as_path());
        while let Some(path) = ancestor {
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Native destination contains a symlink"
                );
            }
            if path == directory {
                break;
            }
            ancestor = path.parent();
        }
        let mut bytes = Vec::new();
        file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= 64 * 1024 * 1024,
            "Native file exceeds 64 MB"
        );
        crate::fs_utils::write_atomic(&target, &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn rejects_native_archive_paths_that_escape_the_destination() {
        for name in [
            "../outside.bin",
            "dir/../../outside.bin",
            "..\\outside.bin",
            "C:/outside.bin",
        ] {
            let root =
                std::env::temp_dir().join(format!("gpui-native-unsafe-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let jar = root.join("native.jar");
            let mut archive = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"unsafe").unwrap();
            archive.finish().unwrap();
            assert!(extract(&jar, &root.join("natives"), &[]).is_err());
            assert!(!root.join("outside.bin").exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
