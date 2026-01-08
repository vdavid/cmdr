//! Tests for hidden file filtering.
//!
//! These tests verify that the `include_hidden` parameter correctly filters
//! files starting with "." from directory listings.

use super::operations::{
    CachedListing, FileEntry, LISTING_CACHE, SortColumn, SortOrder, find_file_index, get_file_at, get_file_range,
    get_total_count, list_directory_end,
};
use super::volume::{InMemoryVolume, Volume};
use std::path::Path;
use std::sync::Arc;

/// Creates a test entry with the given name.
fn make_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/{}", name),
        is_directory: is_dir,
        is_symlink: false,
        size: if is_dir { None } else { Some(100) },
        modified_at: Some(1_700_000_000),
        created_at: Some(1_700_000_000),
        added_at: None,
        opened_at: None,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        icon_id: if is_dir { "dir".to_string() } else { "file".to_string() },
        extended_metadata_loaded: true,
    }
}

/// Creates a test fixture with a mix of hidden and visible files.
fn create_test_volume() -> Arc<InMemoryVolume> {
    let entries = vec![
        make_entry(".hidden_dir", true),
        make_entry(".hidden_file", false),
        make_entry(".gitignore", false),
        make_entry("Documents", true),
        make_entry("Downloads", true),
        make_entry("file.txt", false),
        make_entry("readme.md", false),
    ];
    Arc::new(InMemoryVolume::with_entries("TestVolume", entries))
}

// ============================================================================
// Tests for get_total_count with include_hidden
// ============================================================================

#[test]
fn test_get_total_count_with_hidden_includes_all() {
    let volume = create_test_volume();

    // Manually insert into listing cache (simulating list_directory_start)
    let listing_id = "test-total-count-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let count = get_total_count(&listing_id, true).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    // All 7 entries should be counted
    assert_eq!(count, 7, "Should count all entries including hidden");
}

#[test]
fn test_get_total_count_without_hidden_excludes_dot_files() {
    let volume = create_test_volume();

    let listing_id = "test-total-count-no-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let count = get_total_count(&listing_id, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    // Only 4 visible entries: Documents, Downloads, file.txt, readme.md
    assert_eq!(count, 4, "Should only count non-hidden entries");
}

// ============================================================================
// Tests for get_file_range with include_hidden
// ============================================================================

#[test]
fn test_get_file_range_with_hidden_returns_all() {
    let volume = create_test_volume();

    let listing_id = "test-range-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let range = get_file_range(&listing_id, 0, 10, true).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert_eq!(range.len(), 7, "Should return all 7 entries");

    // Verify hidden files are present
    let names: Vec<&str> = range.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&".hidden_dir"), "Should include .hidden_dir");
    assert!(names.contains(&".hidden_file"), "Should include .hidden_file");
    assert!(names.contains(&".gitignore"), "Should include .gitignore");
}

#[test]
fn test_get_file_range_without_hidden_excludes_dot_files() {
    let volume = create_test_volume();

    let listing_id = "test-range-no-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let range = get_file_range(&listing_id, 0, 10, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert_eq!(range.len(), 4, "Should return only 4 visible entries");

    // Verify hidden files are NOT present
    let names: Vec<&str> = range.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&".hidden_dir"), "Should not include .hidden_dir");
    assert!(!names.contains(&".hidden_file"), "Should not include .hidden_file");
    assert!(!names.contains(&".gitignore"), "Should not include .gitignore");

    // Verify visible files ARE present
    assert!(names.contains(&"Documents"), "Should include Documents");
    assert!(names.contains(&"file.txt"), "Should include file.txt");
}

