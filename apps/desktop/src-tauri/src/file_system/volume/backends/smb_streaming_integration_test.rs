//! Streaming integration tests for the SMB backend (require Docker SMB containers).
//!
//! Covers the byte-path surface: `open_read_stream` and `write_from_stream`
//! across their shapes (progress, cancel, cross-volume copy, large/multi-chunk
//! files, cancel-by-drop, local-source large files) plus the streaming-write
//! error paths (mid-write cancel, source-error partial cleanup). Every test
//! here is `#[ignore]`d so default runs skip it. Start the containers with
//! `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::InMemoryVolume;

// ── SMB streaming integration tests (Docker) ───────────────────

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = b"streaming read test content";
    vol.create_file(Path::new(&format!("{}/read.txt", dir)), data)
        .await
        .unwrap();

    let mut stream = vol
        .open_read_stream(Path::new(&format!("{}/read.txt", dir)))
        .await
        .unwrap();
    assert_eq!(stream.total_size(), data.len() as u64);

    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);
    assert_eq!(stream.bytes_read(), data.len() as u64);

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Create a source via InMemoryVolume
    let source = InMemoryVolume::new("Source");
    let data: Vec<u8> = (0..=255).cycle().take(50_000).collect();
    source.create_file(Path::new("/payload.bin"), &data).await.unwrap();

    let stream = source.open_read_stream(Path::new("/payload.bin")).await.unwrap();
    let no_progress = &|_: u64, _: u64| std::ops::ControlFlow::Continue(());
    let bytes = vol
        .write_from_stream(Path::new(&format!("{}/payload.bin", dir)), 50_000, stream, no_progress)
        .await
        .unwrap();
    assert_eq!(bytes, 50_000);

    // Read back and verify content integrity
    let mut verify = vol
        .open_read_stream(Path::new(&format!("{}/payload.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data);

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_with_progress() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data = vec![0xCD; 200_000]; // ~200 KB
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let bytes = vol
        .write_from_stream(
            Path::new(&format!("{}/big.bin", dir)),
            200_000,
            stream,
            &|bytes_done, total| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                last_bytes.store(bytes_done, Ordering::Relaxed);
                assert_eq!(total, 200_000);
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();

    assert_eq!(bytes, 200_000);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 1,
        "expected at least 1 progress call"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), 200_000);

    // Byte-level integrity: a progress-reporting write that loses or
    // duplicates chunks would still satisfy the "progress_calls >= 1
    // and final bytes_done == 200_000" assertions; hash the destination
    // against the source to catch that.
    let mut verify = vol
        .open_read_stream(Path::new(&format!("{}/big.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::with_capacity(200_000);
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data, "destination bytes must match source bytes");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_cancel() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data = vec![0xEF; 500_000]; // ~500 KB, several chunks
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let call_count = std::sync::atomic::AtomicUsize::new(0);
    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let result = vol
        .write_from_stream(Path::new(&format!("{}/big.bin", dir)), 500_000, stream, &|_, _| {
            let n = call_count.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(result.is_err(), "expected cancellation error");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_cross_volume_streaming_copy() {
    // Full end-to-end: InMemoryVolume → SmbVolume via open_read_stream + write_from_stream.
    // Tests the same path that copy_single_path uses for non-local volumes.
    use std::sync::atomic::{AtomicUsize, Ordering};

    let smb_vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&smb_vol, &dir).await;
    smb_vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data: Vec<u8> = (0..=255).cycle().take(100_000).collect();
    source.create_file(Path::new("/photo.bin"), &data).await.unwrap();

    let progress_calls = AtomicUsize::new(0);

    // Read from InMemory, write to SMB (the same path copy_single_path takes)
    let stream = source.open_read_stream(Path::new("/photo.bin")).await.unwrap();
    let bytes = smb_vol
        .write_from_stream(Path::new(&format!("{}/photo.bin", dir)), 100_000, stream, &|_, _| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, 100_000);
    assert!(progress_calls.load(Ordering::Relaxed) >= 1);

    // Verify content via read back
    let mut verify = smb_vol
        .open_read_stream(Path::new(&format!("{}/photo.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data);

    ensure_clean(&smb_vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream_large_file_spans_many_chunks() {
    // Verifies the streaming reader delivers a multi-MB file correctly
    // across many chunk boundaries. Before the channel-backed rewrite, the
    // whole file was buffered in memory up front.
    //
    // The file has to exceed `max_read_size` (up to 8 MB on Samba) for
    // smb2 to split the read into more than one READ. 20 MB is a safe
    // multiple that stays under the single-chunk ceiling.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // 20 MB: guarantees multiple READs even at 8 MB max_read_size.
    let size = 20 * 1024 * 1024;
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let smb_path = format!("{}/big-stream.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    // Hash chunks as they arrive so a 20 MB mismatch produces a single
    // 32-byte hex pair instead of a 20 MB `Vec<u8>` diff. Also avoids
    // the 20 MB reassembly allocation.
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let mut hasher = blake3::Hasher::new();
    let mut chunks_seen = 0usize;
    let mut total_read = 0usize;
    while let Some(result) = stream.next_chunk().await {
        let chunk = result.unwrap();
        assert!(!chunk.is_empty(), "should not yield empty chunks");
        hasher.update(&chunk);
        total_read += chunk.len();
        chunks_seen += 1;
    }
    assert_eq!(total_read, size, "total bytes streamed must equal source size");
    let readback_hash = *hasher.finalize().as_bytes();
    let expected_hash = hash_bytes(&data);
    assert_eq!(
        readback_hash, expected_hash,
        "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, readback_hash
    );
    assert_eq!(stream.bytes_read(), size as u64);
    assert!(chunks_seen >= 2, "multi-MB file should span multiple chunks");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_read_stream_large_file_multi_chunk() {
    // SMB → local byte path now goes through `open_read_stream`, then the
    // caller writes into whatever destination. Verify that the streaming
    // reader yields multiple chunks for a multi-MB file.
    //
    // `max_read_size` negotiation can go up to 8 MB on modern Samba, so
    // the file has to be >8 MB to guarantee multiple READs.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 20 * 1024 * 1024; // 20 MB, exceeds 8 MB max_read_size
    let data: Vec<u8> = (0..size).map(|i| ((i * 7) % 251) as u8).collect();
    let smb_path = format!("{}/export-large.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    // Hash chunks as they arrive (see the sibling large-file test for
    // why we avoid `assert_eq!` on 20 MB `Vec<u8>`s).
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let mut chunks_seen = 0usize;
    let mut hasher = blake3::Hasher::new();
    let mut total_read = 0usize;
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        chunks_seen += 1;
        hasher.update(&chunk);
        total_read += chunk.len();
    }
    assert!(
        chunks_seen >= 2,
        "streaming should yield multiple chunks for a multi-MB file"
    );
    assert_eq!(total_read, size, "total bytes streamed must equal source size");
    let readback_hash = *hasher.finalize().as_bytes();
    let expected_hash = hash_bytes(&data);
    assert_eq!(
        readback_hash, expected_hash,
        "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, readback_hash
    );

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream_cancel_by_drop() {
    // Drop the stream mid-way and verify that subsequent SMB operations
    // on the same volume still work (producer task released the mutex).
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = vec![0xAA; 2 * 1024 * 1024]; // 2 MB
    let smb_path = format!("{}/cancel-me.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    // Read exactly one chunk then drop
    let _first = stream.next_chunk().await.unwrap().unwrap();
    drop(stream);

    // Subsequent op on the volume should succeed; the producer task
    // must have released the session mutex on cancel.
    let entries = vol.list_directory(Path::new(&dir), None).await.unwrap();
    assert!(entries.iter().any(|e| e.name == "cancel-me.bin"));

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_local_source_large_file() {
    // Local → SMB byte path now goes through LocalPosixVolume's
    // `open_read_stream` + SmbVolume's `write_from_stream`. Verify that
    // multi-MB input triggers multiple progress callbacks and round-trips.
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 4 * 1024 * 1024; // 4 MB, spans multiple import chunks
    let data: Vec<u8> = (0..size).map(|i| ((i * 13) % 251) as u8).collect();

    let local_tmp = std::env::temp_dir().join(format!("cmdr-smb-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&local_tmp);
    std::fs::create_dir_all(&local_tmp).unwrap();
    std::fs::write(local_tmp.join("import-large.bin"), &data).unwrap();

    let local_vol = crate::file_system::volume::LocalPosixVolume::new("local-src", local_tmp.clone());

    let smb_path = format!("{}/import-large.bin", dir);
    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = local_vol.open_read_stream(Path::new("import-large.bin")).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let bytes = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            last_bytes.store(done, Ordering::Relaxed);
            assert_eq!(total, size as u64);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, size as u64);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 2,
        "streaming write should call progress multiple times for a multi-chunk source"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

    // Byte-level integrity: hash the source and the destination and
    // compare. Streaming hash avoids materializing a 4 MB `Vec<u8>`
    // just to `assert_eq!` it, and on mismatch we get a legible hex
    // dump instead of a multi-megabyte diff.
    let expected_hash = hash_bytes(&data);
    let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
    assert_eq!(
        actual_hash, expected_hash,
        "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, actual_hash
    );

    let _ = std::fs::remove_dir_all(&local_tmp);
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_streams_large_file() {
    // InMemoryVolume → SmbVolume via write_from_stream with a multi-chunk
    // source. Verifies the SMB write path now pulls chunks on demand
    // rather than collecting the full source into a Vec<u8>.
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size: usize = 4 * 1024 * 1024; // 4 MB
    let data: Vec<u8> = (0..size).map(|i| ((i * 11) % 251) as u8).collect();

    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/big-stream.bin"), &data).await.unwrap();

    let smb_path = format!("{}/big-stream.bin", dir);
    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big-stream.bin")).await.unwrap();
    let bytes = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            last_bytes.store(done, Ordering::Relaxed);
            assert_eq!(total, size as u64);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, size as u64);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 2,
        "streaming write should call progress multiple times for a multi-chunk source"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

    // Byte-level integrity: streaming hash over the destination catches
    // any chunk drop/duplicate/reuse that "bytes_written == expected"
    // on its own can't see. See the sibling local-source test for the
    // rationale on hashing vs. `assert_eq!` on a 4 MB buffer.
    let expected_hash = hash_bytes(&data);
    let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
    assert_eq!(
        actual_hash, expected_hash,
        "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, actual_hash
    );

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_cancel_mid_write() {
    // Cancel partway through a multi-chunk write via progress-break.
    // Verifies Cancelled is returned and that the SMB session is still
    // usable for subsequent ops (writer.abort() drains in-flight WRITE
    // responses cleanly on cancel, best-effort-deletes the partial file).
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 4 * 1024 * 1024; // 4 MB, several write chunks
    let data = vec![0xC3u8; size];

    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/cancel-me.bin"), &data).await.unwrap();

    let smb_path = format!("{}/cancel-me.bin", dir);
    let call_count = std::sync::atomic::AtomicUsize::new(0);

    let stream = source.open_read_stream(Path::new("/cancel-me.bin")).await.unwrap();
    let result = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|_, _| {
            let n = call_count.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(
        matches!(result, Err(VolumeError::Cancelled(_))),
        "expected Cancelled, got {result:?}"
    );

    // The session must still work after cancel.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    ensure_clean(&vol, &dir).await;
}

/// A read stream that yields a fixed number of good chunks, then a source
/// read error on the next pull. Used to exercise the partial-file cleanup
/// on the write_from_stream ERROR path: once the SMB `FileWriter` is open
/// and a chunk has streamed into it, the source error must propagate AND
/// the half-written file must be deleted from the destination.
struct ErroringReadStream {
    good_chunks: usize,
    chunk: Vec<u8>,
    total_size: u64,
    bytes_read: u64,
    yielded: usize,
}

impl VolumeReadStream for ErroringReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.yielded < self.good_chunks {
                self.yielded += 1;
                self.bytes_read += self.chunk.len() as u64;
                Some(Ok(self.chunk.clone()))
            } else {
                Some(Err(VolumeError::IoError {
                    message: "Injected source read error".to_string(),
                    raw_os_error: None,
                }))
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_source_error_deletes_partial() {
    // Mid-stream source read error on the streaming path leaves a partial
    // file open on the server. The write_from_stream ERROR path must
    // delete that partial (mirroring the cancel branch) and propagate the
    // ORIGINAL error (NOT Cancelled). Without the fix, the destination
    // keeps a half-written file under the user's intended name.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // `size` larger than any plausible max_write_size forces the streaming
    // writer path (not the compound fast-path), so the writer is genuinely
    // open when the source errors on the second pull.
    let chunk = vec![0xA7u8; 256 * 1024]; // 256 KB good chunk
    let size = 64 * 1024 * 1024u64; // 64 MB promised, far above max_write
    let stream = ErroringReadStream {
        good_chunks: 1,
        chunk,
        total_size: size,
        bytes_read: 0,
        yielded: 0,
    };

    let smb_path = format!("{}/partial-on-error.bin", dir);
    let result = vol
        .write_from_stream(Path::new(&smb_path), size, Box::new(stream), &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await;

    // The original IoError must propagate, NOT Cancelled.
    assert!(
        matches!(result, Err(VolumeError::IoError { .. })),
        "expected the source IoError to propagate, got {result:?}"
    );

    // The partial must be gone: cleanup deleted it on a fresh session.
    assert!(
        !vol.exists(Path::new(&smb_path)).await,
        "partial file was left at the destination after a source-read error"
    );

    // The session must still be usable for subsequent ops.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    ensure_clean(&vol, &dir).await;
}
