//! The demo's folder sizes: a fold over the in-memory volume
//! (`InMemoryVolume::subtree_totals`), answering every size question the app's
//! drive index answers, through the same seams. The listing engine asks it as
//! its `ListingIndex` (pane rows), the write engine as its `OperationIndex`
//! (dialog sizes and progress totals), and the `get_dir_stats*` commands read
//! it for the status bar and the `..` row.
//!
//! The volume holds every byte, so each answer is complete and never stale. Answers
//! are memoized per path; a change the volume reports drops only the changed
//! entry's subtree and its ancestors, so a copy recounts the folders it touched
//! and nothing else.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{DirectoryChange, HeldAt, InMemoryVolume, SubtreeTotals};
use cmdr_listing::host::ListingIndex;
use cmdr_operation_log::types::SearchCoverageReason;
use cmdr_operations::KnownSize;
use cmdr_operations::host::{BufferedLeaves, ExpectedTotals, OperationIndex};
use serde::Serialize;

/// The fold over one in-memory volume, memoized per path.
pub struct FolderSizes {
    volume: Arc<InMemoryVolume>,
    memo: Mutex<HashMap<PathBuf, Option<HeldAt>>>,
}

impl FolderSizes {
    /// Sizes read off `volume`.
    pub fn new(volume: Arc<InMemoryVolume>) -> Self {
        Self {
            volume,
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// What the volume holds at each of `paths`, from the memo where it can,
    /// asking the volume once for the rest.
    pub fn held_at(&self, paths: &[PathBuf]) -> Vec<Option<HeldAt>> {
        let missing: Vec<PathBuf> = {
            let memo = self.memo.lock_ignore_poison();
            paths.iter().filter(|p| !memo.contains_key(*p)).cloned().collect()
        };
        // Asked with the memo unlocked: the fold reads the whole store.
        let answers = if missing.is_empty() {
            Vec::new()
        } else {
            self.volume.subtree_totals(&missing)
        };
        let mut memo = self.memo.lock_ignore_poison();
        memo.extend(missing.into_iter().zip(answers));
        paths.iter().map(|p| memo.get(p).copied().flatten()).collect()
    }

    /// The folder totals at `path`, or `None` when it's no folder.
    fn folder(&self, path: &Path) -> Option<SubtreeTotals> {
        match self.held_at(&[path.to_path_buf()]).pop().flatten()? {
            HeldAt::Folder(totals) => Some(totals),
            HeldAt::Leaf { .. } => None,
        }
    }

    /// Forgets what `change` under `parent` can have moved: the changed entry's
    /// own subtree and every folder from `parent` up to the root. Called BEFORE
    /// the panes hear the change, so their enrich reads fresh totals (the app's
    /// composer feeds its index first for the same reason).
    pub fn changed(&self, parent: &Path, change: &DirectoryChange) {
        let mut memo = self.memo.lock_ignore_poison();
        let mut forget_subtree = |root: &Path| memo.retain(|path, _| !path.starts_with(root));
        match change {
            DirectoryChange::Added(entry) | DirectoryChange::Modified(entry) => {
                forget_subtree(Path::new(&entry.path));
            }
            DirectoryChange::Removed(name) => forget_subtree(&parent.join(name)),
            DirectoryChange::Renamed { old_name, new_entry } => {
                forget_subtree(&parent.join(old_name));
                forget_subtree(Path::new(&new_entry.path));
            }
            DirectoryChange::Replaced(_) | DirectoryChange::FullRefresh => forget_subtree(parent),
        }
        for ancestor in parent.ancestors() {
            memo.remove(ancestor);
        }
    }

    /// The `get_dir_stats_batch` answer for `paths`: a folder's totals, `null`
    /// for anything else, as the app's index answers.
    pub fn dir_stats(&self, paths: &[String]) -> Vec<Option<DirStats>> {
        let asked: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        self.held_at(&asked)
            .into_iter()
            .zip(paths)
            .map(|(held, path)| match held? {
                HeldAt::Folder(totals) => Some(DirStats::of(path.clone(), totals)),
                HeldAt::Leaf { .. } => None,
            })
            .collect()
    }
}

/// `DirStats` as the app's `get_dir_stats` sends it (`cmdr-index`'s type);
/// `folder_sizes_test.rs` pins the keys against `bindings.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStats {
    path: String,
    recursive_size: u64,
    recursive_physical_size: u64,
    recursive_file_count: u64,
    recursive_dir_count: u64,
    recursive_has_symlinks: bool,
    /// Never: nothing indexes behind the store's back.
    recursive_size_pending: bool,
    /// Always: the store holds the whole subtree.
    recursive_size_complete: bool,
    /// Never: every change recounts its folders.
    recursive_size_stale: bool,
}

impl DirStats {
    fn of(path: String, totals: SubtreeTotals) -> Self {
        Self {
            path,
            recursive_size: totals.size,
            recursive_physical_size: totals.physical_size,
            recursive_file_count: totals.files,
            recursive_dir_count: totals.dirs,
            recursive_has_symlinks: totals.has_symlinks,
            recursive_size_pending: false,
            recursive_size_complete: true,
            recursive_size_stale: false,
        }
    }
}

impl ListingIndex for FolderSizes {
    /// Writes the seven `recursive_*` fields on folders that aren't symlinks,
    /// and nothing else: the seam's contract.
    fn enrich(&self, _volume_id: &str, entries: &mut [FileEntry]) {
        let folders: Vec<usize> = (0..entries.len())
            .filter(|&i| entries[i].is_directory && !entries[i].is_symlink)
            .collect();
        if folders.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = folders.iter().map(|&i| PathBuf::from(&entries[i].path)).collect();
        for (i, held) in folders.into_iter().zip(self.held_at(&paths)) {
            let Some(HeldAt::Folder(totals)) = held else {
                continue;
            };
            let entry = &mut entries[i];
            entry.recursive_size = Some(totals.size);
            entry.recursive_physical_size = Some(totals.physical_size);
            entry.recursive_file_count = Some(totals.files);
            entry.recursive_dir_count = Some(totals.dirs);
            entry.recursive_has_symlinks = Some(totals.has_symlinks);
            entry.recursive_size_complete = Some(true);
            entry.recursive_size_stale = Some(false);
        }
    }

    /// Nothing to verify: the fold is the store.
    fn verify_directory(&self, _volume_id: &str, _path: &Path) {}
}

impl OperationIndex for FolderSizes {
    fn expected_totals(&self, sources: &[PathBuf]) -> Option<ExpectedTotals> {
        let mut sum = ExpectedTotals { files: 0, bytes: 0 };
        for held in self.held_at(sources) {
            match held? {
                HeldAt::Folder(totals) => {
                    sum.files += totals.files;
                    sum.bytes += totals.size;
                }
                HeldAt::Leaf { size } => {
                    sum.files += 1;
                    sum.bytes += size;
                }
            }
        }
        Some(sum)
    }

    fn recursive_size(&self, path: &Path) -> Option<KnownSize> {
        self.folder(path).map(|totals| KnownSize::Exact { bytes: totals.size })
    }

    /// Downgraded: the fold counts, it keeps no leaf list a journal could
    /// search, and no drive index stands behind it.
    fn search_leaves(&self, _volume_id: &str, _abs_source: &Path, _cap: usize) -> BufferedLeaves {
        BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent)
    }
}
