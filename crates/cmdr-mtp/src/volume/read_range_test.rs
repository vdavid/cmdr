//! The bounded read path over a virtual MTP device.
//!
//! Two contracts, one file because they share the fixture. `read_range` is the
//! DIRECT path: a ranged read costs ONE device operation, not three, so these
//! cells pin the part Cmdr itself issues — the `GetStorageInfo` round trip
//! (`MtpDevice::storage()`) the per-device storage cache must collapse to one per
//! device and must re-issue after an invalidation. `open_read_stream` is the
//! WINDOWED path both the copy engine and native drag-out read through, and the
//! last cell walks a file across several windows to prove the reassembly and the
//! cancel-keeps-the-partial promise the copy engine leans on.

use std::path::Path;

use cmdr_fs::volume::Volume;

use crate::testing::{
    ConnectedDevice, connect_fixture, device_lock, test_connection_manager as connection_manager, volume_for,
};

/// Deterministic bytes: byte `i` is `(i * 31 + 7) % 251`, so any window is
/// checkable against its offset without holding the whole file.
fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

/// Connects a virtual MTP device seeded with `bytes` at `internal/blob.bin`, with
/// the root path cache primed (`read_range` resolves handles cache-only).
async fn connect_device_with_blob(bytes: &[u8]) -> ConnectedDevice {
    connect_device_with_blob_named(bytes, "blob.bin").await
}

/// [`connect_device_with_blob`] under a caller-chosen file name. Seeding happens
/// before the connect, so the device's rescan is what hands the file a handle.
async fn connect_device_with_blob_named(bytes: &[u8], name: &str) -> ConnectedDevice {
    let fixture = crate::virtual_device::setup_virtual_mtp_device();
    std::fs::write(fixture.root().join("internal").join(name), bytes).expect("seed blob on device");
    crate::virtual_device::rescan_virtual_device();
    connect_fixture(connection_manager(), fixture).await
}

/// Every ranged read after the first must reuse the cached `Storage`: the whole
/// point of the direct path is that reading N windows costs N device operations,
/// not 3N. Pre-fix (every read through `open_read_session`) this counted one
/// `GetStorageInfo` per read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_range_resolves_storage_info_once_per_device() {
    let _guard = device_lock().await;
    let bytes = payload(64 * 1024);
    let device = connect_device_with_blob(&bytes).await;
    let volume = volume_for(connection_manager(), &device, None).await;

    for i in 0..5u64 {
        let offset = i * 4096;
        let window = volume
            .read_range(Path::new("/blob.bin"), offset, 4096)
            .await
            .expect("ranged read should succeed");
        assert_eq!(
            window,
            &bytes[offset as usize..offset as usize + 4096],
            "window at offset {offset} should match the seeded payload"
        );
    }

    assert_eq!(
        connection_manager().storage_lookup_count(&device.id).await,
        1,
        "five ranged reads should share one GetStorageInfo round trip"
    );

    device.teardown(connection_manager()).await;
}

/// `StorageInfoChanged` must drop the cached handle: the device is telling us its
/// storage picture moved, and a cached `Storage` carries a snapshot of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_info_invalidation_forces_a_fresh_storage_lookup() {
    let _guard = device_lock().await;
    let bytes = payload(16 * 1024);
    let device = connect_device_with_blob(&bytes).await;
    let volume = volume_for(connection_manager(), &device, None).await;

    volume
        .read_range(Path::new("/blob.bin"), 0, 1024)
        .await
        .expect("first read");
    assert_eq!(connection_manager().storage_lookup_count(&device.id).await, 1);

    connection_manager()
        .invalidate_storage_cache(&device.id, Some(device.storage_id))
        .await;

    volume
        .read_range(Path::new("/blob.bin"), 1024, 1024)
        .await
        .expect("read after invalidation");
    assert_eq!(
        connection_manager().storage_lookup_count(&device.id).await,
        2,
        "an invalidated cache must re-resolve the storage, not serve a stale handle"
    );

    device.teardown(connection_manager()).await;
}

