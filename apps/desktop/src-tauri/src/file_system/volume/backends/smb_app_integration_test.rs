//! The SMB integration cells whose other half is this app, not the protocol.
//!
//! Everything asserting on the backend itself lives with it, in `cmdr-smb`'s
//! `volume::integration_test` and its three byte-path suites, split by contract:
//! `read_stream_integration_test` (what a read hands back),
//! `write_stream_integration_test` (what a write does with a source), and
//! `wire_shape_integration_test` (what an op costs on the wire). What's left here
//! is the two cells that need something only the app has: the pane-close IPC, and
//! a second backend to stream from.
//!
//! Same gating as every SMB integration cell: `#[ignore]`d by default, so start
//! the containers with `./apps/desktop/test/smb-servers/start.sh` and run
//! `cargo nextest run smb_integration --run-ignored all`.

use std::path::Path;
use std::sync::atomic::Ordering;

use cmdr_fs::testing::TestDir;

use super::smb_test_support::*;
use super::*;

/// Regression: closing a pane's listing must NOT tear down the SMB watcher.
///
/// The watcher's lifetime is the VOLUME's (spawned at `connect_smb_volume`,
/// canceled only by `on_unmount` / reconnect), not a pane's. The index relies on
/// this: it must keep receiving change events while the volume's index is live,
/// even with no pane showing the share. `list_directory_end` (the pane-close IPC)
/// only drops a listing-cache entry and its FSEvents `WatchedDirectory` (SMB has
/// none), so it can't reach the watcher. This test pins that: after a pane close,
/// the volume is still watched.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_pane_close_does_not_kill_index_watcher() {
    use crate::file_system::listing::operations::list_directory_end;

    let vol = make_docker_volume().await;
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    assert_eq!(
        vol.listing_watch_coverage(Path::new("/")),
        WatchCoverage::EveryWriter,
        "watcher must be alive right after connect",
    );

    // Simulate a pane closing its listing. Even for listing ids that were never
    // registered, this exercises the close path; the point is that NOTHING in it
    // cancels the volume-scoped SMB watcher.
    list_directory_end("some-pane-listing-id");
    list_directory_end("another-pane-listing-id");

    assert_eq!(
        vol.listing_watch_coverage(Path::new("/")),
        WatchCoverage::EveryWriter,
        "pane close must NOT tear down the volume's index watcher",
    );
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
