//! Tests for listing cache helpers.

use std::path::PathBuf;

use super::caching::{
    CachedListing, LISTING_CACHE, ModifyResult, apply_tags_to_listing, carry_forward_tags, find_listings_for_path,
    find_listings_for_path_on_volume, has_entry, insert_entry_sorted, notify_added, notify_removed,
    remove_entry_by_name, remove_entry_by_path, update_entry_sorted,
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
// Removed patch matches by NAME, not full path (MTP inner-path vs URL notifier)
// ============================================================================

/// Builds an MTP-shaped listing: the directory is the absolute `mtp://…` URL (as
/// pane navigation stores it), while each entry's `path` is the storage-relative
/// INNER form (as `MtpVolume::list_directory` produces it).
fn insert_mtp_style_listing(id: &str) -> String {
    let inner_notes = FileEntry::new(
        "notes.txt".to_string(),
        "/Documents/notes.txt".to_string(),
        false,
        false,
    );
    let inner_report = FileEntry::new(
        "report.txt".to_string(),
        "/Documents/report.txt".to_string(),
        false,
        false,
    );
    insert_test_listing_on_volume(
        id,
        "mtp-dev:65537",
        "mtp://mtp-dev/65537/Documents",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![inner_notes, inner_report],
    )
}

/// Pre-fix regression anchor: matching by the notifier's full URL path never found
/// the inner-path entry, which is why `notify_mutation(Deleted)` silently no-oped for
/// MTP and a moved/deleted file lingered in the source pane.
#[test]
fn full_path_match_misses_inner_mtp_entry_from_url_notifier() {
    let id = insert_mtp_style_listing("mtp_fullpath_miss");
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents/notes.txt");
    assert!(
        remove_entry_by_path("mtp_fullpath_miss", &url).is_none(),
        "URL full-path can't match an inner-path entry — the silent no-op this fix removes"
    );
    cleanup_listing(&id);
}

/// The fix: `remove_entry_by_name` matches by the entry's file name within the
/// directory listing, so the inner-path entry is found from the URL notifier path.
#[test]
fn name_match_removes_inner_mtp_entry() {
    let id = insert_mtp_style_listing("mtp_name_hit");
    let removed = remove_entry_by_name("mtp_name_hit", std::ffi::OsStr::new("notes.txt"));
    assert!(removed.is_some(), "name match removes the inner-path entry");
    assert_eq!(removed.unwrap().1.name, "notes.txt");
    {
        let cache = LISTING_CACHE.read().unwrap();
        let names: Vec<&str> = cache
            .get("mtp_name_hit")
            .unwrap()
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["report.txt"], "only the named entry is removed");
    }
    cleanup_listing(&id);
}

/// End-to-end on the real patch function: `notify_removed` is called by
/// `notify_directory_changed` with the URL full path (`parent_url.join(name)`).
/// It must drop the inner-path entry from the cache. Pre-fix (full-path match) this
/// left the entry in place; post-fix (name match) it is removed.
#[test]
fn notify_removed_drops_inner_mtp_entry_via_url_path() {
    let id = insert_mtp_style_listing("mtp_notify_removed");
    // Exactly what notify_directory_changed builds: parent URL joined with the name.
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents").join("notes.txt");
    notify_removed("mtp_notify_removed", &url);
    {
        let cache = LISTING_CACHE.read().unwrap();
        let names: Vec<&str> = cache
            .get("mtp_notify_removed")
            .unwrap()
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["report.txt"],
            "notify_removed drops notes.txt from the MTP listing"
        );
    }
    cleanup_listing(&id);
}

/// Name matching stays correct for local/SMB listings, whose entry paths already
/// share the notifier's path space (unique names in a directory).
#[test]
fn name_match_removes_local_style_entry() {
    let id = insert_test_listing(
        "local_name_hit",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![
            make_entry("alpha.txt", false, Some(1)),
            make_entry("beta.txt", false, Some(2)),
        ],
    );
    let removed = remove_entry_by_name("local_name_hit", std::ffi::OsStr::new("beta.txt"));
    assert_eq!(removed.expect("removed").1.name, "beta.txt");
    cleanup_listing(&id);
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
