//! When the observers hear about a listing, relative to the cache.
//!
//! - An observer hears an open AFTER the cache insert, and a close AFTER the
//!   cache removal: an observer's detached arm reconciles against cache
//!   membership, so a close it heard early would let a racing arm re-take what
//!   the close just gave back.
//! - The test-only synchronous start tells the observers too, the way the
//!   streaming read does.
//!
//! The app's change watch is the first observer it registers; that the watch
//! arms and disarms on that order is `listing_host/tests/lifecycle_order_test.rs`.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::testing::{Heard, RecordingObserver, TestEngine, TestListing, TestListingGuard, request};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, ListingWatch, Volume, VolumeError};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

/// An engine whose observer records every open and close, serving an empty
/// in-memory volume under `volume_id`.
fn observed_engine(volume_id: &str) -> (TestEngine, Arc<RecordingObserver>) {
    let observer = RecordingObserver::new();
    let engine = TestEngine::builder()
        .volume(volume_id, Arc::new(InMemoryVolume::new("Observed")))
        .observer(observer.clone())
        .build();
    observer.attach(&engine);
    (engine, observer)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observer_hears_an_open_after_the_listing_is_cached() {
    let (engine, observer) = observed_engine("lifecycle-open-vol");
    let listing = TestListingGuard::adopt(&engine, unique_test_id("lifecycle-open"));

    engine
        .read_with(
            &engine.streaming_state(),
            &request(listing.id(), "lifecycle-open-vol", "/"),
        )
        .await
        .expect("the volume lists");

    assert_eq!(
        observer.heard(),
        [Heard::Opened {
            id: listing.id().to_string(),
            cached: true
        }]
    );
}

#[test]
fn an_observer_hears_a_close_after_the_listing_left_the_cache() {
    let (engine, observer) = observed_engine("lifecycle-close-vol");
    let listing = TestListing::new()
        .volume("lifecycle-close-vol")
        .insert(&engine, "lifecycle-close");

    engine.inner.list_directory_end(listing.id());

    assert_eq!(
        observer.heard(),
        [Heard::Closed {
            id: listing.id().to_string(),
            cached: false
        }]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_synchronously_started_listing_tells_its_observers_it_opened() {
    let (engine, observer) = observed_engine("lifecycle-sync-vol");

    let started = engine
        .inner
        .list_directory_start_with_volume(
            "lifecycle-sync-vol",
            Path::new("/"),
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
        .await
        .expect("the volume lists");
    let listing = TestListingGuard::adopt(&engine, started.listing_id);

    assert_eq!(
        observer.heard(),
        [Heard::Opened {
            id: listing.id().to_string(),
            cached: true
        }]
    );
}

/// An in-memory volume that hands out a [`ListingWatch`] per shown directory
/// and counts how many are alive, the way the SMB backend keeps a DFS link's
/// watch for as long as a listing behind it is open.
struct HoldingVolume {
    inner: InMemoryVolume,
    alive: Arc<AtomicUsize>,
}

struct CountedHold(Arc<AtomicUsize>);

impl Drop for CountedHold {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Volume for HoldingVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn watch_listing(&self, _path: &Path) -> Option<ListingWatch> {
        self.alive.fetch_add(1, Ordering::SeqCst);
        Some(ListingWatch::new(CountedHold(Arc::clone(&self.alive))))
    }
}

/// What a volume keeps alive for a shown directory lives exactly as long as
/// the listing: taken on open, released on close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_keeps_its_listing_watch_until_the_listing_closes() {
    let alive = Arc::new(AtomicUsize::new(0));
    let engine = TestEngine::builder()
        .volume(
            "holding-vol",
            Arc::new(HoldingVolume {
                inner: InMemoryVolume::new("Holding"),
                alive: Arc::clone(&alive),
            }),
        )
        .build();
    let listing = TestListingGuard::adopt(&engine, unique_test_id("holding"));

    engine
        .read_with(&engine.streaming_state(), &request(listing.id(), "holding-vol", "/"))
        .await
        .expect("the volume lists");
    assert_eq!(alive.load(Ordering::SeqCst), 1, "the open takes the volume's hold");

    engine.inner.list_directory_end(listing.id());
    assert_eq!(alive.load(Ordering::SeqCst), 0, "the close releases it");
}
