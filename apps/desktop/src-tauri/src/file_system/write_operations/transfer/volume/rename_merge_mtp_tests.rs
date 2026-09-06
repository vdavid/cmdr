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
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use crate::mtp::test_support::{self, ConnectedDevice, device_lock};
use cmdr_mtp::virtual_device::setup_virtual_mtp_device;

/// Seeds a virtual MTP device's backing dir, connects the app to it, and builds
/// a volume over its writable storage.
///
/// ❗ Seeding happens BEFORE the connect on purpose: the connect primes the root
/// listing, and a file written after that is invisible to the cached listing
/// until something invalidates it. The lock comes back so a cell holds it for its
/// whole span; it's process-wide because all virtual devices share one serial,
/// hence one Cmdr device id.
async fn connect_seeded_device(
    files: &[(&str, &[u8])],
) -> (
    ConnectedDevice,
    Arc<dyn Volume>,
    PathBuf,
    tokio::sync::MutexGuard<'static, ()>,
) {
    let lock = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let backing = fixture.root().join("internal");
    for (relative, content) in files {
        write_backing_file(&backing, relative, content);
    }
    // No rescan: the device isn't open yet, so the connect enumerates the backing
    // dir as it now stands. `rescan_virtual_device` is for seeding a device that's
    // ALREADY connected.
    let device = test_support::connect_fixture(fixture).await;
    let volume: Arc<dyn Volume> = Arc::new(test_support::volume_for(&device, None).await);
    (device, volume, backing, lock)
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
    // Source `/src/album`: one clashing child (Skip keeps it here) and one fresh
    // child that moves across. Dest `/dst/album`: the same-named child the source
    // clashes with.
    let (device, volume, backing, _lock) = connect_seeded_device(&[
        ("src/album/clash.txt", b"SRC-clash"),
        ("src/album/fresh.txt", b"SRC-fresh"),
        ("dst/album/clash.txt", b"DEST-clash"),
    ])
    .await;

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

    test_support::teardown(device).await;
}
