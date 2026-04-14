//! Tests for `get_listing_stats()` — total/visible counts, sizes, and selection sums.

use super::caching::{CachedListing, LISTING_CACHE};
use super::operations::{get_listing_stats, list_directory_end};
use super::sorting::DirectorySortMode;
use super::{FileEntry, SortColumn, SortOrder};

/// Creates a test entry with configurable sizes.
fn make_entry(name: &str, is_dir: bool, size: Option<u64>, physical_size: Option<u64>) -> FileEntry {
    let mut entry = FileEntry {
        size: if is_dir { None } else { size },
        physical_size: if is_dir { None } else { physical_size },
        recursive_size: if is_dir { size } else { None },
        recursive_physical_size: if is_dir { physical_size } else { None },
        modified_at: Some(1_700_000_000),
        created_at: Some(1_700_000_000),
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
    };
    // Ensure directories don't have file-level size and vice versa
    if is_dir {
        entry.size = None;
        entry.physical_size = None;
    } else {
        entry.recursive_size = None;
        entry.recursive_physical_size = None;
    }
    entry
}

/// Inserts entries into the listing cache and returns the listing ID.
fn insert_test_listing(id: &str, entries: Vec<FileEntry>) -> String {
    let listing_id = id.to_string();
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.insert(
        listing_id.clone(),
        CachedListing {
            volume_id: "test".to_string(),
            path: std::path::PathBuf::from("/"),
            entries,
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            sequence: std::sync::atomic::AtomicU64::new(0),
        },
    );
    listing_id
}

// ============================================================================
// Basic stats
// ============================================================================

#[test]
fn test_stats_mixed_files_and_dirs() {
    let entries = vec![
        make_entry("Documents", true, Some(50_000), Some(52_000)),
        make_entry("Photos", true, Some(100_000), Some(104_000)),
        make_entry("notes.txt", false, Some(1_024), Some(4_096)),
        make_entry("report.pdf", false, Some(2_048), Some(4_096)),
        make_entry("tiny.log", false, Some(10), Some(4_096)),
    ];
    let listing_id = insert_test_listing("test-stats-mixed", entries);

    let stats = get_listing_stats(&listing_id, true, None).unwrap();

    list_directory_end(&listing_id);

    assert_eq!(stats.total_dirs, 2);
    assert_eq!(stats.total_files, 3);
    assert_eq!(stats.total_size, 50_000 + 100_000 + 1_024 + 2_048 + 10);
    assert_eq!(stats.total_physical_size, 52_000 + 104_000 + 4_096 + 4_096 + 4_096);
    assert!(stats.selected_files.is_none());
    assert!(stats.selected_dirs.is_none());
    assert!(stats.selected_size.is_none());
    assert!(stats.selected_physical_size.is_none());
}

// ============================================================================
// Hidden file filtering
// ============================================================================

#[test]
fn test_stats_hidden_files_excluded() {
    let entries = vec![
        make_entry(".config", true, Some(5_000), Some(8_192)),
        make_entry(".bashrc", false, Some(512), Some(4_096)),
        make_entry("Documents", true, Some(50_000), Some(52_000)),
        make_entry("readme.md", false, Some(1_024), Some(4_096)),
    ];
    let listing_id = insert_test_listing("test-stats-hidden-excluded", entries);

    let stats_all = get_listing_stats(&listing_id, true, None).unwrap();
    let stats_visible = get_listing_stats(&listing_id, false, None).unwrap();

    list_directory_end(&listing_id);

    // With hidden: all 4 entries
    assert_eq!(stats_all.total_dirs, 2);
    assert_eq!(stats_all.total_files, 2);
    assert_eq!(stats_all.total_size, 5_000 + 512 + 50_000 + 1_024);

    // Without hidden: only Documents + readme.md
    assert_eq!(stats_visible.total_dirs, 1);
    assert_eq!(stats_visible.total_files, 1);
    assert_eq!(stats_visible.total_size, 50_000 + 1_024);
    assert_eq!(stats_visible.total_physical_size, 52_000 + 4_096);
}

// ============================================================================
// Selection stats
// ============================================================================

#[test]
fn test_stats_with_selection() {
    let entries = vec![
        make_entry("Documents", true, Some(50_000), Some(52_000)),
        make_entry("Photos", true, Some(100_000), Some(104_000)),
        make_entry("notes.txt", false, Some(1_024), Some(4_096)),
        make_entry("report.pdf", false, Some(2_048), Some(4_096)),
    ];
    let listing_id = insert_test_listing("test-stats-selection", entries);

    // Select indices 0 (Documents) and 2 (notes.txt)
    let stats = get_listing_stats(&listing_id, true, Some(&[0, 2])).unwrap();

    list_directory_end(&listing_id);

    // Totals cover all entries
    assert_eq!(stats.total_dirs, 2);
    assert_eq!(stats.total_files, 2);

    // Selection covers only the two selected entries
    assert_eq!(stats.selected_dirs, Some(1));
    assert_eq!(stats.selected_files, Some(1));
    assert_eq!(stats.selected_size, Some(50_000 + 1_024));
    assert_eq!(stats.selected_physical_size, Some(52_000 + 4_096));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_stats_empty_directory() {
    let listing_id = insert_test_listing("test-stats-empty", vec![]);

    let stats = get_listing_stats(&listing_id, true, None).unwrap();

    list_directory_end(&listing_id);

    assert_eq!(stats.total_dirs, 0);
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_size, 0);
    assert_eq!(stats.total_physical_size, 0);
    assert!(stats.selected_files.is_none());
}

#[test]
fn test_stats_all_hidden_without_hidden_flag() {
    let entries = vec![
        make_entry(".git", true, Some(200_000), Some(204_800)),
        make_entry(".gitignore", false, Some(256), Some(4_096)),
        make_entry(".env", false, Some(128), Some(4_096)),
    ];
    let listing_id = insert_test_listing("test-stats-all-hidden", entries);

    let stats = get_listing_stats(&listing_id, false, None).unwrap();

    list_directory_end(&listing_id);

    // Everything is hidden, so visible stats are all zero
    assert_eq!(stats.total_dirs, 0);
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_size, 0);
    assert_eq!(stats.total_physical_size, 0);
}

#[test]
fn test_stats_selection_with_out_of_bounds_index_is_ignored() {
    let entries = vec![make_entry("file.txt", false, Some(1_000), Some(4_096))];
    let listing_id = insert_test_listing("test-stats-oob-selection", entries);

    // Index 0 is valid, index 99 is out of bounds
    let stats = get_listing_stats(&listing_id, true, Some(&[0, 99])).unwrap();

    list_directory_end(&listing_id);

    // Only the valid index should be counted
    assert_eq!(stats.selected_files, Some(1));
    assert_eq!(stats.selected_dirs, Some(0));
    assert_eq!(stats.selected_size, Some(1_000));
}

#[test]
fn test_stats_entries_without_sizes() {
    // Entries where size is None (e.g., network volumes that don't report sizes)
    let entries = vec![
        make_entry("remote_dir", true, None, None),
        make_entry("unknown.dat", false, None, None),
    ];
    let listing_id = insert_test_listing("test-stats-no-sizes", entries);

    let stats = get_listing_stats(&listing_id, true, Some(&[0, 1])).unwrap();

    list_directory_end(&listing_id);

    assert_eq!(stats.total_dirs, 1);
    assert_eq!(stats.total_files, 1);
    assert_eq!(stats.total_size, 0);
    assert_eq!(stats.total_physical_size, 0);
    assert_eq!(stats.selected_size, Some(0));
    assert_eq!(stats.selected_physical_size, Some(0));
}