/// The short-read / EOF tolerance survives the direct path: a window that runs
/// past the end returns the available tail, and a read starting at or past EOF
/// returns empty rather than erroring or hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_range_clamps_at_end_of_file() {
    let _guard = device_lock().await;
    let bytes = payload(5000);
    let device = connect_device_with_blob(&bytes).await;
    let volume = volume_for(connection_manager(), &device, None).await;

    let tail = volume
        .read_range(Path::new("/blob.bin"), 4000, 4096)
        .await
        .expect("a read overrunning EOF should succeed");
    assert_eq!(tail, &bytes[4000..], "the overrunning read returns the tail only");

    let past_eof = volume
        .read_range(Path::new("/blob.bin"), 5000, 1024)
        .await
        .expect("a read starting at EOF should succeed");
    assert!(
        past_eof.is_empty(),
        "a read at EOF returns no bytes, got {}",
        past_eof.len()
    );

    device.teardown(connection_manager()).await;
}

/// End-to-end over the real wire: a multi-window file read through
/// `MtpVolume::open_read_stream` (the SHARED read path used by both the copy
/// and native drag-out) reassembles to the exact source bytes. Drives
/// repeated `GetPartialObject64` at advancing offsets via the virtual
/// transport, with the window shrunk so a small fixture spans several windows.
/// This is the bounded-window analogue of a copy's source read; the cells above
/// pin the same offset accounting on the direct `read_range` path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_window_read_assembles_byte_exact() {
    let _guard = device_lock().await;
    // A fixture larger than one (shrunk) window, written into the internal
    // backing dir before the connect so a rescan hands it a handle.
    let payload = payload(3500);
    let device = connect_device_with_blob_named(&payload, "bigfile.bin").await;

    // 1000-byte windows over 3500 bytes ⇒ 4 windows (1000, 1000, 1000, 500).
    super::testing::set_read_window(1000);

    let vol = volume_for(connection_manager(), &device, None).await;
    let mut stream = vol
        .open_read_stream(Path::new("/bigfile.bin"))
        .await
        .expect("open_read_stream should succeed");

    assert_eq!(stream.total_size(), payload.len() as u64);

    let mut assembled = Vec::new();
    let mut windows = 0;
    while let Some(item) = stream.next_chunk().await {
        let chunk = item.expect("each window read should be Ok");
        assert!(!chunk.is_empty(), "a window before EOF must not be empty");
        windows += 1;
        assembled.extend_from_slice(&chunk);
    }

    assert_eq!(
        assembled, payload,
        "bounded windows reassemble to the exact source bytes"
    );
    assert_eq!(stream.bytes_read(), payload.len() as u64);
    assert!(
        windows >= 2,
        "the fixture must span multiple bounded windows (got {windows}); else this isn't testing windowing"
    );

    // Cancel-keeps-partials, on the same fixture (one test owns the
    // read-window override, so there's no cross-test race on it). Open a
    // fresh stream, read ONE window, then `cancel_and_release`: it holds
    // nothing between windows, so it returns without a device drain, and the
    // bytes already delivered (the kept partial) survive in `bytes_read`.
    // Dropping afterward must not panic. This is Cmdr's stream contract — the
    // window bookkeeping is mtp-rs's, but "a cancel mid-read keeps the
    // partial" is what the copy engine relies on.
    let mut partial = vol
        .open_read_stream(Path::new("/bigfile.bin"))
        .await
        .expect("open_read_stream should succeed");
    let first = partial.next_chunk().await.expect("a window").expect("ok");
    assert_eq!(first.len(), 1000, "the first window is one full (shrunk) window");
    assert_eq!(partial.bytes_read(), 1000, "offset advanced by the window length");
    partial.cancel_and_release().await;
    assert_eq!(partial.bytes_read(), 1000, "the kept partial offset survives a cancel");
    drop(partial); // no panic, nothing held

    super::testing::set_read_window(0);
    device.teardown(connection_manager()).await;
}
