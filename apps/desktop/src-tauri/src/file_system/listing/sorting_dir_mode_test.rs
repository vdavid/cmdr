//! How a directory sorts against the files beside it.
//!
//! `DirectorySortMode` is the policy: `AlwaysByName` keeps directories in name
//! order whatever column is active, `LikeFiles` ranks them by the same key the
//! files use. The size key is a directory's `recursive_size`, which the drive
//! index fills in and which carries its own honest-size coverage flags, so
//! "unknown" has to stay distinct from "empty" and from a lower bound.
//!
//! The column-ordering suite is `sorting_test`; name order itself is
//! `collation_test`.

use super::sorting::DirectorySortMode;
use super::sorting::sort_entries;
use super::sorting_test_support::{make_dir_with_recursive_size, make_entry};
use super::{FileEntry, SortColumn, SortOrder};

// ============================================================================
// Directory sort mode tests
// ============================================================================

#[test]
fn test_dir_sort_like_files_by_recursive_size_ascending() {
    let mut entries = vec![
        make_dir_with_recursive_size("big_dir", Some(10000), None),
        make_dir_with_recursive_size("small_dir", Some(100), None),
        make_dir_with_recursive_size("medium_dir", Some(5000), None),
        make_entry("file.txt", false, Some(500), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["small_dir", "medium_dir", "big_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_like_files_by_recursive_size_descending() {
    let mut entries = vec![
        make_dir_with_recursive_size("big_dir", Some(10000), None),
        make_dir_with_recursive_size("small_dir", Some(100), None),
        make_dir_with_recursive_size("medium_dir", Some(5000), None),
        make_entry("file.txt", false, Some(500), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["big_dir", "medium_dir", "small_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_like_files_size_none_sorts_last() {
    let mut entries = vec![
        make_dir_with_recursive_size("unknown_dir", None, None),
        make_dir_with_recursive_size("known_dir", Some(5000), None),
        make_dir_with_recursive_size("also_unknown", None, None),
        make_entry("file.txt", false, Some(500), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // known_dir first, then unknown dirs by name, then files
    assert_eq!(names, vec!["known_dir", "also_unknown", "unknown_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_like_files_size_none_sorts_last_descending() {
    let mut entries = vec![
        make_dir_with_recursive_size("unknown_dir", None, None),
        make_dir_with_recursive_size("known_big", Some(10000), None),
        make_dir_with_recursive_size("known_small", Some(100), None),
        make_dir_with_recursive_size("also_unknown", None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Descending: big first, small second, then unknown dirs last (by name, reversed)
    assert_eq!(names, vec!["known_big", "known_small", "unknown_dir", "also_unknown"]);
}

/// Build a dir entry with the honest-size coverage flags set, for sort tests.
fn make_dir_honest(name: &str, recursive_size: Option<u64>, complete: Option<bool>) -> FileEntry {
    let mut entry = make_entry(name, true, None, None);
    entry.recursive_size = recursive_size;
    entry.recursive_size_complete = complete;
    entry
}

#[test]
fn test_dir_sort_unknown_distinct_from_empty_and_lower_bound() {
    // The three honest-size classes must sort distinctly, NOT re-conflate:
    // - genuinely-empty (`complete=true`, size 0) is a KNOWN 0 → sorts first (smallest known)
    // - lower-bound (`complete=false`, size 5000, rendered `≥`) sorts by its floor 5000
    // - exact (`complete=true`, size 100) sorts by 100
    // - unknown (`complete=false`, size 0, rendered `—`) AND not-enriched (None)
    //   both sort LAST, by name.
    let mut entries = vec![
        make_dir_honest("unknown_dash", Some(0), Some(false)),   // `—`
        make_dir_honest("lower_bound", Some(5000), Some(false)), // `≥5000`
        make_dir_honest("empty", Some(0), Some(true)),           // exact 0 bytes
        make_dir_honest("exact_small", Some(100), Some(true)),   // exact 100
        make_dir_honest("not_enriched", None, None),             // None
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Known by value (0, 100, 5000), then unknowns last by name (not_enriched, unknown_dash).
    assert_eq!(
        names,
        vec!["empty", "exact_small", "lower_bound", "not_enriched", "unknown_dash"]
    );
}

#[test]
fn test_dir_sort_unknown_sorts_last_descending() {
    // Even descending, the unknown `—` and not-enriched dirs stay LAST (after the
    // known values, which reverse among themselves), never jumping to the top.
    let mut entries = vec![
        make_dir_honest("unknown_dash", Some(0), Some(false)),
        make_dir_honest("empty", Some(0), Some(true)),
        make_dir_honest("big", Some(9000), Some(true)),
        make_dir_honest("lower_bound", Some(5000), Some(false)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Known descending: big(9000), lower_bound(5000), empty(0); unknown last.
    assert_eq!(names, vec!["big", "lower_bound", "empty", "unknown_dash"]);
}

#[test]
fn test_dir_sort_always_by_name_ignores_size() {
    let mut entries = vec![
        make_dir_with_recursive_size("zebra_dir", Some(100), None),
        make_dir_with_recursive_size("alpha_dir", Some(10000), None),
        make_entry("file.txt", false, Some(500), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::AlwaysByName,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Dirs sorted by name ascending, then files by size
    assert_eq!(names, vec!["alpha_dir", "zebra_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_always_by_name_ignores_modified() {
    let mut entries = vec![
        make_dir_with_recursive_size("zebra_dir", None, Some(1700000003)),
        make_dir_with_recursive_size("alpha_dir", None, Some(1700000001)),
        make_entry("file.txt", false, Some(500), Some(1700000002)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Modified,
        SortOrder::Ascending,
        DirectorySortMode::AlwaysByName,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Dirs sorted by name ascending (ignoring modified), then files by modified
    assert_eq!(names, vec!["alpha_dir", "zebra_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_always_by_name_descending() {
    let mut entries = vec![
        make_dir_with_recursive_size("alpha_dir", Some(10000), None),
        make_dir_with_recursive_size("zebra_dir", Some(100), None),
        make_entry("file.txt", false, Some(500), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Dirs sorted by name descending (sort order applies), then files by size descending
    assert_eq!(names, vec!["zebra_dir", "alpha_dir", "file.txt"]);
}

#[test]
fn test_dir_sort_like_files_equal_size_secondary_name() {
    let mut entries = vec![
        make_dir_with_recursive_size("zebra_dir", Some(5000), None),
        make_dir_with_recursive_size("alpha_dir", Some(5000), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // Equal size → secondary sort by name ascending
    assert_eq!(names, vec!["alpha_dir", "zebra_dir"]);
}
