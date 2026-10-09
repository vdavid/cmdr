//! What a transfer MEANS on a live SFTP server: merges, policies, moves in both
//! directions, same-server moves and copies, and destinations addressed the way
//! the transfer dialog addresses them.
//!
//! The backend-blind scenarios live in `network_semantics_test_support.rs`,
//! shared with the other network backends so a claim can't hold on one and
//! quietly rot on another. The cells stay here because the integration lane
//! selects them by the `sftp_integration_` name prefix.
//!
//! The two dialog-address cells at the bottom are SFTP's own: they pin how
//! `cmdr_fs::volume::root_anchored` meets this backend's app root, which five app
//! sites go through (`commands/file_system/listing.rs`, `write_operations/routing.rs`
//! twice, `transfer/volume/copy.rs`, `transfer/volume/move.rs`).
//!
//! ❗ **The dialog cell is the one that would catch the doubling.** `root_anchored`
//! JOINS anything not under the root onto it, so a bare `/srv/data/photos` would
//! come back `sftp://…/srv/data/srv/data/photos`, strip to a real server path, and
//! land the user's files somewhere they never named. The SMB twin
//! (`smb_transfer_semantics_test.rs`) exists for the same reason and cost a
//! 360 GB move once (ERR-XCP5Q).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{EntryKind, Volume};
use cmdr_sftp::volume::testing::{FIXTURE_ROOT, scratch_dir};

use super::network_move_drift_test_support::{
    a_file_added_mid_move_off_the_server_stays, a_file_saved_over_mid_move_off_the_server_stays,
};
use super::network_semantics_test_support::{
    a_copy_into_a_missing_nested_destination_makes_every_level,
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash,
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file, a_folder_moved_off_the_server_leaves_no_source,
    a_folder_moved_onto_the_server_leaves_no_source, a_move_merge_onto_the_server_spares_what_it_skipped,
    a_multi_megabyte_file_round_trips_byte_exact, a_rename_policy_lands_the_clash_beside_the_users_file,
    a_same_server_copy_duplicates_a_tree, a_same_server_move_merges_without_a_folder_prompt,
    a_same_server_move_without_a_clash_moves_the_folder_whole, local_volume,
    many_files_at_full_concurrency_land_intact, overwrite_smaller_replaces_only_the_smaller_destination, seed,
    try_read,
};
use super::network_transfer_test_support::clean_deep;
use super::sftp_test_support::{SftpFixture, connect, fixture, fixture_on};
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::write_operations::{
    CollectorEventSink, VolumeCopyConfig, WriteOperationState, move_volumes_with_progress,
};
use crate::test_support::TestDir;

