//! What a batch of path-keyed updates is allowed to cost.
//!
//! The defect these pin: filling Finder tags for a directory looked each updated
//! path up with a full walk of the listing, under the cache's WRITE lock, once
//! per path. A 500-path enrichment chunk against a 75,000-entry directory came
//! to 64 ms of walking, and the frontend sends 150 of those chunks to cover the
//! directory (`docs/notes/listing-wedge-impact-2026-08-22.md`). The cost is a
//! scan COUNT, so that is what these assert; a wall-clock assertion would be
//! flaky and silent on a small fixture.

use std::path::PathBuf;

use super::caching::{
    ModifyResult, apply_tags_to_listing, carry_forward_tags, has_entry, insert_entry_sorted, remove_entries_by_paths,
    update_entry_sorted,
};
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

// ============================================================================
// The single-path callers on the watcher's hot path
// ============================================================================

/// A watcher event carries ONE path, far under `BUILD_FROM_BATCH_SIZE`, so these
/// callers never build a map. What they must do is ride one the tag sweep already
/// built, which means looking the row up BEFORE handing `entries` out for
/// mutation: `entries_mut` drops the map, so a lookup after it can only walk.
const RIDING_A_MAP: u64 = 8;

/// The row a single-path lookup reaches for. Deep enough that a walk to it is
/// unmistakable against [`RIDING_A_MAP`].
fn deep_row() -> String {
    format!("/big/file-{:06}.bin", ENTRY_COUNT - 1)
}

/// A listing whose path map the sweep's first chunk has already built.
fn mapped_listing(tag: &str) -> TestListingGuard {
    let listing = big_listing(tag);
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));
    listing
}

/// A re-stat's tag carry-forward is a lookup by path like any other.
#[test]
fn a_re_stat_rides_the_map_a_tag_sweep_built() {
    let listing = mapped_listing("path-index-carry-forward");
    // A row the sweep's chunk DID tag, so the carry-forward has something to carry.
    let mut restat = entry(&format!("file-{:06}.bin", ENTRY_COUNT - ENTRY_COUNT / CHUNK));

    let examined = examined_while(|| carry_forward_tags(listing.id(), &mut restat));

    assert_eq!(
        restat.tags.len(),
        1,
        "the re-stat came back without the tags the cache was holding"
    );
    assert!(
        examined <= RIDING_A_MAP,
        "a re-stat's tag carry-forward examined {examined} entries on a mapped listing"
    );
}

/// `has_entry` classifies every watcher path as an add, a modify, or a removal,
/// so it runs before any of the other three.
#[test]
fn an_existence_check_rides_the_map_a_tag_sweep_built() {
    let listing = mapped_listing("path-index-has-entry");
    let mut found = false;

    let examined = examined_while(|| found = has_entry(listing.id(), &deep_row()));

    assert!(found, "the deep row is cached");
    assert!(
        examined <= RIDING_A_MAP,
        "an existence check examined {examined} entries on a mapped listing"
    );
}

/// A removal has to find its row before it can drop it.
#[test]
fn a_removal_rides_the_map_a_tag_sweep_built() {
    let listing = mapped_listing("path-index-remove");
    let mut removed = Vec::new();

    let examined = examined_while(|| removed = remove_entries_by_paths(listing.id(), &[PathBuf::from(deep_row())]));

    assert_eq!(
        removed.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
        vec![ENTRY_COUNT - 1],
        "the removal landed on the wrong row"
    );
    assert!(
        examined <= RIDING_A_MAP,
        "a removal examined {examined} entries on a mapped listing"
    );
}

/// The sharp one: it mutates, so its lookup has to happen while the map is still
/// standing, and its mutation still has to drop that map
/// ([`a_mutation_still_drops_the_map_it_rode`]).
#[test]
fn a_modify_rides_the_map_a_tag_sweep_built() {
    let listing = mapped_listing("path-index-modify");
    let restat = entry(&format!("file-{:06}.bin", ENTRY_COUNT - 1));
    let mut result = None;

    let examined = examined_while(|| result = update_entry_sorted(listing.id(), restat));

    assert!(
        matches!(result, Some(ModifyResult::UpdatedInPlace { index }) if index == ENTRY_COUNT - 1),
        "the modify landed on {result:?}"
    );
    assert!(
        examined <= RIDING_A_MAP,
        "a modify examined {examined} entries on a mapped listing"
    );
}

/// An insert's duplicate guard is the same lookup, one branch earlier.
#[test]
fn an_inserts_duplicate_guard_rides_the_map_a_tag_sweep_built() {
    let listing = mapped_listing("path-index-insert-guard");
    let duplicate = entry(&format!("file-{:06}.bin", ENTRY_COUNT - 1));
    let mut inserted = Some(0);

    let examined = examined_while(|| inserted = insert_entry_sorted(listing.id(), duplicate));

    assert_eq!(inserted, None, "a path the listing already held was inserted anyway");
    assert!(
        examined <= RIDING_A_MAP,
        "an insert's duplicate guard examined {examined} entries on a mapped listing"
    );
}

