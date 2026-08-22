//! How many times an accessor is allowed to walk the listing.
//!
//! The defect these pin: a pane parked at the bottom of a 74,144-entry directory
//! answered one MCP mirror sync with ~7.4 million visibility-predicate
//! evaluations on the main thread, and the app stopped answering IPC at all
//! (`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`). The cost is a scan
//! COUNT, so that is what these assert; a wall-clock assertion would be both
//! flaky and silent on a small fixture.

use std::path::Path;
use std::sync::Arc;

use super::caching_test_support::{TestListing, TestListingGuard};
use super::metadata::FileEntry;
use super::operations::{find_file_index, get_file_at, get_file_range, get_total_count};
use crate::file_system::listing::caching::insert_entry_sorted;
use crate::file_system::staging::{ShowTempsGuard, StagingTemp};

use super::visible_rows::scan_probe;

/// Big enough that a per-row rescan is unmistakable in the count, small enough
/// that the fixture builds in milliseconds.
const ENTRY_COUNT: usize = 20_000;

/// How many rows the MCP pane mirror fetches for its visible range.
const VISIBLE_ROWS: usize = 100;

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/big/{name}"), false, false)
}

fn big_listing(tag: &str) -> TestListingGuard {
    let entries = (0..ENTRY_COUNT).map(|i| entry(&format!("file-{i:06}.bin"))).collect();
    TestListing::new()
        .volume("test")
        .path("/big")
        .entries(entries)
        .insert(tag)
}

/// Entries examined while running `f`, on this thread.
fn examined_while(f: impl FnOnce()) -> u64 {
    let before = scan_probe::examined();
    f();
    scan_probe::examined() - before
}

/// The wedge itself: the MCP mirror asks for the last 100 rows one at a time.
/// Each of those is one `#[tauri::command]` on the main thread, so 100 full
/// walks of a big directory is what stops the app answering.
#[test]
fn fetching_a_hundred_rows_near_the_end_walks_the_listing_at_most_once() {
    let listing = big_listing("visible-rows-tail");
    let first = ENTRY_COUNT - VISIBLE_ROWS;

    let examined = examined_while(|| {
        for index in first..ENTRY_COUNT {
            let row = get_file_at(listing.id(), index, true).expect("listing is cached");
            assert_eq!(row.expect("row is in bounds").name, format!("file-{index:06}.bin"));
        }
    });

    assert!(
        examined <= ENTRY_COUNT as u64,
        "100 row fetches examined {examined} entries; one walk of {ENTRY_COUNT} is the budget"
    );
}

/// Depth must not be the multiplier. The same 100 fetches at the top and at the
/// bottom of a listing cost the same, so a user scrolling down doesn't pay more.
#[test]
fn a_row_near_the_end_costs_what_a_row_near_the_start_costs() {
    let listing = big_listing("visible-rows-depth");

    let at_top = examined_while(|| {
        for index in 0..VISIBLE_ROWS {
            get_file_at(listing.id(), index, true).expect("listing is cached");
        }
    });
    let at_bottom = examined_while(|| {
        for index in ENTRY_COUNT - VISIBLE_ROWS..ENTRY_COUNT {
            get_file_at(listing.id(), index, true).expect("listing is cached");
        }
    });

    assert!(
        at_bottom <= at_top + VISIBLE_ROWS as u64,
        "rows near the end examined {at_bottom} entries against {at_top} near the start"
    );
}

/// The out-of-bounds answer the frontend races into during a refetch costs a
/// walk of its own today, on top of the one that already came up empty.
#[test]
fn an_out_of_bounds_row_walks_the_listing_at_most_once() {
    let listing = big_listing("visible-rows-oob");

    let examined = examined_while(|| {
        assert!(
            get_file_at(listing.id(), ENTRY_COUNT + 10, true)
                .expect("listing is cached")
                .is_none(),
            "past the end of the listing"
        );
    });

    assert!(
        examined <= ENTRY_COUNT as u64,
        "one out-of-bounds fetch examined {examined} entries; one walk of {ENTRY_COUNT} is the budget"
    );
}

/// A second read of an unchanged listing re-uses the first read's answer instead
/// of walking again. This is what makes the per-event mirror sync free.
#[test]
fn a_repeated_read_of_an_unchanged_listing_does_not_walk_again() {
    let listing = big_listing("visible-rows-repeat");
    get_total_count(listing.id(), true).expect("listing is cached");

    let examined = examined_while(|| {
        for _ in 0..10 {
            get_total_count(listing.id(), true).expect("listing is cached");
            get_file_range(listing.id(), 0, 50, true).expect("listing is cached");
        }
    });

    assert_eq!(examined, 0, "an unchanged listing was walked again");
}

// ============================================================================
// Correctness: the map has to agree with a plain walk, in every state
// ============================================================================

