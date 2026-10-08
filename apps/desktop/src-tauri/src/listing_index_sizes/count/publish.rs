//! Putting a count's readings where the pane reads them: the listing cache first,
//! then `listing-index-sizes-changed`, exactly like the drive index's, so the
//! size cell, its hourglass, the status bar, and a size sort need no new path.

use cmdr_fs::volume::ListingProgress;
use cmdr_index::store::DirStats;

use super::jobs;
use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;
use crate::ignore_poison::RwLockIgnorePoison;
use crate::listing_index_sizes::refresh::RowSizes;
use crate::listing_index_sizes::{FolderSizes, ListingIndexSizesChanged};

pub(super) type Sink<'a> = &'a (dyn Fn(ListingIndexSizesChanged) + Sync);

/// Where a folder's count stands, for its reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Walk {
    /// Being walked: the running total, with the hourglass.
    Running,
    /// Done and everything read: the exact size.
    Done,
    /// Stopped, or done with parts it couldn't read: a lower bound, no hourglass.
    Partial,
}

pub(super) fn reading(folder: &str, progress: ListingProgress, physical: u64, walk: Walk) -> DirStats {
    DirStats {
        path: folder.to_string(),
        recursive_size: progress.bytes,
        recursive_physical_size: physical,
        recursive_file_count: progress.files as u64,
        recursive_dir_count: progress.dirs as u64,
        recursive_has_symlinks: false,
        recursive_size_pending: walk == Walk::Running,
        recursive_size_pending_changes_in: None,
        recursive_size_complete: walk == Walk::Done,
        recursive_size_stale: false,
    }
}

/// What a cache write found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wrote {
    /// The folder's row took the reading.
    Row,
    /// The listing is open but no longer holds the folder.
    NoRow,
    /// The listing is gone: its pane moved on.
    NoListing,
}

pub(super) fn listing_is_open(listing_id: &str) -> bool {
    LISTING_CACHE.read_ignore_poison().get(listing_id).is_some()
}

/// Writes `stats` onto the folder's cached row, then sends it to the pane. The
/// row's size is the count's from here on (`jobs::mark_manual`).
pub(super) fn publish(listing_id: &str, folder: &str, stats: DirStats, sink: Sink<'_>) -> Wrote {
    let wrote = write_to_cache(listing_id, folder, |entry| {
        RowSizes::of(entry, false).after(Some(&stats)).apply_to(entry);
    });
    if wrote == Wrote::Row {
        jobs::mark_manual(listing_id, folder);
        sink(ListingIndexSizesChanged {
            listing_id: listing_id.to_string(),
            full: false,
            folders: vec![FolderSizes {
                path: folder.to_string(),
                stats: Some(stats),
            }],
            current_dir_changed: false,
            current_dir: None,
        });
    }
    wrote
}

/// Puts back the sizes a row showed before the count touched it (a walk that
/// never ran, or couldn't read the folder). The pane re-reads its window from the
/// cache (`full`), since "no size" isn't a reading an event can carry.
pub(super) fn restore(listing_id: &str, folder: &str, before: RowSizes, before_manual: bool, sink: Sink<'_>) {
    if before_manual {
        jobs::mark_manual(listing_id, folder);
    } else {
        jobs::unmark_manual(listing_id, folder);
    }
    if write_to_cache(listing_id, folder, |entry| before.apply_to(entry)) == Wrote::Row {
        sink(ListingIndexSizesChanged {
            listing_id: listing_id.to_string(),
            full: true,
            folders: Vec::new(),
            current_dir_changed: false,
            current_dir: None,
        });
    }
}

fn write_to_cache(listing_id: &str, folder: &str, write: impl FnOnce(&mut FileEntry)) -> Wrote {
    let mut cache = LISTING_CACHE.write_ignore_poison();
    let Some(listing) = cache.get_mut(listing_id) else {
        return Wrote::NoListing;
    };
    let mut write = Some(write);
    let mut wrote = Wrote::NoRow;
    listing.update_index_sizes_by_path(&[folder.to_string()], |_, entry| {
        if let Some(write) = write.take() {
            write(entry);
            wrote = Wrote::Row;
        }
    });
    wrote
}
