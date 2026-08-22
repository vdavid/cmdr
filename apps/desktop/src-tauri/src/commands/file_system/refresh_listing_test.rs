//! Tests for `refresh_listing`: the watcher-backed short-circuit, the `force` flag
//! that bypasses it, and the wait that must never drop a slow re-read.
//!
//! Split out under the directory's `*_test.rs` convention. The harness is a
//! counter-wrapping `InMemoryVolume` (adapted from
//! `write_operations::delete_volume_reuse_tests`) whose `listing_watch_coverage` is
//! flipped per test and whose reads can be stalled on a gate, seeded into
//! `LISTING_CACHE` and `VolumeManager`. Then we call the command and assert whether
//! `list_directory` ran, and whether it finished.
use super::*;
use crate::file_system::listing::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError, WatchCoverage};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Wraps an `InMemoryVolume` and counts `list_directory` calls. `watched` is
/// flipped per test to pin both short-circuit and fall-through behaviour.
struct CountingVolume {
    inner: InMemoryVolume,
    watched: AtomicBool,
    list_dir_calls: AtomicUsize,
    list_dir_completions: AtomicUsize,
    /// Held closed to stall `list_directory` mid-read, so a test can outlast the
    /// command's wait and then check whether the read survived it.
    gate: Arc<tokio::sync::Notify>,
    gated: AtomicBool,
}

impl CountingVolume {
    fn new(name: &str, watched: bool) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            watched: AtomicBool::new(watched),
            list_dir_calls: AtomicUsize::new(0),
            list_dir_completions: AtomicUsize::new(0),
            gate: Arc::new(tokio::sync::Notify::new()),
            gated: AtomicBool::new(false),
        }
    }

    fn list_dir_count(&self) -> usize {
        self.list_dir_calls.load(Ordering::Relaxed)
    }

    /// Reads block until [`Self::open_gate`] is called.
    fn close_gate(&self) {
        self.gated.store(true, Ordering::Relaxed);
    }

    fn open_gate(&self) {
        self.gated.store(false, Ordering::Relaxed);
        self.gate.notify_waiters();
    }

    /// Reads that ran all the way to their result, rather than being dropped.
    fn completed_reads(&self) -> usize {
        self.list_dir_completions.load(Ordering::Relaxed)
    }
}

impl Volume for CountingVolume {
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
        self.list_dir_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            while self.gated.load(Ordering::Relaxed) {
                self.gate.notified().await;
            }
            let entries = self.inner.list_directory(path, on_progress).await;
            self.list_dir_completions.fetch_add(1, Ordering::Relaxed);
            entries
        })
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

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        if self.watched.load(Ordering::Relaxed) {
            WatchCoverage::EveryWriter
        } else {
            WatchCoverage::None
        }
    }
}

fn insert_listing(tag: &str, volume_id: &str, path: &str) -> TestListingGuard {
    TestListing::new().volume(volume_id).path(path).sequence(1).insert(tag)
}

/// Watched volume: short-circuit fires, `list_directory` never called.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_listing_short_circuits_on_watched_volume() {
    let vid = unique_test_id("refresh-listing-short-circuit-vid");
    let path = "/dcim";

    let vol = Arc::new(CountingVolume::new("watched-vol", true));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    let listing = insert_listing("refresh-listing-short-circuit", &vid, path);

    let result = refresh_listing(listing.id().to_string(), false).await;

    assert!(!result.timed_out, "short-circuit returns timed_out=false");
    assert_eq!(
        vol.list_dir_count(),
        0,
        "watched-backed refresh_listing must skip list_directory",
    );

    get_volume_manager().unregister(&vid);
}

/// Watched volume, `force: true`: the short-circuit is bypassed and the
/// directory is actually re-read. This is the user's ⌘R and the MCP `refresh`
/// tool: "I think the cache is stale" has to mean a real re-read, even where
/// the watcher claims to see every writer (SMB's watcher doesn't).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forced_refresh_listing_rereads_watched_volume() {
    let vid = unique_test_id("refresh-listing-forced-vid");
    let path = "/dcim";

    let vol = Arc::new(CountingVolume::new("watched-vol", true));
    // Populate one file so `list_directory` succeeds.
    vol.inner.create_file(Path::new("/dcim/a.jpg"), b"alpha").await.unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    let listing = insert_listing("refresh-listing-forced", &vid, path);

    let result = refresh_listing(listing.id().to_string(), true).await;

    assert!(!result.timed_out, "fast InMemory list_directory shouldn't time out");
    assert!(
        vol.list_dir_count() >= 1,
        "a forced refresh must re-read even a watcher-backed listing (count was {})",
        vol.list_dir_count(),
    );

    get_volume_manager().unregister(&vid);
}

