//! Overlay rows survive EVERY read path that builds or replaces a listing:
//! the streaming read, a watcher's `FullRefresh`, a backend swap's respell
//! re-read, the watcher's own full re-read (`handle_directory_change`, also ⌘R),
//! and the test-only synchronous start. Each one re-reads through the volume,
//! which holds no contributed row, so a path that skips the overlays strips
//! them from an open pane.
//!
//! A device backend's `Replaced` hand-over records zero contributed rows when no
//! overlay applies to its volume, pinned at its arm in `notify_directory_changed`,
//! and escalates to a full re-read of the directory when one does.
//!
//! The overlay is a `StaticOverlay` marker, which decorates only a volume with
//! its name. What the git portal decides is `file_system/git/overlay_tests.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::caching::DirectoryChange;
use super::testing::{StaticOverlay, TestEngine, TestListing, TestListingGuard, request};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::{unique_test_id, wait_until_async};
use cmdr_fs::volume::InMemoryVolume;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), false, false)
}

/// The name a volume carries to have the marker overlay decorate it.
const MARKER_VOLUME_NAME: &str = "seam-pins marker volume";

/// The one row the marker overlay folds into every listing on a marker volume.
const MARKER_ROW: &str = "zz-marker-overlay-row";

/// An engine with the marker overlay.
fn marked_engine() -> TestEngine {
    TestEngine::builder()
        .overlay(StaticOverlay::new(MARKER_VOLUME_NAME, MARKER_ROW))
        .build()
}

/// A marker volume holding `a.txt` and `b.txt` at its root, registered under a
/// fresh id in an engine with the marker overlay.
struct MarkerVolume {
    engine: TestEngine,
    id: String,
}

impl MarkerVolume {
    fn new(tag: &str) -> Self {
        let engine = marked_engine();
        let id = unique_test_id(tag);
        engine.volumes.register(
            &id,
            Arc::new(InMemoryVolume::with_entries(
                MARKER_VOLUME_NAME,
                vec![entry("a.txt"), entry("b.txt")],
            )),
        );
        Self { engine, id }
    }

    /// A pane's listing of the root as an earlier read left it, holding `a.txt`
    /// only and no contributed row, so a re-read has something to change.
    fn stale_listing(&self, tag: &str) -> TestListingGuard {
        TestListing::new()
            .volume(&self.id)
            .path("/")
            .entries(vec![entry("a.txt")])
            .insert(&self.engine, tag)
    }
}

