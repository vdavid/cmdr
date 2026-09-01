//! The name-collision half of `Volume::scan_for_conflicts`, shared by every
//! backend that answers it by listing the destination.
//!
//! One copy, not one per backend: the field mapping onto `ScanConflict` is what
//! the transfer dialog classifies collisions by, and four hand-kept copies drift
//! on exactly the field nobody re-tests (`dest_is_directory`, say).

use super::{ScanConflict, SourceItemInfo};
use crate::entry::FileEntry;

/// Pairs each source item with the destination entry carrying the same name.
/// `dest_entries` is the destination directory's listing; items with no
/// same-named entry produce nothing. Order follows `source_items`.
pub fn conflicts_in_listing(source_items: &[SourceItemInfo], dest_entries: &[FileEntry]) -> Vec<ScanConflict> {
    let mut conflicts = Vec::new();

    for item in source_items {
        if let Some(existing) = dest_entries.iter().find(|e| e.name == item.name) {
            let dest_modified = existing.modified_at.map(|s| s as i64);
            conflicts.push(ScanConflict {
                source_path: item.name.clone(),
                dest_path: existing.path.clone(),
                source_size: item.size,
                dest_size: existing.size.unwrap_or(0),
                source_modified: item.modified,
                dest_modified,
                source_is_directory: item.is_directory,
                dest_is_directory: existing.is_directory,
            });
        }
    }

    conflicts
}

#[cfg(test)]
#[path = "scan_conflicts_test.rs"]
mod scan_conflicts_test;