/// Unwatched volume: fall-through path runs (`handle_directory_change` calls
/// `list_directory`). The InMemoryVolume's directory exists so we get a real
/// listing rather than NotFound.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_listing_falls_through_on_unwatched() {
    let vid = unique_test_id("refresh-listing-fallthrough-vid");
    let path = "/dcim";

    let vol = Arc::new(CountingVolume::new("unwatched-vol", false));
    // Populate one file so `list_directory` succeeds.
    vol.inner.create_file(Path::new("/dcim/a.jpg"), b"alpha").await.unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    let listing = insert_listing("refresh-listing-fallthrough", &vid, path);

    let result = refresh_listing(listing.id().to_string(), false).await;

    assert!(!result.timed_out, "fast InMemory list_directory shouldn't time out");
    assert!(
        vol.list_dir_count() >= 1,
        "unwatched volume must fall through to list_directory (count was {})",
        vol.list_dir_count(),
    );

    get_volume_manager().unregister(&vid);
}

/// A read slower than the command's wait must SURVIVE the wait: the command
/// answers `timed_out: true` so the FE stops blocking, and the re-read keeps
/// going and lands its diff. Dropping it instead would abandon an MTP read
/// mid-PTP-transaction, which wedges a phone until it's replugged, and would
/// throw away the very re-read the user asked for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_slower_than_the_wait_still_finishes() {
    let vid = unique_test_id("refresh-listing-slow-read-vid");
    let path = "/dcim";

    let vol = Arc::new(CountingVolume::new("slow-vol", false));
    vol.inner.create_file(Path::new("/dcim/a.jpg"), b"alpha").await.unwrap();
    vol.close_gate();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);
    let listing = insert_listing("refresh-listing-slow-read", &vid, path);

    let result = refresh_listing_within(listing.id().to_string(), false, Duration::from_millis(50)).await;

    assert!(result.timed_out, "a read still running past the wait reports timed_out");
    assert_eq!(vol.completed_reads(), 0, "the read is still stalled at this point");

    vol.open_gate();
    crate::test_support::wait_until_async(Duration::from_secs(2), "the stalled re-read to finish", || {
        vol.completed_reads() == 1
    })
    .await;

    get_volume_manager().unregister(&vid);
}

/// No cache entry for the listing_id: today's behaviour is a clean no-op
/// (`handle_directory_change` early-returns). The short-circuit must NOT
/// suppress that path or panic; we just assert the call completes cleanly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_listing_falls_through_on_missing_listing() {
    let lid = unique_test_id("refresh-listing-missing");
    // No insert_listing call; no register call.
    let result = refresh_listing(lid, false).await;
    assert!(
        !result.timed_out,
        "missing listing should resolve quickly without timeout"
    );
}

/// Cache has the listing but the volume isn't registered: short-circuit
/// can't ask `listing_watch_coverage`, so we fall through to today's behaviour
/// (`handle_directory_change` finds no volume, falls back to local std::fs
/// for the path which doesn't exist, and returns cleanly without panic).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_listing_falls_through_when_volume_not_registered() {
    let vid = unique_test_id("refresh-listing-unregistered-vid");
    // Use a path that doesn't exist on disk so the std::fs fallback returns
    // NotFound and the function exits cleanly.
    let path = "/tmp/cmdr-refresh-listing-test-nonexistent-path-xyz123";

    // Note: NO get_volume_manager().register() call.
    let listing = insert_listing("refresh-listing-unregistered", &vid, path);

    let result = refresh_listing(listing.id().to_string(), false).await;

    assert!(
        !result.timed_out,
        "unregistered-volume fallthrough should resolve quickly"
    );
}
