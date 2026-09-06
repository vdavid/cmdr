//! `Volume::delete` stops at one node, on `MtpVolume`.
//!
//! MTP is the one backend that has to IMPLEMENT the non-recursion contract
//! rather than inherit it: POSIX gets `ENOTEMPTY` from `remove_dir` and SMB gets
//! `STATUS_DIRECTORY_NOT_EMPTY` from the server, but PTP `DeleteObject` on a
//! folder is whatever the code around it decides. So the scope lives in
//! `MtpConnectionManager` (`MtpDeleteScope`), and this file pins both halves of
//! the split plus the shared cross-backend assertion.

use std::path::Path;

use cmdr_fs::volume::{Volume, VolumeError};

use crate::connection::{MtpConnectionError, MtpDeleteScope};
use crate::testing::{ConnectedDevice, connect_virtual_device, device_lock, test_connection_manager, volume_for};
use crate::volume::MtpVolume;

/// A connected device and a volume over its writable storage, with `primed`
/// listed as well as the root.
async fn connect_virtual_volume(primed: Option<&str>) -> (ConnectedDevice, MtpVolume) {
    let device = connect_virtual_device(test_connection_manager()).await;
    let volume = volume_for(test_connection_manager(), &device, primed).await;
    (device, volume)
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
    let _guard = device_lock().await;
    let (device, vol) = connect_virtual_volume(None).await;

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
    let raw = test_connection_manager()
        .delete_object(&device.id, device.storage_id, "/Documents", MtpDeleteScope::SingleNode)
        .await;
    assert!(
        matches!(raw, Err(MtpConnectionError::DirectoryNotEmpty { .. })),
        "the connection layer must answer with the typed refusal, got {raw:?}"
    );

    device.teardown(test_connection_manager()).await;
}

/// The refusal must not overshoot: an EMPTY folder still deletes through
/// `Volume::delete`. Every tree delete in the app walks the tree itself and
/// hands `delete` an already-empty directory, so this is the shape production
/// actually relies on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_still_removes_an_empty_folder() {
    let _guard = device_lock().await;
    let (device, vol) = connect_virtual_volume(None).await;

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

    device.teardown(test_connection_manager()).await;
}

/// The shared `Volume::delete` non-recursion assertion, over a real
/// `MtpVolume`. This is the one that would have caught the bug: MTP claimed the
/// contract by implementing the trait and broke it in the one place nobody was
/// looking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_honors_the_shared_non_recursion_contract() {
    let _guard = device_lock().await;
    let (device, vol) = connect_virtual_volume(Some("/Documents")).await;

    cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact(
        &vol,
        Path::new("/Documents"),
        "report.txt",
    )
    .await;

    device.teardown(test_connection_manager()).await;
}

/// `MtpDeleteScope::Tree` still removes a whole subtree. Pins the one
/// intentional `Tree` caller (`commands::mtp::delete_mtp_object`) so the split
/// stays a decision rather than an accident: if this goes red, someone narrowed
/// the recursive entry point and the IPC command silently stopped working.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tree_scope_still_removes_a_whole_subtree() {
    let _guard = device_lock().await;
    let (device, vol) = connect_virtual_volume(None).await;
    let backing = device.root().join("internal");
    // `/DCIM` holds a file AND a `Burst/` subfolder with its own file, so this
    // covers the recursive branch, not just a one-level directory.
    vol.list_directory(Path::new("/DCIM"), None)
        .await
        .expect("priming the DCIM listing");

    test_connection_manager()
        .delete_object(&device.id, device.storage_id, "/DCIM", MtpDeleteScope::Tree)
        .await
        .expect("a Tree-scoped delete must remove the whole subtree");

    assert!(
        !backing.join("DCIM").exists(),
        "the whole subtree must be gone from the device's storage"
    );

    device.teardown(test_connection_manager()).await;
}
