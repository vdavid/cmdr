//! A listing asked to sort by a date its volume doesn't record: the rows land in
//! Modified order, every answer says so, and the requested column survives.
//!
//! The rule itself (`EffectiveSort::resolve`) is `sorting_effective_test`; this
//! suite is the cache: the first read, resort, the hidden-files toggle, and a
//! re-read whose volume now records the date.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{InMemoryVolume, SupportedDates, Volume};

use super::operations::PaneMarks;
use super::testing::{TestEngine, TestListingGuard, full_refresh, hold_diffs, read_to_end, request};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

/// Three files whose added order runs against their mtime order, so a sort by
/// one can't pass for a sort by the other.
fn downloads() -> Vec<FileEntry> {
    let file = |name: &str, modified: u64, added: u64| FileEntry {
        modified_at: Some(modified),
        added_at: Some(added),
        size: Some(1),
        ..FileEntry::new(name.to_string(), format!("/{name}"), false, false)
    };
    vec![
        file("old_download.pdf", 300, 1),
        file("middle.pdf", 200, 2),
        file("fresh_download.pdf", 100, 3),
    ]
}

fn volume(dates: SupportedDates, entries: Vec<FileEntry>) -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::with_entries("Dates", entries).with_supported_dates(dates))
}

/// Lists `/` on `volume_id`, asked to sort by `column` descending, and waits
/// for it to land.
async fn list_sorted_by(engine: &TestEngine, volume_id: &str, column: SortColumn) -> TestListingGuard {
    let listing = TestListingGuard::adopt(engine, unique_test_id("effective-sort"));
    let mut asked = request(listing.id(), volume_id, "/");
    asked.sort_by = column;
    asked.sort_order = SortOrder::Descending;
    read_to_end(engine, &asked).await.expect("the listing lands");
    listing
}

fn no_marks() -> PaneMarks<'static> {
    PaneMarks {
        cursor_filename: None,
        selected: None,
        all_selected: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_without_added_dates_sorts_an_added_request_by_modified() {
    let engine = TestEngine::builder()
        .volume("no-added", volume(SupportedDates::CREATED, downloads()))
        .build();

    let listing = list_sorted_by(&engine, "no-added", SortColumn::Added).await;

    // Newest mtime first: the fresh download, whose mtime is oldest, lands last.
    assert_eq!(
        listing.entry_names(),
        ["old_download.pdf", "middle.pdf", "fresh_download.pdf"]
    );
    assert_eq!(
        engine.events.effective_sorts.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), SortColumn::Modified)],
        "listing-complete says which column the rows are in"
    );
    listing.with_listing(|cached| assert_eq!(cached.requested_column(), SortColumn::Added));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_with_added_dates_sorts_by_added() {
    let engine = TestEngine::builder()
        .volume("with-added", volume(SupportedDates::ALL, downloads()))
        .build();

    let listing = list_sorted_by(&engine, "with-added", SortColumn::Added).await;

    assert_eq!(
        listing.entry_names(),
        ["fresh_download.pdf", "middle.pdf", "old_download.pdf"]
    );
    assert_eq!(
        engine.events.effective_sorts.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), SortColumn::Added)]
    );
}

/// Resorting away and back keeps answering the effective column, and the
/// listing keeps the REQUESTED one, so the next resort asks for it again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resort_and_the_hidden_toggle_answer_the_effective_column() {
    let engine = TestEngine::builder()
        .volume("resort-no-added", volume(SupportedDates::CREATED, downloads()))
        .build();
    let listing = list_sorted_by(&engine, "resort-no-added", SortColumn::Added).await;
    let id = listing.listing_id();
    let resort = |column| {
        engine
            .resort(
                &id,
                (column, SortOrder::Descending, DirectorySortMode::LikeFiles),
                true,
                no_marks(),
                None,
            )
            .expect("the listing is open")
    };

    assert_eq!(resort(SortColumn::Name).effective_sort_by, SortColumn::Name);
    listing.with_listing(|cached| assert_eq!(cached.requested_column(), SortColumn::Name));

    assert_eq!(resort(SortColumn::Added).effective_sort_by, SortColumn::Modified);
    listing.with_listing(|cached| assert_eq!(cached.requested_column(), SortColumn::Added));
    assert_eq!(
        listing.entry_names(),
        ["old_download.pdf", "middle.pdf", "fresh_download.pdf"]
    );

    let toggled = engine
        .set_include_hidden(&id, false, None, no_marks())
        .expect("the listing is open");
    assert_eq!(toggled.effective_sort_by, SortColumn::Modified);
}

/// A re-read through a volume that now records Added (an smbfs mount upgraded
/// to a backend that does) re-sorts into Added and tells the pane, even when no
/// row changed: a one-row folder still has to hear its displayed date moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reread_that_gains_the_date_publishes_the_new_effective_column() {
    // No added date on the row itself, so the row reads the same through both
    // volumes and the diff has nothing but the column to say.
    let one_row = || {
        vec![FileEntry::new(
            "only.txt".to_string(),
            "/only.txt".to_string(),
            false,
            false,
        )]
    };
    let engine = TestEngine::builder()
        .volume("swapped", volume(SupportedDates::CREATED, one_row()))
        .build();
    let listing = list_sorted_by(&engine, "swapped", SortColumn::Added).await;
    hold_diffs(&engine, listing.id());

    engine
        .volumes
        .register("swapped", volume(SupportedDates::ALL, one_row()));
    full_refresh(&engine, "swapped", Path::new("/"), listing.id()).await;

    let batches = engine.inner.diffs.pending_batches(listing.id());
    assert_eq!(batches.len(), 1, "one batch for the moved column: {batches:?}");
    assert!(batches[0].changes.is_empty(), "no row changed");
    assert_eq!(batches[0].effective_sort_by, Some(SortColumn::Added));
    listing.with_listing(|cached| assert_eq!(cached.effective().column(), SortColumn::Added));

    // The same dates again: nothing moved, nothing to say.
    full_refresh(&engine, "swapped", Path::new("/"), listing.id()).await;
    assert_eq!(engine.inner.diffs.pending_batches(listing.id()).len(), 1);
}

/// Losing the date re-sorts the rows into Modified order in the same batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reread_that_loses_the_date_resorts_by_modified() {
    let engine = TestEngine::builder()
        .volume("downgraded", volume(SupportedDates::ALL, downloads()))
        .build();
    let listing = list_sorted_by(&engine, "downgraded", SortColumn::Added).await;
    hold_diffs(&engine, listing.id());

    engine
        .volumes
        .register("downgraded", volume(SupportedDates::NONE, downloads()));
    full_refresh(&engine, "downgraded", Path::new("/"), listing.id()).await;

    assert_eq!(
        listing.entry_names(),
        ["old_download.pdf", "middle.pdf", "fresh_download.pdf"]
    );
    let batches = engine.inner.diffs.pending_batches(listing.id());
    assert_eq!(
        batches.last().and_then(|batch| batch.effective_sort_by),
        Some(SortColumn::Modified)
    );
    listing.with_listing(|cached| assert_eq!(cached.requested_column(), SortColumn::Added));
}
