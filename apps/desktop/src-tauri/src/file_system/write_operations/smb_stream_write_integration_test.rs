//! A LOCAL source streaming onto a real SMB share.
//!
//! `cmdr-smb`'s own `write_stream_integration_test` covers what a write does with
//! a source in general; this cell is here because its source is a second app
//! backend, `LocalPosixVolume`, which is the pairing every local-to-share copy
//! actually runs.
//!
//! Same gating as every SMB integration cell: `#[ignore]`d by default, so start
//! the containers with `./apps/desktop/test/smb-servers/start.sh` and run
//! `cargo nextest run smb_integration --run-ignored all`.

use std::path::Path;
use std::sync::atomic::Ordering;

use cmdr_fs::testing::TestDir;

use super::smb_test_support::*;

/// A multi-MB local file streams onto the share through `LocalPosixVolume`'s
/// `open_read_stream` into `SmbVolume`'s `write_from_stream`: progress fires more
/// than once (so the chunked path really ran) and the destination hashes equal to
/// the source.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_local_source_large_file() {
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 4 * 1024 * 1024; // 4 MB, spans multiple import chunks
    let data: Vec<u8> = (0..size).map(|i| ((i * 13) % 251) as u8).collect();

    let local_tmp = TestDir::new("smb-import");
    std::fs::write(local_tmp.join("import-large.bin"), &data).unwrap();

    let local_vol = crate::file_system::volume::LocalPosixVolume::new("local-src", local_tmp.to_path_buf());

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

    ensure_clean(&vol, &dir).await;
}
