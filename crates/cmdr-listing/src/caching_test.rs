//! Tests for listing cache helpers.

use std::path::{Path, PathBuf};

use super::Listings;
use super::caching::DirectoryChange;
use super::diff::PaneRows;
use super::testing::{TestEngine, TestListing, TestListingGuard};
use cmdr_fs::entry::{FileEntry, TagRef};
use cmdr_fs::testing::unique_test_id;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

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
            .entries()
            .iter()
            .find(|e| e.path == path)
            .expect("entry is cached")
            .tags
            .clone()
    })
}

/// Creates a minimal test entry.
pub(super) fn make_entry(name: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        ..FileEntry::new(name.to_string(), format!("/test/{}", name), is_dir, false)
    }
}

pub(super) fn make_dir_entry(name: &str, recursive_size: Option<u64>) -> FileEntry {
    let mut e = make_entry(name, true, None);
    e.recursive_size = recursive_size;
    e
}

/// Caches a listing in `engine` under a unique id derived from `tag`.
pub(super) fn insert_test_listing(
    engine: &Listings,
    tag: &str,
    path: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
) -> TestListingGuard {
    insert_test_listing_on_volume(engine, tag, "root", path, sort_by, sort_order, dir_sort_mode, entries)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the engine, then every field a cached listing carries"
)]
fn insert_test_listing_on_volume(
    engine: &Listings,
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
        .insert(engine, tag)
}

// ============================================================================
// find_listings_for_path_on_volume(None, …) tests (the volume-less match)
// ============================================================================

#[test]
fn test_find_listings_for_path_zero_matches() {
    let engine = TestEngine::new();
    let results = engine
        .inner
        .find_listings_for_path_on_volume(None, &PathBuf::from("/nonexistent/path"));
    assert!(results.is_empty());
}

#[test]
fn test_find_listings_for_path_one_match() {
    let engine = TestEngine::new();
    let path = format!("/home/user/{}", unique_test_id("find-1match"));
    let listing = insert_test_listing(
        &engine,
        "find_1match",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = engine
        .inner
        .find_listings_for_path_on_volume(None, &PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results, [listing.id()]);
}

#[test]
fn test_find_listings_for_path_two_matches() {
    let engine = TestEngine::new();
    // Two panes on ONE directory.
    let path = format!("/shared/dir/{}", unique_test_id("find-2match"));
    let listing1 = insert_test_listing(
        &engine,
        "find_2match_a",
        &path,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::AlwaysByName,
        vec![],
    );
    let listing2 = insert_test_listing(
        &engine,
        "find_2match_b",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = engine
        .inner
        .find_listings_for_path_on_volume(None, &PathBuf::from(&path));
    assert_eq!(results.len(), 2);

    // Both IDs should be present (order unspecified since HashMap is unordered)
    let ids: Vec<&str> = results.iter().map(String::as_str).collect();
    assert!(ids.contains(&listing1.id()));
    assert!(ids.contains(&listing2.id()));
}

// ============================================================================
// insert_entry_sorted tests
// ============================================================================

#[test]
fn test_insert_entry_sorted_name_asc() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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
    let rows = listing
        .engine()
        .inner
        .insert_entry_sorted(listing.id(), make_entry("beta.txt", false, Some(100)));
    assert_eq!(
        rows,
        Some(PaneRows {
            before: None,
            after: Some(1)
        })
    );

    assert_eq!(listing.entry_names(), ["alpha.txt", "beta.txt", "gamma.txt"]);
}

#[test]
fn test_insert_entry_sorted_size_desc_dirs_first() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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
    let rows = listing
        .engine()
        .inner
        .insert_entry_sorted(listing.id(), make_dir_entry("mid_dir", Some(5000)));
    assert_eq!(
        rows,
        Some(PaneRows {
            before: None,
            after: Some(1)
        })
    );

    assert_eq!(
        listing.entry_names(),
        ["big_dir", "mid_dir", "small_dir", "large.txt", "tiny.txt"]
    );
}

#[test]
fn test_insert_entry_sorted_returns_none_for_duplicate() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "insert_dup",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    // Try inserting an entry with the same path
    let result = listing
        .engine()
        .inner
        .insert_entry_sorted(listing.id(), make_entry("alpha.txt", false, Some(200)));
    assert_eq!(result, None);

    assert_eq!(listing.entries().len(), 1);
}

