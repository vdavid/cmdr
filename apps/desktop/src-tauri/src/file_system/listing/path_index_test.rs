//! What a batch of path-keyed updates is allowed to cost.
//!
//! The defect these pin: filling Finder tags for a directory looked each updated
//! path up with a full walk of the listing, under the cache's WRITE lock, once
//! per path. A 500-path enrichment chunk against a 75,000-entry directory came
//! to 64 ms of walking, and the frontend sends 150 of those chunks to cover the
//! directory (`docs/notes/listing-wedge-impact-2026-08-22.md`). The cost is a
//! scan COUNT, so that is what these assert; a wall-clock assertion would be
//! flaky and silent on a small fixture.

use super::caching::{apply_tags_to_listing, insert_entry_sorted};
use super::caching_test_support::{TestListing, TestListingGuard};
use super::metadata::{FileEntry, TagRef};
use super::operations::get_file_at;
use super::path_index::lookup_probe;
use super::visible_rows::scan_probe;

/// Big enough that a per-path rescan is unmistakable in the count, small enough
/// that the fixture builds in milliseconds.
const ENTRY_COUNT: usize = 20_000;

/// What the frontend's background tag sweep sends per call (`TAG_SWEEP_CHUNK`).
const CHUNK: usize = 500;

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

/// A chunk of tag updates spread over the whole listing, the way the sweep's
/// chunks reach the deep rows.
fn chunk_of_updates(count: usize) -> Vec<(String, Vec<TagRef>)> {
    let stride = ENTRY_COUNT / count;
    (0..count)
        .map(|i| {
            (
                format!("/big/file-{:06}.bin", i * stride),
                vec![TagRef {
                    name: "Red".to_string(),
                    color: 6,
                }],
            )
        })
        .collect()
}

/// Entries examined by a path lookup while running `f`, on this thread.
fn examined_while(f: impl FnOnce()) -> u64 {
    let before = lookup_probe::examined();
    f();
    lookup_probe::examined() - before
}

/// The defect itself: one enrichment chunk must cost one walk of the listing,
/// not one per path in the chunk.
#[test]
fn a_chunk_of_tag_updates_walks_the_listing_at_most_once() {
    let listing = big_listing("path-index-chunk");

    let examined = examined_while(|| apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK)));

    assert!(
        examined <= (ENTRY_COUNT + CHUNK * 2) as u64,
        "a {CHUNK}-path chunk examined {examined}; the budget is one whole walk of {ENTRY_COUNT} plus a lookup each"
    );
}

/// Depth must not be the multiplier. Tagging the last rows of a listing costs
/// what tagging the first rows costs, so a sweep doesn't get slower as it goes.
#[test]
fn tagging_the_end_of_a_listing_costs_what_tagging_the_start_costs() {
    let listing = big_listing("path-index-depth");
    let update = |index: usize| {
        vec![(
            format!("/big/file-{index:06}.bin"),
            vec![TagRef {
                name: "Blue".to_string(),
                color: 4,
            }],
        )]
    };

    // Warm the map the way the sweep's first chunk does, so this measures the
    // steady state rather than the build.
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));

    let at_start = examined_while(|| apply_tags_to_listing(listing.id(), update(1)));
    let at_end = examined_while(|| apply_tags_to_listing(listing.id(), update(ENTRY_COUNT - 1)));

    assert!(
        at_end <= at_start + 8,
        "tagging the last row examined {at_end} against {at_start} for the first row"
    );
}

/// The second quadratic: a tag write must not drop the row map, or every chunk
/// of the sweep makes the next pane read rebuild it. Tags are not part of a
/// name, so nothing about which rows a pane shows can have changed.
#[test]
fn a_tag_update_leaves_the_row_map_standing() {
    let listing = big_listing("path-index-row-map");
    // Build the row map, as any pane read does.
    get_file_at(listing.id(), 0, true).expect("listing is cached");

    let before = scan_probe::examined();
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));
    get_file_at(listing.id(), ENTRY_COUNT - 1, true).expect("listing is cached");
    let rescanned = scan_probe::examined() - before;

    assert_eq!(
        rescanned, 0,
        "a tag chunk plus a row read rebuilt the row map (examined {rescanned}); tags can't change what a pane shows"
    );
}

/// A handful of paths (a context-menu tag toggle) must not build a map it uses
/// once. Building is linear in the listing, so on a big directory that would
/// turn a 20 µs write into a 33 ms one.
#[test]
fn a_context_menu_sized_update_builds_no_map() {
    let listing = big_listing("path-index-small");

    let before = lookup_probe::builds();
    apply_tags_to_listing(listing.id(), chunk_of_updates(4));
    let builds = lookup_probe::builds() - before;

    assert_eq!(builds, 0, "a 4-path update built {builds} map(s) for its own use");
}

/// …and once a map exists, a small update rides it rather than walking.
#[test]
fn a_small_update_rides_a_map_that_already_exists() {
    let listing = big_listing("path-index-small-warm");
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));

    let examined = examined_while(|| apply_tags_to_listing(listing.id(), chunk_of_updates(4)));

    assert!(
        examined <= 16,
        "a 4-path update on a mapped listing examined {examined}"
    );
}

/// The map must not outlive the entry positions it describes. Inserting a row
/// shifts every index after it, and the next tag update has to land on the right
/// file anyway.
#[test]
fn tags_land_correctly_after_an_insert_moves_every_index() {
    let listing = big_listing("path-index-invalidation");
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));

    // Sorts to the very front, so every existing entry's index moves by one.
    insert_entry_sorted(listing.id(), entry("aaa-newcomer.bin")).expect("listing is cached");

    let target = format!("/big/file-{:06}.bin", ENTRY_COUNT - 1);
    apply_tags_to_listing(
        listing.id(),
        vec![(
            target.clone(),
            vec![TagRef {
                name: "Green".to_string(),
                color: 2,
            }],
        )],
    );

    let tagged: Vec<String> = listing
        .entries()
        .iter()
        .filter(|e| e.tags.iter().any(|t| t.name == "Green"))
        .map(|e| e.path.clone())
        .collect();
    assert_eq!(tagged, vec![target], "the tag landed on the wrong row after an insert");
}
