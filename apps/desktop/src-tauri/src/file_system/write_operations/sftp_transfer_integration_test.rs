//! Real copies between a local disk and a live SFTP server, driven through the
//! app's own `copy_between_volumes`.
//!
//! ❗ **The point is the entry point.** `cmdr-sftp`'s own Docker suite exercises
//! every method the copy engine calls, and it was fully green while both
//! directions of an actual copy were broken in the app: one on a capability
//! predicate the crate never states (`supports_export`), the other on a
//! pre-flight check that read the backend's honest "I can't measure free space"
//! as "there's no room". Neither is reachable from inside the crate, because
//! neither lives there. So these cells start where the transfer dialog starts.
//!
//! Every cell checksums the bytes at BOTH ends. A copy that lands a file of the
//! right length full of the wrong bytes is a data-loss bug that an `exists()`
//! assertion reports as a pass.
//!
//! The scenarios themselves live in `network_transfer_test_support.rs`, shared
//! with the WebDAV suite so a claim can't hold on one backend and quietly rot on
//! the other. The cells stay here because the integration lane selects them by
//! the `sftp_integration_` name prefix.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_sftp::volume::testing::{FIXTURE_PASSWORD, connect_fixture, fixture_host, fixture_params, scratch_dir};

use super::network_transfer_test_support::{
    a_cancelled_upload_leaves_nothing_behind, a_directory_tree_lands_intact_off_the_server,
    a_directory_tree_lands_intact_on_the_server, a_pre_existing_destination_still_probes_each_name,
    an_overwrite_answer_replaces_the_destination_bytes, awkward_names_survive_a_round_trip, clean_deep, read_all,
    run_copy, self_describing_bytes, sha256,
};
use crate::file_system::volume::LocalPosixVolume;
use crate::test_support::TestDir;

/// Big enough to cross the read and write windows rather than ride in one
/// request, so reassembly and offset bookkeeping are actually exercised.
const PAYLOAD_BYTES: usize = 700_000;

/// A live fixture volume and a scratch directory of its own on the export.
///
/// ❗ Every cell takes a fresh one: the cells share one export and `nextest` runs
/// them in parallel, so a fixed name would have two of them renaming each
/// other's files. `what` says which cell to look at when one leaves a mess
/// behind.
async fn fixture(what: &str) -> (Arc<dyn Volume>, PathBuf) {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = PathBuf::from(scratch_dir(what));
    volume.create_directory(&dir).await.expect("scratch dir");
    (Arc::new(volume), dir)
}

/// Copy OFF the server: the direction `supports_export()` gates.
///
/// Before that predicate was stated, this never started at all — `copy_between_volumes`
/// refused it synchronously and logged nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_off_a_server_lands_every_byte() {
    let (remote, dir) = fixture("app-copy-from").await;

    let content = self_describing_bytes(PAYLOAD_BYTES, "downloaded.bin");
    let remote_file = dir.join("downloaded.bin");
    remote
        .create_file(&remote_file, &content)
        .await
        .expect("seed the file on the server");

    // The checksum at the SOURCE end, taken off the server itself rather than
    // from the buffer we wrote, so a bad seed can't make a bad copy look good.
    let source_digest = sha256(&read_all(remote.as_ref(), &remote_file).await);
    assert_eq!(source_digest, sha256(&content), "the fixture seed must round-trip");

    let local_dir = TestDir::new("sftp_copy_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    run_copy(
        "copy-off-sftp",
        Arc::clone(&remote),
        vec![remote_file.clone()],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    let landed = read_all(local.as_ref(), Path::new("downloaded.bin")).await;
    assert_eq!(landed.len(), content.len(), "the copy landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        source_digest,
        "the bytes on local disk must checksum to what the server holds"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// Copy ONTO the server: the direction the free-space pre-flight killed.
///
/// SFTP answers `get_space_info` with `NotSupported` on purpose, and the
/// pre-flight used to propagate that as a failure, so every copy in died after
/// roughly half a second with a message that named neither the check nor the
/// reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_onto_a_server_lands_every_byte() {
    let (remote, dir) = fixture("app-copy-to").await;

    // ❗ The destination really can't answer the space question, which is the
    // whole point of this cell.
    assert!(
        matches!(
            remote.get_space_info().await,
            Err(cmdr_fs::volume::VolumeError::NotSupported)
        ),
        "this cell only means something while the server can't report free space"
    );

    let content = self_describing_bytes(PAYLOAD_BYTES, "uploaded.bin");
    let local_dir = TestDir::new("sftp_copy_onto_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
    local
        .create_file(Path::new("uploaded.bin"), &content)
        .await
        .expect("seed the local file");

    let source_digest = sha256(&read_all(local.as_ref(), Path::new("uploaded.bin")).await);
    assert_eq!(source_digest, sha256(&content), "the local seed must round-trip");

    run_copy(
        "copy-onto-sftp",
        Arc::clone(&local),
        vec![PathBuf::from("uploaded.bin")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    let landed = read_all(remote.as_ref(), &dir.join("uploaded.bin")).await;
    assert_eq!(landed.len(), content.len(), "the copy landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        source_digest,
        "the bytes on the server must checksum to what local disk holds"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_a_directory_tree_onto_a_server_lands_it_intact() {
    let (remote, dir) = fixture("app-tree-to").await;
    a_directory_tree_lands_intact_on_the_server(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_a_directory_tree_off_a_server_lands_it_intact() {
    let (remote, dir) = fixture("app-tree-from").await;
    a_directory_tree_lands_intact_off_the_server(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_cancelled_upload_leaves_nothing_behind() {
    let (remote, dir) = fixture("app-cancel").await;
    a_cancelled_upload_leaves_nothing_behind(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_an_overwrite_answer_replaces_the_destination_bytes() {
    let (remote, dir) = fixture("app-overwrite").await;
    an_overwrite_answer_replaces_the_destination_bytes(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_pre_existing_destination_still_probes_each_name() {
    let (remote, dir) = fixture("app-pre-existing").await;
    a_pre_existing_destination_still_probes_each_name(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_awkward_names_survive_a_round_trip() {
    let (remote, dir) = fixture("app-awkward-names").await;
    awkward_names_survive_a_round_trip(remote, dir).await;
}
