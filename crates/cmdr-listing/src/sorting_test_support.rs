//! The entry builders the sorting suites share, and the English sort they run.
//!
//! `sorting_test`, `sorting_dir_mode_test`, and `collation_test` all need a
//! `FileEntry` with nothing interesting on it but the fields under test, and the
//! first two both build directories carrying a recursive size. Builders used by
//! ONE suite stay in that suite: the symlink in `sorting_test`, the honest-size
//! directory in `sorting_dir_mode_test`, the per-locale collators in `collation_test`.

use cmdr_fs::entry::FileEntry;

use cmdr_fs::volume::SupportedDates;

use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

use super::sorting::{EffectiveSort, SortableEntry};
use super::testing::collator_for;

/// Creates a test entry with the given name and properties.
pub(super) fn make_entry(name: &str, is_dir: bool, size: Option<u64>, modified: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: modified,
        created_at: modified, // Use same value for simplicity
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
    }
}

/// A directory whose size key is the recursive size the drive index filled in.
///
/// Shared because both the dir-mode suite (which is about this key) and the
/// comparator-parity pair in `sorting_test` (which needs a dirs-first scenario
/// to compare the two readings over) build one.
pub(super) fn make_dir_with_recursive_size(
    name: &str,
    recursive_size: Option<u64>,
    modified: Option<u64>,
) -> FileEntry {
    let mut entry = make_entry(name, true, None, modified);
    entry.recursive_size = recursive_size;
    entry
}

/// `sorting::sort_entries` the way an engine reading English sorts, on a volume
/// that records every date (so the column is the one asked for). Column and
/// directory order don't depend on the language; `collation_test` pins the
/// ones that do.
pub(super) fn sort_entries(
    entries: &mut [FileEntry],
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) {
    super::sorting::sort_entries(
        &collator_for("en"),
        entries,
        every_date(sort_by, sort_order, dir_sort_mode),
    );
}

/// `sorting::entry_comparator` the way an engine reading English compares, on a
/// volume that records every date.
pub(super) fn entry_comparator<E: SortableEntry + ?Sized>(
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> impl Fn(&E, &E) -> std::cmp::Ordering {
    super::sorting::entry_comparator(collator_for("en"), every_date(sort_by, sort_order, dir_sort_mode))
}

/// The sort as asked, on a volume that records every date.
pub(super) fn every_date(
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> EffectiveSort {
    EffectiveSort::resolve(sort_by, sort_order, dir_sort_mode, SupportedDates::ALL)
}
