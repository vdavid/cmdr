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
// is_entry_modified axis coverage (cargo-mutants survivors)
// ============================================================================
//
// is_entry_modified returns true when ANY of: size, modified_at, permissions,
// is_directory, is_symlink differ. The existing tests only varied size, so
// the `||` chain mutants (each → `&&`) and the per-field `!= → ==` mutants
// all survived. These tests pin each axis individually.
//
// is_entry_modified is private; we exercise it via compute_diff, which marks
// the entry as "modify" only when is_entry_modified returns true.

#[test]
fn diff_marks_entry_modified_when_modified_at_differs() {
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.modified_at = Some(1000);
    new_entry.modified_at = Some(2000);
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "modified_at change should produce a modify diff");
    assert_eq!(diff[0].change_type, "modify");
}

#[test]
fn diff_marks_entry_modified_when_permissions_differ() {
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.permissions = 0o644;
    new_entry.permissions = 0o755;
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "permissions change should produce a modify diff");
    assert_eq!(diff[0].change_type, "modify");
}

#[test]
fn diff_marks_entry_modified_when_is_directory_flips() {
    // Same path/name but the entry transitioned from file → directory
    // (atomic-replace of a file with a dir of the same name). The watcher
    // should report this as a modify so the UI rerenders the icon and clears
    // the size column. Kills the `is_directory != → ==` mutant and the
    // `|| → &&` mutant on its line.
    let old_entry = FileEntry {
        is_directory: false,
        ..make_entry("thing", Some(100))
    };
    let new_entry = FileEntry {
        is_directory: true,
        size: Some(100),
        ..make_entry("thing", Some(100))
    };
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "is_directory flip should produce a modify diff");
    assert_eq!(diff[0].change_type, "modify");
}

#[test]
fn diff_marks_entry_modified_when_is_symlink_flips() {
    let old_entry = FileEntry {
        is_symlink: false,
        ..make_entry("thing", Some(100))
    };
    let new_entry = FileEntry {
        is_symlink: true,
        ..make_entry("thing", Some(100))
    };
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "is_symlink flip should produce a modify diff");
    assert_eq!(diff[0].change_type, "modify");
}

#[test]
fn diff_does_not_mark_modified_when_only_owner_or_group_change() {
    // Negative case for the `|| → &&` mutants on every axis: if any of those
    // flipped, this test (which only changes a field is_entry_modified
    // doesn't watch) would suddenly start producing a modify diff.
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.owner = "alice".to_string();
    new_entry.owner = "bob".to_string();
    new_entry.group = "wheel".to_string();
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert!(
        diff.is_empty(),
        "owner/group changes alone must NOT trigger a modify diff (is_entry_modified watches only size, mtime, perms, kind, symlink)"
    );
}

// ============================================================================
// compute_diff structural pins (mixed adds + removes + modifies)
// ============================================================================

#[test]
fn diff_includes_add_modify_and_remove_in_one_pass() {
    // Old: a.txt (size 100), b.txt (size 200)
    // New: a.txt (size 300, modified), c.txt (size 50, added)
    // → 3 changes: remove b, modify a, add c.
    // Also pins the index semantics: removes use the OLD index, adds/modifies use the NEW index.
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
    let new = vec![make_entry("a.txt", Some(300)), make_entry("c.txt", Some(50))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 3, "expected add + modify + remove");

    let by_type: std::collections::HashMap<&str, &super::watcher::DiffChange> =
        diff.iter().map(|c| (c.change_type.as_str(), c)).collect();
    let modify = by_type.get("modify").expect("modify present");
    let add = by_type.get("add").expect("add present");
    let remove = by_type.get("remove").expect("remove present");

    assert_eq!(modify.entry.name, "a.txt");
    assert_eq!(modify.index, 0, "modify uses NEW index");
    assert_eq!(add.entry.name, "c.txt");
    assert_eq!(add.index, 1, "add uses NEW index");
    assert_eq!(remove.entry.name, "b.txt");
    assert_eq!(remove.index, 1, "remove uses OLD index");
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
                created_at: std::time::Instant::now(),
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
                created_at: std::time::Instant::now(),
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