#[test]
fn test_insert_entry_sorted_returns_none_for_missing_listing() {
    let engine = TestEngine::new();
    let result = engine
        .inner
        .insert_entry_sorted("nonexistent_listing_id", make_entry("test.txt", false, Some(100)));
    assert_eq!(result, None);
}

#[test]
fn notify_added_upserts_when_entry_already_present() {
    let engine = TestEngine::new();
    // Race that motivated the upsert: SMB watcher fires an Added event mid-write
    // (stat catches the file at partial size), then `write_from_stream`'s own
    // post-close `notify_mutation` fires its Added with the final size. Without
    // upsert the first-write wins (Samba's mid-write partial size sticks) and
    // the FE shows the wrong size until the next manual refresh. With upsert
    // the second observation updates the cached entry to the final size.
    let listing = insert_test_listing(
        &engine,
        "notify_added_upsert",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // First observation: partial size (what the watcher would see mid-write).
    listing
        .engine()
        .inner
        .notify_added(listing.id(), make_entry("photo.jpg", false, Some(2_359_284)));
    // Second observation: final size (what the post-close stat sees).
    listing
        .engine()
        .inner
        .notify_added(listing.id(), make_entry("photo.jpg", false, Some(4_989_168)));

    let entries = listing.entries();
    assert_eq!(entries.len(), 1, "should still be exactly one entry, not duplicated");
    assert_eq!(
        entries[0].size,
        Some(4_989_168),
        "second (final) size must overwrite the partial-size observation"
    );
}

/// A directory change patches the panes with no `AppHandle` anywhere: the
/// engine reports through its events seam, which drops what nobody hears, so
/// there's nothing to gate on. In the app no listing exists before setup.
#[test]
fn a_directory_change_patches_a_listing_in_a_process_with_no_app_handle() {
    let engine = TestEngine::new();
    let volume_id = unique_test_id("notify-no-app-vol");
    let listing = insert_test_listing_on_volume(
        &engine,
        "notify-no-app",
        &volume_id,
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1))],
    );

    engine.inner.notify_directory_changed(
        &volume_id,
        Path::new("/test"),
        DirectoryChange::Added(make_entry("b.txt", false, Some(2))),
    );

    assert_eq!(listing.entry_names(), ["a.txt", "b.txt"]);
}

// ============================================================================
// remove_entries_by_paths tests
// ============================================================================

#[test]
fn test_remove_entries_by_paths_returns_correct_index_and_entry() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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

    let removed = listing
        .engine()
        .inner
        .remove_entries_by_paths(listing.id(), &[PathBuf::from("/test/beta.txt")]);
    assert_eq!(removed.len(), 1);
    let (rows, entry) = &removed[0];
    assert_eq!(
        *rows,
        PaneRows {
            before: Some(1),
            after: None
        }
    );
    assert_eq!(entry.name, "beta.txt");

    assert_eq!(listing.entry_names(), ["alpha.txt", "gamma.txt"]);
}

#[test]
fn test_remove_entries_by_paths_skips_a_missing_entry() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "remove_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let removed = listing
        .engine()
        .inner
        .remove_entries_by_paths(listing.id(), &[PathBuf::from("/test/nonexistent.txt")]);
    assert!(removed.is_empty());
}

#[test]
fn test_remove_entries_by_paths_returns_nothing_for_missing_listing() {
    let engine = TestEngine::new();
    let removed = engine
        .inner
        .remove_entries_by_paths("nonexistent_listing", &[PathBuf::from("/test/foo.txt")]);
    assert!(removed.is_empty());
}

// ============================================================================
// Removed patch matches by NAME, not full path (MTP inner-path vs URL notifier)
// ============================================================================

/// Builds an MTP-shaped listing: the directory is the absolute `mtp://…` URL (as
/// pane navigation stores it), while each entry's `path` is the storage-relative
/// INNER form (as `MtpVolume::list_directory` produces it).
fn insert_mtp_style_listing(engine: &Listings, tag: &str) -> TestListingGuard {
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
        engine,
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
    let engine = TestEngine::new();
    let listing = insert_mtp_style_listing(&engine, "mtp_fullpath_miss");
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents/notes.txt");
    assert!(
        listing
            .engine()
            .inner
            .remove_entries_by_paths(listing.id(), &[url])
            .is_empty(),
        "URL full-path can't match an inner-path entry — the silent no-op this fix removes"
    );
}

