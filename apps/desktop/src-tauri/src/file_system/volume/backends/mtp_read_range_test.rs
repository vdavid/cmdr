//! `MtpVolume::read_range` over a virtual MTP device: the bounded-read contract.
//!
//! The point of the direct path is that a ranged read costs ONE device
//! operation, not three. The virtual device exposes no PTP-operation counter, so
//! these tests pin the part Cmdr itself issues: the `GetStorageInfo` round trip
//! (`MtpDevice::storage()`), which the per-device storage cache must collapse to
//! one per device, and must re-issue after an invalidation.

use super::*;
use crate::mtp::connection::connection_manager;
use std::path::Path;

/// Deterministic bytes: byte `i` is `(i * 31 + 7) % 251`, so any window is
/// checkable against its offset without holding the whole file.
fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

/// A connected virtual device, torn down (disconnect + unregister) on drop so the
/// next test doesn't inherit its registration under the shared device id.
struct Device {
    id: String,
    storage_id: u32,
    location_id: u64,
    _root: tempfile::TempDir,
}

/// Connects a virtual MTP device seeded with `bytes` at `internal/blob.bin`, with
/// the root path cache primed (`read_range` resolves handles cache-only).
async fn connect_device_with_blob(bytes: &[u8]) -> Device {
    use crate::mtp::virtual_device::{rescan_virtual_device, setup_virtual_mtp_device_at};

    let root = tempfile::tempdir().expect("tmp device root");
    let location_id = setup_virtual_mtp_device_at(root.path());
    std::fs::write(root.path().join("internal/blob.bin"), bytes).expect("seed blob on device");
    rescan_virtual_device();

    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("a storage").id;
    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");
    Device {
        id: device_id,
        storage_id,
        location_id,
        _root: root,
    }
}

async fn teardown(device: Device) {
    connection_manager()
        .disconnect(&device.id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .ok();
    crate::mtp::virtual_device::unregister_virtual_mtp_device(device.location_id);
}

/// Every ranged read after the first must reuse the cached `Storage`: the whole
/// point of the direct path is that reading N windows costs N device operations,
/// not 3N. Pre-fix (every read through `open_read_session`) this counted one
/// `GetStorageInfo` per read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_range_resolves_storage_info_once_per_device() {
    let _guard = crate::mtp::virtual_device::virtual_device_test_lock().lock().await;
    let bytes = payload(64 * 1024);
    let device = connect_device_with_blob(&bytes).await;
    let volume = MtpVolume::new(&device.id, device.storage_id, "Internal");

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

    teardown(device).await;
}

/// `StorageInfoChanged` must drop the cached handle: the device is telling us its
/// storage picture moved, and a cached `Storage` carries a snapshot of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_info_invalidation_forces_a_fresh_storage_lookup() {
    let _guard = crate::mtp::virtual_device::virtual_device_test_lock().lock().await;
    let bytes = payload(16 * 1024);
    let device = connect_device_with_blob(&bytes).await;
    let volume = MtpVolume::new(&device.id, device.storage_id, "Internal");

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

    teardown(device).await;
}

/// The short-read / EOF tolerance survives the direct path: a window that runs
/// past the end returns the available tail, and a read starting at or past EOF
/// returns empty rather than erroring or hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_range_clamps_at_end_of_file() {
    let _guard = crate::mtp::virtual_device::virtual_device_test_lock().lock().await;
    let bytes = payload(5000);
    let device = connect_device_with_blob(&bytes).await;
    let volume = MtpVolume::new(&device.id, device.storage_id, "Internal");

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

    teardown(device).await;
}
