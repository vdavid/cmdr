//! The same-volume rename-merge, driven end to end against a real `MtpVolume`
//! over a virtual MTP device.
//!
//! The rename-merge's Skip guarantee is architectural, not backend-specific: a
//! source level still holding a child the user chose to keep survives, because
//! `Volume::delete` refuses a non-empty directory (`crates/cmdr-fs/src/volume/mod.rs`).
//! Every other backend's suite proves that with `LocalPosixVolume` or
//! `InMemoryVolume`, both of which honor the contract for free. MTP is the one
//! backend that talks to a device instead of a filesystem, so it's the one that
//! can quietly stop honoring it — which is exactly what this file pins.
//!
//! Behind the `virtual-mtp` feature so the whole production stack (top-level
//! hints, the driver's conflict detection, `rename_merge_directory`, and
//! `MtpVolume`'s own `rename` / `delete`) runs against a device-shaped backend.

#![cfg(feature = "virtual-mtp")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::move_same::move_within_same_volume_with_progress;
use crate::file_system::volume::{MtpVolume, Volume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use crate::mtp::connection::DeviceWatch;
use crate::mtp::connection::events::no_device_events;
use crate::mtp::connection::{MtpDisconnectReason, connection_manager};
use crate::mtp::virtual_device::{
    rescan_virtual_device, setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
};

/// Keeps a test's virtual device alive and exclusive: the process-wide lock (all
/// virtual devices share one serial, hence one Cmdr device id) plus the fixture
/// owning its temp backing dir. Dropping it unregisters the device, so a
/// finished test's registration can't answer for the next one — including when
/// an assertion unwinds past the explicit teardown.
struct VirtualDeviceGuard {
    _lock: tokio::sync::MutexGuard<'static, ()>,
    fixture: crate::mtp::virtual_device::VirtualDeviceFixture,
}

impl Drop for VirtualDeviceGuard {
    fn drop(&mut self) {
        unregister_virtual_mtp_device(self.fixture.location_id);
    }
}

/// Connects the virtual MTP device and builds an `MtpVolume` over its first
/// (writable) storage. Returns the device id, the volume, the backing dir of
/// that storage, and the guard that keeps both alive.
async fn connect_virtual_device() -> (String, Arc<dyn Volume>, PathBuf, VirtualDeviceGuard) {
    let guard = VirtualDeviceGuard {
        _lock: virtual_device_test_lock().lock().await,
        fixture: setup_virtual_mtp_device(),
    };
    let location_id = guard.fixture.location_id;
    let backing = guard.fixture.root().join("internal");
    // The virtual device reports a serial, so its Cmdr id is serial-based; take
    // it from discovery rather than rebuilding it from the location id.
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, &no_device_events(), DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect");
    let storage_id = info.storages.first().expect("at least one virtual storage").id;
    let volume: Arc<dyn Volume> = Arc::new(MtpVolume::new(&device_id, storage_id, "Test"));
    (device_id, volume, backing, guard)
}

/// `resolve_path_to_handle` is cache-only, so a path is unreachable until an
/// ancestor listing has put its handle in the cache. Walk each level top-down.
async fn prime_path_handles(volume: &Arc<dyn Volume>, dirs: &[&str]) {
    for dir in dirs {
        volume
            .list_directory(Path::new(dir), None)
            .await
            .unwrap_or_else(|e| panic!("priming listing for {dir}: {e:?}"));
    }
}

fn write_backing_file(backing: &Path, rel: &str, content: &[u8]) {
    let abs = backing.join(rel);
    std::fs::create_dir_all(abs.parent().expect("a seeded file always has a parent"))
        .expect("creating the backing dir for a seeded file");
    std::fs::write(abs, content).expect("seeding a backing file");
}

/// A child the user chose to Skip keeps its only copy on MTP too.
///
/// Move `/src/album` onto the existing `/dst/album`, answer Skip on the one
/// clashing child. The merge leaves the skipped child in the source, then the
/// inside-out cleanup asks `Volume::delete` to remove `/src/album`. A backend
/// that recurses there destroys the exact file the user chose to keep — with no
/// probe error, no race, and no failure reported to the user.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_merge_move_keeps_a_skipped_child_on_mtp() {
    let (device_id, volume, backing, _guard) = connect_virtual_device().await;

    // Source `/src/album`: one clashing child (Skip keeps it here) and one
    // fresh child that moves across.
    write_backing_file(&backing, "src/album/clash.txt", b"SRC-clash");
    write_backing_file(&backing, "src/album/fresh.txt", b"SRC-fresh");
    // Dest `/dst/album`: the same-named child the source clashes with.
    write_backing_file(&backing, "dst/album/clash.txt", b"DEST-clash");
    rescan_virtual_device().expect("the virtual device must rescan its backing dir");

    prime_path_handles(&volume, &["/", "/src", "/src/album", "/dst", "/dst/album"]).await;

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    // Skip resolves every clash without a dialog: `resolve_child` returns
    // `Ok(None)` straight from the policy.
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-mtp-merge-skip",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/album")],
        Path::new("/dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // The destination keeps its own copy (that's what Skip means there)…
    let dest_album = volume
        .list_directory(Path::new("/dst/album"), None)
        .await
        .expect("listing the merged destination");
    assert!(
        dest_album.iter().any(|e| e.name == "clash.txt"),
        "the destination's clashing file must survive a Skip"
    );
    assert!(
        dest_album.iter().any(|e| e.name == "fresh.txt"),
        "the non-clashing child must have moved across"
    );

    // …and the source keeps its only copy of the skipped child. This is the
    // guarantee: the user said "keep both", so the file the app didn't move
    // must still be where it was. Check the device's storage first — it's the
    // ground truth, and a backend that recursed took the file itself, not just
    // its cache entry.
    assert!(
        backing.join("src/album/clash.txt").exists(),
        "the skipped child's ONLY copy must still be on the device's storage"
    );
    let source_album = volume
        .list_directory(Path::new("/src/album"), None)
        .await
        .expect("the source dir holding a skipped child must survive");
    assert!(
        source_album.iter().any(|e| e.name == "clash.txt"),
        "the skipped child must still be listed in the source, found {:?}",
        source_album.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    connection_manager()
        .disconnect(&device_id, &no_device_events(), MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect");
}