/// The fix: `remove_entry_by_name` matches by the entry's file name within the
/// directory listing, so the inner-path entry is found from the URL notifier path.
#[test]
fn name_match_removes_inner_mtp_entry() {
    let engine = TestEngine::new();
    let listing = insert_mtp_style_listing(&engine, "mtp_name_hit");
    let removed = listing
        .engine()
        .inner
        .remove_entry_by_name(listing.id(), std::ffi::OsStr::new("notes.txt"));
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
    let engine = TestEngine::new();
    let listing = insert_mtp_style_listing(&engine, "mtp_notify_removed");
    // Exactly what notify_directory_changed builds: parent URL joined with the name.
    let url = PathBuf::from("mtp://mtp-dev/65537/Documents").join("notes.txt");
    listing.engine().inner.notify_removed(listing.id(), &url);
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
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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
    let removed = listing
        .engine()
        .inner
        .remove_entry_by_name(listing.id(), std::ffi::OsStr::new("beta.txt"));
    assert_eq!(removed.expect("removed").1.name, "beta.txt");
}

// ============================================================================
// has_entry tests
// ============================================================================

#[test]
fn test_has_entry_true_for_existing() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "has_entry_yes",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(listing.engine().inner.has_entry(listing.id(), "/test/alpha.txt"));
}

#[test]
fn test_has_entry_false_for_missing() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "has_entry_no",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    assert!(!listing.engine().inner.has_entry(listing.id(), "/test/nonexistent.txt"));
}

#[test]
fn test_has_entry_false_for_missing_listing() {
    let engine = TestEngine::new();
    assert!(!engine.inner.has_entry("nonexistent_listing", "/test/foo.txt"));
}

// ============================================================================
// find_listings_for_path_on_volume tests
// ============================================================================

#[test]
fn test_find_listings_for_path_on_volume_filters_by_volume() {
    let engine = TestEngine::new();
    let path = format!("/shared/dir/{}", unique_test_id("vol-filter"));
    let smb = unique_test_id("smb-nas");
    let listing1 = insert_test_listing_on_volume(
        &engine,
        "vol_filter_root",
        "root",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );
    let listing2 = insert_test_listing_on_volume(
        &engine,
        "vol_filter_smb",
        &smb,
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    // Filter by "root": only the root listing.
    let results = engine
        .inner
        .find_listings_for_path_on_volume(Some("root"), &PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results, [listing1.id()]);

    // Filter by the SMB volume: only the SMB listing.
    let results = engine
        .inner
        .find_listings_for_path_on_volume(Some(&smb), &PathBuf::from(&path));
    assert_eq!(results.len(), 1);
    assert_eq!(results, [listing2.id()]);

    // No filter: both.
    let results = engine
        .inner
        .find_listings_for_path_on_volume(None, &PathBuf::from(&path));
    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_listings_for_path_on_volume_no_match() {
    let engine = TestEngine::new();
    let path = format!("/some/{}", unique_test_id("vol-nomatch"));
    let _listing = insert_test_listing_on_volume(
        &engine,
        "vol_nomatch",
        "root",
        &path,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = engine
        .inner
        .find_listings_for_path_on_volume(Some("smb-nas"), &PathBuf::from(&path));
    assert!(results.is_empty());
}

// ============================================================================
// find_listings_on_volume tests (FullRefresh fallback path)
// ============================================================================
//
// The FullRefresh re-read and its ordering are the cells at the end of this
// file; these pin the `find_listings_on_volume` lookup its fallback path uses.

#[test]
fn test_find_listings_on_volume_returns_all_volume_listings() {
    let engine = TestEngine::new();
    let share = unique_test_id("smb-share");
    let listing1 = insert_test_listing_on_volume(
        &engine,
        "flov_listing1",
        &share,
        "/mnt/share/docs",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("readme.txt", false, Some(100))],
    );
    let listing2 = insert_test_listing_on_volume(
        &engine,
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
        &engine,
        "flov_other",
        &unique_test_id("different-vol"),
        "/mnt/other",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![],
    );

    let results = engine.inner.find_listings_on_volume(&share);
    assert_eq!(
        results.len(),
        2,
        "Expected 2 listings for {share}, got {}",
        results.len()
    );

    let ids: Vec<&str> = results.iter().map(|(id, ..)| id.as_str()).collect();
    assert!(ids.contains(&listing1.id()));
    assert!(ids.contains(&listing2.id()));

    // Verify the paths are returned correctly
    let listing1 = results.iter().find(|(id, ..)| id == listing1.id()).unwrap();
    assert_eq!(listing1.1, PathBuf::from("/mnt/share/docs"));

    let listing2 = results.iter().find(|(id, ..)| id == listing2.id()).unwrap();
    assert_eq!(listing2.1, PathBuf::from("/mnt/share/photos"));
}

#[test]
fn test_find_listings_on_volume_empty_for_unknown_volume() {
    let engine = TestEngine::new();

    let results = engine.inner.find_listings_on_volume("nonexistent-volume-id");
    assert!(results.is_empty());
}

// ============================================================================
// Finder-tag enrichment + carry-forward tests
// ============================================================================

#[test]
fn apply_tags_sets_tags_on_matching_entry() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "tags_apply",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1)), make_entry("b.txt", false, Some(2))],
    );

    listing.engine().inner.apply_tags_to_listing(
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
    let engine = TestEngine::new();
    // A file that already has tags; an empty read must clear them (removal
    // propagation — the counterpart to carry-forward).
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let listing = insert_test_listing(
        &engine,
        "tags_clear",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    listing
        .engine()
        .inner
        .apply_tags_to_listing(listing.id(), vec![("/test/a.txt".to_string(), Vec::new())]);

    assert_eq!(cached_tags(&listing, "/test/a.txt"), Vec::<TagRef>::new());
}

#[test]
fn apply_tags_skips_unknown_paths() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "tags_unknown",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("a.txt", false, Some(1))],
    );

    // Path not in the listing (scrolled away / removed): no panic, no change.
    listing
        .engine()
        .inner
        .apply_tags_to_listing(listing.id(), vec![("/test/gone.txt".to_string(), vec![tag("Blue", 4)])]);

    assert_eq!(cached_tags(&listing, "/test/a.txt"), Vec::<TagRef>::new());
}