#[test]
fn test_get_file_range_pagination_respects_hidden_filter() {
    let volume = create_test_volume();

    let listing_id = "test-range-pagination".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    // Get first 2 visible entries
    let page1 = get_file_range(&listing_id, 0, 2, false).unwrap();
    // Get next 2 visible entries
    let page2 = get_file_range(&listing_id, 2, 2, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert_eq!(page1.len(), 2, "First page should have 2 entries");
    assert_eq!(page2.len(), 2, "Second page should have 2 entries");

    // Verify no hidden files in either page
    for entry in page1.iter().chain(page2.iter()) {
        assert!(
            !entry.name.starts_with('.'),
            "Found hidden file {} in non-hidden listing",
            entry.name
        );
    }
}

// ============================================================================
// Tests for find_file_index with include_hidden
// ============================================================================

#[test]
fn test_find_file_index_hidden_file_with_hidden_enabled() {
    let volume = create_test_volume();

    let listing_id = "test-find-hidden-enabled".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let index = find_file_index(&listing_id, ".gitignore", true).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert!(index.is_some(), "Should find .gitignore with hidden enabled");
}

#[test]
fn test_find_file_index_hidden_file_with_hidden_disabled() {
    let volume = create_test_volume();

    let listing_id = "test-find-hidden-disabled".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let index = find_file_index(&listing_id, ".gitignore", false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert!(index.is_none(), "Should NOT find .gitignore with hidden disabled");
}

#[test]
fn test_find_file_index_visible_file_index_changes_with_hidden_setting() {
    let volume = create_test_volume();

    let listing_id = "test-find-visible-index-changes".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    // Find "Documents" with hidden enabled (should be after hidden dirs)
    let index_with_hidden = find_file_index(&listing_id, "Documents", true).unwrap();
    // Find "Documents" with hidden disabled (should be at the start)
    let index_without_hidden = find_file_index(&listing_id, "Documents", false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    // With hidden files, hidden dirs come first, then Documents
    assert!(
        index_with_hidden.unwrap() > 0,
        "Documents should not be first when hidden dirs are shown"
    );
    // Without hidden files, Documents should be first (or early)
    assert_eq!(
        index_without_hidden.unwrap(),
        0,
        "Documents should be first when hidden files are excluded"
    );
}

// ============================================================================
// Tests for get_file_at with include_hidden
// ============================================================================

#[test]
fn test_get_file_at_index_0_with_hidden_enabled() {
    let volume = create_test_volume();

    let listing_id = "test-at-0-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let entry = get_file_at(&listing_id, 0, true).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    // With hidden enabled, first entry should be a hidden dir (sorted alphabetically)
    let entry = entry.expect("Should have entry at index 0");
    assert!(
        entry.name.starts_with('.'),
        "First entry with hidden enabled should be a hidden file, got {}",
        entry.name
    );
}

#[test]
fn test_get_file_at_index_0_with_hidden_disabled() {
    let volume = create_test_volume();

    let listing_id = "test-at-0-no-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let entry = get_file_at(&listing_id, 0, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    // With hidden disabled, first entry should be Documents (first visible dir)
    let entry = entry.expect("Should have entry at index 0");
    assert!(
        !entry.name.starts_with('.'),
        "First entry with hidden disabled should NOT be a hidden file"
    );
    assert_eq!(entry.name, "Documents", "First visible entry should be Documents");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_directory_with_only_hidden_files() {
    let entries = vec![
        make_entry(".bashrc", false),
        make_entry(".profile", false),
        make_entry(".zshrc", false),
    ];
    let volume = Arc::new(InMemoryVolume::with_entries("AllHidden", entries));

    let listing_id = "test-all-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let count_with = get_total_count(&listing_id, true).unwrap();
    let count_without = get_total_count(&listing_id, false).unwrap();
    let range_without = get_file_range(&listing_id, 0, 10, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert_eq!(count_with, 3, "All 3 hidden files should be counted");
    assert_eq!(count_without, 0, "No visible files to count");
    assert!(range_without.is_empty(), "No visible files to return");
}

#[test]
fn test_directory_with_no_hidden_files() {
    let entries = vec![make_entry("Documents", true), make_entry("file.txt", false)];
    let volume = Arc::new(InMemoryVolume::with_entries("NoHidden", entries));

    let listing_id = "test-no-hidden".to_string();
    let entries = volume.list_directory(Path::new("")).unwrap();

    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: "test".to_string(),
                path: std::path::PathBuf::from("/"),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
            },
        );
    }

    let count_with = get_total_count(&listing_id, true).unwrap();
    let count_without = get_total_count(&listing_id, false).unwrap();

    // Cleanup
    list_directory_end(&listing_id);

    assert_eq!(count_with, 2, "Both files should be counted");
    assert_eq!(count_without, 2, "Both files should be counted (none are hidden)");
}
