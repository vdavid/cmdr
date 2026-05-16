//! Tests for listing cache helpers.

use std::path::PathBuf;

use super::caching::{
    CachedListing, LISTING_CACHE, ModifyResult, find_listings_for_path, find_listings_for_path_on_volume, has_entry,
    insert_entry_sorted, remove_entry_by_path, update_entry_sorted,
};
use super::metadata::FileEntry;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};

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

        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
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
