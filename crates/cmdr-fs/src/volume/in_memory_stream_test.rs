//! `InMemoryVolume`'s streaming surface (`open_read_stream` / `write_from_stream`),
//! its multi-file delete edge cases, and the test-only knobs that make it lie
//! (`with_lane_key`, `set_stat_failing`). Core CRUD is `in_memory_test.rs`; the scan
//! surface is `in_memory_scan_test.rs`.

use super::*;
use std::path::Path;

// ============================================================================
// Layer 4: Delete, export/import, and edge case tests
// ============================================================================

#[tokio::test]
async fn test_delete_multiple_files() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/dir")).await.unwrap();
    volume.create_file(Path::new("/dir/a.txt"), b"a").await.unwrap();
    volume.create_file(Path::new("/dir/b.txt"), b"b").await.unwrap();
    volume.create_file(Path::new("/dir/c.txt"), b"c").await.unwrap();

    volume.delete(Path::new("/dir/b.txt")).await.unwrap();
    assert!(volume.exists(Path::new("/dir/a.txt")).await);
    assert!(!volume.exists(Path::new("/dir/b.txt")).await);
    assert!(volume.exists(Path::new("/dir/c.txt")).await);
}

#[tokio::test]
async fn test_open_read_stream_missing_file() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.open_read_stream(Path::new("/nope.txt")).await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

#[tokio::test]
async fn test_round_trip_stream_copy() {
    // Cross-volume round-trip via the unified streaming path: drive the
    // source's read stream into the destination's `write_from_stream`.
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");

    let data: Vec<u8> = (0..=255).cycle().take(50_000).collect();
    source.create_file(Path::new("/payload.bin"), &data).await.unwrap();

    let stream = source.open_read_stream(Path::new("/payload.bin")).await.unwrap();
    let size = stream.total_size();
    let bytes = dest
        .write_from_stream(Path::new("/payload.bin"), size, stream, &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(bytes, data.len() as u64);

    // Verify content integrity via streaming
    let mut stream = dest.open_read_stream(Path::new("/payload.bin")).await.unwrap();
    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);
}

// ============================================================================
// Layer 1: Streaming tests (open_read_stream + write_from_stream)
// ============================================================================

#[test]
fn test_supports_streaming() {
    let volume = InMemoryVolume::new("Test");
    assert!(volume.supports_streaming());
}

#[tokio::test]
async fn test_open_read_stream_small_file() {
    let volume = InMemoryVolume::new("Test");
    volume
        .create_file(Path::new("/hello.txt"), b"Hello, world!")
        .await
        .unwrap();

    let mut stream = volume.open_read_stream(Path::new("/hello.txt")).await.unwrap();
    assert_eq!(stream.total_size(), 13);
    assert_eq!(stream.bytes_read(), 0);

    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"Hello, world!");
    assert_eq!(stream.bytes_read(), 13);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn test_open_read_stream_empty_file() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/empty.txt"), b"").await.unwrap();

    let mut stream = volume.open_read_stream(Path::new("/empty.txt")).await.unwrap();
    assert_eq!(stream.total_size(), 0);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn test_open_read_stream_multi_chunk() {
    let volume = InMemoryVolume::new("Test");
    // Create a file larger than IN_MEMORY_STREAM_CHUNK_SIZE (64 KB)
    let data: Vec<u8> = (0..=255).cycle().take(100_000).collect();
    volume.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let mut stream = volume.open_read_stream(Path::new("/big.bin")).await.unwrap();
    assert_eq!(stream.total_size(), 100_000);

    let mut reassembled = Vec::new();
    let mut chunk_count = 0;
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
        chunk_count += 1;
    }
    assert_eq!(reassembled, data);
    assert!(chunk_count > 1, "expected multiple chunks, got {}", chunk_count);
    assert_eq!(stream.bytes_read(), 100_000);
}

#[tokio::test]
async fn test_open_read_stream_not_found() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.open_read_stream(Path::new("/nope.txt")).await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

#[tokio::test]
async fn test_open_read_stream_directory_fails() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/dir")).await.unwrap();
    let result = volume.open_read_stream(Path::new("/dir")).await;
    assert!(matches!(result, Err(VolumeError::IoError { .. })));
}

