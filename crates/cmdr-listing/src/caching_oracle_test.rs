//! `try_get_authoritative_listing` tests, the fresh-listing oracle.
//!
//! They pin the volume's coverage answer through `WatchCoverageVolume`
//! (`testing`), which is what lets all three be tested without a real change
//! watch.

use std::path::Path;
use std::sync::Arc;

use super::Listings;
use super::testing::{TestEngine, TestListing, TestListingGuard, WatchCoverageVolume};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{Volume, WatchCoverage};

fn make_test_entry(name: &str) -> FileEntry {
    FileEntry {
        size: Some(123),
        permissions: 0o644,
        owner: "test".to_string(),
        group: "staff".to_string(),
        ..FileEntry::new(name.to_string(), format!("/oracle/{}", name), false, false)
    }
}

/// A cached listing in `engine` with a controllable sequence.
fn insert_listing_with_sequence(
    engine: &Listings,
    tag: &str,
    volume_id: &str,
    path: &str,
    entries: Vec<FileEntry>,
    sequence: u64,
) -> TestListingGuard {
    TestListing::new()
        .volume(volume_id)
        .path(path)
        .entries(entries)
        .sequence(sequence)
        .insert(engine, tag)
}

fn unique(suffix: &str) -> String {
    unique_test_id(&format!("oracle-{suffix}"))
}

#[test]
fn try_get_authoritative_listing_hit_when_volume_reports_every_writer() {
    let engine = TestEngine::new();
    let vid = unique("hit_vid");
    let path = "/oracle/hit";

    let vol = Arc::new(WatchCoverageVolume::new("hit-vol", WatchCoverage::EveryWriter));
    engine.volumes.register(&vid, vol);

    let entries = vec![make_test_entry("a.txt"), make_test_entry("b.txt")];
    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, entries.clone(), 0);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_some(), "expected Some(entries) on watched listing");
    let returned = result.unwrap();
    assert_eq!(returned.len(), entries.len());
    assert_eq!(returned[0].name, "a.txt");
    assert_eq!(returned[1].name, "b.txt");
}

#[test]
fn try_get_authoritative_listing_miss_when_volume_reports_no_coverage() {
    let engine = TestEngine::new();
    let vid = unique("miss_watch_vid");
    let path = "/oracle/miss_watch";

    let vol = Arc::new(WatchCoverageVolume::new("miss-vol", WatchCoverage::None));
    engine.volumes.register(&vid, vol);

    let entries = vec![make_test_entry("a.txt")];
    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, entries, 0);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when watcher is dead");
}

/// A LIVE watch that can't see other writers must not authorize skipping a read.
///
/// This is the OS-mounted-share case (`/Volumes/testshare` served by
/// `LocalPosixVolume`): FSEvents is armed and does update the pane from this
/// machine's writes, so every "is it watched?" signal says yes, while another
/// client's changes never arrive. Handing these entries to a delete walker or a
/// copy scan is exactly how a file nobody told us about gets missed.
#[test]
fn try_get_authoritative_listing_miss_when_watch_sees_only_this_machine() {
    let engine = TestEngine::new();
    let vid = unique("this_machine_vid");
    let path = "/oracle/this_machine";

    let vol = Arc::new(WatchCoverageVolume::new(
        "mounted-share-vol",
        WatchCoverage::ThisMachineOnly,
    ));
    engine.volumes.register(&vid, vol);

    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(
        result.is_none(),
        "a watch blind to other writers must not substitute for a read"
    );
}

#[test]
fn try_get_authoritative_listing_miss_when_no_listing_exists() {
    let engine = TestEngine::new();
    let vid = unique("miss_no_listing_vid");
    let vol = Arc::new(WatchCoverageVolume::new("no-listing-vol", WatchCoverage::EveryWriter));
    engine.volumes.register(&vid, vol);

    let result = engine
        .inner
        .try_get_authoritative_listing(&vid, Path::new("/oracle/nothing_here"));
    assert!(result.is_none(), "expected None when no listing matches");
}

