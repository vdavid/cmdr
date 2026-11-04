//! `update_entry_sorted`: when a re-stat'd entry is patched in place and when
//! it moves to a new sorted position (`sorting::sort_key_changed`), under the
//! listing's effective sort.
//!
//! Split from `caching_test`, whose fixtures it shares.

use super::caching_test::{insert_test_listing, make_dir_entry, make_entry};
use super::diff::PaneRows;
use super::testing::TestEngine;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

#[test]
fn test_update_entry_sorted_in_place_for_non_sort_change() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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

    let result = listing.engine().inner.update_entry_sorted(listing.id(), updated);
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(1),
            after: Some(1)
        })
    );

    assert_eq!(listing.entries()[1].permissions, 0o755);
}

#[test]
fn test_update_entry_sorted_moved_for_size_change() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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
    let result = listing.engine().inner.update_entry_sorted(listing.id(), updated);
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(0),
            after: Some(2)
        })
    );

    assert_eq!(listing.entry_names(), ["medium.txt", "large.txt", "small.txt"]);
}

#[test]
fn test_update_entry_sorted_moved_for_modified_at_change() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
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

    let result = listing.engine().inner.update_entry_sorted(listing.id(), updated);
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(0),
            after: Some(1)
        })
    );
}

/// Regression anchor: under a date sort, a re-stat whose ONLY change is that
/// date re-places the row. Change detection once read size, mtime, and kind
/// alone, so a new creation time patched the row in place, out of order.
#[test]
fn test_update_entry_sorted_moves_a_row_whose_creation_time_changed() {
    let entry = |name: &str, created: u64| {
        let mut e = make_entry(name, false, Some(100));
        e.modified_at = Some(1000);
        e.created_at = Some(created);
        e
    };
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "update_created",
        "/test",
        SortColumn::Created,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![entry("first.txt", 1000), entry("second.txt", 2000)],
    );

    // first.txt is now the newest, and only its creation time moved.
    let result = listing
        .engine()
        .inner
        .update_entry_sorted(listing.id(), entry("first.txt", 3000));
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(0),
            after: Some(1)
        }),
        "a creation-time change moves the row"
    );
    assert_eq!(listing.entry_names(), ["second.txt", "first.txt"]);
    assert_eq!(listing.entries()[1].created_at, Some(3000), "the patch landed");

    // A kind change re-places it: folders sort first.
    let mut as_folder = entry("first.txt", 3000);
    as_folder.is_directory = true;
    as_folder.size = None;
    let result = listing.engine().inner.update_entry_sorted(listing.id(), as_folder);
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(1),
            after: Some(0)
        }),
        "a kind change moves the row"
    );
}

/// Under an Added sort, a new added time re-places the row: an item moved out
/// and back in lands where the sort says.
#[test]
fn test_update_entry_sorted_moves_a_row_whose_added_time_changed() {
    let entry = |name: &str, added: u64| {
        let mut e = make_entry(name, false, Some(100));
        e.modified_at = Some(1000);
        e.added_at = Some(added);
        e
    };
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "update_added",
        "/test",
        SortColumn::Added,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        vec![entry("newest.txt", 2000), entry("older.txt", 1000)],
    );

    let result = listing
        .engine()
        .inner
        .update_entry_sorted(listing.id(), entry("older.txt", 3000));
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(1),
            after: Some(0)
        })
    );
    assert_eq!(listing.entry_names(), ["older.txt", "newest.txt"]);
}

/// Under a Size sort with folders ranked like files, a folder's key is its
/// recursive size, so a folder that grew moves.
#[test]
fn test_update_entry_sorted_moves_a_folder_whose_recursive_size_changed() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "update_dir_size",
        "/test",
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_dir_entry("grows", Some(10)), make_dir_entry("stays", Some(500))],
    );

    let result = listing
        .engine()
        .inner
        .update_entry_sorted(listing.id(), make_dir_entry("grows", Some(5000)));
    assert_eq!(
        result,
        Some(PaneRows {
            before: Some(0),
            after: Some(1)
        })
    );
    assert_eq!(listing.entry_names(), ["stays", "grows"]);
}

#[test]
fn test_update_entry_sorted_returns_none_for_missing_entry() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "update_miss",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("alpha.txt", false, Some(100))],
    );

    let result = listing
        .engine()
        .inner
        .update_entry_sorted(listing.id(), make_entry("nonexistent.txt", false, Some(100)));
    assert!(result.is_none());
}

#[test]
fn test_update_entry_sorted_returns_none_for_missing_listing() {
    let engine = TestEngine::new();
    let result = engine
        .inner
        .update_entry_sorted("nonexistent_listing", make_entry("test.txt", false, Some(100)));
    assert!(result.is_none());
}