/// The re-read landed (`b.txt` arrived) and carries the marker row, counted.
fn assert_decorated(listing: &TestListingGuard) {
    let names = listing.entry_names();
    assert!(names.iter().any(|n| n == "b.txt"), "the re-read landed: {names:?}");
    assert!(
        names.iter().any(|n| n == MARKER_ROW),
        "the overlay row survived: {names:?}"
    );
    assert!(
        listing.with_listing(|l| l.has_overlay_rows()),
        "the contributed row is counted, so the oracle keeps declining this listing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_streaming_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-streaming");
    let listing = TestListingGuard::adopt(&volume.engine, unique_test_id("overlay-path-streaming"));

    volume
        .engine
        .read_with(
            &volume.engine.streaming_state(),
            &request(listing.id(), &volume.id, Path::new("/")),
        )
        .await
        .expect("the marker volume lists");

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_refresh_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-full-refresh");
    let listing = volume.stale_listing("overlay-path-full-refresh");

    volume
        .engine
        .inner
        .notify_full_refresh(volume.id.clone(), "/".into(), vec![listing.id().to_string()])
        .await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_respell_re_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-respell");
    let listing = volume.stale_listing("overlay-path-respell");

    volume
        .engine
        .inner
        .respell_listing(volume.id.clone(), listing.id().to_string(), "/".into())
        .await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_watchers_full_re_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-watcher");
    let listing = volume.stale_listing("overlay-path-watcher");

    // `Listings::refresh` IS the watcher's full re-read; the app's
    // `handle_directory_change` only acts on how it ended.
    volume.engine.refresh(&listing.listing_id()).await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_synchronous_start_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-sync-start");

    let started = volume
        .engine
        .inner
        .list_directory_start_with_volume(
            &volume.id,
            Path::new("/"),
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
        .await
        .expect("the marker volume lists");
    let listing = TestListingGuard::adopt(&volume.engine, started.listing_id);

    assert_decorated(&listing);
}

/// The `Replaced` half: a backend's own re-read becomes the listing with zero
/// contributed rows, even on a listing an earlier read decorated.
#[test]
fn a_replaced_hand_over_records_zero_overlay_rows() {
    let engine = marked_engine();
    let volume_id = unique_test_id("overlay-path-replaced-vol");
    let listing = TestListing::new()
        .volume(&volume_id)
        .path("/")
        .entries(vec![entry("a.txt"), entry(MARKER_ROW)])
        .overlay_rows(1)
        .insert(&engine, "overlay-path-replaced");

    engine.inner.notify_directory_changed(
        &volume_id,
        Path::new("/"),
        DirectoryChange::Replaced(vec![entry("a.txt"), entry("b.txt")]),
    );

    assert_eq!(listing.entry_names(), ["a.txt", "b.txt"]);
    assert!(
        !listing.with_listing(|l| l.has_overlay_rows()),
        "a backend's hand-over counts no contributed row"
    );
}

/// Behavior change 4 (issue #301): a `Replaced` hand-over on a volume an overlay
/// applies to can't decorate in place (the arm is synchronous and the overlays
/// aren't), so it drops the backend's entries and re-reads the directory the way
/// a `FullRefresh` does, which folds the overlay rows back in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replaced_hand_over_on_a_decorated_volume_re_reads_with_the_overlays() {
    let volume = MarkerVolume::new("overlay-path-replaced-escalates");
    let listing = volume.stale_listing("overlay-path-replaced-escalates");

    volume.engine.inner.notify_directory_changed(
        &volume.id,
        Path::new("/"),
        DirectoryChange::Replaced(vec![entry("a.txt"), entry("from-the-backend.txt")]),
    );

    wait_until_async(Duration::from_secs(5), "the escalated re-read to land", || {
        listing.entry_names().iter().any(|n| n == MARKER_ROW)
    })
    .await;
    assert_decorated(&listing);
    assert!(
        !listing.entry_names().iter().any(|n| n == "from-the-backend.txt"),
        "the backend's entries are dropped, not half-applied: {:?}",
        listing.entry_names()
    );
}

/// A watcher DIFF patch is not a re-read: it hands the cache entries that a
/// previous read already decorated, so it must leave the contributed-row count
/// exactly where it was. Zeroing it would tell the fresh-listing oracle that a
/// pane's `.git/` view is a picture of the directory, and hand rows with no
/// inode behind them to a delete walker.
#[test]
fn a_diff_patch_leaves_the_contributed_row_count_alone() {
    use super::pipeline::{OverlayRows, Prepared};
    use super::testing::replace_entries;

    let engine = marked_engine();
    let mut entries = vec![entry("a.txt"), entry(MARKER_ROW)];
    let listing = TestListing::new()
        .volume(&unique_test_id("overlay-diff-vol"))
        .path("/")
        .entries(entries.clone())
        .overlay_rows(1)
        .insert(&engine, "overlay-diff");

    // What the watcher does when FSEvents reports one new file: the decorated
    // set plus a row, written straight back.
    entries.push(entry("COMMIT_EDITMSG"));
    replace_entries(
        &engine,
        listing.id(),
        Prepared::for_test(entries, OverlayRows::Unchanged),
    );

    assert!(
        listing.has_overlay_rows(),
        "a diff patch must not turn a pane's decorated listing into a walker's picture of the directory"
    );
    let names = listing.entry_names();
    assert!(names.iter().any(|n| n == "COMMIT_EDITMSG"), "{names:?}");
    assert!(names.iter().any(|n| n == MARKER_ROW), "{names:?}");
}
