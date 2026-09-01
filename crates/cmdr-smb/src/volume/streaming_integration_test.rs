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

use super::streams::InlineReadStream;
use super::test_support::*;
use super::*;
use cmdr_fs::volume::InMemoryVolume;
use std::pin::Pin;

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

    let call_count = AtomicUsize::new(0);
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
    let call_count = AtomicUsize::new(0);

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

// ── The single-shot write promise (staging exemption) ──────────

/// `(requests_sent, compound_requests_sent)` on the volume's main connection.
async fn request_counts(vol: &SmbVolume) -> (u64, u64) {
    let d = vol.diagnostics().await.expect("a connected volume has diagnostics");
    (
        d.primary.metrics.requests_sent,
        d.primary.metrics.compound_requests_sent,
    )
}

/// The transfer layer skips its `.cmdr-tmp-*` staging for a write this backend
/// promises to land in ONE shot, so the promise has to hold against a real
/// server: a write that fits one WRITE must leave as a single compound frame
/// (CREATE+WRITE+FLUSH+CLOSE), which is what makes it all-or-nothing.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_single_shot_write_leaves_as_one_compound_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = vec![0xABu8; 4096];
    let size = data.len() as u64;
    assert!(
        vol.write_is_single_shot(size).await,
        "4 KiB fits one WRITE on every SMB2 dialect"
    );

    let smb_path = format!("{}/one-shot.bin", dir);
    let (requests_before, compounds_before) = request_counts(&vol).await;
    let written = vol
        .write_from_stream(
            Path::new(&smb_path),
            size,
            Box::new(InlineReadStream::new(data.clone())),
            &|_, _| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(written, size);
    // TWO compound frames leave the wire, four ops each (verified against Samba
    // in the `smb-consumer` container, 2026-08-01): the write's
    // CREATE+WRITE+FLUSH+CLOSE, then the CREATE+QUERY_INFO+CLOSE stat every SMB
    // write ends with to patch the listing cache. What matters is that NOTHING
    // outside a compound frame went out — a streaming write would show its
    // separate CREATE, WRITE, and CLOSE round trips here.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (2, 8),
        "the write must leave as ONE compound frame (plus the post-write stat), with no loose round trips"
    );

    // The bytes are at the FINAL name the moment the write returns — no temp,
    // nothing to land.
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    let mut read_back = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        read_back.extend_from_slice(&chunk);
    }
    assert_eq!(read_back, data);
    let names: Vec<String> = vol
        .list_directory(Path::new(&dir), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["one-shot.bin".to_string()], "no leftovers; got {names:?}");

    ensure_clean(&vol, &dir).await;
}

/// The other direction against a real server: a file too big for one WRITE gets
/// NO promise, so the transfer layer keeps staging it. ❌ The answer must come
/// from the negotiated `max_write_size`, never from a size the caller picked.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_write_over_the_negotiated_limit_is_not_single_shot() {
    let vol = make_docker_volume().await;
    let max_write = vol
        .negotiated_max_write()
        .await
        .expect("a connected volume has negotiated params");

    assert!(vol.write_is_single_shot(max_write).await, "the limit itself fits");
    assert!(
        !vol.write_is_single_shot(max_write + 1).await,
        "one byte over needs a second WRITE, so the write is no longer all-or-nothing"
    );
    assert!(
        !vol.write_is_single_shot(0).await,
        "an empty file has no WRITE to compound with; it takes the streaming writer"
    );
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_progress_reports_confirmed_bytes_not_queued_ones() {
    // A streaming write pipelines up to `MAX_PIPELINE_WINDOW` WRITEs before it
    // waits for any of them, so bytes handed to the pipeline and bytes the
    // server has acknowledged are two different numbers. Progress must report
    // the second one.
    //
    // The tell is that acknowledged bytes STALL while the window fills: several
    // chunks go out before any response comes back, so consecutive callbacks
    // repeat a value. Counting queued bytes instead makes every callback strictly
    // larger than the last, which is what made a 6.7 MB/s NAS copy show its bar
    // racing ahead and then sitting frozen for 40 s at a time while the queue
    // drained (ERR-9WZRR).
    //
    // ❌ Don't assert the first callback reports 0. Credit pressure can force a
    // drain inside the very first `write_chunk`, so that pins the fixture's credit
    // behavior rather than the accounting under test.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // MUST exceed the negotiated `max_write`, or the write is single-shot and
    // takes the compound fast-path, which buffers the source in memory and never
    // reaches the streaming writer this test is about. Sizing this as a literal
    // is how it silently tested nothing: the fixture Samba negotiates a `max_write`
    // far above any round number you'd reach for.
    let max_write = vol
        .negotiated_max_write()
        .await
        .expect("a connected volume has negotiated params");
    let size = max_write + 64 * 1024;
    assert!(
        !vol.write_is_single_shot(size).await,
        "this test only means something on the streaming writer"
    );

    let source = InMemoryVolume::new("Source");
    let data = vec![0x5A; size as usize];
    source.create_file(Path::new("/pipelined.bin"), &data).await.unwrap();

    let reported = std::sync::Mutex::new(Vec::<u64>::new());

    let stream = source.open_read_stream(Path::new("/pipelined.bin")).await.unwrap();
    let bytes = vol
        .write_from_stream(
            Path::new(&format!("{}/pipelined.bin", dir)),
            size,
            stream,
            &|bytes_done, total| {
                assert_eq!(total, size);
                reported.lock().unwrap().push(bytes_done);
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();

    let reported = reported.into_inner().unwrap();
    assert!(!reported.is_empty(), "expected progress callbacks");

    assert!(
        reported.windows(2).any(|w| w[0] == w[1]),
        "acknowledged bytes must stall while the pipeline window fills, so some consecutive \
         callbacks must repeat a value; a strictly increasing sequence means progress is \
         counting bytes handed to the pipeline, not bytes the server confirmed: {reported:?}"
    );

    assert!(
        reported.windows(2).all(|w| w[0] <= w[1]),
        "progress must never go backwards: {reported:?}"
    );
    assert_eq!(
        *reported.last().unwrap(),
        size,
        "the write is committed, so the last callback must account for every byte"
    );
    assert_eq!(bytes, size);

    ensure_clean(&vol, &dir).await;
}

// ── The hinted read: one sized frame, and no truncation ─────────

/// Drains a read stream to the end and hands back everything it produced.
async fn drain(mut stream: Box<dyn VolumeReadStream>) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect("no read error"));
    }
    out
}

