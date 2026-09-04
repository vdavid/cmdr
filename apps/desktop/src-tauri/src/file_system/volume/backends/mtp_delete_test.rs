//! `Volume::delete` stops at one node, on `MtpVolume`.
//!
//! MTP is the one backend that has to IMPLEMENT the non-recursion contract
//! rather than inherit it: POSIX gets `ENOTEMPTY` from `remove_dir` and SMB gets
//! `STATUS_DIRECTORY_NOT_EMPTY` from the server, but PTP `DeleteObject` on a
//! folder is whatever the code around it decides. So the scope lives in
//! `MtpConnectionManager` (`MtpDeleteScope`), and this file pins both halves of
//! the split plus the shared cross-backend assertion.
//!
//! Every test here drives a virtual MTP device, so the whole file carries that
//! feature gate (declared in `backends/mod.rs`).

use super::*;
use std::path::Path;

use crate::mtp::connection::DeviceWatch;
use crate::mtp::connection::MtpConnectionError;
use crate::mtp::connection_manager;
use std::sync::Arc;

/// Connects the virtual device and builds an `MtpVolume` over its writable
/// storage, with the root listing primed (`resolve_path_to_handle` is
/// cache-only). Returns the device id, storage id, the volume, and the
/// storage's backing dir.
async fn connect_virtual_volume(
    fixture: &crate::mtp::virtual_device::VirtualDeviceFixture,
) -> (String, u32, MtpVolume, std::path::PathBuf) {
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;
    let vol = MtpVolume::new(Arc::clone(connection_manager()), &device_id, storage_id, "Test");
    vol.list_directory(Path::new("/"), None)
        .await
        .expect("priming the root listing");
    (device_id, storage_id, vol, fixture.root().join("internal"))
}

/// `MtpVolume::delete` on a folder that still holds a file refuses with a TYPED
/// error and destroys nothing.
///
/// The typed part matters as much as the refusal: callers classify on the
/// variant, never on the wording, and a
/// backend-neutral `IoError` carrying `ENOTEMPTY` is what makes MTP answer the
/// same question the same way LocalPosix and SMB do.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_refuses_a_non_empty_folder_with_a_typed_error() {
    use crate::mtp::virtual_device::{
        setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, storage_id, vol, _backing) = connect_virtual_volume(&fixture).await;

    let outcome = vol.delete(Path::new("/Documents")).await;
    assert!(
        matches!(
            outcome,
            Err(VolumeError::IoError {
                raw_os_error: Some(_),
                ..
            })
        ),
        "a non-empty folder must refuse with a typed IoError carrying ENOTEMPTY, got {outcome:?}"
    );

    // Nothing went, on the device or in the path cache.
    let listing = vol
        .list_directory(Path::new("/Documents"), None)
        .await
        .expect("the refused folder must still be listable");
    assert!(
        listing.iter().any(|e| e.name == "report.txt"),
        "a refused delete must destroy nothing, found {:?}",
        listing.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // And the connection layer's own answer is the typed variant, not a message.
    let raw = connection_manager()
        .delete_object(
            &device_id,
            storage_id,
            "/Documents",
            crate::mtp::connection::MtpDeleteScope::SingleNode,
        )
        .await;
    assert!(
        matches!(raw, Err(MtpConnectionError::DirectoryNotEmpty { .. })),
        "the connection layer must answer with the typed refusal, got {raw:?}"
    );

    connection_manager()
        .disconnect(&device_id, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(fixture.location_id);
}

/// The refusal must not overshoot: an EMPTY folder still deletes through
/// `Volume::delete`. Every tree delete in the app walks the tree itself and
/// hands `delete` an already-empty directory, so this is the shape production
/// actually relies on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_still_removes_an_empty_folder() {
    use crate::mtp::virtual_device::{
        setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, _storage_id, vol, _backing) = connect_virtual_volume(&fixture).await;

    vol.create_directory(Path::new("/empty-one"))
        .await
        .expect("creating an empty folder");
    vol.delete(Path::new("/empty-one"))
        .await
        .expect("an EMPTY folder must still delete");

    let root = vol.list_directory(Path::new("/"), None).await.expect("listing root");
    assert!(
        !root.iter().any(|e| e.name == "empty-one"),
        "the empty folder must be gone"
    );

    connection_manager()
        .disconnect(&device_id, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(fixture.location_id);
}

/// The shared `Volume::delete` non-recursion assertion, over a real
/// `MtpVolume`. This is the one that would have caught the bug: MTP claimed the
/// contract by implementing the trait and broke it in the one place nobody was
/// looking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_honors_the_shared_non_recursion_contract() {
    use crate::mtp::virtual_device::{
        setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, _storage_id, vol, _backing) = connect_virtual_volume(&fixture).await;
    vol.list_directory(Path::new("/Documents"), None)
        .await
        .expect("priming the Documents listing");

    cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact(
        &vol,
        Path::new("/Documents"),
        "report.txt",
    )
    .await;

    connection_manager()
        .disconnect(&device_id, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(fixture.location_id);
}

/// `MtpDeleteScope::Tree` still removes a whole subtree. Pins the one
/// intentional `Tree` caller (`commands::mtp::delete_mtp_object`) so the split
/// stays a decision rather than an accident: if this goes red, someone narrowed
/// the recursive entry point and the IPC command silently stopped working.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tree_scope_still_removes_a_whole_subtree() {
    use crate::mtp::virtual_device::{
        setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let (device_id, storage_id, vol, backing) = connect_virtual_volume(&fixture).await;
    // `/DCIM` holds a file AND a `Burst/` subfolder with its own file, so this
    // covers the recursive branch, not just a one-level directory.
    vol.list_directory(Path::new("/DCIM"), None)
        .await
        .expect("priming the DCIM listing");

    connection_manager()
        .delete_object(
            &device_id,
            storage_id,
            "/DCIM",
            crate::mtp::connection::MtpDeleteScope::Tree,
        )
        .await
        .expect("a Tree-scoped delete must remove the whole subtree");

    assert!(
        !backing.join("DCIM").exists(),
        "the whole subtree must be gone from the device's storage"
    );

    connection_manager()
        .disconnect(&device_id, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(fixture.location_id);
}
