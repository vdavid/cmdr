//! The shared `Volume` conformance promises, asserted against a virtual MTP
//! device.
//!
//! These live apart from `mtp_test.rs` because they assert something different
//! in kind: not how this backend behaves, but that it keeps the contracts
//! `cmdr_fs::volume::conformance` holds EVERY backend to. `mtp_delete_test.rs`
//! is the third sibling — the non-recursion contract needs enough MTP-specific
//! scaffolding (`MtpDeleteScope`) to earn its own file. The SMB twin is
//! `cmdr-smb`'s `volume::conformance_test`.
//!
//! Every test here drives a virtual MTP device, so the whole file carries that
//! feature gate (declared in `backends/mod.rs`).

use super::*;
use std::path::Path;

use crate::mtp::connection::connection_manager;
use crate::mtp::virtual_device::{
    VirtualDeviceFixture, setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
};

/// Connects the virtual device and builds an `MtpVolume` over its writable
/// storage, with the root listing primed (`resolve_path_to_handle` is
/// cache-only).
async fn connect_primed_volume(fixture: &VirtualDeviceFixture) -> (String, MtpVolume) {
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;
    let volume = MtpVolume::new(&device_id, storage_id, "Test");
    volume
        .list_directory(Path::new("/"), None)
        .await
        .expect("priming the root listing");
    (device_id, volume)
}

/// Disconnects AND unregisters. Both halves are load-bearing: a device left
/// registered survives into the next test in this binary, which then connects to
/// a stale storage handle over a `TempDir` that's already gone and fails on its
/// first write with a bare protocol error.
async fn teardown(device_id: &str, fixture: &VirtualDeviceFixture) {
    connection_manager()
        .disconnect(device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(fixture.location_id);
}

/// The shared `Volume::create_directory_all` honesty assertion, over a real
/// `MtpVolume` — the backend the honesty question was written for.
///
/// MTP is the one that answers `create_directory_errors_on_existing_dir() ==
/// false`, so the trait's default walk can't learn "it was already there" from a
/// collision error; it has to learn it from the `exists` probe it runs first. If
/// that probe were ever dropped as redundant, `create_folder` would make a
/// SECOND `Documents` beside the first and the walk would report `Created` — and
/// the transfer driver would then skip every destination conflict probe inside a
/// folder full of the user's files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_directory_all_honors_the_shared_honesty_contract() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, volume) = connect_primed_volume(&fixture).await;

    cmdr_fs::volume::conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        &volume,
        Path::new("/Documents"),
    )
    .await;

    teardown(&device_id, &fixture).await;
}

/// The shared writability-declaration assertion: `is_writable()` and what the
/// device actually accepts say the same thing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn is_writable_honors_the_shared_declaration_contract() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, volume) = connect_primed_volume(&fixture).await;

    cmdr_fs::volume::conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new("/scratch"))
        .await;

    teardown(&device_id, &fixture).await;
}

/// The shared export-handshake assertion, over a real `MtpVolume`: bytes come
/// back through bounded `GetPartialObject64` windows, and `supports_export()`
/// says so.
///
/// ❗ Seeded through the device's BACKING DIR plus a rescan, not `create_file`:
/// MTP answers that `NotSupported` (an upload here is `write_from_stream`, one
/// `SendObject` transaction), which is why this backend's suite has no
/// `create_file` conformance cell either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_honors_the_shared_handshake_contract() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let content = b"the bytes a copy would move";
    std::fs::write(fixture.root().join("internal/exported.txt"), content).expect("seed the file on the device");
    crate::mtp::virtual_device::rescan_virtual_device();
    let (device_id, volume) = connect_primed_volume(&fixture).await;

    cmdr_fs::volume::conformance::assert_export_matches_the_bytes_offered(&volume, Path::new("/exported.txt"), content)
        .await;

    teardown(&device_id, &fixture).await;
}

/// The shared `NotFound`-payload assertion, over a real `MtpVolume`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn not_found_honors_the_shared_path_payload_contract() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, volume) = connect_primed_volume(&fixture).await;

    cmdr_fs::volume::conformance::assert_not_found_carries_the_path(&volume, Path::new("/no-such-file.txt")).await;

    teardown(&device_id, &fixture).await;
}