// ── Merges and policies ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_deep_clash_merge_under_skip_keeps_every_dest_only_file() {
    let (remote, dir) = fixture("merge-skip").await;
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_deep_clash_merge_under_overwrite_replaces_only_the_clash() {
    let (remote, dir) = fixture("merge-overwrite").await;
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_rename_policy_lands_the_clash_beside_the_users_file() {
    let (remote, dir) = fixture("merge-rename").await;
    a_rename_policy_lands_the_clash_beside_the_users_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_overwrite_smaller_replaces_only_the_smaller_destination() {
    let (remote, dir) = fixture("overwrite-smaller").await;
    overwrite_smaller_replaces_only_the_smaller_destination(remote, dir).await;
}

// ── Moves across the volume boundary ─────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_move_merge_onto_the_server_spares_what_it_skipped() {
    let (remote, dir) = fixture("move-merge-skip").await;
    a_move_merge_onto_the_server_spares_what_it_skipped(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_folder_moved_onto_the_server_leaves_no_source() {
    let (remote, dir) = fixture("move-onto").await;
    a_folder_moved_onto_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_folder_moved_off_the_server_leaves_no_source() {
    let (remote, dir) = fixture("move-off").await;
    a_folder_moved_off_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_file_saved_over_mid_move_off_the_server_stays() {
    let (remote, dir) = fixture("drift-saved-over").await;
    a_file_saved_over_mid_move_off_the_server_stays(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_file_added_mid_move_off_the_server_stays() {
    let (remote, dir) = fixture("drift-added").await;
    a_file_added_mid_move_off_the_server_stays(remote, dir).await;
}

/// A link inside a folder moved off the server is the link, never its target
/// (#140): the folder it points at sits OUTSIDE the selection, and keeps every
/// file. SFTP only: SMB and WebDAV can't hold a link.
///
/// ❗ The move itself FAILS today, on the link: SFTP lists a link with its own
/// (lstat) attributes, so a link to a folder reads as a file, the copy walk
/// streams it, and the server refuses to open a folder. Losing nothing is what
/// this cell pins, so it holds whichever way the move ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_link_in_a_folder_moved_off_the_server_leaves_its_target() {
    let sftp = connect(SftpFixture::Stock).await;
    let dir = PathBuf::from(scratch_dir("move-off-link"));
    seed(
        &sftp,
        &dir,
        &[("outside/keep.txt", b"outside the selection"), ("tree/a.txt", b"alpha")],
    )
    .await;
    sftp.create_symlink(&dir.join("tree/ln"), &dir.join("outside"))
        .await
        .expect("the fixture makes links");
    let remote: Arc<dyn Volume> = Arc::new(sftp);
    assert!(
        matches!(remote.entry_kind(&dir.join("tree/ln")).await, Ok(EntryKind::Symlink)),
        "precondition: the server holds a link, not a copy"
    );
    let (_local_dir, local) = local_volume("move-off-link");

    let outcome = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "move-off-link",
        &Arc::new(WriteOperationState::new(Duration::from_millis(200))),
        Arc::clone(&remote),
        &[dir.join("tree")],
        local,
        Path::new(""),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert_eq!(
        try_read(remote.as_ref(), &dir.join("outside/keep.txt"))
            .await
            .as_deref(),
        Some(&b"outside the selection"[..]),
        "the move went through the link and took its target's file (move ended {outcome:?})"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

// ── Staying on the server ────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_same_server_move_merges_without_a_folder_prompt() {
    let (remote, dir) = fixture("same-move-merge").await;
    a_same_server_move_merges_without_a_folder_prompt(remote, dir).await;
}

/// On the server without `posix-rename@openssh.com`, where a rename that must
/// not replace anything takes the plain `SSH_FXP_RENAME` path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_same_server_move_merges_without_posix_rename() {
    let (remote, dir) = fixture_on(SftpFixture::NoPosixRename, "same-move-merge-plain").await;
    a_same_server_move_merges_without_a_folder_prompt(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_same_server_move_without_a_clash_moves_the_folder_whole() {
    let (remote, dir) = fixture("same-move").await;
    a_same_server_move_without_a_clash_moves_the_folder_whole(remote, dir).await;
}

/// The stock server copies for itself (`copy-data`), so no byte crosses the
/// link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_same_server_copy_duplicates_a_tree() {
    let (remote, dir) = fixture("same-copy").await;
    a_same_server_copy_duplicates_a_tree(remote, dir).await;
}

/// The server without `copy-data`, where the same copy has to stream through
/// the client and must land the same tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_same_server_copy_streams_where_the_server_cant_copy() {
    let (remote, dir) = fixture_on(SftpFixture::NoPosixRename, "same-copy-streamed").await;
    a_same_server_copy_duplicates_a_tree(remote, dir).await;
}

// ── Destinations, sizes, and load ────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_copy_into_a_missing_nested_destination_makes_every_level() {
    let (remote, dir) = fixture("missing-nested").await;
    a_copy_into_a_missing_nested_destination_makes_every_level(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_multi_megabyte_file_round_trips_byte_exact() {
    let (remote, dir) = fixture("multi-megabyte").await;
    a_multi_megabyte_file_round_trips_byte_exact(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_many_files_at_full_concurrency_land_intact() {
    let (remote, dir) = fixture("many-files").await;
    many_files_at_full_concurrency_land_intact(remote, dir).await;
}

// ── Destinations the dialog addresses ────────────────────────────────

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
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));

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
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));

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
