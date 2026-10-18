//! In-memory filename index and ranked fuzzy path matching; never indexes contents.
use crate::{
    minecraft_files::{is_path_editable, scoped, MinecraftEntry, MAX_READ_BYTES},
    paths::LauncherPaths,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

pub const MAX_RESULTS: usize = 2000;
#[derive(Default)]
pub struct SearchResults {
    pub entries: Vec<MinecraftEntry>,
    pub truncated: bool,
    pub unreadable: usize,
}
#[derive(Default)]
pub struct FileIndex {
    snapshot: Mutex<Option<Snapshot>>,
    revision: AtomicU64,
    #[cfg(test)]
    scans: AtomicU64,
}
struct IndexedPath {
    rel: String,
    folded: String,
}
struct Snapshot {
    root: PathBuf,
    revision: u64,
    built_at: Instant,
    folders: Vec<(PathBuf, Option<SystemTime>)>,
    files: Vec<IndexedPath>,
    unreadable: usize,
}
impl FileIndex {
    pub fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
    }
    pub async fn search(
        self: &Arc<Self>,
        paths: &LauncherPaths,
        id: &str,
        query: &str,
        cancelled: Arc<AtomicBool>,
    ) -> anyhow::Result<SearchResults> {
        let lease = crate::lifecycle::admit(paths)?;
        let lifecycle = crate::lifecycle::for_profile(paths);
        let paths = paths.clone();
        let id = id.to_owned();
        let query = query.trim().replace('\\', "/").to_lowercase();
        let index = self.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let stopped = || cancelled.load(Ordering::Relaxed) || lifecycle.is_closing();
            if query.is_empty() || stopped() {
                return Ok(SearchResults::default());
            }
            let root = scoped(&paths, &id, ".minecraft", "", false)?;
            let mut cache = index.snapshot.lock().unwrap_or_else(|e| e.into_inner());
            if stopped() {
                return Ok(SearchResults::default());
            }
            let revision = index.revision.load(Ordering::Relaxed);
            let fresh = cache.as_ref().is_some_and(|s| {
                s.root == root
                    && s.revision == revision
                    // Directory timestamps may share a tick with a rename, or
                    // be coarse on network filesystems. Never keep such a
                    // snapshot indefinitely while the user keeps searching.
                    && s.built_at.elapsed() < Duration::from_secs(5)
                    && s.folders.iter().all(|(dir, stamp)| {
                        !stopped()
                            && std::fs::symlink_metadata(dir)
                                .ok()
                                .filter(|m| !linked(m))
                                .and_then(|m| m.modified().ok())
                                .as_ref()
                                == stamp.as_ref()
                    })
            });
            if !fresh {
                let mut snapshot = Snapshot {
                    root: root.clone(),
                    revision,
                    built_at: Instant::now(),
                    folders: Vec::new(),
                    files: Vec::new(),
                    unreadable: 0,
                };
                let mut walk = walkdir::WalkDir::new(&root).follow_links(false).into_iter();
                while let Some(entry) = walk.next() {
                    if stopped() {
                        return Ok(SearchResults::default());
                    }
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => {
                            snapshot.unreadable += 1;
                            continue;
                        }
                    };
                    let meta = match std::fs::symlink_metadata(entry.path()) {
                        Ok(m) => m,
                        Err(_) => {
                            snapshot.unreadable += 1;
                            continue;
                        }
                    };
                    if linked(&meta) {
                        if entry.file_type().is_dir() {
                            walk.skip_current_dir();
                        }
                        continue;
                    }
                    if meta.is_dir() {
                        snapshot
                            .folders
                            .push((entry.path().to_owned(), meta.modified().ok()));
                    } else if meta.is_file() {
                        let rel = entry
                            .path()
                            .strip_prefix(&root)?
                            .to_string_lossy()
                            .replace('\\', "/");
                        snapshot.files.push(IndexedPath {
                            folded: rel.to_lowercase(),
                            rel,
                        });
                    }
                }
                #[cfg(test)]
                index.scans.fetch_add(1, Ordering::Relaxed);
                snapshot.built_at = Instant::now();
                *cache = Some(snapshot);
            }
            let snapshot = cache.as_ref().unwrap();
            let tokens: Vec<_> = query.split_whitespace().collect();
            let mut ranked = Vec::new();
            for file in &snapshot.files {
                if stopped() {
                    return Ok(SearchResults::default());
                }
                if let Some(score) = path_score(&file.folded, &query, &tokens) {
                    ranked.push((score, file));
                }
            }
            let compare = |a: &(i64, &IndexedPath), b: &(i64, &IndexedPath)| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.folded.cmp(&b.1.folded))
                    .then_with(|| a.1.rel.cmp(&b.1.rel))
            };
            let truncated = ranked.len() > MAX_RESULTS;
            if truncated {
                ranked.select_nth_unstable_by(MAX_RESULTS, compare);
                ranked.truncate(MAX_RESULTS);
            }
            ranked.sort_unstable_by(compare);
            let mut found = SearchResults {
                truncated,
                unreadable: snapshot.unreadable,
                entries: Vec::with_capacity(ranked.len()),
            };
            for (_, file) in ranked {
                if stopped() {
                    return Ok(SearchResults::default());
                }
                // Check matched files at query time: removed files, sizes and override
                // indicators cannot be served from stale per-file metadata.
                let path = match scoped(&paths, &id, ".minecraft", &file.rel, false) {
                    Ok(p) => p,
                    Err(_) => {
                        index.invalidate();
                        continue;
                    }
                };
                let meta = match std::fs::symlink_metadata(path) {
                    Ok(m) if m.is_file() && !linked(&m) => m,
                    _ => {
                        index.invalidate();
                        continue;
                    }
                };
                let editable = is_path_editable(&file.rel);
                let persistent = scoped(&paths, &id, "persistent-minecraft", &file.rel, false)
                    .ok()
                    .is_some_and(|p| p.is_file());
                found.entries.push(MinecraftEntry {
                    name: file.rel.rsplit('/').next().unwrap().to_owned(),
                    rel_path: file.rel.clone(),
                    is_dir: false,
                    editable,
                    has_persistent_override: persistent,
                    too_large_to_edit: editable && meta.len() > MAX_READ_BYTES,
                });
            }
            Ok(found)
        })
        .await?
    }
}
fn linked(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}
/// Each whitespace-separated token must match the path. Contiguous filename and
/// component matches outrank ordered-character subsequences; exact names win.
fn path_score(path: &str, query: &str, tokens: &[&str]) -> Option<i64> {
    let basename = path.rsplit('/').next().unwrap_or(path);
    let mut score = if path == query {
        20000
    } else if basename == query {
        10000
    } else {
        0
    };
    for token in tokens {
        if let Some(position) = path.rfind(token) {
            score += if basename.contains(token) {
                2000
            } else if position == 0 || path.as_bytes()[position - 1] == b'/' {
                1000
            } else {
                500
            };
            score += token.chars().count() as i64 * 10;
        } else {
            let mut chars = path.char_indices();
            let mut previous = None;
            let mut gaps = 0;
            for wanted in token.chars() {
                let (offset, found) = chars.find(|(_, ch)| *ch == wanted)?;
                if let Some(end) = previous {
                    gaps += offset - end;
                }
                previous = Some(offset + found.len_utf8());
            }
            score += 100 - (gaps as i64).min(99);
        }
    }
    Some(score - path.chars().count() as i64 / 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn search(
        paths: &LauncherPaths,
        id: &str,
        query: &str,
        cancelled: Arc<AtomicBool>,
    ) -> anyhow::Result<SearchResults> {
        Arc::<FileIndex>::default()
            .search(paths, id, query, cancelled)
            .await
    }
    #[test]
    fn path_tokens_and_fuzzy_characters_rank_actual_names_first() {
        let q = "server ranks";
        assert!(path_score(
            "serverutilities/server/ranks.txt",
            q,
            &q.split_whitespace().collect::<Vec<_>>()
        )
        .is_some());
        assert!(path_score(
            "serverutilities/server/ranks.txt",
            "srvr rnks",
            &["srvr", "rnks"]
        )
        .is_some());
        assert!(path_score("config/client.txt", q, &["server", "ranks"]).is_none());
        assert!(
            path_score("ranks.txt", "ranks.txt", &["ranks.txt"])
                > path_score("config/ranks.txt.bak", "ranks.txt", &["ranks.txt"])
        );
    }
    #[tokio::test]
    async fn index_reuses_scans_and_detects_added_removed_and_renamed_paths() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("file-index-{}", uuid::Uuid::new_v4())),
        );
        let game = paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/serverutilities/server");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("ranks.txt"), "test").unwrap();
        let index = Arc::<FileIndex>::default();
        assert_eq!(
            index
                .search(&paths, "pack", "server ranks", Arc::new(false.into()))
                .await
                .unwrap()
                .entries
                .len(),
            1
        );
        index
            .search(&paths, "pack", "rnks", Arc::new(false.into()))
            .await
            .unwrap();
        assert_eq!(
            index.scans.load(Ordering::Relaxed),
            1,
            "Typing must reuse the existing index"
        );
        std::fs::rename(game.join("ranks.txt"), game.join("permissions.txt")).unwrap();
        // Reproduce a filesystem timestamp tick shared by the scan and rename.
        // Validation sees unchanged stamps even though a cached file has moved.
        for (folder, stamp) in &mut index.snapshot.lock().unwrap().as_mut().unwrap().folders {
            *stamp = std::fs::metadata(folder).unwrap().modified().ok();
        }
        assert!(index
            .search(&paths, "pack", "ranks.txt", Arc::new(false.into()))
            .await
            .unwrap()
            .entries
            .is_empty());
        assert_eq!(
            index
                .search(&paths, "pack", "permissions", Arc::new(false.into()))
                .await
                .unwrap()
                .entries
                .len(),
            1
        );
        assert!(index.scans.load(Ordering::Relaxed) >= 2);
        std::fs::remove_file(game.join("permissions.txt")).unwrap();
        assert!(index
            .search(&paths, "pack", "permissions", Arc::new(false.into()))
            .await
            .unwrap()
            .entries
            .is_empty());
        // Clear the removal invalidation, then add a file without an observable
        // timestamp change. The safety refresh must also discover pure additions.
        index
            .search(&paths, "pack", "added", Arc::new(false.into()))
            .await
            .unwrap();
        std::fs::write(game.join("added.cfg"), "new").unwrap();
        {
            let mut cache = index.snapshot.lock().unwrap();
            let snapshot = cache.as_mut().unwrap();
            for (folder, stamp) in &mut snapshot.folders {
                *stamp = std::fs::metadata(folder).unwrap().modified().ok();
            }
            snapshot.built_at = Instant::now() - Duration::from_secs(6);
        }
        assert_eq!(
            index
                .search(&paths, "pack", "added", Arc::new(false.into()))
                .await
                .unwrap()
                .entries
                .len(),
            1
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
    #[tokio::test]
    async fn searches_unexpanded_paths_unicode_and_read_only_files_and_cancels() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("file-search-{}", uuid::Uuid::new_v4())),
        );
        let instance = paths.instance_dir("pack").unwrap();
        for root in [".minecraft", "persistent-minecraft"] {
            std::fs::create_dir_all(instance.join(root).join("config/nested")).unwrap();
            std::fs::write(instance.join(root).join("config/nested/Café.cfg"), "a").unwrap();
        }
        std::fs::create_dir_all(instance.join(".minecraft/mods")).unwrap();
        std::fs::write(instance.join(".minecraft/mods/test.jar"), "binary").unwrap();
        let results = search(&paths, "pack", "NESTED\\CAFÉ", Arc::new(false.into()))
            .await
            .unwrap();
        assert_eq!(results.entries.len(), 1);
        assert_eq!(results.entries[0].rel_path, "config/nested/Café.cfg");
        assert!(results.entries[0].has_persistent_override);
        let readonly = search(&paths, "pack", ".jar", Arc::new(false.into()))
            .await
            .unwrap();
        assert_eq!(readonly.entries.len(), 1);
        assert!(!readonly.entries[0].editable);
        assert!(search(&paths, "pack", "café", Arc::new(true.into()))
            .await
            .unwrap()
            .entries
            .is_empty());
        assert!(search(&paths, "pack", "missing", Arc::new(false.into()))
            .await
            .unwrap()
            .entries
            .is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }

    #[tokio::test]
    async fn broad_search_is_bounded_and_marks_truncation() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("file-search-{}", uuid::Uuid::new_v4())),
        );
        let game = paths.instance_dir("pack").unwrap().join(".minecraft");
        std::fs::create_dir_all(&game).unwrap();
        for index in 0..=MAX_RESULTS {
            std::fs::write(game.join(format!("{index:04}.cfg")), "x").unwrap();
        }
        let index = Arc::<FileIndex>::default();
        let results = index
            .search(&paths, "pack", ".cfg", Arc::new(false.into()))
            .await
            .unwrap();
        assert_eq!(results.entries.len(), MAX_RESULTS);
        assert!(results.truncated);
        assert_eq!(results.entries[0].rel_path, "0000.cfg");
        let start = std::time::Instant::now();
        for _ in 0..10 {
            assert_eq!(
                index
                    .search(&paths, "pack", "1000.cfg", Arc::new(false.into()))
                    .await
                    .unwrap()
                    .entries[0]
                    .rel_path,
                "1000.cfg"
            );
        }
        eprintln!(
            "Indexed path search across {} files: {:.2} ms/query",
            MAX_RESULTS + 1,
            start.elapsed().as_secs_f64() * 100.
        );
        assert_eq!(index.scans.load(Ordering::Relaxed), 1);
        assert!(search(&paths, "pack", " ", Arc::new(false.into()))
            .await
            .unwrap()
            .entries
            .is_empty());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
