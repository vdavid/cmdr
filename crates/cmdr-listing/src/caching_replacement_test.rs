//! `DirectoryChange::Replaced` tests: a backend hands over a whole re-read
//! directory and the host makes it the pane's contents.
//!
//! These drive the per-listing half, `publish_replacement`, directly; the arm
//! that finds the listings is `caching_test.rs`'s.
use super::testing::{TestEngine, TestListing, pending_changes, publish_replacement};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::unique_test_id;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

/// Creates a minimal test entry.
fn make_entry(name: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        ..FileEntry::new(name.to_string(), format!("/test/{}", name), is_dir, false)
    }
}

/// The replacement lands in the cache and the pane is told what moved. This is
/// what a device backend gets for handing over entries instead of patching the
/// cache itself.
#[test]
fn a_replacement_becomes_the_listings_contents_and_is_published() {
    let listing = TestListing::new()
        .path("/test/replaced")
        .entries(vec![
            make_entry("kept.txt", false, Some(1)),
            make_entry("gone.txt", false, Some(2)),
        ])
        .insert(&TestEngine::new(), "replaced-basic");

    publish_replacement(
        listing.engine(),
        listing.id(),
        vec![
            make_entry("kept.txt", false, Some(1)),
            make_entry("new.txt", false, Some(3)),
        ],
        0,
    );

    assert_eq!(listing.entry_names(), ["kept.txt", "new.txt"]);
    assert!(
        !pending_changes(listing.engine(), listing.id()).is_empty(),
        "the pane has to hear about the swap, not just the cache"
    );
}

/// The diff the pane is told about has to describe the list the cache ends up
/// holding, so the entries are sorted the listing's way BEFORE they're compared.
/// A backend reports in whatever order its protocol answers in (MTP answers by
/// object handle), so without this a descending pane gets indices pointing at the
/// wrong rows.
#[test]
fn a_replacement_is_sorted_the_listings_way_before_it_is_diffed() {
    let listing = TestListing::new()
        .path("/test/replaced-sorted")
        .sort(SortColumn::Name, SortOrder::Descending, DirectorySortMode::LikeFiles)
        .entries(vec![
            make_entry("c.txt", false, Some(1)),
            make_entry("a.txt", false, Some(2)),
        ])
        .insert(&TestEngine::new(), "replaced-sorted");

    // Device order: neither the pane's order nor its reverse.
    publish_replacement(
        listing.engine(),
        listing.id(),
        vec![
            make_entry("a.txt", false, Some(2)),
            make_entry("b.txt", false, Some(4)),
            make_entry("c.txt", false, Some(1)),
        ],
        0,
    );

    assert_eq!(listing.entry_names(), ["c.txt", "b.txt", "a.txt"]);
}

/// A device that re-lists on every event mostly finds nothing changed, so an
/// identical replacement must cost the frontend nothing.
#[test]
fn an_identical_replacement_publishes_nothing() {
    let listing = TestListing::new()
        .path("/test/replaced-noop")
        .entries(vec![make_entry("same.txt", false, Some(7))])
        .insert(&TestEngine::new(), "replaced-noop");

    publish_replacement(
        listing.engine(),
        listing.id(),
        vec![make_entry("same.txt", false, Some(7))],
        0,
    );

    assert_eq!(listing.entry_names(), ["same.txt"]);
    assert_eq!(
        pending_changes(listing.engine(), listing.id()).len(),
        0,
        "no difference means nothing to publish"
    );
}

/// A listing that closed between the re-read and the report is the common race
/// on a device that unplugs, so it has to be silent rather than a panic.
#[test]
fn a_replacement_for_a_listing_that_closed_is_silent() {
    let engine = TestEngine::new();
    publish_replacement(
        &engine,
        &unique_test_id("replaced-gone"),
        vec![make_entry("x.txt", false, Some(1))],
        0,
    );
}