#[test]
fn carry_forward_restores_tags_on_empty_restat() {
    let engine = TestEngine::new();
    // Simulates a watcher re-stat: the new entry has empty tags (get_single_entry
    // reads no xattr), so carry-forward must restore the cached tags.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Green", 2)];
    let listing = insert_test_listing(
        &engine,
        "tags_carry",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut restat = make_entry("a.txt", false, Some(99)); // empty tags, like a fresh stat
    listing.engine().inner.carry_forward_deferred(listing.id(), &mut restat);
    assert_eq!(restat.tags, vec![tag("Green", 2)], "carry-forward restores cached tags");

    // And after storing the re-stat through the modify path, the tags survive.
    listing.engine().inner.update_entry_sorted(listing.id(), restat);
    assert_eq!(cached_tags(&listing, "/test/a.txt"), vec![tag("Green", 2)]);
}

#[test]
fn carry_forward_does_not_overwrite_incoming_tags() {
    let engine = TestEngine::new();
    // When the incoming entry already carries tags (the enrich path), carry-forward
    // must leave them untouched so a real change isn't masked.
    let mut tagged = make_entry("a.txt", false, Some(1));
    tagged.tags = vec![tag("Red", 6)];
    let listing = insert_test_listing(
        &engine,
        "tags_no_overwrite",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![tagged],
    );

    let mut incoming = make_entry("a.txt", false, Some(1));
    incoming.tags = vec![tag("Blue", 4)];
    listing
        .engine()
        .inner
        .carry_forward_deferred(listing.id(), &mut incoming);
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
    let engine = TestEngine::new();
    let inner = std::sync::Arc::clone(&engine.inner);
    let handle = std::thread::spawn(move || {
        inner.spawn_full_refresh(
            "no-such-volume".to_string(),
            PathBuf::from("/test/spawn_full_refresh"),
            vec!["sfr_listing".to_string()],
        );
    });

    assert!(
        handle.join().is_ok(),
        "dispatching a FullRefresh from a runtime-less thread must not panic"
    );
}

// ============================================================================
// Concurrent FullRefresh ordering
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_refresh_cannot_overwrite_the_listing_a_newer_one_already_wrote() {
    let engine = TestEngine::new();
    // The hazard, seen as an E2E flake on 2026-08-30: a burst of writes into a watched
    // folder fires several `FullRefresh`es, each of which reads the directory and then
    // REPLACES the cached listing wholesale. Read and write are not atomic together, so
    // a read that started earlier can land later and reinstate a directory state that
    // has already been superseded. Nothing re-reads afterwards, so the pane keeps
    // showing the older truth until some unrelated event happens to correct it: files
    // that are on disk are simply missing, for as long as the folder stays quiet.
    //
    // Scripted so the FIRST read is the SLOW one: refresh A sees the folder mid-write
    // (`alpha` only) and takes 150 ms; refresh B sees it complete and returns at once.
    use super::testing::ScriptedListVolume;
    use std::sync::Arc;
    use std::time::Duration;

    let volume_id = unique_test_id("refresh-order-vol");
    let dir = PathBuf::from(format!("/test/{}", unique_test_id("refresh-order-dir")));

    let mid_write = vec![make_entry("alpha", true, None)];
    let complete = vec![
        make_entry("alpha", true, None),
        make_entry("bravo", true, None),
        make_entry("charlie", true, None),
        make_entry("delta.txt", false, Some(4)),
    ];

    engine.volumes.register(
        &volume_id,
        Arc::new(ScriptedListVolume::new(
            "scripted",
            vec![
                (Duration::from_millis(150), mid_write.clone()),
                (Duration::from_millis(0), complete.clone()),
            ],
        )),
    );

    let listing = TestListing::new()
        .volume(&volume_id)
        .path(dir.clone())
        .entries(mid_write.clone())
        .insert(&engine, "refresh-order");

    let listings = vec![listing.id().to_string()];

    // A starts first and reads the mid-write folder slowly; B starts second and reads
    // the finished folder immediately. Both are in flight at once, exactly as two
    // watcher events during one burst are.
    let a = engine
        .inner
        .notify_full_refresh(volume_id.clone(), dir.clone(), listings.clone());
    let b = async {
        // allowed-test-sleep: a deliberate head start, so A is provably the refresh that
        // started first. The ORDER is the subject; there is no condition to wait on.
        tokio::time::sleep(Duration::from_millis(20)).await;
        engine
            .inner
            .notify_full_refresh(volume_id.clone(), dir.clone(), listings.clone())
            .await;
    };
    tokio::join!(a, b);

    let names: Vec<String> = listing.entries().into_iter().map(|e| e.name).collect();
    assert_eq!(
        names,
        vec!["alpha", "bravo", "charlie", "delta.txt"],
        "the cached listing must end at the newest directory state; the slow refresh's \
         stale snapshot reinstated entries that no longer reflect the folder"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_storm_of_refreshes_costs_two_reads_of_the_directory_not_one_per_event() {
    let engine = TestEngine::new();
    // Serializing refreshes would be enough for correctness on its own, but it would
    // also queue one read per watcher event, and a burst on a big directory is exactly
    // when that is most expensive. At most one refresh runs and one waits: a third
    // arrival returns immediately, because the one already queued starts its read AFTER
    // the running one finishes and so cannot answer with anything staler.
    use super::testing::ScriptedListVolume;
    use std::sync::Arc;
    use std::time::Duration;

    let volume_id = unique_test_id("refresh-storm-vol");
    let dir = PathBuf::from(format!("/test/{}", unique_test_id("refresh-storm-dir")));
    let settled = vec![make_entry("alpha", true, None), make_entry("bravo", true, None)];

    // Six scripted reads available; a slow first one guarantees the rest pile up.
    let volume = Arc::new(ScriptedListVolume::new(
        "scripted",
        (0..6)
            .map(|i| {
                (
                    if i == 0 {
                        Duration::from_millis(120)
                    } else {
                        Duration::from_millis(0)
                    },
                    settled.clone(),
                )
            })
            .collect(),
    ));
    engine.volumes.register(&volume_id, volume.clone());

    let listing = TestListing::new()
        .volume(&volume_id)
        .path(dir.clone())
        .entries(Vec::new())
        .insert(&engine, "refresh-storm");
    let listings = vec![listing.id().to_string()];

    let first = engine
        .inner
        .notify_full_refresh(volume_id.clone(), dir.clone(), listings.clone());
    let rest = async {
        // allowed-test-sleep: the burst has to land while the first read is still in
        // flight, which is a head start, not a condition anything can signal.
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Five more events land while the first read is still in flight, all at once —
        // awaiting them in turn would serialize the ARRIVALS and never test coalescing.
        let mut handles = Vec::new();
        for _ in 0..5 {
            let (v, d, l) = (volume_id.clone(), dir.clone(), listings.clone());
            let inner = Arc::clone(&engine.inner);
            handles.push(tokio::spawn(async move { inner.notify_full_refresh(v, d, l).await }));
        }
        for h in handles {
            let _ = h.await;
        }
    };
    tokio::join!(first, rest);

    assert_eq!(
        volume.unread(),
        4,
        "six events during one burst must cost two directory reads (one running, one queued), not six"
    );
    let names: Vec<String> = listing.entries().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["alpha", "bravo"], "and the listing still ends settled");
}
