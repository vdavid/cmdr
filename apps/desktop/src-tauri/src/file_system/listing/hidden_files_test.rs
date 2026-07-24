//! Tests for hidden file filtering.
//!
//! These tests verify that the `include_hidden` parameter correctly filters
//! files starting with "." from directory listings.

use super::FileEntry;
use super::caching_test_support::{TestListing, TestListingGuard};
use super::operations::{find_file_index, find_file_indices, get_file_at, get_file_range, get_total_count};
use crate::file_system::volume::{InMemoryVolume, Volume};
use std::path::Path;
use std::sync::Arc;

/// Creates a test entry with the given name.
fn make_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        size: if is_dir { None } else { Some(100) },
        modified_at: Some(1_700_000_000),
        created_at: Some(1_700_000_000),
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
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

/// Caches `entries` under a unique listing id, owned by the caller: the guard
/// tears the entry down on drop, including when an assertion panics first.
fn insert_test_listing(tag: &str, entries: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new().volume("test").path("/").entries(entries).insert(tag)
}

// ============================================================================
// Tests for get_total_count with include_hidden
// ============================================================================

#[tokio::test]
async fn test_get_total_count_with_hidden_includes_all() {
    let volume = create_test_volume();

    // Manually insert into listing cache (simulating list_directory_start)
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-total-count-hidden", entries);

    let count = get_total_count(listing.id(), true).unwrap();

    // All 7 entries should be counted
    assert_eq!(count, 7, "Should count all entries including hidden");
}

#[tokio::test]
async fn test_get_total_count_without_hidden_excludes_dot_files() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-total-count-no-hidden", entries);

    let count = get_total_count(listing.id(), false).unwrap();

    // Only 4 visible entries: Documents, Downloads, file.txt, readme.md
    assert_eq!(count, 4, "Should only count non-hidden entries");
}

// ============================================================================
// Tests for get_file_range with include_hidden
// ============================================================================

#[tokio::test]
async fn test_get_file_range_with_hidden_returns_all() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-range-hidden", entries);

    let range = get_file_range(listing.id(), 0, 10, true).unwrap();

    assert_eq!(range.len(), 7, "Should return all 7 entries");

    // Verify hidden files are present
    let names: Vec<&str> = range.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&".hidden_dir"), "Should include .hidden_dir");
    assert!(names.contains(&".hidden_file"), "Should include .hidden_file");
    assert!(names.contains(&".gitignore"), "Should include .gitignore");
}

#[tokio::test]
async fn test_get_file_range_without_hidden_excludes_dot_files() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-range-no-hidden", entries);

    let range = get_file_range(listing.id(), 0, 10, false).unwrap();

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

#[tokio::test]
async fn test_get_file_range_pagination_respects_hidden_filter() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-range-pagination", entries);

    // Get first 2 visible entries
    let page1 = get_file_range(listing.id(), 0, 2, false).unwrap();
    // Get next 2 visible entries
    let page2 = get_file_range(listing.id(), 2, 2, false).unwrap();

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

#[tokio::test]
async fn test_find_file_index_hidden_file_with_hidden_enabled() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-hidden-enabled", entries);

    let index = find_file_index(listing.id(), ".gitignore", true).unwrap();

    assert!(index.is_some(), "Should find .gitignore with hidden enabled");
}

#[tokio::test]
async fn test_find_file_index_hidden_file_with_hidden_disabled() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-hidden-disabled", entries);

    let index = find_file_index(listing.id(), ".gitignore", false).unwrap();

    assert!(index.is_none(), "Should NOT find .gitignore with hidden disabled");
}

#[tokio::test]
async fn test_find_file_index_visible_file_index_changes_with_hidden_setting() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-visible-index-changes", entries);

    // Find "Documents" with hidden enabled (should be after hidden dirs)
    let index_with_hidden = find_file_index(listing.id(), "Documents", true).unwrap();
    // Find "Documents" with hidden disabled (should be at the start)
    let index_without_hidden = find_file_index(listing.id(), "Documents", false).unwrap();

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

#[tokio::test]
async fn test_get_file_at_index_0_with_hidden_enabled() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-at-0-hidden", entries);

    let entry = get_file_at(listing.id(), 0, true).unwrap();

    // With hidden enabled, first entry should be a hidden dir (sorted alphabetically)
    let entry = entry.expect("Should have entry at index 0");
    assert!(
        entry.name.starts_with('.'),
        "First entry with hidden enabled should be a hidden file, got {}",
        entry.name
    );
}