/// ❗ Riding a map is not keeping it. Both mutating callers hand `entries` out
/// through `entries_mut`, which drops the map, so the row indices it holds can
/// never outlive the positions they describe.
#[test]
fn a_mutation_still_drops_the_map_it_rode() {
    let listing = mapped_listing("path-index-mutation-drops");

    update_entry_sorted(listing.id(), entry(&format!("file-{:06}.bin", ENTRY_COUNT - 1)))
        .expect("the deep row is cached");

    let before = lookup_probe::builds();
    apply_tags_to_listing(listing.id(), chunk_of_updates(CHUNK));

    assert_eq!(
        lookup_probe::builds() - before,
        1,
        "a modify left its map standing, so it now describes positions that have moved"
    );
}

/// ❗ And riding a map is not building one. A watcher event on an untouched
/// 300,000-entry listing must not pay ~20 ms to build a map it uses once and
/// then, for the mutating callers, drops on the way out.
#[test]
fn a_single_path_caller_builds_no_map() {
    let listing = big_listing("path-index-single-cold");
    let mut restat = entry(&format!("file-{:06}.bin", ENTRY_COUNT - 1));

    let before = lookup_probe::builds();
    has_entry(listing.id(), &deep_row());
    carry_forward_tags(listing.id(), &mut restat);
    insert_entry_sorted(listing.id(), restat.clone());
    update_entry_sorted(listing.id(), restat);
    remove_entries_by_paths(listing.id(), &[PathBuf::from(deep_row())]);

    assert_eq!(
        lookup_probe::builds() - before,
        0,
        "a single-path caller built a map for its own use"
    );
}

// ============================================================================
// The watcher's removals, which ARE a batch
// ============================================================================

/// The caller that already loops a path lookup: one coalesced watcher event
/// carrying many removals (a directory emptied, a `git checkout` across a big
/// tree). Each removal walked the listing twice, once to record the pre-removal
/// index the diff needs and once inside the removal itself, so it is the same
/// quadratic as § "Entries by path" with a different caller in front of it.
#[test]
fn a_batch_of_removals_walks_the_listing_at_most_once() {
    let listing = big_listing("path-index-remove-batch");
    let stride = ENTRY_COUNT / CHUNK;
    let paths: Vec<PathBuf> = (0..CHUNK)
        .map(|i| PathBuf::from(format!("/big/file-{:06}.bin", i * stride)))
        .collect();
    let mut removed = Vec::new();

    let examined = examined_while(|| removed = remove_entries_by_paths(listing.id(), &paths));

    assert_eq!(removed.len(), CHUNK, "the batch skipped rows it was asked to remove");
    assert!(
        examined <= (ENTRY_COUNT + CHUNK * 2) as u64,
        "a {CHUNK}-path removal examined {examined}; the budget is one whole walk of {ENTRY_COUNT} plus a lookup each"
    );
}

/// The indices are the diff's, so they must all be in the PRE-removal listing
/// space, and they must come back highest-first, which is the order that keeps
/// each removal from shifting the next one's row.
#[test]
fn a_batch_of_removals_reports_pre_removal_indices_highest_first() {
    let listing = big_listing("path-index-remove-batch-indices");
    let paths: Vec<PathBuf> = [2usize, 7, 5]
        .iter()
        .map(|i| PathBuf::from(format!("/big/file-{i:06}.bin")))
        .collect();

    let removed = remove_entries_by_paths(listing.id(), &paths);

    let indices: Vec<usize> = removed.iter().map(|(index, _)| *index).collect();
    assert_eq!(indices, vec![7, 5, 2], "removal indices came back in the wrong order");
    let names: Vec<&str> = removed.iter().map(|(_, entry)| entry.name.as_str()).collect();
    assert_eq!(names, ["file-000007.bin", "file-000005.bin", "file-000002.bin"]);
    assert_eq!(listing.entries().len(), ENTRY_COUNT - 3);
}

/// A path the listing doesn't hold is skipped rather than reported: the watcher
/// stats paths another event may already have removed.
#[test]
fn a_batch_of_removals_skips_paths_the_listing_never_held() {
    let listing = big_listing("path-index-remove-batch-missing");
    let paths = vec![
        PathBuf::from("/big/file-000003.bin"),
        PathBuf::from("/big/never-existed.bin"),
    ];

    let removed = remove_entries_by_paths(listing.id(), &paths);

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].0, 3);
}
