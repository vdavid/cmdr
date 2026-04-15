//! Tests for file system operations

use super::reading::{get_extended_metadata_batch, list_directory_core};
use crate::file_system::provider::FileSystemProvider;
use crate::file_system::real_provider::RealFileSystemProvider;
use std::fs;

#[test]
fn test_list_directory() {
    let provider = RealFileSystemProvider;
    // Create our own temp directory to avoid permission issues
    let temp_dir = std::env::temp_dir().join("cmdr_list_test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    let result = provider.list_directory(&temp_dir);

    // Cleanup
    let _ = fs::remove_dir(&temp_dir);

    assert!(result.is_ok(), "list_directory failed: {:?}", result.err());
}

#[test]
fn test_list_directory_entries_have_names() {
    let provider = RealFileSystemProvider;
    let temp_dir = std::env::temp_dir().join("cmdr_ops_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let test_file = temp_dir.join("test_file.txt");
    fs::write(&test_file, "content").unwrap();

    let entries = provider.list_directory(&temp_dir).unwrap();

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_dir(&temp_dir);

    assert!(!entries.is_empty());
    assert!(entries.iter().any(|e| e.name == "test_file.txt"));
}

// ============================================================================
// Tests for two-phase loading functions
// ============================================================================

#[test]
fn test_list_directory_core_returns_entries_without_extended_metadata() {
    let temp_dir = std::env::temp_dir().join("cmdr_core_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let test_file = temp_dir.join("core_test.txt");
    fs::write(&test_file, "content").unwrap();

    let entries = list_directory_core(&temp_dir).unwrap();

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_dir(&temp_dir);

    assert!(!entries.is_empty());
    let file_entry = entries.iter().find(|e| e.name == "core_test.txt").unwrap();

    // Core metadata should be present
    assert!(!file_entry.name.is_empty());
    assert!(!file_entry.path.is_empty());
    assert!(!file_entry.owner.is_empty());

    // Extended metadata should NOT be loaded
    assert!(!file_entry.extended_metadata_loaded);
    assert!(file_entry.added_at.is_none());
    assert!(file_entry.opened_at.is_none());
}

#[test]
fn test_list_directory_core_is_sorted() {
    let temp_dir = std::env::temp_dir().join("cmdr_sort_test");
    fs::create_dir_all(&temp_dir).unwrap();

    // Create files in non-alphabetical order
    fs::write(temp_dir.join("zebra.txt"), "").unwrap();
    fs::write(temp_dir.join("alpha.txt"), "").unwrap();
    fs::create_dir(temp_dir.join("a_dir")).unwrap();

    let entries = list_directory_core(&temp_dir).unwrap();

    // Cleanup
    let _ = fs::remove_file(temp_dir.join("zebra.txt"));
    let _ = fs::remove_file(temp_dir.join("alpha.txt"));
    let _ = fs::remove_dir(temp_dir.join("a_dir"));
    let _ = fs::remove_dir(&temp_dir);

    // Directories should come first, then sorted alphabetically
    assert!(entries.len() >= 3);
    assert_eq!(entries[0].name, "a_dir");
    assert!(entries[0].is_directory);
}

#[test]
fn test_get_extended_metadata_batch() {
    let temp_dir = std::env::temp_dir().join("cmdr_extended_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let test_file = temp_dir.join("extended_test.txt");
    fs::write(&test_file, "content").unwrap();

    let paths = vec![test_file.to_string_lossy().to_string()];
    let extended = get_extended_metadata_batch(paths.clone());

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_dir(&temp_dir);

    assert_eq!(extended.len(), 1);
    assert_eq!(extended[0].path, paths[0]);

    // On macOS, these should have values; on other platforms, None
    #[cfg(target_os = "macos")]
    {
        // addedAt and openedAt may or may not be set depending on the file
        // but the function should run without error
    }
}

#[test]
fn test_get_extended_metadata_batch_empty_input() {
    let extended = get_extended_metadata_batch(vec![]);
    assert!(extended.is_empty());
}

// ============================================================================
// Tests for get_single_entry
// ============================================================================

#[test]
fn test_get_single_entry_file() {
    let temp_dir = std::env::temp_dir().join("cmdr_single_entry_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let test_file = temp_dir.join("single_file.txt");
    fs::write(&test_file, "test content").unwrap();

    let entry = super::get_single_entry(&test_file).unwrap();

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_dir(&temp_dir);

    assert_eq!(entry.name, "single_file.txt");
    assert!(!entry.is_directory);
    assert!(!entry.is_symlink);
    assert_eq!(entry.size, Some(12)); // "test content" is 12 bytes
    assert!(!entry.extended_metadata_loaded);
}

#[test]
fn test_get_single_entry_directory() {
    let temp_dir = std::env::temp_dir().join("cmdr_single_dir_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let entry = super::get_single_entry(&temp_dir).unwrap();

    // Cleanup
    let _ = fs::remove_dir(&temp_dir);

    assert!(entry.name.contains("cmdr_single_dir_test"));
    assert!(entry.is_directory);
    assert!(!entry.is_symlink);
    assert!(entry.size.is_none());
}

#[test]
fn test_get_single_entry_nonexistent() {
    let result = super::get_single_entry(std::path::Path::new("/definitely_does_not_exist_12345"));
    assert!(result.is_err());
}

// ============================================================================
// Tests for streaming directory listing
// ============================================================================

#[test]
fn test_cancel_listing_sets_flag() {
    use super::cancel_listing;
    use super::streaming::{STREAMING_STATE, StreamingListingState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Create a test listing ID and state
    let listing_id = "test-cancel-listing-12345";
    let state = Arc::new(StreamingListingState {
        cancelled: AtomicBool::new(false),
    });

    // Store it in the streaming state cache
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        cache.insert(listing_id.to_string(), Arc::clone(&state));
    }

    // Verify flag is initially false
    assert!(!state.cancelled.load(Ordering::Relaxed));

    // Call cancel_listing
    cancel_listing(listing_id);

    // Verify flag is now true
    assert!(state.cancelled.load(Ordering::Relaxed));

    // Cleanup
    {
        let mut cache = STREAMING_STATE.write().unwrap();
        cache.remove(listing_id);
    }
}

#[test]
fn test_cancel_listing_nonexistent_does_not_panic() {
    use super::cancel_listing;

    // Should not panic when listing doesn't exist
    cancel_listing("nonexistent-listing-id");
}

#[test]
fn test_process_dir_entry_returns_file_entry() {
    use super::reading::process_dir_entry;

    let temp_dir = std::env::temp_dir().join("cmdr_process_entry_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let test_file = temp_dir.join("process_test.txt");
    fs::write(&test_file, "test content").unwrap();

    // Read directory and get a DirEntry
    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == "process_test.txt").unwrap();

    let file_entry = process_dir_entry(dir_entry);

    // Cleanup
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_dir(&temp_dir);

    assert!(file_entry.is_some());
    let entry = file_entry.unwrap();
    assert_eq!(entry.name, "process_test.txt");
    assert!(!entry.is_directory);
    assert!(!entry.is_symlink);
    assert_eq!(entry.size, Some(12)); // "test content" is 12 bytes
}

#[test]
fn test_process_dir_entry_handles_directory() {
    use super::reading::process_dir_entry;

    let temp_dir = std::env::temp_dir().join("cmdr_process_dir_test");
    fs::create_dir_all(&temp_dir).unwrap();

    let sub_dir = temp_dir.join("sub_directory");
    fs::create_dir(&sub_dir).unwrap();

    // Read directory and get a DirEntry
    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == "sub_directory").unwrap();

    let file_entry = process_dir_entry(dir_entry);

    // Cleanup
    let _ = fs::remove_dir(&sub_dir);
    let _ = fs::remove_dir(&temp_dir);

    assert!(file_entry.is_some());
    let entry = file_entry.unwrap();
    assert_eq!(entry.name, "sub_directory");
    assert!(entry.is_directory);
    assert!(entry.size.is_none());
}

// ============================================================================
// Tests for list_directory_start_with_volume
// ============================================================================

/// Tests that `list_directory_start_with_volume` reads entries from an InMemoryVolume,
/// caches them in LISTING_CACHE, and returns the correct count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_caches_entries() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::listing::caching::LISTING_CACHE;
    use crate::file_system::listing::metadata::FileEntry;
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::watcher::stop_watching;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-ldswv-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with three files
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestLDSWV",
        vec![
            FileEntry {
                size: Some(100),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("p.txt".to_string(), "/testdir/p.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(200),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("q.txt".to_string(), "/testdir/q.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(300),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("r.txt".to_string(), "/testdir/r.txt".to_string(), false, false)
            },
        ],
    ));

    get_volume_manager().register(&volume_id, volume);

    // Run on a blocking thread because the function internally uses
    // `Handle::current().block_on()` which panics if called from an async context.
    let vid = volume_id.clone();
    let dp = dir_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        super::list_directory_start_with_volume(
            &vid,
            &dp,
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
    })
    .await
    .unwrap();

    assert!(
        result.is_ok(),
        "list_directory_start_with_volume failed: {:?}",
        result.err()
    );
    let result = result.unwrap();
    assert_eq!(result.total_count, 3);

    // Verify LISTING_CACHE has the entries
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get(&result.listing_id).unwrap();
        assert_eq!(listing.entries.len(), 3);
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"p.txt"));
        assert!(names.contains(&"q.txt"));
        assert!(names.contains(&"r.txt"));
        assert_eq!(listing.volume_id, volume_id);
        assert_eq!(listing.path, dir_path);
    }

    // Cleanup: stop watcher and remove from cache
    stop_watching(&result.listing_id);
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(&result.listing_id);
    }
    get_volume_manager().unregister(&volume_id);
}

/// Tests that `list_directory_start_with_volume` returns an error for a nonexistent volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_unknown_volume() {
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use std::path::PathBuf;

    let result = tokio::task::spawn_blocking(move || {
        super::list_directory_start_with_volume(
            "nonexistent-volume-id",
            &PathBuf::from("/some/path"),
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
    })
    .await
    .unwrap();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
