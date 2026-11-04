//! What sits below a path of an `InMemoryVolume`, counted from the store
//! itself: complete and never stale, since the store holds every byte. The
//! browser demo's folder sizes stand on it (`crates/cmdr-listing-wasm`'s
//! `folder_sizes.rs`), so the demo shows real sizes without the drive index.

use std::path::{Path, PathBuf};

use super::InMemoryVolume;
use crate::ignore_poison::RwLockIgnorePoison;

/// The totals of everything below one path, counted the way the drive index
/// counts them (`cmdr-index`'s aggregator): every non-folder entry, symlinks
/// included, is a file carrying its own size; a symlink below sets
/// `has_symlinks`; the path itself isn't counted.
// DEFAULT-OK: all zero is an empty folder's honest answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubtreeTotals {
    /// Logical bytes over the subtree.
    pub size: u64,
    /// On-disk bytes over the subtree: a file's `physical_size`, or its logical
    /// size where the store models none (memory holds what a file claims).
    pub physical_size: u64,
    /// Files anywhere below.
    pub files: u64,
    /// Folders anywhere below.
    pub dirs: u64,
    /// Whether a symlink sits anywhere below.
    pub has_symlinks: bool,
}

/// What the store holds at a path, for [`InMemoryVolume::subtree_totals`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldAt {
    /// A folder (the root included), with everything below it.
    Folder(SubtreeTotals),
    /// A file or a symlink, with its own size.
    Leaf {
        /// Its logical size.
        size: u64,
    },
}

impl InMemoryVolume {
    /// What the store holds at each of `paths`, in one read of the store:
    /// `None` for a path it doesn't hold. O(entries × paths), so a caller
    /// batches its questions and memoizes the answers.
    pub fn subtree_totals(&self, paths: &[PathBuf]) -> Vec<Option<HeldAt>> {
        let entries = self.entries.read_ignore_poison();
        paths
            .iter()
            .map(|path| {
                let path = self.normalize(path);
                let is_root = path == self.root || path == Path::new("/");
                match entries.get(&path) {
                    Some(held) if !held.metadata.is_directory || held.metadata.is_symlink => Some(HeldAt::Leaf {
                        size: held.metadata.size.unwrap_or(0),
                    }),
                    None if !is_root => None,
                    _ => {
                        let mut totals = SubtreeTotals::default();
                        for (below, held) in entries.iter() {
                            if below == &path || !below.starts_with(&path) {
                                continue;
                            }
                            let meta = &held.metadata;
                            if meta.is_directory && !meta.is_symlink {
                                totals.dirs += 1;
                                continue;
                            }
                            let size = meta.size.unwrap_or(0);
                            totals.files += 1;
                            totals.size += size;
                            totals.physical_size += meta.physical_size.unwrap_or(size);
                            totals.has_symlinks |= meta.is_symlink;
                        }
                        Some(HeldAt::Folder(totals))
                    }
                }
            })
            .collect()
    }
}
