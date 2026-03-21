//! Tests for listing cache helpers.

use std::path::PathBuf;

use super::caching::{
    CachedListing, LISTING_CACHE, ModifyResult, find_listings_for_path, has_entry, insert_entry_sorted,
    remove_entry_by_path, update_entry_sorted,
};
use super::metadata::FileEntry;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};

/// Creates a minimal test entry.
fn make_entry(name: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/test/{}", name),
        is_directory: is_dir,
        is_symlink: false,
        size,
        physical_size: None,
        modified_at: None,
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        icon_id: if is_dir { "dir".to_string() } else { "file".to_string() },
        extended_metadata_loaded: true,
        recursive_size: None,
        recursive_physical_size: None,
        recursive_file_count: None,
        recursive_dir_count: None,
    }
}

fn make_dir_entry(name: &str, recursive_size: Option<u64>) -> FileEntry {
    let mut e = make_entry(name, true, None);
    e.recursive_size = recursive_size;
    e
}

/// Inserts a test listing into the cache. Returns the listing_id.
fn insert_test_listing(
    id: &str,
    path: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
) -> String {
    let listing_id = id.to_string();
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.insert(
        listing_id.clone(),
        CachedListing {
            volume_id: "root".to_string(),
            path: PathBuf::from(path),
            entries,
            sort_by,
            sort_order,
            directory_sort_mode: dir_sort_mode,
        },
    );
    listing_id
}

fn cleanup_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.remove(id);
}

// ============================================================================
// find_listings_for_path tests
// ============================================================================

#[test]
fn test_find_listings_for_path_zero_matches() {
    let results = find_listings_for_path(&PathBuf::from("/nonexistent/path"));
    assert!(results.is_empty());
}

#[test]
fn test_find_listings_for_path_one_match() {
    let id = insert_test_listing(
        "find_1match",
        "/home/user/docs",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path(&PathBuf::from("/home/user/docs"));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "find_1match");
    assert_eq!(results[0].1, SortColumn::Name);
    assert_eq!(results[0].2, SortOrder::Ascending);
    assert_eq!(results[0].3, DirectorySortMode::LikeFiles);

    cleanup_listing(&id);
}

#[test]
fn test_find_listings_for_path_two_matches() {
    let id1 = insert_test_listing(
        "find_2match_a",
        "/shared/dir",
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    let id2 = insert_test_listing(
        "find_2match_b",
        "/shared/dir",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path(&PathBuf::from("/shared/dir"));
    assert_eq!(results.len(), 2);

    // Both IDs should be present (order unspecified since HashMap is unordered)
    let ids: Vec<&str> = results.iter().map(|(id, _, _, _)| id.as_str()).collect();
    assert!(ids.contains(&"find_2match_a"));
    assert!(ids.contains(&"find_2match_b"));

    cleanup_listing(&id1);
    cleanup_listing(&id2);
}

// ============================================================================
// insert_entry_sorted tests
// ============================================================================

#[test]
fn test_insert_entry_sorted_name_asc() {
    let id = insert_test_listing(
        "insert_name_asc",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            make_entry("alpha.txt", false, Some(100)),
            make_entry("gamma.txt", false, Some(100)),
        ],
    );

    // Insert "beta.txt" — should land between alpha and gamma
    let index = insert_entry_sorted("insert_name_asc", make_entry("beta.txt", false, Some(100)));
    assert_eq!(index, Some(1));

    // Verify cache contents
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("insert_name_asc").unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha.txt", "beta.txt", "gamma.txt"]);
    }

    cleanup_listing(&id);
}

#[test]
fn test_insert_entry_sorted_size_desc_dirs_first() {
    let id = insert_test_listing(
        "insert_size_desc",
        "/test",
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        vec![
            make_dir_entry("big_dir", Some(10000)),
            make_dir_entry("small_dir", Some(100)),
            make_entry("large.txt", false, Some(5000)),
            make_entry("tiny.txt", false, Some(10)),
        ],
    );

    // Insert a directory with medium recursive size — should go between big_dir and small_dir
    let index = insert_entry_sorted("insert_size_desc", make_dir_entry("mid_dir", Some(5000)));
    assert_eq!(index, Some(1));

    // Verify order
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("insert_size_desc").unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["big_dir", "mid_dir", "small_dir", "large.txt", "tiny.txt"]);
    }

    cleanup_listing(&id);
}

#[test]
fn test_insert_entry_sorted_returns_none_for_duplicate() {
    let id = insert_test_listing(
        "insert_dup",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    // Try inserting an entry with the same path
    let result = insert_entry_sorted("insert_dup", make_entry("alpha.txt", false, Some(200)));
    assert_eq!(result, None);

    // Verify cache still has just one entry
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("insert_dup").unwrap();
        assert_eq!(listing.entries.len(), 1);
    }

    cleanup_listing(&id);
}

#[test]
fn test_insert_entry_sorted_returns_none_for_missing_listing() {
    let result = insert_entry_sorted("nonexistent_listing_id", make_entry("test.txt", false, Some(100)));
    assert_eq!(result, None);
}

