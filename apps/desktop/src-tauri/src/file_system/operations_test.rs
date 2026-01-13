//! Tests for file system operations

use super::operations::{get_extended_metadata_batch, list_directory_core};
use super::provider::FileSystemProvider;
use super::real_provider::RealFileSystemProvider;
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

    let entry = super::operations::get_single_entry(&test_file).unwrap();

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

    let entry = super::operations::get_single_entry(&temp_dir).unwrap();

    // Cleanup
    let _ = fs::remove_dir(&temp_dir);

    assert!(entry.name.contains("cmdr_single_dir_test"));
    assert!(entry.is_directory);
    assert!(!entry.is_symlink);
    assert!(entry.size.is_none());
}

#[test]
fn test_get_single_entry_nonexistent() {
    let result = super::operations::get_single_entry(std::path::Path::new("/definitely_does_not_exist_12345"));
    assert!(result.is_err());
}

// ============================================================================
// Tests for streaming directory listing
// ============================================================================

#[test]
fn test_cancel_listing_sets_flag() {
    use super::operations::{STREAMING_STATE, StreamingListingState, cancel_listing};
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
    use super::operations::cancel_listing;

    // Should not panic when listing doesn't exist
    cancel_listing("nonexistent-listing-id");
}

#[test]
fn test_process_dir_entry_returns_file_entry() {
    use super::operations::process_dir_entry;

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
    use super::operations::process_dir_entry;

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
