//! The shared walk, against a tree held in a map.
//!
//! ❗ These are the cells that used to live once per backend. The walk is the
//! arithmetic behind a transfer estimate, so a wrong total here is a wrong
//! number under the user's decision to proceed.

use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::ignore_poison::IgnorePoison;

/// A tree with no server under it: a map of directory path to its children.
struct FakeTree {
    dirs: HashMap<String, Vec<FileEntry>>,
    files: HashMap<String, u64>,
    /// Every path the walk asked about, so a cell can hold it to one listing
    /// per directory and no stat per child.
    asked: Mutex<Vec<String>>,
}

impl FakeTree {
    fn new() -> Self {
        Self {
            dirs: HashMap::new(),
            files: HashMap::new(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn with_dir(mut self, path: &str, children: &[(&str, bool, u64)]) -> Self {
        let entries = children
            .iter()
            .map(|(name, is_dir, size)| {
                let child = format!("{}/{name}", path.trim_end_matches('/'));
                let mut entry = FileEntry::new(name.to_string(), child, *is_dir, false);
                entry.size = (!*is_dir).then_some(*size);
                entry
            })
            .collect();
        self.dirs.insert(path.to_string(), entries);
        for (name, is_dir, size) in children {
            if !is_dir {
                self.files
                    .insert(format!("{}/{name}", path.trim_end_matches('/')), *size);
            }
        }
        self
    }

    /// Adds one SYMLINKED directory child to an existing directory. The target
    /// is a real directory in the map, so a walk that follows the link would
    /// find something to count.
    fn with_symlinked_dir(mut self, parent: &str, name: &str) -> Self {
        let child = format!("{}/{name}", parent.trim_end_matches('/'));
        let entry = FileEntry::new(name.to_string(), child, true, true);
        self.dirs.entry(parent.to_string()).or_default().push(entry);
        self
    }

    fn listings_of(&self, path: &str) -> usize {
        self.asked.lock_ignore_poison().iter().filter(|p| *p == path).count()
    }
}

impl ScanSource for FakeTree {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(async move {
            let key = path.to_string_lossy().into_owned();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if self.dirs.contains_key(&key) {
                return Ok(FileEntry::new(name, key, true, false));
            }
            match self.files.get(&key) {
                Some(size) => {
                    let mut entry = FileEntry::new(name, key, false, false);
                    entry.size = Some(*size);
                    Ok(entry)
                }
                None => Err(VolumeError::NotFound(key)),
            }
        })
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let key = path.to_string_lossy().into_owned();
            self.asked.lock_ignore_poison().push(key.clone());
            self.dirs.get(&key).cloned().ok_or(VolumeError::NotFound(key))
        })
    }
}

fn item(name: &str, size: u64, is_directory: bool) -> SourceItemInfo {
    SourceItemInfo {
        name: name.to_string(),
        size,
        modified: None,
        is_directory,
    }
}

#[tokio::test]
async fn a_subtree_counts_every_level_and_keeps_dedup_in_lockstep() {
    let tree = FakeTree::new()
        .with_dir("/top", &[("a.txt", false, 10), ("deep", true, 0)])
        .with_dir("/top/deep", &[("b.txt", false, 32), ("c.txt", false, 8)]);
    let ticker = ScanTicker::new(None);

    let scan = scan_tree(&tree, Path::new("/top"), &ticker).await.expect("the walk");

    assert_eq!(scan.file_count, 3);
    assert_eq!(scan.dir_count, 2, "the top and the nested directory both count");
    assert_eq!(scan.total_bytes, 50);
    assert_eq!(
        scan.dedup_bytes, scan.total_bytes,
        "a backend reaching this walk has no link count, so the two move together"
    );
    assert!(scan.top_level_is_directory);
}

#[tokio::test]
async fn a_single_file_is_one_file_and_no_directory() {
    let tree = FakeTree::new().with_dir("/top", &[("a.txt", false, 10)]);
    let ticker = ScanTicker::new(None);

    let scan = scan_tree(&tree, Path::new("/top/a.txt"), &ticker)
        .await
        .expect("the walk");

    assert_eq!((scan.file_count, scan.dir_count, scan.total_bytes), (1, 0, 10));
    assert!(!scan.top_level_is_directory);
}

#[tokio::test]
async fn the_walk_lists_each_directory_once_and_stats_no_child() {
    // ❗ The cell behind the module's cost claim: a stat per child turns a
    // 1,000-file folder from one round trip into a thousand.
    let tree = FakeTree::new()
        .with_dir("/top", &[("a.txt", false, 1), ("b.txt", false, 2), ("deep", true, 0)])
        .with_dir("/top/deep", &[("c.txt", false, 4)]);
    let ticker = ScanTicker::new(None);

    scan_tree(&tree, Path::new("/top"), &ticker).await.expect("the walk");

    assert_eq!(tree.listings_of("/top"), 1);
    assert_eq!(tree.listings_of("/top/deep"), 1);
    assert_eq!(tree.listings_of("/top/a.txt"), 0, "a file is never listed or stat'd");
}

