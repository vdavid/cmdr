//! The READ byte path against a real server (requires Docker SMB containers).
//!
//! Owns what `open_read_stream` / `open_read_stream_with_hint` hand back: the
//! whole file, chunk boundaries and all, across a plain read, a multi-MB read
//! that spans several READs, a stream dropped mid-way, and the two size-drift
//! arms of the hinted fast path (a file that grew, a file that shrank), which
//! both have to serve the file as it is NOW. What that costs on the wire is a
//! separate contract, in `wire_shape_integration_test.rs`; the write side is
//! `write_stream_integration_test.rs`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `volume`; shared helpers come from
//! `super::test_support`.

use super::test_support::*;
use super::*;

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

// ── The hinted read: bytes that survive a stale size ───────────

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
