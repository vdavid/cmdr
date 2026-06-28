//! Tests for listing cache helpers.

use std::path::PathBuf;

use super::caching::{
    CachedListing, LISTING_CACHE, ModifyResult, apply_tags_to_listing, carry_forward_tags, find_listings_for_path,
    find_listings_for_path_on_volume, has_entry, insert_entry_sorted, notify_added, remove_entry_by_path,
    update_entry_sorted,
};
use super::metadata::{FileEntry, TagRef};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

/// Reads the tags currently cached for `path` in listing `id`.
fn cached_tags(id: &str, path: &str) -> Vec<TagRef> {
    let cache = LISTING_CACHE.read().unwrap();
    let listing = cache.get(id).unwrap();
    listing.entries.iter().find(|e| e.path == path).unwrap().tags.clone()
}

/// Creates a minimal test entry.
fn make_entry(name: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/test/{}", name), is_dir, false)
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
    insert_test_listing_on_volume(id, "root", path, sort_by, sort_order, dir_sort_mode, entries)
}

fn insert_test_listing_on_volume(
    id: &str,
    volume_id: &str,
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
            volume_id: volume_id.to_string(),
            path: PathBuf::from(path),
            entries,
            sort_by,
            sort_order,
            directory_sort_mode: dir_sort_mode,
            sequence: std::sync::atomic::AtomicU64::new(0),
            created_at: std::time::Instant::now(),
            last_accessed_ms: std::sync::atomic::AtomicU64::new(0),
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
        "/shared/dir/two_matches",
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    let id2 = insert_test_listing(
        "find_2match_b",
        "/shared/dir/two_matches",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path(&PathBuf::from("/shared/dir/two_matches"));
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

    // Insert "beta.txt", should land between alpha and gamma
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

    // Insert a directory with medium recursive size, should go between big_dir and small_dir
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

#[test]
fn notify_added_upserts_when_entry_already_present() {
    // Race that motivated the upsert: SMB watcher fires an Added event mid-write
    // (stat catches the file at partial size), then `write_from_stream`'s own
    // post-close `notify_mutation` fires its Added with the final size. Without
    // upsert the first-write wins (Samba's mid-write partial size sticks) and
    // the FE shows the wrong size until the next manual refresh. With upsert
    // the second observation updates the cached entry to the final size.
    let id = insert_test_listing(
        "notify_added_upsert",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // First observation: partial size (what the watcher would see mid-write).
    notify_added("notify_added_upsert", make_entry("photo.jpg", false, Some(2_359_284)));
    // Second observation: final size (what the post-close stat sees).
    notify_added("notify_added_upsert", make_entry("photo.jpg", false, Some(4_989_168)));

    let cache = LISTING_CACHE.read().unwrap();
    let listing = cache.get("notify_added_upsert").unwrap();
    assert_eq!(
        listing.entries.len(),
        1,
        "should still be exactly one entry, not duplicated"
    );
    assert_eq!(
        listing.entries[0].size,
        Some(4_989_168),
        "second (final) size must overwrite the partial-size observation"
    );
    drop(cache);
    cleanup_listing(&id);
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

// ============================================================================
// find_listings_for_path_on_volume tests
// ============================================================================

#[test]
fn test_find_listings_for_path_on_volume_filters_by_volume() {
    let id1 = insert_test_listing_on_volume(
        "vol_filter_root",
        "root",
        "/shared/dir/vol_filter",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );
    let id2 = insert_test_listing_on_volume(
        "vol_filter_smb",
        "smb-nas",
        "/shared/dir/vol_filter",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // Filter by "root": only id1
    let results = find_listings_for_path_on_volume(Some("root"), &PathBuf::from("/shared/dir/vol_filter"));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "vol_filter_root");

    // Filter by "smb-nas": only id2
    let results = find_listings_for_path_on_volume(Some("smb-nas"), &PathBuf::from("/shared/dir/vol_filter"));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "vol_filter_smb");

    // No filter: both
    let results = find_listings_for_path_on_volume(None, &PathBuf::from("/shared/dir/vol_filter"));
    assert_eq!(results.len(), 2);

    cleanup_listing(&id1);
    cleanup_listing(&id2);
}

#[test]
fn test_find_listings_for_path_on_volume_no_match() {
    let id = insert_test_listing_on_volume(
        "vol_nomatch",
        "root",
        "/some/dir",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path_on_volume(Some("smb-nas"), &PathBuf::from("/some/dir"));
    assert!(results.is_empty());

    cleanup_listing(&id);
}

// ============================================================================
// find_listings_on_volume tests (FullRefresh fallback path)
// ============================================================================
//
// `notify_directory_changed(FullRefresh)` requires a `tauri::AppHandle` (obtained
// from WATCHER_MANAGER) and returns early if it's None. Since AppHandle can't be
// constructed in unit tests, we can't test the full FullRefresh notification path
// directly.
//
// Instead, the FullRefresh re-read + cache update logic is tested via
// `handle_directory_change` in watcher_test.rs (which shares the same mechanism
// and handles missing AppHandle gracefully). Here we test the `find_listings_on_volume`
// helper that the FullRefresh fallback path depends on.

#[test]
fn test_find_listings_on_volume_returns_all_volume_listings() {
    use super::caching::find_listings_on_volume;

    let id1 = insert_test_listing_on_volume(
        "flov_listing1",
        "smb-share-42",
        "/mnt/share/docs",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("readme.txt", false, Some(100))],
    );
    let id2 = insert_test_listing_on_volume(
        "flov_listing2",
        "smb-share-42",
        "/mnt/share/photos",
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    // Different volume: should not be returned
    let id3 = insert_test_listing_on_volume(
        "flov_other",
        "different-vol",
        "/mnt/other",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_on_volume("smb-share-42");
    assert_eq!(
        results.len(),
        2,
        "Expected 2 listings for smb-share-42, got {}",
        results.len()
    );

    let ids: Vec<&str> = results.iter().map(|(id, ..)| id.as_str()).collect();
    assert!(ids.contains(&"flov_listing1"));
    assert!(ids.contains(&"flov_listing2"));

    // Verify paths and sort params are returned correctly
    let listing1 = results.iter().find(|(id, ..)| id == "flov_listing1").unwrap();
    assert_eq!(listing1.1, PathBuf::from("/mnt/share/docs"));
    assert_eq!(listing1.2, SortColumn::Name);

    let listing2 = results.iter().find(|(id, ..)| id == "flov_listing2").unwrap();
    assert_eq!(listing2.1, PathBuf::from("/mnt/share/photos"));
    assert_eq!(listing2.2, SortColumn::Size);
    assert_eq!(listing2.3, SortOrder::Descending);
    assert_eq!(listing2.4, DirectorySortMode::AlwaysByName);

    cleanup_listing(&id1);
    cleanup_listing(&id2);
    cleanup_listing(&id3);
}

#[test]
fn test_find_listings_on_volume_empty_for_unknown_volume() {
    use super::caching::find_listings_on_volume;

    let results = find_listings_on_volume("nonexistent-volume-id");
    assert!(results.is_empty());
}

// ============================================================================
// try_get_watched_listing tests (M1 oracle)
// ============================================================================
//
// These tests use a small `WatchedFlagVolume` wrapper around `InMemoryVolume`
// because `InMemoryVolume::listing_is_watched` always returns false (the
// default). The wrapper lets tests pin the watcher flag to `true` or `false`
// without touching `WATCHER_MANAGER` (which would require an `AppHandle`).

mod oracle_tests {
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use super::super::caching::{CachedListing, LISTING_CACHE, try_get_watched_listing};
    use super::super::metadata::FileEntry;
    use super::super::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::{
        BatchScanResult, CopyScanResult, InMemoryVolume, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError,
        VolumeReadStream,
    };

    /// A test-only volume wrapper that overrides `listing_is_watched` with a
    /// flag controlled per test. Delegates every other method to the inner
    /// `InMemoryVolume`.
    struct WatchedFlagVolume {
        inner: InMemoryVolume,
        watched: AtomicBool,
    }

    impl WatchedFlagVolume {
        fn new(name: &str, watched: bool) -> Self {
            Self {
                inner: InMemoryVolume::new(name),
                watched: AtomicBool::new(watched),
            }
        }

        fn set_watched(&self, v: bool) {
            self.watched.store(v, Ordering::Relaxed);
        }
    }

    impl Volume for WatchedFlagVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }

        fn root(&self) -> &Path {
            self.inner.root()
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            self.inner.list_directory(path, on_progress)
        }

        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            self.inner.get_metadata(path)
        }

        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }

        fn is_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            self.inner.is_directory(path)
        }

        fn listing_is_watched(&self, _path: &Path) -> bool {
            self.watched.load(Ordering::Relaxed)
        }

        fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
            self.inner.get_space_info()
        }

        fn scan_for_copy<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_copy(path)
        }

        fn scan_for_copy_batch<'a>(
            &'a self,
            paths: &'a [PathBuf],
        ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_copy_batch(paths)
        }

        fn scan_for_conflicts<'a>(
            &'a self,
            source_items: &'a [SourceItemInfo],
            dest_path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_conflicts(source_items, dest_path)
        }

        fn open_read_stream<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
            self.inner.open_read_stream(path)
        }
    }

    fn make_test_entry(name: &str) -> FileEntry {
        FileEntry {
            size: Some(123),
            permissions: 0o644,
            owner: "test".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(name.to_string(), format!("/oracle/{}", name), false, false)
        }
    }

    /// Inserts a `CachedListing` directly into `LISTING_CACHE` with a controllable
    /// sequence. Returns the listing_id.
    fn insert_listing_with_sequence(
        id: &str,
        volume_id: &str,
        path: &str,
        entries: Vec<FileEntry>,
        sequence: u64,
    ) -> String {
        let listing_id = id.to_string();
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: volume_id.to_string(),
                path: PathBuf::from(path),
                entries,
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: AtomicU64::new(sequence),
                created_at: std::time::Instant::now(),
                last_accessed_ms: AtomicU64::new(0),
            },
        );
        listing_id
    }

    fn remove_listing(id: &str) {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(id);
    }

    fn unique(suffix: &str) -> String {
        use std::sync::atomic::AtomicU64;
        static N: AtomicU64 = AtomicU64::new(0);
        format!(
            "oracle_{}_{}_{}",
            suffix,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn try_get_watched_listing_hit_when_watcher_reports_true() {
        let vid = unique("hit_vid");
        let lid = unique("hit_lid");
        let path = "/oracle/hit";

        let vol = Arc::new(WatchedFlagVolume::new("hit-vol", true));
        get_volume_manager().register(&vid, vol);

        let entries = vec![make_test_entry("a.txt"), make_test_entry("b.txt")];
        let lid_inserted = insert_listing_with_sequence(&lid, &vid, path, entries.clone(), 0);

        let result = try_get_watched_listing(&vid, Path::new(path));
        assert!(result.is_some(), "expected Some(entries) on watched listing");
        let returned = result.unwrap();
        assert_eq!(returned.len(), entries.len());
        assert_eq!(returned[0].name, "a.txt");
        assert_eq!(returned[1].name, "b.txt");

        remove_listing(&lid_inserted);
        get_volume_manager().unregister(&vid);
    }

    #[test]
    fn try_get_watched_listing_miss_when_watcher_reports_false() {
        let vid = unique("miss_watch_vid");
        let lid = unique("miss_watch_lid");
        let path = "/oracle/miss_watch";

        let vol = Arc::new(WatchedFlagVolume::new("miss-vol", false));
        get_volume_manager().register(&vid, vol);

        let entries = vec![make_test_entry("a.txt")];
        let lid_inserted = insert_listing_with_sequence(&lid, &vid, path, entries, 0);

        let result = try_get_watched_listing(&vid, Path::new(path));
        assert!(result.is_none(), "expected None when watcher is dead");

        remove_listing(&lid_inserted);
        get_volume_manager().unregister(&vid);
    }

    #[test]
    fn try_get_watched_listing_miss_when_no_listing_exists() {
        let vid = unique("miss_no_listing_vid");
        let vol = Arc::new(WatchedFlagVolume::new("no-listing-vol", true));
        get_volume_manager().register(&vid, vol);

        let result = try_get_watched_listing(&vid, Path::new("/oracle/nothing_here"));
        assert!(result.is_none(), "expected None when no listing matches");

        get_volume_manager().unregister(&vid);
    }

    #[test]
    fn try_get_watched_listing_miss_when_volume_not_registered() {
        let vid = unique("no_vol");
        let lid = unique("no_vol_lid");
        let path = "/oracle/no_vol";

        // Listing exists in cache, but no volume is registered for this ID.
        let lid_inserted = insert_listing_with_sequence(&lid, &vid, path, vec![make_test_entry("a.txt")], 0);

        let result = try_get_watched_listing(&vid, Path::new(path));
        assert!(result.is_none(), "expected None when volume isn't registered");

        remove_listing(&lid_inserted);
    }

    #[test]
    fn try_get_watched_listing_picks_highest_sequence() {
        // Two listings on the same (volume_id, path) with different sequence
        // numbers. The oracle must return the entries from the higher-sequence
        // listing, deterministically — never the lower-sequence one.
        let vid = unique("seq_vid");
        let lid_lo = unique("seq_lo");
        let lid_hi = unique("seq_hi");
        let path = "/oracle/seq_path";

        let vol = Arc::new(WatchedFlagVolume::new("seq-vol", true));
        get_volume_manager().register(&vid, vol);

        let lid_lo_inserted = insert_listing_with_sequence(&lid_lo, &vid, path, vec![make_test_entry("low.txt")], 1);
        let lid_hi_inserted = insert_listing_with_sequence(&lid_hi, &vid, path, vec![make_test_entry("high.txt")], 9);

        let result = try_get_watched_listing(&vid, Path::new(path));
        assert!(result.is_some());
        let returned = result.unwrap();
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].name, "high.txt", "expected the higher-sequence listing");

        remove_listing(&lid_lo_inserted);
        remove_listing(&lid_hi_inserted);
        get_volume_manager().unregister(&vid);
    }

    #[test]
    fn try_get_watched_listing_miss_for_start_streaming_watcher_gap() {
        // Simulates the documented race window between
        // `list_directory_start_streaming` populating LISTING_CACHE and
        // `start_watching` inserting into WATCHER_MANAGER: the listing exists
        // in cache, but the volume reports no watcher yet (here: by reporting
        // `false` from the test volume's `listing_is_watched`). The oracle
        // must miss in that window so write ops fall through to a real read.
        let vid = unique("race_vid");
        let lid = unique("race_lid");
        let path = "/oracle/race";

        // `watched=false` mirrors "WATCHER_MANAGER has no entry yet" on the
        // local backend without needing an AppHandle.
        let vol = Arc::new(WatchedFlagVolume::new("race-vol", false));
        get_volume_manager().register(&vid, vol);

        let lid_inserted = insert_listing_with_sequence(&lid, &vid, path, vec![make_test_entry("a.txt")], 0);

        let result = try_get_watched_listing(&vid, Path::new(path));
        assert!(result.is_none(), "expected None during the streaming->watcher gap");

        remove_listing(&lid_inserted);
        get_volume_manager().unregister(&vid);
    }

    #[test]
    fn try_get_watched_listing_reflects_flip_to_unwatched() {
        // Sanity check: flipping the watcher flag flips the oracle's verdict
        // on subsequent calls. Documents that the oracle is a live query and
        // doesn't memoize per-listing.
        let vid = unique("flip_vid");
        let lid = unique("flip_lid");
        let path = "/oracle/flip";

        let vol: Arc<WatchedFlagVolume> = Arc::new(WatchedFlagVolume::new("flip-vol", true));
        get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

        let lid_inserted = insert_listing_with_sequence(&lid, &vid, path, vec![make_test_entry("x.txt")], 0);

        assert!(try_get_watched_listing(&vid, Path::new(path)).is_some());
        vol.set_watched(false);
        assert!(try_get_watched_listing(&vid, Path::new(path)).is_none());

        remove_listing(&lid_inserted);
        get_volume_manager().unregister(&vid);
    }
}

