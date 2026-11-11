//! Path → handle resolution when the path cache doesn't know the path.
//!
//! The cache only knows what some listing put there, and a path reaches an op by
//! many routes that never listed its parent: a pane restored after a reconnect, a
//! search result, a go-to-path, or a pane still showing a file another op already
//! removed. Every one of those used to fail as an untyped "path not in cache", so
//! the user read a generic failure instead of either a working copy or a clear
//! "already moved or deleted".
//!
//! Two halves, both pinned here: a miss re-lists the parent chain and carries on,
//! and a path the device really doesn't hold answers the TYPED
//! `MtpConnectionError::ObjectNotFound` carrying the path that was asked for.

use crate::connection::{MTP_READ_PACING, MtpConnectionError, MtpDeleteScope};
use crate::testing::{connect_virtual_device, device_lock, test_connection_manager};

/// A read of a file whose folder nobody listed heals the cache and succeeds.
///
/// `/DCIM/Burst` is two levels below the primed root, so this covers a heal that
/// has to walk more than one missing ancestor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_below_folders_nobody_listed_heals_and_succeeds() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;
    let expected = std::fs::metadata(device.root().join("internal/DCIM/Burst/burst-001.jpg"))
        .expect("the fixture seeds burst-001.jpg")
        .len();

    let session = test_connection_manager()
        .open_read_session(
            &device.id,
            device.storage_id,
            "/DCIM/Burst/burst-001.jpg",
            0,
            MTP_READ_PACING,
        )
        .await;

    match session {
        Ok(session) => assert_eq!(session.total_size(), expected, "the heal must resolve the right object"),
        Err(e) => panic!("a path whose folders were never listed must heal, got {e:?}"),
    }

    device.teardown(test_connection_manager()).await;
}

/// Acting again on a file an earlier op already deleted answers the typed
/// not-found, naming the file. This is the stale-pane repeat delete from the
/// field: the pane still showed the file, so the user deleted it twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_object_already_deleted_answers_object_not_found_with_its_path() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;
    let manager = test_connection_manager();
    manager
        .list_directory(&device.id, device.storage_id, "/Documents")
        .await
        .expect("listing the folder the pane shows");
    manager
        .delete_object(
            &device.id,
            device.storage_id,
            "/Documents/report.txt",
            MtpDeleteScope::SingleNode,
        )
        .await
        .expect("the first delete removes the file");

    let again = manager
        .delete_object(
            &device.id,
            device.storage_id,
            "/Documents/report.txt",
            MtpDeleteScope::SingleNode,
        )
        .await;
    assert!(
        matches!(&again, Err(MtpConnectionError::ObjectNotFound { path, .. }) if path == "/Documents/report.txt"),
        "a second delete of a gone file must be the typed not-found naming it, got {again:?}"
    );

    let read = manager
        .open_read_session(
            &device.id,
            device.storage_id,
            "Documents/report.txt",
            0,
            MTP_READ_PACING,
        )
        .await;
    assert!(
        matches!(&read, Err(MtpConnectionError::ObjectNotFound { path, .. }) if path == "/Documents/report.txt"),
        "a copy of a gone file must be the typed not-found too, whichever spelling it arrives in, got {:?}",
        read.err()
    );

    device.teardown(test_connection_manager()).await;
}

/// A path under a folder that doesn't exist is not-found for the path ASKED
/// about, not for the missing folder: the user acted on the file, so the message
/// names the file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_under_a_missing_folder_is_not_found_for_the_path_asked_about() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;

    let outcome = test_connection_manager()
        .delete_object(
            &device.id,
            device.storage_id,
            "/Nowhere/ghost.txt",
            MtpDeleteScope::SingleNode,
        )
        .await;
    assert!(
        matches!(&outcome, Err(MtpConnectionError::ObjectNotFound { path, .. }) if path == "/Nowhere/ghost.txt"),
        "got {outcome:?}"
    );

    device.teardown(test_connection_manager()).await;
}
