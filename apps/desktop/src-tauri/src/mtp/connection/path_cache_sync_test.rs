//! `PathHandleCache` stays bidirectional across every mutation.
//!
//! The cache backs two different consumers: `resolve_path_to_handle` reads the
//! FORWARD map to browse, and `resolve_handle_to_path` reads the REVERSE map to
//! turn a pathless PTP change event into a directory to refresh (and to feed the
//! per-volume index). A mutation that writes only one direction leaves no visible
//! symptom at the time — `resolve_handle_to_path` just falls back to a USB
//! parent-chain walk — so the desync surfaces later, as extra round trips per
//! event, and as a WRONG path once the device reuses a handle whose stale reverse
//! entry still points at the old object.
//!
//! These tests assert the invariant at the point each mutation writes it.

use super::connection_manager;
use crate::mtp::virtual_device::{
    setup_virtual_mtp_device_at, unregister_virtual_mtp_device, virtual_device_test_lock,
};
use mtp_rs::ObjectHandle;
use std::path::{Path, PathBuf};

/// A connected virtual device, torn down (disconnect + unregister) by `teardown`.
struct Device {
    id: String,
    storage_id: u32,
    location_id: u64,
    _root: tempfile::TempDir,
}

/// Connects a virtual MTP device with the root listing primed, so mutations can
/// resolve their parent handles (`resolve_path_to_handle` is cache-only).
async fn connect_device() -> Device {
    let root = tempfile::tempdir().expect("tmp device root");
    let location_id = setup_virtual_mtp_device_at(root.path());

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
        .disconnect(&device.id, None, super::MtpDisconnectReason::User)
        .await
        .ok();
    unregister_virtual_mtp_device(device.location_id);
}

async fn reverse_entry(device: &Device, handle: ObjectHandle) -> Option<PathBuf> {
    connection_manager()
        .cached_path_for_handle(&device.id, device.storage_id, handle)
        .await
}

/// `create_folder` caches the new folder's handle, and must cache it BOTH ways:
/// the very next PTP `ObjectAdded` for something inside it resolves through the
/// reverse map.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_folder_records_the_handle_in_both_directions() {
    let _guard = virtual_device_test_lock().lock().await;
    let device = connect_device().await;

    let created = connection_manager()
        .create_folder(&device.id, device.storage_id, "/", "new-folder")
        .await
        .expect("create_folder should succeed");
    let handle = ObjectHandle(created.handle.into());

    assert_eq!(
        reverse_entry(&device, handle).await.as_deref(),
        Some(Path::new("/new-folder")),
        "a created folder must be resolvable handle → path, not only path → handle"
    );

    teardown(device).await;
}

/// A rename moves the object to a new path under the SAME handle, so the reverse
/// entry must follow it. Left stale, a later event for that handle refreshes the
/// pre-rename directory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_moves_the_reverse_entry_to_the_new_path() {
    let _guard = virtual_device_test_lock().lock().await;
    let device = connect_device().await;

    let renamed = connection_manager()
        .rename_object(&device.id, device.storage_id, "/DCIM", "Photos")
        .await
        .expect("rename_object should succeed");
    let handle = ObjectHandle(renamed.handle.into());

    assert_eq!(
        reverse_entry(&device, handle).await.as_deref(),
        Some(Path::new("/Photos")),
        "a renamed object's reverse entry must point at the new path"
    );

    teardown(device).await;
}

/// A move re-parents under the same handle; same requirement as rename.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_updates_the_reverse_entry_to_the_new_parent() {
    let _guard = virtual_device_test_lock().lock().await;
    let device = connect_device().await;

    // Prime the source directory so its child's handle is resolvable.
    connection_manager()
        .list_directory(&device.id, device.storage_id, "Documents")
        .await
        .expect("list Documents should succeed");

    let moved = connection_manager()
        .move_object(&device.id, device.storage_id, "/Documents/notes.txt", "/Music")
        .await
        .expect("move_object should succeed");
    let handle = ObjectHandle(moved.handle.into());

    assert_eq!(
        reverse_entry(&device, handle).await.as_deref(),
        Some(Path::new("/Music/notes.txt")),
        "a moved object's reverse entry must point at the new parent"
    );

    teardown(device).await;
}

/// An upload's new object is cached from `upload_from_stream`; same requirement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_records_the_new_object_in_both_directions() {
    let _guard = virtual_device_test_lock().lock().await;
    let device = connect_device().await;

    let payload: &[u8] = b"uploaded over usb";
    let stream = futures_util::stream::once(async move { Ok(bytes::Bytes::from_static(payload)) });
    connection_manager()
        .upload_from_stream(
            &device.id,
            device.storage_id,
            "Documents",
            "uploaded.txt",
            payload.len() as u64,
            Box::pin(stream),
        )
        .await
        .expect("upload_from_stream should succeed");

    // Read the handle off the FORWARD map, not a fresh listing: a re-list writes
    // both directions and would mask the desync under test.
    let uploaded = Path::new("/Documents/uploaded.txt");
    let handle = connection_manager()
        .cached_handle_for_path(&device.id, device.storage_id, uploaded)
        .await
        .expect("the upload should have cached the new object's handle");

    assert_eq!(
        reverse_entry(&device, handle).await.as_deref(),
        Some(uploaded),
        "an uploaded object must be resolvable handle → path, not only path → handle"
    );

    teardown(device).await;
}

/// A delete must clear BOTH directions. Clearing only the forward map leaves the
/// reverse entry pointing at a path that no longer exists — and Android reuses
/// object handles, so the next object to inherit that handle resolves to the
/// DELETED object's path: a targeted refresh of the wrong directory, and an index
/// upsert at the wrong path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_clears_the_reverse_entry_too() {
    let _guard = virtual_device_test_lock().lock().await;
    let device = connect_device().await;

    connection_manager()
        .list_directory(&device.id, device.storage_id, "Documents")
        .await
        .expect("list Documents should succeed");
    let handle = connection_manager()
        .cached_handle_for_path(&device.id, device.storage_id, Path::new("/Documents/notes.txt"))
        .await
        .expect("the listed file should be in the forward cache");

    connection_manager()
        .delete_object(&device.id, device.storage_id, "/Documents/notes.txt")
        .await
        .expect("delete_object should succeed");

    assert_eq!(
        reverse_entry(&device, handle).await,
        None,
        "a deleted object must leave no reverse entry for a reused handle to hit"
    );

    teardown(device).await;
}
