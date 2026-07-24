//! `try_get_watched_listing` tests (M1 oracle).
//!
//! These tests use a small `WatchedFlagVolume` wrapper around `InMemoryVolume`
//! because `InMemoryVolume::listing_is_watched` always returns false (the
//! default). The wrapper lets tests pin the watcher flag to `true` or `false`
//! without touching `WATCHER_MANAGER` (which would require an `AppHandle`).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::caching::try_get_watched_listing;
use super::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use super::metadata::FileEntry;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream,
};

/// A test-only volume wrapper that overrides `listing_is_watched` with a
/// flag controlled per test. Delegates every other method to the inner
/// `InMemoryVolume`.
struct WatchedFlagVolume {
    inner: InMemoryVolume,
    watched: AtomicBool,
}

impl WatchedFlagVolume {
    fn new(name: &str, watched: bool) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            watched: AtomicBool::new(watched),
        }
    }

    fn set_watched(&self, v: bool) {
        self.watched.store(v, Ordering::Relaxed);
    }
}

impl Volume for WatchedFlagVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        self.watched.load(Ordering::Relaxed)
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch(paths)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_conflicts(source_items, dest_path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
}

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
fn try_get_watched_listing_hit_when_watcher_reports_true() {
    let vid = unique("hit_vid");
    let path = "/oracle/hit";

    let vol = Arc::new(WatchedFlagVolume::new("hit-vol", true));
    get_volume_manager().register(&vid, vol);

    let entries = vec![make_test_entry("a.txt"), make_test_entry("b.txt")];
    let _lid = insert_listing_with_sequence("listing", &vid, path, entries.clone(), 0);

    let result = try_get_watched_listing(&vid, Path::new(path));
    assert!(result.is_some(), "expected Some(entries) on watched listing");
    let returned = result.unwrap();
    assert_eq!(returned.len(), entries.len());
    assert_eq!(returned[0].name, "a.txt");
    assert_eq!(returned[1].name, "b.txt");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_watched_listing_miss_when_watcher_reports_false() {
    let vid = unique("miss_watch_vid");
    let path = "/oracle/miss_watch";

    let vol = Arc::new(WatchedFlagVolume::new("miss-vol", false));
    get_volume_manager().register(&vid, vol);

    let entries = vec![make_test_entry("a.txt")];
    let _lid = insert_listing_with_sequence("listing", &vid, path, entries, 0);

    let result = try_get_watched_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when watcher is dead");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_watched_listing_miss_when_no_listing_exists() {
    let vid = unique("miss_no_listing_vid");
    let vol = Arc::new(WatchedFlagVolume::new("no-listing-vol", true));
    get_volume_manager().register(&vid, vol);

    let result = try_get_watched_listing(&vid, Path::new("/oracle/nothing_here"));
    assert!(result.is_none(), "expected None when no listing matches");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_watched_listing_miss_when_volume_not_registered() {
    let vid = unique("no_vol");
    let path = "/oracle/no_vol";

    // Listing exists in cache, but no volume is registered for this ID.
    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = try_get_watched_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None when volume isn't registered");
}

#[test]
fn try_get_watched_listing_picks_highest_sequence() {
    // Two listings on the same (volume_id, path) with different sequence
    // numbers. The oracle must return the entries from the higher-sequence
    // listing, deterministically — never the lower-sequence one.
    let vid = unique("seq_vid");
    let path = "/oracle/seq_path";

    let vol = Arc::new(WatchedFlagVolume::new("seq-vol", true));
    get_volume_manager().register(&vid, vol);

    let _lid_lo = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("low.txt")], 1);
    let _lid_hi = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("high.txt")], 9);

    let result = try_get_watched_listing(&vid, Path::new(path));
    assert!(result.is_some());
    let returned = result.unwrap();
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].name, "high.txt", "expected the higher-sequence listing");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_watched_listing_miss_for_start_streaming_watcher_gap() {
    // Simulates the documented race window between
    // `list_directory_start_streaming` populating LISTING_CACHE and
    // `start_watching` inserting into WATCHER_MANAGER: the listing exists
    // in cache, but the volume reports no watcher yet (here: by reporting
    // `false` from the test volume's `listing_is_watched`). The oracle
    // must miss in that window so write ops fall through to a real read.
    let vid = unique("race_vid");
    let path = "/oracle/race";

    // `watched=false` mirrors "WATCHER_MANAGER has no entry yet" on the
    // local backend without needing an AppHandle.
    let vol = Arc::new(WatchedFlagVolume::new("race-vol", false));
    get_volume_manager().register(&vid, vol);

    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("a.txt")], 0);

    let result = try_get_watched_listing(&vid, Path::new(path));
    assert!(result.is_none(), "expected None during the streaming->watcher gap");

    get_volume_manager().unregister(&vid);
}

#[test]
fn try_get_watched_listing_reflects_flip_to_unwatched() {
    // Sanity check: flipping the watcher flag flips the oracle's verdict
    // on subsequent calls. Documents that the oracle is a live query and
    // doesn't memoize per-listing.
    let vid = unique("flip_vid");
    let path = "/oracle/flip";

    let vol: Arc<WatchedFlagVolume> = Arc::new(WatchedFlagVolume::new("flip-vol", true));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let _lid = insert_listing_with_sequence("listing", &vid, path, vec![make_test_entry("x.txt")], 0);

    assert!(try_get_watched_listing(&vid, Path::new(path)).is_some());
    vol.set_watched(false);
    assert!(try_get_watched_listing(&vid, Path::new(path)).is_none());

    get_volume_manager().unregister(&vid);
}
