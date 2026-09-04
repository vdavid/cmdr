//! The WRITE byte path against a real server (requires Docker SMB containers).
//!
//! Owns what `write_from_stream` does with a source: the bytes that land,
//! progress shape (including that it reports SERVER-CONFIRMED bytes rather
//! than ones handed to the pipeline), cancel and mid-write cancel, multi-chunk
//! sources, and the error path that must delete the partial (the
//! `ErroringReadStream` double is here for that alone). The cross-volume
//! streaming copy lives here too: it reads from an `InMemoryVolume`, but every
//! assertion it makes is about what arrived on the share. Whether a write
//! leaves as ONE compound frame is a wire-cost question, in
//! `wire_shape_integration_test.rs`; the read side is
//! `read_stream_integration_test.rs`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `volume`; shared helpers come from
//! `super::test_support`.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::InMemoryVolume;
use std::pin::Pin;

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
