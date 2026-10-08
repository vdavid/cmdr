//! Whether a fresh index reading changes what a folder row shows.
//!
//! A row shows its folder's recursive size, the counts in its tooltip, whether the size is exact or a
//! lower bound, whether it's stale, whether the subtree holds symlinks, and the "size updating"
//! hourglass. [`RowSizes`] is exactly those fields, as the listing holds them now; [`RowSizes::after`]
//! is what they'd be once a reading lands, with the same rules the frontend applies. Equal means the
//! reading changes nothing on screen, so nothing is sent: most index batches under background churn
//! (a cache file written and deleted inside one flush, an mtime touch) land here.

use cmdr_index::store::DirStats;

use crate::file_system::listing::metadata::FileEntry;

/// The index-derived fields of one folder row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct RowSizes {
    recursive_size: Option<u64>,
    recursive_physical_size: Option<u64>,
    recursive_file_count: Option<u64>,
    recursive_dir_count: Option<u64>,
    recursive_has_symlinks: Option<bool>,
    recursive_size_complete: Option<bool>,
    recursive_size_stale: Option<bool>,
    /// The hourglass. Not on the cached entry (it comes from the index's in-memory pending set), so
    /// the caller passes what it last sent.
    pending: bool,
}

impl RowSizes {
    /// The row as the cached `entry` holds it, with the hourglass state last sent for it.
    pub(super) fn of(entry: &FileEntry, pending: bool) -> Self {
        Self {
            recursive_size: entry.recursive_size,
            recursive_physical_size: entry.recursive_physical_size,
            recursive_file_count: entry.recursive_file_count,
            recursive_dir_count: entry.recursive_dir_count,
            recursive_has_symlinks: entry.recursive_has_symlinks,
            recursive_size_complete: entry.recursive_size_complete,
            recursive_size_stale: entry.recursive_size_stale,
            pending,
        }
    }

    /// The row once `stats` lands. No stats (the index doesn't cover the folder) keeps the sizes and
    /// clears the hourglass, as the frontend always has: a folder that drained must not stay lit.
    pub(super) fn after(self, stats: Option<&DirStats>) -> Self {
        match stats {
            Some(stats) => Self {
                recursive_size: Some(stats.recursive_size),
                recursive_physical_size: Some(stats.recursive_physical_size),
                recursive_file_count: Some(stats.recursive_file_count),
                recursive_dir_count: Some(stats.recursive_dir_count),
                recursive_has_symlinks: Some(stats.recursive_has_symlinks),
                recursive_size_complete: Some(stats.recursive_size_complete),
                recursive_size_stale: Some(stats.recursive_size_stale),
                pending: stats.recursive_size_pending,
            },
            None => Self { pending: false, ..self },
        }
    }

    pub(super) fn pending(self) -> bool {
        self.pending
    }

    /// What the row shows now, with the hourglass lit: a folder waiting its turn in
    /// an on-demand count (`count/`). An unknown size reads as an incomplete zero,
    /// which the pane draws as `<dir>` plus the hourglass.
    pub(super) fn waiting(self, path: &str) -> DirStats {
        DirStats {
            path: path.to_string(),
            recursive_size: self.recursive_size.unwrap_or(0),
            recursive_physical_size: self.recursive_physical_size.unwrap_or(0),
            recursive_file_count: self.recursive_file_count.unwrap_or(0),
            recursive_dir_count: self.recursive_dir_count.unwrap_or(0),
            recursive_has_symlinks: self.recursive_has_symlinks.unwrap_or(false),
            recursive_size_pending: true,
            recursive_size_pending_changes_in: None,
            recursive_size_complete: false,
            recursive_size_stale: false,
        }
    }

    /// Writes the index fields onto the cached entry (the hourglass isn't an entry field).
    pub(super) fn apply_to(self, entry: &mut FileEntry) {
        entry.recursive_size = self.recursive_size;
        entry.recursive_physical_size = self.recursive_physical_size;
        entry.recursive_file_count = self.recursive_file_count;
        entry.recursive_dir_count = self.recursive_dir_count;
        entry.recursive_has_symlinks = self.recursive_has_symlinks;
        entry.recursive_size_complete = self.recursive_size_complete;
        entry.recursive_size_stale = self.recursive_size_stale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(size: u64, pending: bool) -> DirStats {
        DirStats {
            path: "/Users/me/Library".to_string(),
            recursive_size: size,
            recursive_physical_size: size,
            recursive_file_count: 10,
            recursive_dir_count: 2,
            recursive_has_symlinks: false,
            recursive_size_pending: pending,
            recursive_size_pending_changes_in: None,
            recursive_size_complete: true,
            recursive_size_stale: false,
        }
    }

    fn shown(size: u64, pending: bool) -> RowSizes {
        RowSizes::default().after(Some(&stats(size, pending)))
    }

    #[test]
    fn a_reading_that_matches_the_row_changes_nothing() {
        // The common case under churn: a cache file written and removed inside one flush.
        let row = shown(1_000, false);
        assert_eq!(row.after(Some(&stats(1_000, false))), row);
    }

    #[test]
    fn a_new_size_changes_the_row() {
        let row = shown(1_000, false);
        assert_ne!(row.after(Some(&stats(1_024, false))), row);
    }

    #[test]
    fn the_hourglass_coming_or_going_changes_the_row() {
        let row = shown(1_000, false);
        assert_ne!(row.after(Some(&stats(1_000, true))), row);
        let lit = shown(1_000, true);
        assert_ne!(lit.after(Some(&stats(1_000, false))), lit);
    }

    #[test]
    fn a_folder_the_index_lost_keeps_its_size_but_drops_the_hourglass() {
        let lit = shown(1_000, true);
        let after = lit.after(None);
        assert_ne!(after, lit, "the hourglass has to clear");
        assert!(!after.pending());
        assert_eq!(after.after(None), after, "and a second miss changes nothing more");
    }
}
