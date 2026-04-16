//! Tests for file system watcher

// Note: The watcher tests require async handling and app context
// which makes them difficult to unit test. Key functionality is tested via:
// 1. compute_diff tests in watcher.rs (unit tests for diff logic)
// 2. Manual testing of file watching in the actual app
// 3. Integration tests with the full Tauri app

// The start_watching/stop_watching functions require a running app context
// to emit events, so proper testing requires integration tests.

use super::listing::FileEntry;
use super::volume::Volume;
use super::watcher::compute_diff;

fn make_entry(name: &str, size: Option<u64>) -> FileEntry {
    make_entry_in(name, "/test", size)
}

fn make_entry_in(name: &str, dir: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("{}/{}", dir, name), false, false)
    }
}

#[test]
fn test_compute_diff_addition() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "add");
    assert_eq!(diff[0].entry.name, "b.txt");
    assert_eq!(diff[0].index, 1); // index in new listing
}

#[test]
fn test_compute_diff_removal() {
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "remove");
    assert_eq!(diff[0].entry.name, "b.txt");
    assert_eq!(diff[0].index, 1); // index in old listing
}

#[test]
fn test_compute_diff_modification() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(200))]; // Size changed

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, "modify");
    assert_eq!(diff[0].entry.size, Some(200));
    assert_eq!(diff[0].index, 0); // index in new listing
}

#[test]
fn test_compute_diff_no_change() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert!(diff.is_empty());
}

// ============================================================================
// handle_directory_change integration tests
// ============================================================================

/// Tests that `handle_directory_change` re-reads a directory via the Volume trait
/// and updates the LISTING_CACHE when the volume's contents have changed.
///
/// This also covers the `notify_full_refresh` / `FullRefresh` code path in caching.rs,
/// since both use the same mechanism: re-read via Volume, compute diff, update cache.
/// (`notify_directory_changed(FullRefresh)` requires a `tauri::AppHandle` and returns
/// early without one, so it can't be tested directly in unit tests.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_directory_change_refreshes_from_volume() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::watcher::handle_directory_change;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-hdc-{}", uuid::Uuid::new_v4());
    let listing_id = format!("listing-hdc-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with files X and Y (paths must match dir_path)
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestHDC",
        vec![
            make_entry_in("x.txt", "/testdir", Some(100)),
            make_entry_in("y.txt", "/testdir", Some(200)),
        ],
    ));

    // Register in VolumeManager
    get_volume_manager().register(&volume_id, volume);

    // Insert stale cache with only X
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: volume_id.clone(),
                path: dir_path.clone(),
                entries: vec![make_entry_in("x.txt", "/testdir", Some(100))],
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: std::sync::atomic::AtomicU64::new(0),
            },
        );
    }

    handle_directory_change(&listing_id).await;

    // Assert: cache now has both X and Y
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get(&listing_id).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names.len(), 2, "Expected 2 entries, got: {:?}", names);
        assert!(names.contains(&"x.txt"), "Missing x.txt in {:?}", names);
        assert!(names.contains(&"y.txt"), "Missing y.txt in {:?}", names);
    }

    // Cleanup
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(&listing_id);
    }
    get_volume_manager().unregister(&volume_id);
}

/// Tests that `handle_directory_change` correctly handles an InMemoryVolume where
/// entries were added after the initial cache was populated (simulating a file creation
/// on a remote volume that the watcher detected).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handle_directory_change_detects_new_entries() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::watcher::handle_directory_change;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-new-{}", uuid::Uuid::new_v4());
    let listing_id = format!("listing-new-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with file A initially (paths must match dir_path)
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestNew",
        vec![make_entry_in("a.txt", "/testdir", Some(100))],
    ));

    get_volume_manager().register(&volume_id, volume.clone());

    // Cache reflects current state (A only)
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: volume_id.clone(),
                path: dir_path.clone(),
                entries: vec![make_entry_in("a.txt", "/testdir", Some(100))],
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: std::sync::atomic::AtomicU64::new(0),
            },
        );
    }

    // Add a new file to the volume (simulating external change).
    volume
        .create_file(std::path::Path::new("/testdir/b.txt"), b"new content")
        .await
        .unwrap();

    // Trigger re-read
    handle_directory_change(&listing_id).await;

    // Assert: cache now has A and B
    {
        let cache = LISTING_CACHE.read().unwrap();
        let listing = cache.get(&listing_id).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names.len(), 2, "Expected 2 entries, got: {:?}", names);
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
    }

    // Cleanup
    {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(&listing_id);
    }
    get_volume_manager().unregister(&volume_id);
}