#[test]
fn try_get_authoritative_listing_miss_when_volume_not_registered() {
    let engine = TestEngine::new();
    let vid = unique("no_vol");
    let path = "/oracle/no_vol";

    // Listing exists in cache, but no volume is registered for this ID.
    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when volume isn't registered");
}

#[test]
fn try_get_authoritative_listing_picks_highest_sequence() {
    let engine = TestEngine::new();
    // Two listings on the same (volume_id, path) with different sequence
    // numbers. The oracle must return the entries from the higher-sequence
    // listing, deterministically — never the lower-sequence one.
    let vid = unique("seq_vid");
    let path = "/oracle/seq_path";

    let vol = Arc::new(WatchCoverageVolume::new("seq-vol", WatchCoverage::EveryWriter));
    engine.volumes.register(&vid, vol);

    let _lid_lo = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("low.txt")], 1);
    let _lid_hi = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("high.txt")], 9);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_some());
    let returned = result.unwrap();
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].name, "high.txt", "expected the higher-sequence listing");
}

#[test]
fn try_get_authoritative_listing_miss_for_start_streaming_watcher_gap() {
    let engine = TestEngine::new();
    // Simulates the documented race window between
    // `list_directory_start_streaming` populating the listing cache and
    // `start_watching` inserting into WATCHER_MANAGER: the listing exists
    // in cache, but the volume reports no watcher yet (here: by reporting
    // `None` from the test volume's `listing_watch_coverage`). The oracle
    // must miss in that window so write ops fall through to a real read.
    let vid = unique("race_vid");
    let path = "/oracle/race";

    // `WatchCoverage::None` mirrors "no watch armed yet" on the local backend.
    let vol = Arc::new(WatchCoverageVolume::new("race-vol", WatchCoverage::None));
    engine.volumes.register(&vid, vol);

    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = engine.inner.try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None during the streaming->watcher gap");
}

#[test]
fn try_get_authoritative_listing_reflects_flip_to_unwatched() {
    let engine = TestEngine::new();
    // Sanity check: flipping the watcher flag flips the oracle's verdict
    // on subsequent calls. Documents that the oracle is a live query and
    // doesn't memoize per-listing.
    let vid = unique("flip_vid");
    let path = "/oracle/flip";

    let vol: Arc<WatchCoverageVolume> = Arc::new(WatchCoverageVolume::new("flip-vol", WatchCoverage::EveryWriter));
    engine.volumes.register(&vid, vol.clone() as Arc<dyn Volume>);

    let _lid = insert_listing_with_sequence(&engine, "listing", &vid, path, vec![make_test_entry("x.txt")], 0);

    assert!(
        engine
            .inner
            .try_get_authoritative_listing(&vid, Path::new(path))
            .is_some()
    );
    vol.set_coverage(WatchCoverage::None);
    assert!(
        engine
            .inner
            .try_get_authoritative_listing(&vid, Path::new(path))
            .is_none()
    );
}

/// A poller asks which directories a volume's panes show: each once however
/// many panes show it, nothing from another volume, and nothing once the last
/// pane on it closes.
#[test]
fn open_listing_paths_answers_each_shown_directory_on_the_volume_once() {
    let engine = TestEngine::new();
    let vid = unique("paths_vid");
    let other = unique("paths_other");

    let first = insert_listing_with_sequence(&engine, "paths-1", &vid, "/shown/a", vec![], 0);
    let second = insert_listing_with_sequence(&engine, "paths-2", &vid, "/shown/a", vec![], 0);
    let third = insert_listing_with_sequence(&engine, "paths-3", &vid, "/shown/b", vec![], 0);
    let elsewhere = insert_listing_with_sequence(&engine, "paths-4", &other, "/shown/c", vec![], 0);

    assert_eq!(
        engine.open_listing_paths(&vid),
        [Path::new("/shown/a"), Path::new("/shown/b")],
        "two panes on one directory answer once, and another volume's pane not at all"
    );

    drop((first, second, third));
    assert!(
        engine.open_listing_paths(&vid).is_empty(),
        "a closed pane leaves nothing to poll"
    );
    drop(elsewhere);
}