/// A hinted read of a small file has to leave as ONE compound frame
/// (CREATE+READ+CLOSE): that single round trip is the whole reason the fast path
/// exists, and it's what a 100k-file copy multiplies.
///
/// The READ inside it is sized to the hint, which is invisible on the frame
/// count and very visible on the connection's credit budget: an unsized READ
/// books `max_read` (8 MB, 128 credits) whatever the file weighs, so ten
/// concurrent 4 MB reads ask for 1,300 credits against a ~512-credit window and
/// most of them park instead of copying.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_hinted_read_leaves_as_one_compound_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data: Vec<u8> = (0..=255u8).cycle().take(128 * 1024).collect();
    let path = format!("{}/hinted.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    let (requests_before, compounds_before) = request_counts(&vol).await;
    let stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(data.len() as u64))
        .await
        .unwrap();
    let got = drain(stream).await;
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(got, data, "the fast path must serve the file byte for byte");
    assert_eq!(
        compounds_after - compounds_before,
        1,
        "a hinted small read must be one compound frame, not a 3-RTT streaming open"
    );
    assert_eq!(
        requests_after - requests_before,
        1,
        "and that compound is the ONLY request the read sends"
    );

    ensure_clean(&vol, &dir).await;
}

/// A file that GREW between the scan and the copy must never come back
/// truncated: the sized READ asks for the stale size, so smb2 refuses with
/// `TooLarge` rather than handing back a prefix, and the streaming fallback
/// serves the file as it is now.
///
/// This is the case sizing the read makes stricter: the refusal now trips at the
/// hint instead of at `max_read`, so drift that used to slip through (a file that
/// grew but still fits one 8 MB READ) is caught here.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_file_that_grew_since_the_scan_is_never_read_truncated() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data: Vec<u8> = (0..=255u8).cycle().take(256 * 1024).collect();
    let path = format!("{}/grown.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    // The hint is what the scan saw; the file on the server is four times that.
    let stale_hint = (data.len() / 4) as u64;
    let stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(stale_hint))
        .await
        .unwrap();
    let got = drain(stream).await;

    assert_eq!(
        got.len(),
        data.len(),
        "a stale hint must never shorten the copy: the reader has to serve the file as it is now"
    );
    assert_eq!(got, data);

    ensure_clean(&vol, &dir).await;
}

/// The other drift direction: a file that SHRANK comes back short of the hint,
/// which the fast path treats as "the scan is stale" and re-reads by streaming,
/// so the caller gets today's bytes rather than a padded or partial buffer.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_file_that_shrank_since_the_scan_is_read_in_full() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data: Vec<u8> = (0..=255u8).cycle().take(64 * 1024).collect();
    let path = format!("{}/shrunk.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    let stale_hint = (data.len() * 4) as u64;
    let stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(stale_hint))
        .await
        .unwrap();
    let got = drain(stream).await;

    assert_eq!(
        got, data,
        "the reader must serve the file as it is now, whole and no longer"
    );

    ensure_clean(&vol, &dir).await;
}

/// Against a real server, the copy-slot count stays inside its two bounds: never
/// above what the user asked for (the detached host's default), and never `0` —
/// a copy engine handed zero slots does nothing at all.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_concurrency_stays_within_the_credit_window() {
    let vol = make_docker_volume().await;
    let requested = vol.inner.host().settings().max_concurrent_operations(BACKEND);
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Any op clones the session, which is where the window's capacity is measured.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    let slots = vol.max_concurrent_ops();
    assert!(
        (1..=requested).contains(&slots),
        "copy slots must stay in 1..={requested} once the credit window is measured, got {slots}"
    );

    ensure_clean(&vol, &dir).await;
}
