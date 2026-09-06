//! What a transfer means when the DIALOG addresses the destination, on a live
//! SFTP server.
//!
//! `sftp_transfer_integration_test.rs` spells every destination relative to the
//! volume root, which is one of the two dialects the app uses and the one that
//! can't go wrong. This file covers the other: a destination that went through
//! `cmdr_fs::volume::root_anchored` on its way in, which is what five app sites
//! do (`commands/file_system/listing.rs`, `write_operations/routing.rs` twice,
//! `transfer/volume/copy.rs`, `transfer/volume/move.rs`).
//!
//! ❗ **This is the cell that would catch the doubling.** `root_anchored` JOINS
//! anything not under the root onto it, so a bare `/srv/data/photos` would come
//! back `sftp://…/srv/data/srv/data/photos`, strip to a real server path, and
//! land the user's files somewhere they never named. The SMB twin
//! (`smb_transfer_semantics_test.rs`) exists for the same reason and cost a
//! 360 GB move once (ERR-XCP5Q).
//!
//! The lane selects these by the `sftp_integration_` name prefix.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_sftp::volume::testing::{
    FIXTURE_PASSWORD, FIXTURE_ROOT, connect_fixture, fixture_host, fixture_params, scratch_dir,
};

use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::write_operations::{
    CollectorEventSink, VolumeCopyConfig, WriteOperationState, move_volumes_with_progress,
};
use crate::test_support::TestDir;

/// A live fixture volume and a scratch directory of its own on the export.
async fn fixture(what: &str) -> (Arc<dyn Volume>, PathBuf) {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = PathBuf::from(scratch_dir(what));
    volume.create_directory(&dir).await.expect("scratch dir");
    (Arc::new(volume), dir)
}

/// A MOVE into a server subfolder addressed the way the transfer dialog
/// addresses it: volume-relative, anchored at the IPC boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_move_into_a_dialog_addressed_subfolder_lands_where_the_pane_says() {
    let (remote, dir) = fixture("app-anchored-dest").await;
    let photos = dir.join("photos");
    remote
        .create_directory(&photos)
        .await
        .expect("the destination subfolder");

    let local_dir = TestDir::new("sftp_anchored_dest");
    std::fs::write(local_dir.join("clip.mp4"), b"footage").expect("seed the local source");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    // What the dialog puts on the wire (leading slash, no root: the volume is a
    // separate dropdown), and what the boundary makes of it.
    let dialog_dest = format!("/{}", photos.display());
    let dest_path = cmdr_fs::volume::root_anchored(remote.root(), Path::new(&dialog_dest));
    assert_eq!(
        dest_path,
        remote.root().join(&photos),
        "❗ anchoring puts the volume's APP root in front, scheme and all; a doubled `/srv/data` here \
         is a real path on a real server and quietly the wrong one"
    );

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events,
        "test-op-sftp-dialog-dest",
        &state,
        Arc::clone(&local),
        &[PathBuf::from("clip.mp4")],
        Arc::clone(&remote),
        &dest_path,
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(
        result.is_ok(),
        "a move into an existing server subfolder succeeds: {result:?}"
    );

    // On the server, under the subfolder the dialog named, and nowhere else.
    let landed = remote
        .get_metadata(&photos.join("clip.mp4"))
        .await
        .expect("the moved file is where the pane says it is");
    assert_eq!(landed.size, Some(7));
    assert!(
        !local_dir.join("clip.mp4").exists(),
        "a completed move leaves no source behind"
    );
    assert!(
        !remote.exists(&remote.root().join("srv")).await,
        "❗ and nothing was created under a doubled root: a `srv/` inside the export is what a \
         doubled `/srv/data` leaves behind"
    );

    let _ = remote.delete(&photos.join("clip.mp4")).await;
    let _ = remote.delete(&photos).await;
    let _ = remote.delete(&dir).await;
}

/// ❗ **A BARE server path is refused, and the transfer stops before it writes.**
///
/// The direct route into the same hole: a destination that never went through
/// the prefix. Accepting it as a courtesy is what would let an anchored copy of
/// one land under a doubled root, so the volume answers `NotFound` and the
/// engine reports it instead of creating a second tree on someone's server.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_bare_server_path_destination_is_refused_before_anything_is_written() {
    let (remote, dir) = fixture("app-bare-dest").await;
    let photos = dir.join("photos");
    remote
        .create_directory(&photos)
        .await
        .expect("the destination subfolder");

    let local_dir = TestDir::new("sftp_bare_dest");
    std::fs::write(local_dir.join("clip.mp4"), b"footage").expect("seed the local source");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    // What nothing in the app spells any more: the server's own absolute path,
    // with no prefix on it.
    let bare = PathBuf::from(format!("{FIXTURE_ROOT}/{}", photos.display()));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events,
        "test-op-sftp-bare-dest",
        &state,
        Arc::clone(&local),
        &[PathBuf::from("clip.mp4")],
        Arc::clone(&remote),
        &bare,
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(
        result.is_err(),
        "a destination this volume doesn't own is refused: {result:?}"
    );
    assert!(
        !remote.exists(&photos.join("clip.mp4")).await,
        "❗ and nothing was written on the way to refusing"
    );
    assert!(
        local_dir.join("clip.mp4").exists(),
        "a refused move leaves the source where it was"
    );

    let _ = remote.delete(&photos).await;
    let _ = remote.delete(&dir).await;
}