#[tokio::test]
async fn test_get_file_at_index_0_with_hidden_disabled() {
    let volume = create_test_volume();

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-at-0-no-hidden", entries);

    let entry = get_file_at(listing.id(), 0, false).unwrap();

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

#[tokio::test]
async fn test_directory_with_only_hidden_files() {
    let entries = vec![
        make_entry(".bashrc", false),
        make_entry(".profile", false),
        make_entry(".zshrc", false),
    ];
    let volume = Arc::new(InMemoryVolume::with_entries("AllHidden", entries));

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-all-hidden", entries);

    let count_with = get_total_count(listing.id(), true).unwrap();
    let count_without = get_total_count(listing.id(), false).unwrap();
    let range_without = get_file_range(listing.id(), 0, 10, false).unwrap();

    assert_eq!(count_with, 3, "All 3 hidden files should be counted");
    assert_eq!(count_without, 0, "No visible files to count");
    assert!(range_without.is_empty(), "No visible files to return");
}

#[tokio::test]
async fn test_directory_with_no_hidden_files() {
    let entries = vec![make_entry("Documents", true), make_entry("file.txt", false)];
    let volume = Arc::new(InMemoryVolume::with_entries("NoHidden", entries));

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-no-hidden", entries);

    let count_with = get_total_count(listing.id(), true).unwrap();
    let count_without = get_total_count(listing.id(), false).unwrap();

    assert_eq!(count_with, 2, "Both files should be counted");
    assert_eq!(count_without, 2, "Both files should be counted (none are hidden)");
}

// ============================================================================
// Tests for find_file_indices (batch name→index lookup)
// ============================================================================

#[tokio::test]
async fn test_find_file_indices_basic_lookup() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-basic", entries);

    let names = vec!["Documents".to_string(), "file.txt".to_string()];
    let result = find_file_indices(listing.id(), &names, true).unwrap();

    assert_eq!(result.len(), 2);
    assert!(result.contains_key("Documents"));
    assert!(result.contains_key("file.txt"));
    // Indices must match the singular find_file_index
    // (We already tested the volume has these entries)
}

#[tokio::test]
async fn test_find_file_indices_hidden_filtering() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-hidden", entries);

    let names = vec![
        ".gitignore".to_string(),
        "Documents".to_string(),
        ".hidden_file".to_string(),
    ];

    let with_hidden = find_file_indices(listing.id(), &names, true).unwrap();
    let without_hidden = find_file_indices(listing.id(), &names, false).unwrap();

    assert_eq!(with_hidden.len(), 3, "All 3 found when hidden included");
    assert_eq!(without_hidden.len(), 1, "Only Documents found when hidden excluded");
    assert!(without_hidden.contains_key("Documents"));
}

#[tokio::test]
async fn test_find_file_indices_names_not_in_listing() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-missing", entries);

    let names = vec!["nonexistent.txt".to_string(), "also_missing".to_string()];
    let result = find_file_indices(listing.id(), &names, true).unwrap();

    assert!(result.is_empty(), "No names should be found");
}

#[tokio::test]
async fn test_find_file_indices_empty_names() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-empty", entries);

    let names: Vec<String> = vec![];
    let result = find_file_indices(listing.id(), &names, true).unwrap();

    assert!(result.is_empty(), "Empty input should produce empty output");
}

#[tokio::test]
async fn test_find_file_indices_duplicate_names_in_input() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-dupes", entries);

    let names = vec!["file.txt".to_string(), "file.txt".to_string(), "Documents".to_string()];
    let result = find_file_indices(listing.id(), &names, true).unwrap();

    // Duplicates in input collapse to one key in output
    assert_eq!(result.len(), 2);
    assert!(result.contains_key("file.txt"));
    assert!(result.contains_key("Documents"));
}

#[tokio::test]
async fn test_find_file_indices_consistent_with_find_file_index() {
    let volume = create_test_volume();
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    let listing = insert_test_listing("test-find-indices-consistent", entries);

    let names = vec!["Documents".to_string(), "file.txt".to_string(), "readme.md".to_string()];
    let batch = find_file_indices(listing.id(), &names, false).unwrap();

    for name in &names {
        let single = find_file_index(listing.id(), name, false).unwrap();
        assert_eq!(
            batch.get(name.as_str()).copied(),
            single,
            "Batch and single must agree for '{}'",
            name
        );
    }
}
