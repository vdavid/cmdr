//! Tests for listing cache helpers.

use std::path::PathBuf;

use super::caching::{
    ModifyResult, apply_tags_to_listing, carry_forward_tags, find_listings_for_path, find_listings_for_path_on_volume,
    has_entry, insert_entry_sorted, notify_added, notify_removed, remove_entry_by_name, remove_entry_by_path,
    update_entry_sorted,
};
use super::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use super::metadata::{FileEntry, TagRef};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

/// Reads the tags currently cached for `path` in `listing`.
fn cached_tags(listing: &TestListingGuard, path: &str) -> Vec<TagRef> {
    listing.with_listing(|cached| {
        cached
            .entries
            .iter()
            .find(|e| e.path == path)
            .expect("entry is cached")
            .tags
            .clone()
    })
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

/// Caches a listing under a unique id derived from `tag`. The returned guard owns
/// the cache entry and tears it down on drop, unwind included.
fn insert_test_listing(
    tag: &str,
    path: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
) -> TestListingGuard {
    insert_test_listing_on_volume(tag, "root", path, sort_by, sort_order, dir_sort_mode, entries)
}

fn insert_test_listing_on_volume(
    tag: &str,
    volume_id: &str,
    path: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
) -> TestListingGuard {
    TestListing::new()
        .volume(volume_id)
        .path(path)
        .sort(sort_by, sort_order, dir_sort_mode)
        .entries(entries)
        .insert(tag)
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
    // A path unique to this test: `find_listings_for_path` searches the whole
    // process-global cache, so a shared path would also match a sibling test's
    // listing and break the count assertion.
    let path = format!("/home/user/{}", unique_test_id("find-1match"));
    let listing = insert_test_listing(
        "find_1match",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path(&PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, listing.id());
    assert_eq!(results[0].1, SortColumn::Name);
    assert_eq!(results[0].2, SortOrder::Ascending);
    assert_eq!(results[0].3, DirectorySortMode::LikeFiles);
}

#[test]
fn test_find_listings_for_path_two_matches() {
    // Two panes on ONE directory, at a path unique to this test (see
    // `test_find_listings_for_path_one_match` for why the path can't be shared).
    let path = format!("/shared/dir/{}", unique_test_id("find-2match"));
    let listing1 = insert_test_listing(
        "find_2match_a",
        &path,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    let listing2 = insert_test_listing(
        "find_2match_b",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path(&PathBuf::from(&path));
    assert_eq!(results.len(), 2);

    // Both IDs should be present (order unspecified since HashMap is unordered)
    let ids: Vec<&str> = results.iter().map(|(id, _, _, _)| id.as_str()).collect();
    assert!(ids.contains(&listing1.id()));
    assert!(ids.contains(&listing2.id()));
}

// ============================================================================
// insert_entry_sorted tests
// ============================================================================

#[test]
fn test_insert_entry_sorted_name_asc() {
    let listing = insert_test_listing(
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
    let index = insert_entry_sorted(listing.id(), make_entry("beta.txt", false, Some(100)));
    assert_eq!(index, Some(1));

    assert_eq!(listing.entry_names(), ["alpha.txt", "beta.txt", "gamma.txt"]);
}

#[test]
fn test_insert_entry_sorted_size_desc_dirs_first() {
    let listing = insert_test_listing(
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
    let index = insert_entry_sorted(listing.id(), make_dir_entry("mid_dir", Some(5000)));
    assert_eq!(index, Some(1));

    assert_eq!(
        listing.entry_names(),
        ["big_dir", "mid_dir", "small_dir", "large.txt", "tiny.txt"]
    );
}

#[test]
fn test_insert_entry_sorted_returns_none_for_duplicate() {
    let listing = insert_test_listing(
        "insert_dup",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    // Try inserting an entry with the same path
    let result = insert_entry_sorted(listing.id(), make_entry("alpha.txt", false, Some(200)));
    assert_eq!(result, None);

    assert_eq!(listing.entries().len(), 1);
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
    let listing = insert_test_listing(
        "notify_added_upsert",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // First observation: partial size (what the watcher would see mid-write).
    notify_added(listing.id(), make_entry("photo.jpg", false, Some(2_359_284)));
    // Second observation: final size (what the post-close stat sees).
    notify_added(listing.id(), make_entry("photo.jpg", false, Some(4_989_168)));

    let entries = listing.entries();
    assert_eq!(entries.len(), 1, "should still be exactly one entry, not duplicated");
    assert_eq!(
        entries[0].size,
        Some(4_989_168),
        "second (final) size must overwrite the partial-size observation"
    );
}

// ============================================================================
// remove_entry_by_path tests
// ============================================================================

#[test]
fn test_remove_entry_by_path_returns_correct_index_and_entry() {
    let listing = insert_test_listing(
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

    let result = remove_entry_by_path(listing.id(), &PathBuf::from("/test/beta.txt"));
    assert!(result.is_some());
    let (idx, entry) = result.unwrap();
    assert_eq!(idx, 1);
    assert_eq!(entry.name, "beta.txt");

    assert_eq!(listing.entry_names(), ["alpha.txt", "gamma.txt"]);
}

#[test]
fn test_remove_entry_by_path_returns_none_for_missing_entry() {
    let listing = insert_test_listing(
        "remove_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let result = remove_entry_by_path(listing.id(), &PathBuf::from("/test/nonexistent.txt"));
    assert!(result.is_none());
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
fn insert_mtp_style_listing(tag: &str) -> TestListingGuard {
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
        tag,
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
    let listing = insert_mtp_style_listing("mtp_fullpath_miss");
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents/notes.txt");
    assert!(
        remove_entry_by_path(listing.id(), &url).is_none(),
        "URL full-path can't match an inner-path entry — the silent no-op this fix removes"
    );
}

/// The fix: `remove_entry_by_name` matches by the entry's file name within the
/// directory listing, so the inner-path entry is found from the URL notifier path.
#[test]
fn name_match_removes_inner_mtp_entry() {
    let listing = insert_mtp_style_listing("mtp_name_hit");
    let removed = remove_entry_by_name(listing.id(), std::ffi::OsStr::new("notes.txt"));
    assert!(removed.is_some(), "name match removes the inner-path entry");
    assert_eq!(removed.unwrap().1.name, "notes.txt");
    assert_eq!(listing.entry_names(), ["report.txt"], "only the named entry is removed");
}

/// End-to-end on the real patch function: `notify_removed` is called by
/// `notify_directory_changed` with the URL full path (`parent_url.join(name)`).
/// It must drop the inner-path entry from the cache. Pre-fix (full-path match) this
/// left the entry in place; post-fix (name match) it is removed.
#[test]
fn notify_removed_drops_inner_mtp_entry_via_url_path() {
    let listing = insert_mtp_style_listing("mtp_notify_removed");
    // Exactly what notify_directory_changed builds: parent URL joined with the name.
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents").join("notes.txt");
    notify_removed(listing.id(), &url);
    assert_eq!(
        listing.entry_names(),
        ["report.txt"],
        "notify_removed drops notes.txt from the MTP listing"
    );
}

/// Name matching stays correct for local/SMB listings, whose entry paths already
/// share the notifier's path space (unique names in a directory).
#[test]
fn name_match_removes_local_style_entry() {
    let listing = insert_test_listing(
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
    let removed = remove_entry_by_name(listing.id(), std::ffi::OsStr::new("beta.txt"));
    assert_eq!(removed.expect("removed").1.name, "beta.txt");
}

// ============================================================================
// has_entry tests
// ============================================================================

#[test]
fn test_has_entry_true_for_existing() {
    let listing = insert_test_listing(
        "has_entry_yes",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(has_entry(listing.id(), "/test/alpha.txt"));
}

#[test]
fn test_has_entry_false_for_missing() {
    let listing = insert_test_listing(
        "has_entry_no",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(!has_entry(listing.id(), "/test/nonexistent.txt"));
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
    let listing = insert_test_listing(
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

    let result = update_entry_sorted(listing.id(), updated);
    assert!(matches!(result, Some(ModifyResult::UpdatedInPlace { index: 1 })));

    assert_eq!(listing.entries()[1].permissions, 0o755);
}

#[test]
fn test_update_entry_sorted_moved_for_size_change() {
    let listing = insert_test_listing(
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
    let result = update_entry_sorted(listing.id(), updated);
    assert!(matches!(
        result,
        Some(ModifyResult::Moved {
            old_index: 0,
            new_index: 2
        })
    ));

    assert_eq!(listing.entry_names(), ["medium.txt", "large.txt", "small.txt"]);
}

#[test]
fn test_update_entry_sorted_moved_for_modified_at_change() {
    let listing = insert_test_listing(
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

    let result = update_entry_sorted(listing.id(), updated);
    assert!(matches!(
        result,
        Some(ModifyResult::Moved {
            old_index: 0,
            new_index: 1
        })
    ));
}

#[test]
fn test_update_entry_sorted_returns_none_for_missing_entry() {
    let listing = insert_test_listing(
        "update_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let result = update_entry_sorted(listing.id(), make_entry("nonexistent.txt", false, Some(100)));
    assert!(result.is_none());
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
    // Path and SMB volume id unique to this test: the lookups below count matches
    // across the whole process-global cache.
    let path = format!("/shared/dir/{}", unique_test_id("vol-filter"));
    let smb = unique_test_id("smb-nas");
    let listing1 = insert_test_listing_on_volume(
        "vol_filter_root",
        "root",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );
    let listing2 = insert_test_listing_on_volume(
        "vol_filter_smb",
        &smb,
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // Filter by "root": only the root listing.
    let results = find_listings_for_path_on_volume(Some("root"), &PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, listing1.id());

    // Filter by the SMB volume: only the SMB listing.
    let results = find_listings_for_path_on_volume(Some(&smb), &PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, listing2.id());

    // No filter: both.
    let results = find_listings_for_path_on_volume(None, &PathBuf::from(&path));
    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_listings_for_path_on_volume_no_match() {
    let path = format!("/some/{}", unique_test_id("vol-nomatch"));
    let _listing = insert_test_listing_on_volume(
        "vol_nomatch",
        "root",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_for_path_on_volume(Some("smb-nas"), &PathBuf::from(&path));
    assert!(results.is_empty());
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

    // Volume id unique to this test: `find_listings_on_volume` scans the whole
    // process-global cache, so a shared id would also match a sibling test's listing.
    let share = unique_test_id("smb-share");
    let listing1 = insert_test_listing_on_volume(
        "flov_listing1",
        &share,
        "/mnt/share/docs",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("readme.txt", false, Some(100))],
    );
    let listing2 = insert_test_listing_on_volume(
        "flov_listing2",
        &share,
        "/mnt/share/photos",
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    // Different volume: should not be returned
    let _listing3 = insert_test_listing_on_volume(
        "flov_other",
        &unique_test_id("different-vol"),
        "/mnt/other",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = find_listings_on_volume(&share);
    assert_eq!(
        results.len(),
        2,
        "Expected 2 listings for {share}, got {}",
        results.len()
    );

    let ids: Vec<&str> = results.iter().map(|(id, ..)| id.as_str()).collect();
    assert!(ids.contains(&listing1.id()));
    assert!(ids.contains(&listing2.id()));

    // Verify paths and sort params are returned correctly
    let listing1 = results.iter().find(|(id, ..)| id == listing1.id()).unwrap();
    assert_eq!(listing1.1, PathBuf::from("/mnt/share/docs"));
    assert_eq!(listing1.2, SortColumn::Name);

    let listing2 = results.iter().find(|(id, ..)| id == listing2.id()).unwrap();
    assert_eq!(listing2.1, PathBuf::from("/mnt/share/photos"));
    assert_eq!(listing2.2, SortColumn::Size);
    assert_eq!(listing2.3, SortOrder::Descending);
    assert_eq!(listing2.4, DirectorySortMode::AlwaysByName);
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
    let listing = insert_test_listing(
        "tags_apply",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1)), make_entry("b.txt", false, Some(2))],
    );

    apply_tags_to_listing(
        listing.id(),
        vec![("/test/a.txt".to_string(), vec![tag("Red", 6), tag("Work", 0)])],
    );

    assert_eq!(
        cached_tags(&listing, "/test/a.txt"),
        vec![tag("Red", 6), tag("Work", 0)]
    );
    assert_eq!(cached_tags(&listing, "/test/b.txt"), Vec::<TagRef>::new());
}

#[test]
fn apply_tags_clears_tags_on_external_removal() {
    // A file that already has tags; an empty read must clear them (removal
    // propagation — the counterpart to carry-forward).
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let listing = insert_test_listing(
        "tags_clear",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    apply_tags_to_listing(listing.id(), vec![("/test/a.txt".to_string(), Vec::new())]);

    assert_eq!(cached_tags(&listing, "/test/a.txt"), Vec::<TagRef>::new());
}

#[test]
fn apply_tags_skips_unknown_paths() {
    let listing = insert_test_listing(
        "tags_unknown",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1))],
    );

    // Path not in the listing (scrolled away / removed): no panic, no change.
    apply_tags_to_listing(listing.id(), vec![("/test/gone.txt".to_string(), vec![tag("Blue", 4)])]);

    assert_eq!(cached_tags(&listing, "/test/a.txt"), Vec::<TagRef>::new());
}

#[test]
fn carry_forward_restores_tags_on_empty_restat() {
    // Simulates a watcher re-stat: the new entry has empty tags (get_single_entry
    // reads no xattr), so carry-forward must restore the cached tags.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Green", 2)];
    let listing = insert_test_listing(
        "tags_carry",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut restat = make_entry("a.txt", false, Some(99)); // empty tags, like a fresh stat
    carry_forward_tags(listing.id(), &mut restat);
    assert_eq!(restat.tags, vec![tag("Green", 2)], "carry-forward restores cached tags");

    // And after storing the re-stat through the modify path, the tags survive.
    update_entry_sorted(listing.id(), restat);
    assert_eq!(cached_tags(&listing, "/test/a.txt"), vec![tag("Green", 2)]);
}

#[test]
fn carry_forward_does_not_overwrite_incoming_tags() {
    // When the incoming entry already carries tags (the enrich path), carry-forward
    // must leave them untouched so a real change isn't masked.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let listing = insert_test_listing(
        "tags_no_overwrite",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut incoming = make_entry("a.txt", false, Some(1));
    incoming.tags = vec![tag("Blue", 4)];
    carry_forward_tags(listing.id(), &mut incoming);
    assert_eq!(
        incoming.tags,
        vec![tag("Blue", 4)],
        "carry-forward must not clobber incoming tags"
    );
}

// ============================================================================
// FullRefresh dispatch from a runtime-less thread
// ============================================================================

#[test]
fn spawn_full_refresh_survives_a_thread_with_no_tokio_runtime() {
    // The notify-rs debouncer, the SMB/MTP watcher threads, and the git watcher all
    // call `notify_directory_changed(FullRefresh)` from a plain OS thread that was
    // never entered from a Tokio runtime. A bare `tokio::spawn` panics there ("there
    // is no reactor running"), which took the whole app down in v0.24.0 (CRASH-26SBB).
    use super::caching::spawn_full_refresh;

    let handle = std::thread::spawn(|| {
        spawn_full_refresh(
            "no-such-volume".to_string(),
            PathBuf::from("/test/spawn_full_refresh"),
            vec![(
                "sfr_listing".to_string(),
                SortColumn::Name,
                SortOrder::Ascending,
                DirectorySortMode::LikeFiles,
            )],
        );
    });

    assert!(
        handle.join().is_ok(),
        "dispatching a FullRefresh from a runtime-less thread must not panic"
    );
}