#[tokio::test]
async fn test_write_from_stream_creates_file() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    source
        .create_file(Path::new("/data.bin"), b"source content")
        .await
        .unwrap();

    let stream = source.open_read_stream(Path::new("/data.bin")).await.unwrap();
    let no_progress = &|_: u64, _: u64| std::ops::ControlFlow::Continue(());
    let bytes = dest
        .write_from_stream(Path::new("/data.bin"), 14, stream, no_progress)
        .await
        .unwrap();

    assert_eq!(bytes, 14);
    // Verify content arrived correctly
    let mut verify = dest.open_read_stream(Path::new("/data.bin")).await.unwrap();
    let chunk = verify.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"source content");
}

#[tokio::test]
async fn test_write_from_stream_progress_callback() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    // 100 KB = 2 chunks at 64 KB chunk size
    let data = vec![0xAB; 100_000];
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let progress_calls = std::sync::atomic::AtomicUsize::new(0);
    let last_bytes = std::sync::atomic::AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let bytes = dest
        .write_from_stream(Path::new("/big.bin"), 100_000, stream, &|bytes_done, total| {
            progress_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            last_bytes.store(bytes_done, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(total, 100_000);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, 100_000);
    assert!(
        progress_calls.load(std::sync::atomic::Ordering::Relaxed) >= 2,
        "expected at least 2 progress calls for 100 KB at 64 KB chunks"
    );
    assert_eq!(last_bytes.load(std::sync::atomic::Ordering::Relaxed), 100_000);
}

#[tokio::test]
async fn test_write_from_stream_cancel_via_progress() {
    let source = InMemoryVolume::new("Source");
    let dest = InMemoryVolume::new("Dest");
    // 200 KB = 4 chunks, cancel after first
    let data = vec![0xCD; 200_000];
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let call_count = std::sync::atomic::AtomicUsize::new(0);
    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let result = dest
        .write_from_stream(Path::new("/big.bin"), 200_000, stream, &|_, _| {
            let n = call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, VolumeError::IoError { ref message, .. } if message.contains("cancelled")),
        "expected cancellation error, got: {:?}",
        err
    );
    // File should NOT exist at destination (write was cancelled before create_file)
    assert!(!dest.exists(Path::new("/big.bin")).await);
}

#[test]
fn lane_key_defaults_to_root_when_unset() {
    // No `with_lane_key` ⇒ fall back to the root lane (the trait default), so
    // the ~169 existing `new(...)` sites keep their behavior.
    let volume = InMemoryVolume::new("Test");
    assert_eq!(volume.lane_key().as_str(), volume.root().to_string_lossy());
}

#[test]
fn with_lane_key_overrides_the_lane() {
    let volume = InMemoryVolume::new("Test").with_lane_key("device-a");
    assert_eq!(volume.lane_key().as_str(), "device-a");
}

#[test]
fn same_lane_key_means_same_lane_distinct_means_different() {
    // Drives the manager's serialize-vs-parallel behavior in tests: two volumes
    // sharing a lane key serialize; distinct keys run in parallel.
    let a = InMemoryVolume::new("A").with_lane_key("shared");
    let b = InMemoryVolume::new("B").with_lane_key("shared");
    let c = InMemoryVolume::new("C").with_lane_key("other");
    assert_eq!(a.lane_key(), b.lane_key());
    assert_ne!(a.lane_key(), c.lane_key());
}

/// `set_stat_failing` models an UNANSWERED stat, not a missing path. The
/// difference is the whole reason the knob exists: a `NotFound` is an answer,
/// and code that collapses an unanswered stat into "not a directory" routes a
/// folder into a file-shaped destructive branch.
#[tokio::test]
async fn set_stat_failing_fails_the_stat_without_making_the_path_disappear() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();
    volume.set_stat_failing(Path::new("/album"));

    assert!(
        matches!(
            volume.is_directory(Path::new("/album")).await,
            Err(VolumeError::IoError { .. })
        ),
        "an unanswerable stat is an IoError, never a NotFound"
    );
    assert!(matches!(
        volume.get_metadata(Path::new("/album")).await,
        Err(VolumeError::IoError { .. })
    ));
    assert!(
        volume.exists(Path::new("/album")).await,
        "the path is still there; only the stat refuses to answer"
    );
    assert!(
        volume.is_directory(Path::new("/other")).await.is_err(),
        "a genuinely missing path still reports NotFound"
    );
    assert!(matches!(
        volume.is_directory(Path::new("/other")).await,
        Err(VolumeError::NotFound(_))
    ));
}
