//! Tests for the streaming directory listing pipeline.
//!
//! Uses `CollectorListingEventSink` and `InMemoryVolume` to test
//! `read_directory_with_progress` without a Tauri runtime.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::file_system::listing::caching::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::InMemoryVolume;

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
        cancelled: AtomicBool::new(false),
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
        Err(crate::file_system::volume::VolumeError::NotFound(msg)) => {
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