// ============================================================================
// remove_entry_by_path tests
// ============================================================================

#[test]
fn test_remove_entry_by_path_returns_correct_index_and_entry() {
    let id = insert_test_listing(
        "remove_ok",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            make_entry("alpha.txt", false, Some(100)),
            make_entry("beta.txt", false, Some(200)),
            make_entry("gamma.txt", false, Some(300)),
        ],
    );

    let result = remove_entry_by_path("remove_ok", &PathBuf::from("/test/beta.txt"));
    assert!(result.is_some());
    let (idx, entry) = result.unwrap();
    assert_eq!(idx, 1);
    assert_eq!(entry.name, "beta.txt");

    // Verify remaining entries
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("remove_ok").unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha.txt", "gamma.txt"]);
    }

    cleanup_listing(&id);
}

#[test]
fn test_remove_entry_by_path_returns_none_for_missing_entry() {
    let id = insert_test_listing(
        "remove_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let result = remove_entry_by_path("remove_miss", &PathBuf::from("/test/nonexistent.txt"));
    assert!(result.is_none());

    cleanup_listing(&id);
}

#[test]
fn test_remove_entry_by_path_returns_none_for_missing_listing() {
    let result = remove_entry_by_path("nonexistent_listing", &PathBuf::from("/test/foo.txt"));
    assert!(result.is_none());
}

// ============================================================================
// has_entry tests
// ============================================================================

#[test]
fn test_has_entry_true_for_existing() {
    let id = insert_test_listing(
        "has_entry_yes",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(has_entry("has_entry_yes", "/test/alpha.txt"));

    cleanup_listing(&id);
}

#[test]
fn test_has_entry_false_for_missing() {
    let id = insert_test_listing(
        "has_entry_no",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(!has_entry("has_entry_no", "/test/nonexistent.txt"));

    cleanup_listing(&id);
}

#[test]
fn test_has_entry_false_for_missing_listing() {
    assert!(!has_entry("nonexistent_listing", "/test/foo.txt"));
}

// ============================================================================
// update_entry_sorted tests
// ============================================================================

#[test]
fn test_update_entry_sorted_in_place_for_non_sort_change() {
    let id = insert_test_listing(
        "update_inplace",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            make_entry("alpha.txt", false, Some(100)),
            make_entry("beta.txt", false, Some(200)),
        ],
    );

    // Change permissions only (not sort-relevant for Name sort)
    let mut updated = make_entry("beta.txt", false, Some(200));
    updated.permissions = 0o755;

    let result = update_entry_sorted("update_inplace", updated);
    assert!(matches!(result, Some(ModifyResult::UpdatedInPlace { index: 1 })));

    // Verify the entry was updated
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("update_inplace").unwrap();
        assert_eq!(listing.entries[1].permissions, 0o755);
    }

    cleanup_listing(&id);
}

#[test]
fn test_update_entry_sorted_moved_for_size_change() {
    let id = insert_test_listing(
        "update_moved",
        "/test",
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            make_entry("small.txt", false, Some(10)),
            make_entry("medium.txt", false, Some(100)),
            make_entry("large.txt", false, Some(1000)),
        ],
    );

    // Change small.txt size to be the largest
    let updated = make_entry("small.txt", false, Some(5000));
    let result = update_entry_sorted("update_moved", updated);
    assert!(matches!(
        result,
        Some(ModifyResult::Moved {
            old_index: 0,
            new_index: 2
        })
    ));

    // Verify new order
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get("update_moved").unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["medium.txt", "large.txt", "small.txt"]);
    }

    cleanup_listing(&id);
}

#[test]
fn test_update_entry_sorted_moved_for_modified_at_change() {
    let id = insert_test_listing(
        "update_mtime",
        "/test",
        SortColumn::Modified,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            {
                let mut e = make_entry("old.txt", false, Some(100));
                e.modified_at = Some(1000);
                e
            },
            {
                let mut e = make_entry("new.txt", false, Some(100));
                e.modified_at = Some(2000);
                e
            },
        ],
    );

    // Make old.txt newer than new.txt
    let mut updated = make_entry("old.txt", false, Some(100));
    updated.modified_at = Some(3000);

    let result = update_entry_sorted("update_mtime", updated);
    assert!(matches!(
        result,
        Some(ModifyResult::Moved {
            old_index: 0,
            new_index: 1
        })
    ));

    cleanup_listing(&id);
}

#[test]
fn test_update_entry_sorted_returns_none_for_missing_entry() {
    let id = insert_test_listing(
        "update_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let result = update_entry_sorted("update_miss", make_entry("nonexistent.txt", false, Some(100)));
    assert!(result.is_none());

    cleanup_listing(&id);
}

#[test]
fn test_update_entry_sorted_returns_none_for_missing_listing() {
    let result = update_entry_sorted("nonexistent_listing", make_entry("test.txt", false, Some(100)));
    assert!(result.is_none());
}