#[tokio::test]
async fn a_batch_keeps_climbing_across_paths_rather_than_restarting() {
    let tree = FakeTree::new().with_dir("/top", &[("a.txt", false, 10), ("b.txt", false, 5)]);
    let seen: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let on_progress = |progress: ListingProgress| seen.lock_ignore_poison().push(progress);

    let batch = scan_trees(
        &tree,
        &[PathBuf::from("/top/a.txt"), PathBuf::from("/top/b.txt")],
        Some(&on_progress),
    )
    .await
    .expect("the batch");

    assert_eq!(batch.aggregate.file_count, 2);
    assert_eq!(batch.aggregate.total_bytes, 15);
    assert_eq!(batch.per_path.len(), 2);
    let counts: Vec<usize> = seen.lock_ignore_poison().iter().map(|p| p.files).collect();
    assert_eq!(counts, vec![1, 2], "the second path continues the first's count");
}

#[tokio::test]
async fn a_batch_of_one_carries_the_top_level_type_and_a_batch_of_two_does_not() {
    // ❗ The transfer driver reads `top_level_is_directory` to decide whether a
    // paste lands INTO a folder or beside it, and an aggregate over several
    // paths has no one answer.
    let tree = FakeTree::new()
        .with_dir("/top", &[("only", true, 0), ("a.txt", false, 3)])
        .with_dir("/top/only", &[]);

    let one = scan_trees(&tree, &[PathBuf::from("/top/only")], None)
        .await
        .expect("the batch");
    assert!(one.aggregate.top_level_is_directory);

    let two = scan_trees(&tree, &[PathBuf::from("/top/only"), PathBuf::from("/top/a.txt")], None)
        .await
        .expect("the batch");
    assert!(!two.aggregate.top_level_is_directory);
}

#[tokio::test]
async fn a_destination_that_is_not_there_yet_clashes_with_nothing() {
    // ❗ Otherwise "paste into a folder I'm about to create" reads as a failure.
    let tree = FakeTree::new();

    let conflicts = scan_conflicts(&tree, &[item("a.txt", 1, false)], Path::new("/not-yet"))
        .await
        .expect("a missing destination is not an error");

    assert!(conflicts.is_empty());
}

#[tokio::test]
async fn a_conflict_carries_both_sides_so_a_dialog_can_word_it() {
    let tree = FakeTree::new().with_dir("/dest", &[("a.txt", false, 99), ("keep", true, 0)]);

    let conflicts = scan_conflicts(
        &tree,
        &[
            item("a.txt", 1, false),
            item("keep", 0, true),
            item("new.txt", 7, false),
        ],
        Path::new("/dest"),
    )
    .await
    .expect("the listing");

    assert_eq!(conflicts.len(), 2, "only the two taken names clash");
    let file = &conflicts[0];
    assert_eq!(file.source_path, "a.txt");
    assert_eq!(file.dest_path, "/dest/a.txt");
    assert_eq!((file.source_size, file.dest_size), (1, 99));
    assert!(!file.source_is_directory && !file.dest_is_directory);
    // A dir-onto-dir collision is a silent merge in the frontend, which reads
    // both flags to tell it from a real conflict.
    assert!(conflicts[1].source_is_directory && conflicts[1].dest_is_directory);
}

#[test]
fn folding_an_empty_batch_is_all_zeroes_rather_than_a_panic() {
    let batch = fold_batch(Vec::new());

    assert_eq!(batch.aggregate.file_count, 0);
    assert_eq!(batch.aggregate.total_bytes, 0);
    assert!(!batch.aggregate.top_level_is_directory);
    assert!(batch.per_path.is_empty());
}

/// ❗ A symlinked directory is ONE entry, never a subtree to walk.
///
/// Following one double-counts the target (Android's `/sdcard` and
/// `/storage/emulated/0` are the same bytes) and a link that points at an
/// ancestor turns the scan into a hang. It also has to match what the app-side
/// walker already promises: `scan_preview.rs` skips the same shape, and
/// `scan_preview_preserves_symlink_semantics` is the cell that pins it there.
#[tokio::test]
async fn a_symlinked_directory_counts_as_one_entry_and_is_never_walked() {
    let tree = FakeTree::new()
        .with_dir("/src", &[("real.txt", false, 10)])
        .with_dir("/elsewhere", &[("a.bin", false, 500), ("b.bin", false, 700)])
        .with_symlinked_dir("/src", "elsewhere");

    let scan = scan_one(&tree, Path::new("/src")).await.expect("the scan runs");

    assert_eq!(scan.dir_count, 1, "only `/src` itself is a directory here");
    assert_eq!(
        scan.file_count, 2,
        "the real file plus the link counted as the one entry it is"
    );
    assert_eq!(scan.total_bytes, 10, "the link's target contributes nothing");
    assert_eq!(
        tree.listings_of("/src/elsewhere"),
        0,
        "the walk must never list through a symlink"
    );
}