// ============================================================================
// Orphan-listing backstop reaper tests
// ============================================================================
//
// These pin the defense-in-depth reaper that catches listings whose explicit
// `list_directory_end` IPC was never delivered. The crux is that the reaper keys on
// `last_accessed_ms` (refreshed on every live-pane access), NOT `created_at` (stamped
// once), so a long-open-but-still-used listing is never evicted. The positive test
// proves a stale listing AND its watcher are torn down together; the negative test
// proves a freshly-touched listing survives even when it was created long ago.
mod reaper_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::super::caching::{
        CachedListing, LISTING_CACHE, ORPHAN_IDLE_WINDOW, orphan_ids, reap_orphaned_listings_at,
    };
    use super::super::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::watcher::{WATCHER_MANAGER, start_watching};

    fn unique(suffix: &str) -> String {
        static N: AtomicU64 = AtomicU64::new(0);
        format!(
            "reaper_{}_{}_{}",
            suffix,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Inserts a listing with an explicit `last_accessed_ms` stamp so we can simulate an
    /// orphan deterministically (no sleeping, no real clock advance).
    fn insert_with_last_accessed(id: &str, path: &str, last_accessed_ms: u64) {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.insert(
            id.to_string(),
            CachedListing {
                volume_id: "root".to_string(),
                path: PathBuf::from(path),
                entries: Vec::new(),
                sort_by: SortColumn::Name,
                sort_order: SortOrder::Ascending,
                directory_sort_mode: DirectorySortMode::LikeFiles,
                sequence: AtomicU64::new(0),
                // `created_at` stays "now" on purpose: it proves the reaper does NOT key on
                // creation time. A long-open listing has a recent `created_at` relative to
                // session start too, but the point is that even a brand-new `created_at`
                // doesn't save a listing whose `last_accessed_ms` is stale.
                created_at: std::time::Instant::now(),
                last_accessed_ms: AtomicU64::new(last_accessed_ms),
            },
        );
    }

    fn remove_listing(id: &str) {
        let mut cache = LISTING_CACHE.write().unwrap();
        cache.remove(id);
    }

    fn in_cache(id: &str) -> bool {
        LISTING_CACHE.read().unwrap().contains_key(id)
    }

    fn is_watched(id: &str) -> bool {
        WATCHER_MANAGER.read().unwrap().watches.contains_key(id)
    }

    // ---- pure helper: orphan_ids ------------------------------------------------

    #[test]
    fn orphan_ids_flags_only_listings_idle_past_the_window() {
        let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
        let now = 100 * window_ms; // far enough that subtractions don't underflow

        let stamps = [
            ("fresh", now),                      // idle 0
            ("recent", now - 1),                 // idle 1 ms
            ("just_under", now - window_ms + 1), // idle window-1: NOT orphan
            ("exactly", now - window_ms),        // idle == window: orphan (>=)
            ("ancient", now - 10 * window_ms),   // idle 10x window: orphan
        ];

        let mut ids = orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms)));
        ids.sort();

        assert_eq!(ids, vec!["ancient".to_string(), "exactly".to_string()]);
    }

    #[test]
    fn orphan_ids_empty_for_all_fresh() {
        let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
        let now = 1_000_000_000u64;
        let stamps = [("a", now), ("b", now - 5), ("c", now - 100)];
        assert!(orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms))).is_empty());
    }

    // ---- reap: positive (orphan + its watcher torn down together) ----------------
    //
    // We inject `now_ms` and `window_ms` because the real idle clock is relative to
    // process start, so a genuine 6 h gap can't be produced in a unit test. The listing
    // is stamped `last_accessed_ms = 0`; calling the reaper with `now = window` makes its
    // idle time exactly the window → orphan.

    #[test]
    fn reaper_evicts_stale_listing_and_its_watcher_together() {
        // A real directory so `start_watching` can attach an FSEvents watcher. No
        // AppHandle is needed to STORE the watcher in WATCHER_MANAGER; the handle only
        // matters for emitting events, which this test doesn't exercise.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let lid = unique("orphan");
        let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

        insert_with_last_accessed(&lid, &path, 0); // last access at the epoch (ancient)
        start_watching(&lid, dir.path()).expect("start_watching should succeed on a real dir");

        assert!(in_cache(&lid), "precondition: listing is cached");
        assert!(is_watched(&lid), "precondition: watcher is attached");

        // now = window → idle == window → orphan.
        let reaped = reap_orphaned_listings_at(window_ms, window_ms);

        assert!(reaped.contains(&lid), "reaper should report the orphaned listing");
        assert!(!in_cache(&lid), "reaper must remove the cache entry");
        assert!(
            !is_watched(&lid),
            "reaper must tear down the watcher too (reusing list_directory_end's stop_watching)"
        );
    }

    // ---- reap: negative (recently-touched, long-created listing survives) --------

    #[test]
    fn reaper_keeps_recently_touched_listing_even_if_created_long_ago() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let lid = unique("live");
        let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

        // Touched "just now" relative to the injected clock: last_accessed_ms == now. The
        // listing's `created_at` is real-now (set in the helper), but the reaper ignores
        // created_at entirely — only the fresh access stamp matters. Pick a `now` far past
        // the window so the only reason this listing survives is its fresh stamp, not a
        // small absolute clock.
        let now = 50 * window_ms;
        insert_with_last_accessed(&lid, &path, now);
        start_watching(&lid, dir.path()).expect("start_watching should succeed on a real dir");

        let reaped = reap_orphaned_listings_at(now, window_ms);

        assert!(
            !reaped.contains(&lid),
            "a just-touched listing must NOT be reaped (this is the don't-evict-live guarantee)"
        );
        assert!(in_cache(&lid), "live listing's cache entry must survive");
        assert!(is_watched(&lid), "live listing's watcher must survive");

        // Cleanup
        crate::file_system::listing::operations::list_directory_end(&lid);
        remove_listing(&lid);
    }

    // ---- touch() refreshes the stamp so a long-open pane is never reaped ---------

    #[test]
    fn touch_rescues_a_listing_that_would_otherwise_be_orphaned() {
        let lid = unique("touch");
        let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
        let now = 50 * window_ms;

        // Start stale (last access at the epoch → idle == now == 50x window → orphan)...
        insert_with_last_accessed(&lid, "/no/watcher", 0);
        assert!(
            !orphan_ids(now, window_ms, std::iter::once((lid.as_str(), 0))).is_empty(),
            "precondition: a stamp of 0 is orphan-eligible at now = 50x window"
        );

        // ...then a live-pane access touches it (same path the read accessors take: hold
        // the cache read lock and stamp `last_accessed_ms` via `touch()`). `touch()` uses
        // the REAL clock (`epoch_millis_now()`), which in-process is a tiny value — far
        // below the injected `now` of 50x window. So the touched stamp's idle time at the
        // injected `now` is still ~50x window, which would STILL look orphaned under that
        // synthetic clock. To prove touch() works against the clock it actually uses, run
        // the reaper with the real clock + real window: the just-touched stamp is fresh.
        {
            let cache = LISTING_CACHE.read().unwrap();
            cache.get(&lid).unwrap().touch();
        }

        let reaped = reap_orphaned_listings_at(
            super::super::caching::epoch_millis_now(),
            ORPHAN_IDLE_WINDOW.as_millis() as u64,
        );
        assert!(!reaped.contains(&lid), "touch() must reset the idle clock");
        assert!(in_cache(&lid), "touched listing survives the sweep");

        remove_listing(&lid);
    }
}

