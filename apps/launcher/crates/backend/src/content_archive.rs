//! Validate archives without extracting untrusted paths into the profile.
use anyhow::{ensure, Context};
use std::{
    collections::HashSet,
    io::{Cursor, Read, Write},
};
pub(crate) const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const MAX_FILE: u64 = 64 * 1024 * 1024;

pub(crate) fn prepare(raw: Vec<u8>, resource_pack: bool) -> anyhow::Result<Vec<u8>> {
    ensure!(
        raw.len() as u64 <= MAX_ARCHIVE,
        "Archive exceeds the 512 MB limit"
    );
    let mut archive =
        zip::ZipArchive::new(Cursor::new(&raw)).context("Not a valid ZIP/JAR archive")?;
    ensure!(archive.len() <= 100_000, "Archive contains too many files");
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut roots = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name().trim_end_matches('/');
        ensure!(
            !name.is_empty() && !name.contains('\\'),
            "Invalid archive path"
        );
        ensure!(
            crate::minecraft_files::sanitize_rel_path(name)? == name,
            "Invalid archive path"
        );
        ensure!(
            names.insert(name.to_lowercase()),
            "Duplicate archive path: {name}"
        );
        ensure!(
            file.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "Archive contains a symlink"
        );
        expanded = expanded
            .checked_add(file.size())
            .context("Archive size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED && file.size() <= MAX_FILE,
            "Archive expands beyond the import limit"
        );
        if name == "pack.mcmeta" || name.ends_with("/pack.mcmeta") {
            roots.push(name.strip_suffix("pack.mcmeta").unwrap().to_owned());
        }
    }
    if !resource_pack {
        // Consume every entry to verify decompression/CRC before the JAR is loadable.
        for i in 0..archive.len() {
            std::io::copy(&mut archive.by_index(i)?, &mut std::io::sink())?;
        }
        return Ok(raw);
    }
    ensure!(
        roots.len() == 1,
        "Expected one resource pack containing pack.mcmeta"
    );
    let prefix = &roots[0];
    ensure!(
        prefix.is_empty() || prefix.matches('/').count() == 1,
        "Resource pack must be at the archive root or inside one directory"
    );
    let mut meta = String::new();
    archive
        .by_name(&format!("{prefix}pack.mcmeta"))?
        .take(64 * 1024)
        .read_to_string(&mut meta)?;
    let meta: serde_json::Value = serde_json::from_str(meta.trim_start_matches('\u{feff}'))?;
    ensure!(
        meta["pack"]["pack_format"].as_u64() == Some(1),
        "This instance needs a Minecraft 1.7.10 resource pack (pack_format 1)"
    );
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file
            .name()
            .strip_prefix(prefix)
            .context("Files exist outside the resource-pack directory")?
            .to_owned();
        output.start_file(name, options)?;
        // Keep upstream licenses, credits and every asset. Only remove the wrapper.
        std::io::copy(&mut file, &mut output)?;
    }
    output.flush()?;
    Ok(output.finish()?.into_inner())
}

#[cfg(test)]
pub(crate) fn fixture(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, raw) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(raw).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_repository_zip_and_preserves_credits() {
        let raw = fixture(&[
            ("project/pack.mcmeta", b"{\"pack\":{\"pack_format\":1}}"),
            ("project/assets/x.png", b"texture"),
            ("project/LICENSE.txt", b"credits"),
        ]);
        let mut zip = zip::ZipArchive::new(Cursor::new(prepare(raw, true).unwrap())).unwrap();
        assert!(zip.by_name("pack.mcmeta").is_ok());
        let mut text = String::new();
        zip.by_name("LICENSE.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "credits");
    }
    #[test]
    fn rejects_traversal_duplicates_wrong_format_and_multiple_packs() {
        let meta = b"{\"pack\":{\"pack_format\":1}}".as_slice();
        for files in [
            vec![("pack.mcmeta", meta), ("../outside", b"bad")],
            vec![("pack.mcmeta", meta), ("PACK.MCMETA", meta)],
            vec![("pack.mcmeta", b"{\"pack\":{\"pack_format\":15}}".as_slice())],
            vec![("a/pack.mcmeta", meta), ("b/pack.mcmeta", meta)],
        ] {
            assert!(prepare(fixture(&files), true).is_err());
        }
        assert!(prepare(b"not a jar".to_vec(), false).is_err());
    }
}
