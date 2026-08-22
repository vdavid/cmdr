//! Tests for the streaming directory listing pipeline.
//!
//! Uses `CollectorListingEventSink` and `InMemoryVolume` to test
//! `read_directory_with_progress` without a Tauri runtime.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;

use crate::file_system::listing::caching_test_support::{TestListingGuard, unique_test_id};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, LocalPosixVolume, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::{TestDir, wait_until_async};

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
    get_volume_manager().register(volume_id, volume);
}

/// Removes the test volume. The listing entry is owned by a `TestListingGuard`,
/// which tears it down on drop (unwind included).
fn cleanup(volume_id: &str) {
    get_volume_manager().unregister(volume_id);
}

fn new_state() -> Arc<StreamingListingState> {
    Arc::new(StreamingListingState {
        cancel: CancellationToken::new(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_populates_cache() {
    let volume_id = &format!("test-cache-{}", uuid::Uuid::new_v4());
    let listing = TestListingGuard::adopt(unique_test_id("streaming-cache"));

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
        listing.id(),
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
    listing.with_listing(|cached| {
        assert_eq!(cached.entries().len(), 3);
        // Dirs first, then alpha
        assert_eq!(cached.entries()[0].name, "photos");
        assert!(cached.entries()[0].is_directory);
        assert_eq!(cached.entries()[1].name, "apple.txt");
        assert_eq!(cached.entries()[2].name, "zebra.txt");
    });

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

    cleanup(volume_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_emits_opening_and_complete() {
    let volume_id = &format!("test-events-{}", uuid::Uuid::new_v4());
    let listing = TestListingGuard::adopt(unique_test_id("streaming-events"));

    register_test_volume(volume_id, vec![test_entry("file.txt", false)]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing.id(),
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
    assert_eq!(opening[0], listing.id());

    let complete = sink.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].0, listing.id());

    let read_complete = sink.read_complete.lock().unwrap();
    assert_eq!(read_complete.len(), 1);

    // No errors or cancellations
    assert!(sink.errors.lock().unwrap().is_empty());
    assert!(sink.cancelled.lock().unwrap().is_empty());

    cleanup(volume_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_cancellation() {
    let volume_id = &format!("test-cancel-{}", uuid::Uuid::new_v4());
    let listing = TestListingGuard::adopt(unique_test_id("streaming-cancel"));

    register_test_volume(volume_id, vec![test_entry("file.txt", false)]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    // Set cancelled BEFORE calling
    state.cancel.cancel();

    let result = read_directory_with_progress(
        &events,
        listing.id(),
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
    assert_eq!(cancelled[0], listing.id());

    // No entries cached
    assert!(!listing.is_cached());

    // No complete event
    assert!(sink.complete.lock().unwrap().is_empty());

    cleanup(volume_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_list_volume_not_found() {
    let listing = TestListingGuard::adopt(unique_test_id("streaming-notfound"));
    let volume_id = &format!("nonexistent-volume-{}", uuid::Uuid::new_v4());

    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing.id(),
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
    let listing = TestListingGuard::adopt(unique_test_id("streaming-empty"));

    register_test_volume(volume_id, vec![]);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing.id(),
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

    // Cache should have 0 entries (`with_listing` panics if it wasn't cached at all)
    listing.with_listing(|cached| assert_eq!(cached.entries().len(), 0));

    // Complete should report 0
    let complete = sink.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].1, 0);

    cleanup(volume_id);
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
        cancel: Option<CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'static>> {
        let started = Arc::clone(&self.started);
        let finished = Arc::clone(&self.finished);
        let aborted = Arc::clone(&self.aborted);
        Box::pin(async move {
            let mut witness = AbortWitness { aborted, armed: true };
            started.store(true, Ordering::SeqCst);
            for _ in 0..2_000 {
                if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
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
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.listing_body(cancel.cloned())
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
    let listing = TestListingGuard::adopt(unique_test_id("streaming-coop"));

    let volume = Arc::new(CooperativeCancelVolume::new());
    let started = Arc::clone(&volume.started);
    let finished = Arc::clone(&volume.finished);
    let aborted = Arc::clone(&volume.aborted);
    get_volume_manager().register(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let read = {
        let events = Arc::clone(&events);
        let state = Arc::clone(&state);
        let volume_id = volume_id.clone();
        let listing_id = listing.id().to_string();
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

    // The same one step as `cancel_listing`.
    state.cancel.cancel();

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

    cleanup(volume_id);
}

/// The whole chain a real local folder takes to the pane's "Loaded N files..." line:
/// `read_directory_with_progress` -> `Volume::list_directory` -> `listing-progress`. The
/// other tests here run on `InMemoryVolume`, which can't catch a local backend that drops
/// its `on_progress`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_directory_read_emits_progress_events() {
    let dir = TestDir::new("streaming_local_progress");
    for i in 0..5_000 {
        std::fs::write(dir.join(format!("file_{i:05}.txt")), b"x").expect("writing a scratch file succeeds");
    }

    let volume_id = &format!("test-local-progress-{}", uuid::Uuid::new_v4());
    let listing = TestListingGuard::adopt(unique_test_id("streaming-local-progress"));
    get_volume_manager().register(volume_id, Arc::new(LocalPosixVolume::new("Test Volume", &*dir)));

    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = new_state();

    let result = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        volume_id,
        Path::new(""),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;
    assert!(result.is_ok(), "listing the scratch dir succeeds: {:?}", result.err());

    let progress = sink.progress.lock_ignore_poison();
    assert!(
        !progress.is_empty(),
        "reading 5,000 local entries emitted no listing-progress, so the pane would sit on \"Opening folder...\""
    );
    assert!(
        progress.iter().all(|(id, _)| id == listing.id()),
        "progress must be tagged with the listing that asked for it"
    );
    assert_eq!(
        sink.read_complete.lock_ignore_poison().first().map(|(_, total)| *total),
        Some(5_000),
        "read-complete reports the full count"
    );

    cleanup(volume_id);
}