// ============================================================================
// Finder-tag enrichment + carry-forward tests
// ============================================================================

#[test]
fn apply_tags_sets_tags_on_matching_entry() {
    let id = insert_test_listing(
        "tags_apply",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1)), make_entry("b.txt", false, Some(2))],
    );

    apply_tags_to_listing(
        "tags_apply",
        vec![("/test/a.txt".to_string(), vec![tag("Red", 6), tag("Work", 0)])],
    );

    assert_eq!(
        cached_tags("tags_apply", "/test/a.txt"),
        vec![tag("Red", 6), tag("Work", 0)]
    );
    assert_eq!(cached_tags("tags_apply", "/test/b.txt"), Vec::<TagRef>::new());
    cleanup_listing(&id);
}

#[test]
fn apply_tags_clears_tags_on_external_removal() {
    // A file that already has tags; an empty read must clear them (removal
    // propagation — the counterpart to carry-forward).
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let id = insert_test_listing(
        "tags_clear",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    apply_tags_to_listing("tags_clear", vec![("/test/a.txt".to_string(), Vec::new())]);

    assert_eq!(cached_tags("tags_clear", "/test/a.txt"), Vec::<TagRef>::new());
    cleanup_listing(&id);
}

#[test]
fn apply_tags_skips_unknown_paths() {
    let id = insert_test_listing(
        "tags_unknown",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1))],
    );

    // Path not in the listing (scrolled away / removed): no panic, no change.
    apply_tags_to_listing(
        "tags_unknown",
        vec![("/test/gone.txt".to_string(), vec![tag("Blue", 4)])],
    );

    assert_eq!(cached_tags("tags_unknown", "/test/a.txt"), Vec::<TagRef>::new());
    cleanup_listing(&id);
}

