//! Tests for the streaming directory listing pipeline.
//!
//! Uses `CollectorListingEventSink` and `InMemoryVolume` to test
//! `read_directory_with_progress` without a Tauri runtime.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::file_system::listing::caching::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};
use crate::test_support::wait_until_async;

/// Creates a test file entry under the root directory.
fn test_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        size: if is_dir { None } else { Some(1024) },
        modified_at: Some(1_640_000_000),
        created_at: Some(1_639_000_000),
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
    }
}

/// Registers an `InMemoryVolume` with the global `VolumeManager` and returns the volume ID.
/// Caller must call `cleanup_volume` after the test.
fn register_test_volume(volume_id: &str, entries: Vec<FileEntry>) {
    let volume = Arc::new(InMemoryVolume::with_entries("Test Volume", entries));
    crate::file_system::get_volume_manager().register(volume_id, volume);
}

/// Removes the test volume and listing cache entry.
fn cleanup(volume_id: &str, listing_id: &str) {
    crate::file_system::get_volume_manager().unregister(volume_id);
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.remove(listing_id);
    }
}

fn new_state() -> Arc<StreamingListingState> {
    Arc::new(StreamingListingState {
        cancelled: Arc::new(AtomicBool::new(false)),
        cancel_notify: tokio::sync::Notify::new(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_populates_cache() {
    let volume_id = &format!("test-cache-{}", uuid::Uuid::new_v4());
    let listing_id = &format!("listing-cache-{}", uuid::Uuid::new_v4());

    let entries = vec![
        test_entry("photos", true),
        test_entry("zebra.txt", false),
        test_entry("apple.txt", false),
    ];
    register_test_volume(volume_id, entries);

    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing_id,
        &state,
        volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);

    // Verify cache
    {
        let cache = LISTING_CACHE.read().unwrap();
        let cached = cache.get(listing_id).expect("Listing should be cached");
        assert_eq!(cached.entries.len(), 3);
        // Dirs first, then alpha
        assert_eq!(cached.entries[0].name, "photos");
        assert!(cached.entries[0].is_directory);
        assert_eq!(cached.entries[1].name, "apple.txt");
        assert_eq!(cached.entries[2].name, "zebra.txt");
    }

    // Verify complete event
    let collector = events.as_ref() as *const dyn ListingEventSink as *const CollectorListingEventSink;
    // SAFETY: (test) `events` was constructed just above as `Arc::new(CollectorListingEventSink::new())`,
    // so the trait object's concrete type is statically known to be `CollectorListingEventSink`. The
    // pointer comes straight from that live `Arc` (no provenance gap), so the downcast and reborrow are
    // valid for the borrow's lifetime.
    let collector = unsafe { &*collector };
    let complete = collector.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].1, 3); // total_count

    cleanup(volume_id, listing_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_emits_opening_and_complete() {
    let volume_id = &format!("test-events-{}", uuid::Uuid::new_v4());
    let listing_id = &format!("listing-events-{}", uuid::Uuid::new_v4());

    register_test_volume(volume_id, vec![test_entry("file.txt", false)]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing_id,
        &state,
        volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_ok());

    let opening = sink.opening.lock().unwrap();
    assert_eq!(opening.len(), 1);
    assert_eq!(opening[0], listing_id.as_str());

    let complete = sink.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].0, listing_id.as_str());

    let read_complete = sink.read_complete.lock().unwrap();
    assert_eq!(read_complete.len(), 1);

    // No errors or cancellations
    assert!(sink.errors.lock().unwrap().is_empty());
    assert!(sink.cancelled.lock().unwrap().is_empty());

    cleanup(volume_id, listing_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_cancellation() {
    let volume_id = &format!("test-cancel-{}", uuid::Uuid::new_v4());
    let listing_id = &format!("listing-cancel-{}", uuid::Uuid::new_v4());

    register_test_volume(volume_id, vec![test_entry("file.txt", false)]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    // Set cancelled BEFORE calling
    state.cancelled.store(true, Ordering::Relaxed);

    let result = read_directory_with_progress(
        &events,
        listing_id,
        &state,
        volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_ok());

    // Cancelled event should be emitted
    let cancelled = sink.cancelled.lock().unwrap();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0], listing_id.as_str());

    // No entries cached
    {
        let cache = LISTING_CACHE.read().unwrap();
        assert!(cache.get(listing_id).is_none());
    }

    // No complete event
    assert!(sink.complete.lock().unwrap().is_empty());

    cleanup(volume_id, listing_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_volume_not_found() {
    let listing_id = &format!("listing-notfound-{}", uuid::Uuid::new_v4());
    let volume_id = &format!("nonexistent-volume-{}", uuid::Uuid::new_v4());

    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing_id,
        &state,
        volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_err());
    match result {
        Err(VolumeError::NotFound(msg)) => {
            assert!(msg.contains("Volume not found"), "Unexpected message: {}", msg);
        }
        other => panic!("Expected VolumeError::NotFound, got {:?}", other),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_empty_directory() {
    let volume_id = &format!("test-empty-{}", uuid::Uuid::new_v4());
    let listing_id = &format!("listing-empty-{}", uuid::Uuid::new_v4());

    register_test_volume(volume_id, vec![]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing_id,
        &state,
        volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_ok());

    // Cache should have 0 entries
    {
        let cache = LISTING_CACHE.read().unwrap();
        let cached = cache.get(listing_id).expect("Listing should be cached even when empty");
        assert_eq!(cached.entries.len(), 0);
    }

    // Complete should report 0
    let complete = sink.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].1, 0);

    cleanup(volume_id, listing_id);
}

/// A volume whose listing only ends when its cancel flag flips: the stand-in for
/// an MTP device, where the listing is a long chain of USB round trips and the
/// backend bails between them.
///
/// Records which of the two exits happened. `finished` means the listing future
/// ran to its own cooperative end; `aborted` means it was dropped mid-listing,
/// which on a real phone abandons a PTP transaction and wedges the device.
struct CooperativeCancelVolume {
    root: std::path::PathBuf,
    started: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    aborted: Arc<AtomicBool>,
}

/// Flips `aborted` unless disarmed, so a dropped listing future is observable.
struct AbortWitness {
    aborted: Arc<AtomicBool>,
    armed: bool,
}

impl Drop for AbortWitness {
    fn drop(&mut self) {
        if self.armed {
            self.aborted.store(true, Ordering::SeqCst);
        }
    }
}

impl CooperativeCancelVolume {
    fn new() -> Self {
        Self {
            root: std::path::PathBuf::from("/"),
            started: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
            aborted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The listing body both `list_directory` and `list_directory_with_cancel`
    /// run: spin until the token flips, with a hard iteration cap so a missing
    /// token fails the test instead of hanging the suite.
    fn listing_body(
        &self,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'static>> {
        let started = Arc::clone(&self.started);
        let finished = Arc::clone(&self.finished);
        let aborted = Arc::clone(&self.aborted);
        Box::pin(async move {
            let mut witness = AbortWitness { aborted, armed: true };
            started.store(true, Ordering::SeqCst);
            for _ in 0..2_000 {
                if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                    break;
                }
                // allowed-test-sleep: this fake backend simulates a long, cancellable listing; the
                // per-iteration wait is what keeps it in flight long enough for a cancel to land mid-run
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            witness.armed = false;
            finished.store(true, Ordering::SeqCst);
            Ok(Vec::new())
        })
    }
}

impl Volume for CooperativeCancelVolume {
    fn name(&self) -> &str {
        "Cooperative cancel volume"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        // No token: this is the shape that can only be stopped by dropping the
        // future, which is exactly what must not happen.
        self.listing_body(None)
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.listing_body(cancel.map(Arc::clone))
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotFound("not implemented".to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(true) })
    }
}

/// The budget for either flag below: the backend flips both within a few polls, so a
/// timeout means the cancel path stopped working, never load.
const FLAG_FLIPS_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Cancelling a listing must let the backend unwind at its own safe boundary,
/// never drop its future. On MTP a dropped future abandons an in-flight PTP
/// transaction and wedges the phone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cancel_unwinds_the_listing_instead_of_aborting_it() {
    let volume_id = &format!("test-coop-{}", uuid::Uuid::new_v4());
    let listing_id = &format!("listing-coop-{}", uuid::Uuid::new_v4());

    let volume = Arc::new(CooperativeCancelVolume::new());
    let started = Arc::clone(&volume.started);
    let finished = Arc::clone(&volume.finished);
    let aborted = Arc::clone(&volume.aborted);
    crate::file_system::get_volume_manager().register(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let read = {
        let events = Arc::clone(&events);
        let state = Arc::clone(&state);
        let volume_id = volume_id.clone();
        let listing_id = listing_id.clone();
        tokio::spawn(async move {
            read_directory_with_progress(
                &events,
                &listing_id,
                &state,
                &volume_id,
                Path::new("/"),
                true,
                SortColumn::Name,
                SortOrder::Ascending,
                DirectorySortMode::LikeFiles,
            )
            .await
        })
    };

    wait_until_async(FLAG_FLIPS_WITHIN, "the listing to start before we cancel it", || {
        started.load(Ordering::SeqCst)
    })
    .await;

    // Same two steps as `cancel_listing`.
    state.cancelled.store(true, Ordering::Relaxed);
    state.cancel_notify.notify_waiters();

    let result = read.await.expect("listing task must not panic");
    assert!(result.is_ok());
    assert_eq!(
        sink.cancelled.lock().unwrap().len(),
        1,
        "the user must see a prompt cancel"
    );

    wait_until_async(
        FLAG_FLIPS_WITHIN,
        "the backend to reach its own cooperative end after a cancel",
        || finished.load(Ordering::SeqCst),
    )
    .await;
    assert!(
        !aborted.load(Ordering::SeqCst),
        "the listing future was dropped mid-flight; on MTP that abandons a PTP transaction and wedges the device"
    );

    cleanup(volume_id, listing_id);
}