/// Every accessor's answer for a listing, so one assertion covers all of them.
fn rows_of(listing: &TestListingGuard, include_hidden: bool) -> Vec<String> {
    let by_range: Vec<String> = get_file_range(listing.id(), 0, 1_000, include_hidden)
        .expect("listing is cached")
        .into_iter()
        .map(|e| e.name)
        .collect();

    assert_eq!(
        get_total_count(listing.id(), include_hidden).expect("listing is cached"),
        by_range.len(),
        "the count and the range disagree about how many rows there are"
    );

    for (row, name) in by_range.iter().enumerate() {
        assert_eq!(
            get_file_at(listing.id(), row, include_hidden)
                .expect("listing is cached")
                .map(|e| e.name)
                .as_deref(),
            Some(name.as_str()),
            "row {row} reads differently one at a time than in a range"
        );
        assert_eq!(
            find_file_index(listing.id(), name, include_hidden).expect("listing is cached"),
            Some(row),
            "looking `{name}` up by name lands somewhere else"
        );
    }
    assert!(
        get_file_at(listing.id(), by_range.len(), include_hidden)
            .expect("listing is cached")
            .is_none(),
        "one past the last row must read as nothing"
    );

    by_range
}

/// A scratch file a live operation owns. The returned `Arc` IS the operation:
/// drop it and the operation has settled.
fn in_flight_temp(name: &str, operation: &Arc<()>) -> StagingTemp {
    StagingTemp::mint(Path::new(&format!("/{name}")), Some(Arc::downgrade(operation)))
}

fn short_name(temp: &StagingTemp) -> String {
    temp.path()
        .file_name()
        .expect("a minted temp always has a file name")
        .to_string_lossy()
        .into_owned()
}

/// The row map keeps hidden scratch in a side list and merges it back in on
/// every read. This is that merge, with scratch at the front, in the middle, and
/// at the end, so an off-by-one in the merge shows up as the wrong file rather
/// than as nothing at all.
#[test]
fn scratch_scattered_through_a_listing_never_shifts_the_rows_around_it() {
    let _show = ShowTempsGuard::set(false);
    let operation = Arc::new(());
    let first = in_flight_temp("aaa.bin", &operation);
    let middle = in_flight_temp("mmm.bin", &operation);
    let last = in_flight_temp("zzz.bin", &operation);

    let listing = TestListing::new()
        .volume("test")
        .path("/scattered")
        .entries(vec![
            entry(&short_name(&first)),
            entry("bravo.txt"),
            entry("delta.txt"),
            entry(&short_name(&middle)),
            entry("echo.txt"),
            entry(&short_name(&last)),
        ])
        .insert("visible-rows-scattered");

    assert_eq!(
        rows_of(&listing, true),
        vec!["bravo.txt", "delta.txt", "echo.txt"],
        "the pane shows the three real files and nothing else"
    );

    // The operation settles. Nothing about the listing changed, but all three
    // are leftovers now, and a leftover is a real file the user must see.
    drop(operation);

    assert_eq!(
        rows_of(&listing, true),
        vec![
            short_name(&first),
            "bravo.txt".to_string(),
            "delta.txt".to_string(),
            short_name(&middle),
            "echo.txt".to_string(),
            short_name(&last),
        ],
        "a settled operation's leftovers rejoin the rows in listing order"
    );
}

/// Both axes at once, and the dotfile one toggling under a live listing. The two
/// maps are kept apart, so flipping the setting can't hand a pane the other
/// answer's rows.
#[test]
fn hiding_dotfiles_and_hiding_scratch_stay_independent() {
    let _show = ShowTempsGuard::set(false);
    let operation = Arc::new(());
    let temp = in_flight_temp("copying.bin", &operation);

    let listing = TestListing::new()
        .volume("test")
        .path("/axes")
        .entries(vec![
            entry(".config"),
            entry(&short_name(&temp)),
            entry("notes.txt"),
            entry("photo.jpg"),
        ])
        .insert("visible-rows-axes");

    assert_eq!(
        rows_of(&listing, true),
        vec![".config", "notes.txt", "photo.jpg"],
        "hidden files shown: the dotfile appears, the scratch file still doesn't"
    );
    assert_eq!(
        rows_of(&listing, false),
        vec!["notes.txt", "photo.jpg"],
        "hidden files off: neither appears"
    );
    assert_eq!(
        rows_of(&listing, true),
        vec![".config", "notes.txt", "photo.jpg"],
        "flipping back gives the first answer again"
    );
}

/// A mutation has to reach the map. Adding a row through the watcher's own patch
/// helper is the path that would otherwise leave the pane pointing at the file
/// that used to be there.
#[test]
fn a_row_added_after_the_first_read_shows_up_in_the_next_one() {
    let listing = TestListing::new()
        .volume("test")
        .path("/mutating")
        .entries(vec![entry("bravo.txt"), entry("delta.txt")])
        .insert("visible-rows-mutation");

    assert_eq!(rows_of(&listing, true), vec!["bravo.txt", "delta.txt"]);

    insert_entry_sorted(listing.id(), entry("charlie.txt")).expect("the row is new to this listing");

    assert_eq!(
        rows_of(&listing, true),
        vec!["bravo.txt", "charlie.txt", "delta.txt"],
        "the new row landed in sort order and every accessor sees it"
    );
}
