//! Tests for `volume/strategy.rs`'s `copy_single_path`: basic local→local copy,
//! cancellation, and the cross-volume streaming-copy path (single file,
//! multi-chunk with progress, mid-file cancel, empty file, missing source,
//! streaming-route selection, and recursive directory copy).

use super::test_support::{SLOW_CHUNK_COUNT, SlowSource, make_state};
use super::*;
use crate::file_system::write_operations::state::{OperationIntent, cancel_write_operation};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::transfer::transfer_driver::{LeafProgressLedger, ObservedProgress};
use crate::test_support::wait_until_async;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume, Volume, VolumeError};
use crate::test_support::TestDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_copy_single_path_local_to_local() {
    use std::fs;

    let src_dir = TestDir::new("copy_single_src");
    let dst_dir = TestDir::new("copy_single_dst");

    fs::write(src_dir.join("source.txt"), "Source content").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dst_dir.to_str().unwrap()));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    let bytes = copy_single_path(
        &source,
        Path::new("source.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("dest.txt"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 14); // "Source content"
    assert_eq!(fs::read_to_string(dst_dir.join("dest.txt")).unwrap(), "Source content");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_copy_single_path_cancelled() {
    use std::fs;

    let src_dir = TestDir::new("copy_cancel_src");
    let dst_dir = TestDir::new("copy_cancel_dst");

    fs::write(src_dir.join("source.txt"), "Content").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dst_dir.to_str().unwrap()));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);

    let result = copy_single_path(
        &source,
        Path::new("source.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("dest.txt"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err().error, VolumeError::Cancelled(_)));
}

// ========================================================================
// Cross-volume streaming copy tests (InMemoryVolume pairs)
// ========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_single_file() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source
        .create_file(Path::new("/photo.jpg"), b"JPEG data here")
        .await
        .unwrap();

    let state = make_state();
    let bytes = copy_single_path(
        &source,
        Path::new("/photo.jpg"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/photo.jpg"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 14);
    // Verify content
    let mut stream = dest.open_read_stream(Path::new("/photo.jpg")).await.unwrap();
    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"JPEG data here");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_large_file_with_progress() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    let data: Vec<u8> = (0..=255).cycle().take(200_000).collect();
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let state = make_state();
    // What the copy REPORTS, read off the progress events the dialog reads.
    let progress = ObservedProgress::new(&state, "large-file-progress");

    let bytes = copy_single_path(
        &source,
        Path::new("/big.bin"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/big.bin"),
        &state,
        &CreatedPaths::default(),
        &progress.source,
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 200_000);
    assert!(
        progress.emits.load(Ordering::Relaxed) >= 2,
        "a multi-chunk file must report progress as it streams, not once at the end"
    );
    assert_eq!(
        progress.bytes.load(Ordering::Relaxed),
        200_000,
        "the last thing the user is told must be the whole file"
    );
    assert_eq!(
        progress.files.load(Ordering::Relaxed),
        1,
        "and the File bar must have crossed exactly one leaf"
    );

    // Verify content integrity
    let mut stream = dest.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_cancel_mid_file() {
    // A GATED source, so the copy is provably mid-file when the cancel lands.
    // Racing a cancel against an in-memory copy loses: the file is over before
    // the canceller wakes, and the test then asserts nothing.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let source: Arc<dyn Volume> = Arc::new(SlowSource {
        gate: Arc::clone(&gate),
    });
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    // Tier 1 travels through the engine's own per-chunk progress callback, which
    // is what carries the stop down to the backend so it drops its own partial.
    let op = TestOperationGuard::register_state("copy-cancel-mid-file", make_state());
    let state = Arc::clone(op.state());
    let progress = ObservedProgress::new(&state, op.id());

    let created = CreatedPaths::default();
    let copy = copy_single_path(
        &source,
        Path::new("/big.bin"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/big.bin"),
        &state,
        &created,
        &progress.source,
        None,
        WriteStaging::Stage,
    );
    tokio::pin!(copy);

    // One chunk through, then the copy blocks waiting for the next permit.
    gate.add_permits(1);
    tokio::select! {
        r = &mut copy => panic!("a {SLOW_CHUNK_COUNT}-chunk copy can't be done after one permit: {r:?}"),
        () = wait_until_async(Duration::from_secs(10), "the first chunk to land", || {
            progress.bytes.load(Ordering::Relaxed) > 0
        }) => {}
    }

    cancel_write_operation(op.id(), false);
    gate.add_permits(SLOW_CHUNK_COUNT);
    let result = copy.await;

    assert!(result.is_err(), "a cancelled copy must not report success");
    // Nothing lands at the destination: the backend dropped its own partial when
    // the progress callback broke, and the staged temp never took the real name.
    assert!(!dest.exists(Path::new("/big.bin")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_empty_file() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source.create_file(Path::new("/empty.txt"), b"").await.unwrap();

    let state = make_state();
    let bytes = copy_single_path(
        &source,
        Path::new("/empty.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/empty.txt"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 0);
    assert!(dest.exists(Path::new("/empty.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_nonexistent_source_fails() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    let state = make_state();
    let result = copy_single_path(
        &source,
        Path::new("/nope.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/nope.txt"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_uses_streaming_for_non_local_volumes() {
    // InMemoryVolume has local_path() = None and supports_streaming() = true.
    // Verify that copy_single_path routes through the streaming path.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    // Verify the routing assumptions
    assert!(source.local_path().is_none(), "InMemoryVolume should not be local");
    assert!(source.supports_streaming(), "InMemoryVolume should support streaming");
    assert!(dest.supports_streaming(), "InMemoryVolume should support streaming");

    source
        .create_file(Path::new("/test.txt"), b"routed correctly")
        .await
        .unwrap();

    let state = make_state();
    let progress = ObservedProgress::new(&state, "routing-leaf-count");
    let bytes = copy_single_path(
        &source,
        Path::new("/test.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/test.txt"),
        &state,
        &CreatedPaths::default(),
        &progress.source,
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 16);
    assert_eq!(
        progress.files.load(Ordering::Relaxed),
        1,
        "on_file_complete should fire"
    );

    let mut stream = dest.open_read_stream(Path::new("/test.txt")).await.unwrap();
    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"routed correctly");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_directory_recursive() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    source.create_directory(Path::new("/docs")).await.unwrap();
    source
        .create_file(Path::new("/docs/readme.txt"), b"Read me")
        .await
        .unwrap();
    source
        .create_file(Path::new("/docs/notes.txt"), b"Notes here")
        .await
        .unwrap();

    let state = make_state();
    let progress = ObservedProgress::new(&state, "routing-leaf-count");
    let bytes = copy_single_path(
        &source,
        Path::new("/docs"),
        Some(true),
        SourceFileFacts::default(),
        &dest,
        Path::new("/docs"),
        &state,
        &CreatedPaths::default(),
        &progress.source,
        None,
        WriteStaging::Stage,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 17); // 7 + 10
    assert_eq!(progress.files.load(Ordering::Relaxed), 2);

    assert!(dest.exists(Path::new("/docs/readme.txt")).await);
    assert!(dest.exists(Path::new("/docs/notes.txt")).await);
}
