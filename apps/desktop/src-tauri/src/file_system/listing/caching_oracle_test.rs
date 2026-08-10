//! `try_get_authoritative_listing` tests, the fresh-listing oracle.
//!
//! They pin the volume's coverage answer through `WatchCoverageVolume`
//! (`caching_test_support`), which is what lets all three be tested without an
//! `AppHandle` or a real `WATCHER_MANAGER` entry.

use std::path::Path;
use std::sync::Arc;

use super::caching::try_get_authoritative_listing;
use super::caching_test_support::{TestListing, TestListingGuard, WatchCoverageVolume, unique_test_id};
use super::metadata::FileEntry;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{Volume, WatchCoverage};

fn make_test_entry(name: &str) -> FileEntry {
    FileEntry {
        size: Some(123),
        permissions: 0o644,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/oracle/{}", name), false, false)
    }
}

/// A test-owned cached listing with a controllable sequence, on the test's own
/// volume id. Removed from `LISTING_CACHE` when the returned guard drops.
fn insert_listing_with_sequence(
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
        .insert(tag)
}

fn unique(suffix: &str) -> String {
    unique_test_id(&format!("oracle-{suffix}"))
}

#[test]
fn try_get_authoritative_listing_hit_when_volume_reports_every_writer() {
    let vid = unique("hit_vid");
    let path = "/oracle/hit";

    let vol = Arc::new(WatchCoverageVolume::new("hit-vol", WatchCoverage::EveryWriter));
    get_volume_manager().register(&vid, vol);

    let entries = vec![make_test_entry("a.txt"), make_test_entry("b.txt")];
    let _lid = insert_listing_with_sequence("listing", &vid, path, entries.clone(), 0);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_some(), "expected Some(entries) on watched listing");
    let returned = result.unwrap();
    assert_eq!(returned.len(), entries.len());
    assert_eq!(returned[0].name, "a.txt");
    assert_eq!(returned[1].name, "b.txt");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_authoritative_listing_miss_when_volume_reports_no_coverage() {
    let vid = unique("miss_watch_vid");
    let path = "/oracle/miss_watch";

    let vol = Arc::new(WatchCoverageVolume::new("miss-vol", WatchCoverage::None));
    get_volume_manager().register(&vid, vol);

    let entries = vec![make_test_entry("a.txt")];
    let _lid = insert_listing_with_sequence("listing", &vid, path, entries, 0);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when watcher is dead");

    get_volume_manager().unregister(&vid);
}

/// A LIVE watch that can't see other writers must not authorize skipping a read.
///
/// This is the OS-mounted-share case (`/Volumes/naspi` served by
/// `LocalPosixVolume`): FSEvents is armed and does update the pane from this
/// machine's writes, so every "is it watched?" signal says yes, while another
/// client's changes never arrive. Handing these entries to a delete walker or a
/// copy scan is exactly how a file nobody told us about gets missed.
#[test]
fn try_get_authoritative_listing_miss_when_watch_sees_only_this_machine() {
    let vid = unique("this_machine_vid");
    let path = "/oracle/this_machine";

    let vol = Arc::new(WatchCoverageVolume::new(
        "mounted-share-vol",
        WatchCoverage::ThisMachineOnly,
    ));
    get_volume_manager().register(&vid, vol);

    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(
        result.is_none(),
        "a watch blind to other writers must not substitute for a read"
    );

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_authoritative_listing_miss_when_no_listing_exists() {
    let vid = unique("miss_no_listing_vid");
    let vol = Arc::new(WatchCoverageVolume::new("no-listing-vol", WatchCoverage::EveryWriter));
    get_volume_manager().register(&vid, vol);

    let result = try_get_authoritative_listing(&vid, Path::new("/oracle/nothing_here"));
    assert!(result.is_none(), "expected None when no listing matches");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_authoritative_listing_miss_when_volume_not_registered() {
    let vid = unique("no_vol");
    let path = "/oracle/no_vol";

    // Listing exists in cache, but no volume is registered for this ID.
    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when volume isn't registered");
}

#[test]
fn try_get_authoritative_listing_picks_highest_sequence() {
    // Two listings on the same (volume_id, path) with different sequence
    // numbers. The oracle must return the entries from the higher-sequence
    // listing, deterministically — never the lower-sequence one.
    let vid = unique("seq_vid");
    let path = "/oracle/seq_path";

    let vol = Arc::new(WatchCoverageVolume::new("seq-vol", WatchCoverage::EveryWriter));
    get_volume_manager().register(&vid, vol);

    let _lid_lo = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("low.txt")], 1);
    let _lid_hi = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("high.txt")], 9);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_some());
    let returned = result.unwrap();
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].name, "high.txt", "expected the higher-sequence listing");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_authoritative_listing_miss_for_start_streaming_watcher_gap() {
    // Simulates the documented race window between
    // `list_directory_start_streaming` populating LISTING_CACHE and
    // `start_watching` inserting into WATCHER_MANAGER: the listing exists
    // in cache, but the volume reports no watcher yet (here: by reporting
    // `None` from the test volume's `listing_watch_coverage`). The oracle
    // must miss in that window so write ops fall through to a real read.
    let vid = unique("race_vid");
    let path = "/oracle/race";

    // `WatchCoverage::None` mirrors "WATCHER_MANAGER has no entry yet" on the
    // local backend without needing an AppHandle.
    let vol = Arc::new(WatchCoverageVolume::new("race-vol", WatchCoverage::None));
    get_volume_manager().register(&vid, vol);

    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = try_get_authoritative_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None during the streaming->watcher gap");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_authoritative_listing_reflects_flip_to_unwatched() {
    // Sanity check: flipping the watcher flag flips the oracle's verdict
    // on subsequent calls. Documents that the oracle is a live query and
    // doesn't memoize per-listing.
    let vid = unique("flip_vid");
    let path = "/oracle/flip";

    let vol: Arc<WatchCoverageVolume> = Arc::new(WatchCoverageVolume::new("flip-vol", WatchCoverage::EveryWriter));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("x.txt")], 0);

    assert!(try_get_authoritative_listing(&vid, Path::new(path)).is_some());
    vol.set_coverage(WatchCoverage::None);
    assert!(try_get_authoritative_listing(&vid, Path::new(path)).is_none());

    get_volume_manager().unregister(&vid);
}
