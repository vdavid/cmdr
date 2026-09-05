//! The shared `Volume` conformance promises, asserted against a virtual MTP
//! device.
//!
//! These live apart from `volume_impl_test.rs` because they assert something
//! different in kind: not how this backend behaves, but that it keeps the
//! contracts `cmdr_fs::volume::conformance` holds EVERY backend to.
//! `delete_test.rs` is the third sibling — the non-recursion contract needs
//! enough MTP-specific scaffolding (`MtpDeleteScope`) to earn its own file. The
//! SMB twin is `cmdr-smb`'s `volume::conformance_test`.

use std::path::Path;

use crate::testing::{ConnectedDevice, connect_virtual_device, device_lock, test_connection_manager, volume_for};
use crate::volume::MtpVolume;

/// A connected device and a volume over its writable storage, with `primed`
/// listed as well as the root.
async fn connect_primed_volume(primed: Option<&str>) -> (ConnectedDevice, MtpVolume) {
    let device = connect_virtual_device(test_connection_manager()).await;
    let volume = volume_for(test_connection_manager(), &device, primed).await;
    (device, volume)
}

/// The shared `Volume::rename` no-clobber assertion, over a real `MtpVolume`.
///
/// MTP is the backend with no protocol-level exclusivity to lean on: PTP has no
/// rename-if-absent, so the refusal is a hand-written `exists` probe in front of
/// `rename_object` and nothing but this assertion would notice it going away. It
/// wouldn't even leave a visible collision behind: the protocol happily hosts two
/// siblings with the same name, and the virtual device's rename is a
/// `std::fs::rename`, which replaces the destination and answers OK. Why the
/// probe can't be anything tighter: `mtp/volume_impl.rs`, on the guard itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_honors_the_shared_no_clobber_contract() {
    let _guard = device_lock().await;
    let (device, volume) = connect_primed_volume(Some("/Documents")).await;

    cmdr_fs::volume::conformance::assert_rename_refuses_an_existing_destination(
        &volume,
        Path::new("/Documents/report.txt"),
        Path::new("/Documents/notes.txt"),
    )
    .await;

    device.teardown(test_connection_manager()).await;
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
    let _guard = device_lock().await;
    let (device, volume) = connect_primed_volume(None).await;

    cmdr_fs::volume::conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        &volume,
        Path::new("/Documents"),
    )
    .await;

    device.teardown(test_connection_manager()).await;
}

/// The shared writability-declaration assertion: `is_writable()` and what the
/// device actually accepts say the same thing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn is_writable_honors_the_shared_declaration_contract() {
    let _guard = device_lock().await;
    let (device, volume) = connect_primed_volume(None).await;

    cmdr_fs::volume::conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new("/scratch"))
        .await;

    device.teardown(test_connection_manager()).await;
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
    let _guard = device_lock().await;
    let content = b"the bytes a copy would move";
    let fixture = crate::virtual_device::setup_virtual_mtp_device();
    std::fs::write(fixture.root().join("internal/exported.txt"), content).expect("seed the file on the device");
    crate::virtual_device::rescan_virtual_device();
    let device = crate::testing::connect_fixture(test_connection_manager(), fixture).await;
    let volume = volume_for(test_connection_manager(), &device, None).await;

    cmdr_fs::volume::conformance::assert_export_matches_the_bytes_offered(&volume, Path::new("/exported.txt"), content)
        .await;

    device.teardown(test_connection_manager()).await;
}

/// The shared `NotFound`-payload assertion, over a real `MtpVolume`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn not_found_honors_the_shared_path_payload_contract() {
    let _guard = device_lock().await;
    let (device, volume) = connect_primed_volume(None).await;

    cmdr_fs::volume::conformance::assert_not_found_carries_the_path(&volume, Path::new("/no-such-file.txt")).await;

    device.teardown(test_connection_manager()).await;
}

/// The shared conflict-scan assertion, over a real `MtpVolume`: a destination
/// the paste would create answers "nothing clashes", not the `NotFound` the
/// cache-aware listing hands back for a path it can't resolve.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflict_scan_honors_the_shared_missing_destination_contract() {
    let _guard = device_lock().await;
    let (device, volume) = connect_primed_volume(None).await;

    cmdr_fs::volume::conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(
        &volume,
        Path::new("/not-created-yet"),
    )
    .await;

    device.teardown(test_connection_manager()).await;
}