#[test]
fn carry_forward_restores_tags_on_empty_restat() {
    // Simulates a watcher re-stat: the new entry has empty tags (get_single_entry
    // reads no xattr), so carry-forward must restore the cached tags.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Green", 2)];
    let id = insert_test_listing(
        "tags_carry",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut restat = make_entry("a.txt", false, Some(99)); // empty tags, like a fresh stat
    carry_forward_tags("tags_carry", &mut restat);
    assert_eq!(restat.tags, vec![tag("Green", 2)], "carry-forward restores cached tags");

    // And after storing the re-stat through the modify path, the tags survive.
    update_entry_sorted("tags_carry", restat);
    assert_eq!(cached_tags("tags_carry", "/test/a.txt"), vec![tag("Green", 2)]);
    cleanup_listing(&id);
}

#[test]
fn carry_forward_does_not_overwrite_incoming_tags() {
    // When the incoming entry already carries tags (the enrich path), carry-forward
    // must leave them untouched so a real change isn't masked.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let id = insert_test_listing(
        "tags_no_overwrite",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut incoming = make_entry("a.txt", false, Some(1));
    incoming.tags = vec![tag("Blue", 4)];
    carry_forward_tags("tags_no_overwrite", &mut incoming);
    assert_eq!(
        incoming.tags,
        vec![tag("Blue", 4)],
        "carry-forward must not clobber incoming tags"
    );
    cleanup_listing(&id);
}
