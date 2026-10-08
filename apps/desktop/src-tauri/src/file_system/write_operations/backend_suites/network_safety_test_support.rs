//! The data-safety cells every network backend owes, written once.
//!
//! Each is a way a transfer or a delete could take something the user didn't
//! give it: a copy that fails mid-merge and "cleans up" the merged folder, a
//! move whose sweep runs on a wrong belief about what landed, a delete that acts
//! on a different set of paths than it previewed, an unanswerable type probe
//! read as a licence to clear a folder, and a download stopped mid-file.
//!
//! `safety_grid_tests.rs` covers the same axes against in-memory doubles, and
//! says why these stay against a live server: a real one publishes bytes at the
//! write path, has real failure timing, and has a `create_directory` and a
//! `delete` whose answers are the whole question. The fault is always injected
//! on the LOCAL side (`FaultyVolume`), so the server is exactly what users run.
//!
//! Same contract as `network_transfer_test_support.rs`: a live volume and a
//! scratch directory in, the directory gone afterwards, and the `#[tokio::test]`
//! cells in the backend files the lane selects by name prefix.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{Volume, VolumeError};

use super::super::event_sinks::CollectorEventSink;
use super::super::seed_incoherent_scan_result_for_test;
use super::super::state::{WriteOperationState, cancel_write_operation};
use super::super::transfer::volume::{FaultyOp, FaultyVolume, copy_volumes_with_progress, move_volumes_with_progress};
use super::super::types::{ConflictResolution, VolumeCopyConfig, WriteOperationConfig};
use super::network_gated_source_test_support::{CANCEL_PAYLOAD_BYTES, gated_reads, gated_upload};
use super::network_semantics_test_support::{local_volume, names_in, seed, try_read};
use super::network_transfer_test_support::{
    assert_no_staging_litter, budget, clean_deep, self_describing_bytes, sha256, start_copy,
};
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

/// The read failure the mid-operation cells inject into the LOCAL source.
fn injected_read_failure() -> VolumeError {
    VolumeError::IoError {
        message: "Injected read failure".into(),
        raw_os_error: Some(5), // EIO
    }
}

/// The user's folder on the server: two files nothing may touch, at two depths.
const USERS_ALBUM: [(&str, &[u8]); 2] = [("keep.txt", b"DEST-keep"), ("sub/keep2.txt", b"DEST-keep2")];

/// The folder arriving, whose second read the fault refuses.
const ARRIVING_ALBUM: [(&str, &[u8]); 3] = [
    ("one.bin", b"SRC-one"),
    ("two.bin", b"SRC-two"),
    ("three.bin", b"SRC-three"),
];

/// A local `album` that dies on its second read, and a preview cache hit that
/// counted three files and recorded NO per-source result: the shape the
/// production bug rode in on (the LOCAL `std::fs` walk emitted it for three
/// months), which made the driver believe the source was a FILE.
async fn failing_album(label: &str) -> (TestDir, Arc<LocalPosixVolume>, Arc<dyn Volume>, VolumeCopyConfig) {
    let local_dir = TestDir::new(label);
    let local = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
    seed(local.as_ref(), Path::new("album"), &ARRIVING_ALBUM).await;
    let source: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&local))
        .failing_call(FaultyOp::OpenReadStream, 2, injected_read_failure())
        .arc();

    let preview_id = format!("{label}-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from("album")], 3, 23);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };
    (local_dir, local, source, config)
}

/// Asserts the user's folder on the server still holds every file it had.
async fn assert_users_album_intact(remote: &dyn Volume, album: &Path, what: &str) {
    for (relative, bytes) in USERS_ALBUM {
        assert_eq!(
            try_read(remote, &album.join(relative)).await.as_deref(),
            Some(bytes),
            "❗ {what} took the user's {relative} off the server"
        );
    }
}

/// THE PRODUCTION BUG, ON THE WIRE: a local folder copied onto a same-named
/// folder the server already has, failing mid-copy. The cleanup that follows a
/// failure must never sweep the MERGED destination root.
pub(super) async fn a_failed_folder_copy_onto_a_users_folder_keeps_their_files(remote: Arc<dyn Volume>, dir: PathBuf) {
    let album = dir.join("album");
    seed(remote.as_ref(), &album, &USERS_ALBUM).await;
    let (_local_dir, local, source, config) = failing_album("failed-merge-copy").await;

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "failed-merge-copy",
        &Arc::new(WriteOperationState::new(Duration::from_millis(0))),
        source,
        &[PathBuf::from("album")],
        Arc::clone(&remote),
        &dir,
        &config,
    )
    .await;
    assert!(result.is_err(), "the injected read failure must fail the copy");

    assert_users_album_intact(remote.as_ref(), &album, "a failed copy").await;
    for (relative, _) in ARRIVING_ALBUM {
        assert!(
            local.exists(&Path::new("album").join(relative)).await,
            "a failed copy never takes a source file, yet album/{relative} is gone"
        );
    }
    assert_no_staging_litter(remote.as_ref(), &album, "a failed copy").await;

    clean_deep(remote.as_ref(), &dir).await;
}

/// The same failure for a MOVE, where a wrong belief about what landed costs the
/// only remaining copy: every source byte must be readable from one side or the
/// other afterwards.
pub(super) async fn a_failed_folder_move_onto_a_users_folder_loses_no_byte(remote: Arc<dyn Volume>, dir: PathBuf) {
    let album = dir.join("album");
    seed(remote.as_ref(), &album, &USERS_ALBUM).await;
    let (_local_dir, local, source, config) = failing_album("failed-merge-move").await;

    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "failed-merge-move",
        &Arc::new(WriteOperationState::new(Duration::from_millis(0))),
        source,
        &[PathBuf::from("album")],
        Arc::clone(&remote),
        &dir,
        &config,
    )
    .await;
    assert!(result.is_err(), "the injected read failure must fail the move");

    assert_users_album_intact(remote.as_ref(), &album, "a failed move").await;
    for (relative, bytes) in ARRIVING_ALBUM {
        let at_source = try_read(local.as_ref(), &Path::new("album").join(relative)).await;
        let at_dest = try_read(remote.as_ref(), &album.join(relative)).await;
        assert!(
            at_source.as_deref() == Some(bytes) || at_dest.as_deref() == Some(bytes),
            "❗ album/{relative} is gone from BOTH sides after a failed move: data destroyed"
        );
    }

    clean_deep(remote.as_ref(), &dir).await;
}

/// `remote` in the registry under an id nothing else uses, for the operations
/// that look their volume up.
///
/// ❗ Unregistering RETIRES the volume (`on_unmount`), and a retired network
/// volume drops its session, so a cell cleans its scratch BEFORE [`Self::leave`].
pub(super) struct Registered {
    pub(super) id: String,
}

impl Registered {
    pub(super) fn new(remote: &Arc<dyn Volume>, what: &str) -> Self {
        let id = format!("{what}-{}", uuid::Uuid::new_v4());
        get_volume_manager().register(&id, Arc::clone(remote));
        Self { id }
    }

    pub(super) fn leave(self) {
        get_volume_manager().unregister(&self.id);
    }
}

/// Runs one volume delete of `sources` to the end.
pub(super) async fn delete_on(
    remote: &Arc<dyn Volume>,
    volume_id: &str,
    label: &str,
    sources: &[PathBuf],
    preview_id: Option<String>,
) -> Result<(), super::super::types::WriteOperationError> {
    let config = WriteOperationConfig {
        preview_id,
        ..WriteOperationConfig::default()
    };
    super::super::delete_volume_files_for_test(
        Arc::clone(remote),
        volume_id,
        &CollectorEventSink::new(),
        label,
        &Arc::new(WriteOperationState::new(Duration::from_millis(0))),
        sources,
        &config,
    )
    .await
}

/// A delete consuming a LOCAL-shaped preview's cache removes the requested tree
/// and nothing else. Delete is the one operation with no rollback, and the
/// cache binding is what stops a `preview_id` from authorizing work on a
/// different set of paths.
pub(super) async fn a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
) {
    let requested = dir.join("requested");
    let other = dir.join("other");
    seed(
        remote.as_ref(),
        &requested,
        &[
            ("r0.bin", b"R"),
            ("r1.bin", b"R"),
            ("deep/r2.bin", b"R"),
            ("deep/r3.bin", b"R"),
        ],
    )
    .await;
    seed(remote.as_ref(), &other, &[("untouched.txt", b"OTHER-untouched")]).await;
    let registered = Registered::new(&remote, "delete-preview");

    let preview_id = format!("delete-preview-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![requested.clone()], 4, 4);
    let result = delete_on(
        &remote,
        &registered.id,
        "delete-preview",
        std::slice::from_ref(&requested),
        Some(preview_id),
    )
    .await;
    assert!(result.is_ok(), "the delete should succeed: {result:?}");

    assert!(
        !remote.exists(&requested).await,
        "the requested tree survived the delete"
    );
    assert_eq!(
        try_read(remote.as_ref(), &other.join("untouched.txt")).await.as_deref(),
        Some(&b"OTHER-untouched"[..]),
        "❗ the delete removed a sibling it was never asked to touch"
    );

    clean_deep(remote.as_ref(), &dir).await;
    registered.leave();
}

/// A plain delete of a NON-EMPTY folder, a file beside it, and nothing else:
/// the server has no recursive delete, so the walker removes files first and
/// folders deepest-first. The siblings that weren't selected survive.
pub(super) async fn a_delete_of_a_non_empty_folder_takes_exactly_the_selection(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(
        remote.as_ref(),
        &dir,
        &[
            ("doomed/a.txt", b"a"),
            ("doomed/empty.txt", b""),
            ("doomed/nested/b.txt", b"b"),
            ("doomed/nested/deeper/c.txt", b"c"),
            ("loose.txt", b"loose"),
            ("kept/k.txt", b"KEPT"),
            ("kept.txt", b"KEPT-FILE"),
        ],
    )
    .await;
    remote
        .create_directory(&dir.join("doomed/empty-dir"))
        .await
        .expect("making an empty folder inside the doomed one");
    let registered = Registered::new(&remote, "delete-tree");

    let result = delete_on(
        &remote,
        &registered.id,
        "delete-tree",
        &[dir.join("doomed"), dir.join("loose.txt")],
        None,
    )
    .await;
    assert!(result.is_ok(), "the delete should succeed: {result:?}");

    assert_eq!(
        names_in(remote.as_ref(), &dir).await,
        vec!["kept".to_string(), "kept.txt".to_string()],
        "exactly the selection is gone"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("kept/k.txt")).await.as_deref(),
        Some(&b"KEPT"[..])
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("kept.txt")).await.as_deref(),
        Some(&b"KEPT-FILE"[..])
    );

    clean_deep(remote.as_ref(), &dir).await;
    registered.leave();
}

/// A local FILE onto a same-named FOLDER on the server, policy Overwrite, with
/// the source's type unanswerable. A wrong `false` makes the resolver see a
/// file→folder clash and reach for the recursive delete of the user's folder;
/// it must fail the item instead.
pub(super) async fn an_unknown_source_type_never_clears_a_server_folder(remote: Arc<dyn Volume>, dir: PathBuf) {
    let swap = dir.join("swap");
    seed(remote.as_ref(), &swap, &[("inner.txt", b"DEST-inner")]).await;

    let (local_dir, _) = local_volume("unknown-source-type");
    std::fs::write(local_dir.join("swap"), b"SRC-swap-file").expect("seeding the local file");
    let local = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
    let faulty = FaultyVolume::wrapping(local)
        .failing_call(FaultyOp::IsDirectory, 1, injected_read_failure())
        .arc();

    // The empty-`per_path` cache hit is what makes the probe happen at all: with
    // a real scan the preflight hands the resolver a confident answer and the
    // armed fault never fires, so this would assert the UNFAULTED behavior.
    let preview_id = format!("unknown-type-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from("swap")], 1, 13);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };

    let _ = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "unknown-source-type",
        &Arc::new(WriteOperationState::new(Duration::from_millis(0))),
        Arc::clone(&faulty) as Arc<dyn Volume>,
        &[PathBuf::from("swap")],
        Arc::clone(&remote),
        &dir,
        &config,
    )
    .await;

    assert!(
        faulty.fault_fired(FaultyOp::IsDirectory),
        "the source type was never probed, so this cell proved nothing about an unanswerable probe"
    );
    assert!(
        remote.is_directory(&swap).await.unwrap_or(false),
        "❗ the server's folder was replaced after an unanswerable source probe"
    );
    assert_eq!(
        try_read(remote.as_ref(), &swap.join("inner.txt")).await.as_deref(),
        Some(&b"DEST-inner"[..]),
        "❗ the server folder's contents were cleared after an unanswerable source probe"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// How big the file a download cancel interrupts is: more than two of any
/// backend's read chunks, so two permits can't finish it.
///
/// ❗ SMB streams in chunks of the server's `max_read_size`, 8 MiB on the fixture's
/// Samba, so anything at or under 16 MiB can leave the server in one or two
/// chunks and the "two chunks out, the third held" premise never holds.
const DOWNLOAD_BYTES: usize = 20 * 1024 * 1024;

/// A copy OFF the server stopped mid-file leaves local disk as it found it: the
/// user's filename never appears for bytes that didn't all arrive, and no
/// staging sibling survives.
///
/// The server's own read stream is released a chunk per permit, so "the cancel
/// landed mid-file" is a fact rather than a race against a fast link.
pub(super) async fn a_cancelled_download_leaves_nothing_behind(remote: Arc<dyn Volume>, dir: PathBuf) {
    remote
        .create_file(&dir.join("big.bin"), &self_describing_bytes(DOWNLOAD_BYTES, "download"))
        .await
        .expect("seeding the file on the server");
    let source = gated_reads(Arc::clone(&remote));
    let (local_dir, local) = local_volume("cancelled-download");

    let running = start_copy(
        "cancelled-download",
        Arc::clone(&source.volume),
        vec![dir.join("big.bin")],
        Arc::clone(&local),
        PathBuf::from(""),
        VolumeCopyConfig::default(),
    )
    .await;

    // Two chunks through, the third held: local disk now holds an open,
    // incomplete staging sibling. ❗ TWO, so the first is known to have been
    // taken by the destination rather than merely offered.
    source.gate.add_permits(2);
    crate::test_support::wait_until_async(budget(Duration::from_secs(6)), "two chunks to leave the server", || {
        source.handed_out.load(std::sync::atomic::Ordering::SeqCst) >= 2
    })
    .await;
    cancel_write_operation(&running.operation_id, false);
    source.gate.add_permits(10_000);
    running.settle().await;

    assert!(
        running.events.complete.lock_ignore_poison().is_empty(),
        "the cancel must land while the download is still running; it completed instead"
    );
    assert!(
        !running.events.cancelled.lock_ignore_poison().is_empty(),
        "a cancelled copy says so"
    );
    let left: Vec<String> = std::fs::read_dir(&*local_dir)
        .expect("listing local disk after the cancel")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(
        !left.iter().any(|n| n == "big.bin"),
        "❗ the user's filename must never appear for a download that didn't finish; local disk holds {left:?}"
    );
    assert_no_staging_litter(local.as_ref(), Path::new(""), "a cancelled download").await;

    clean_deep(remote.as_ref(), &dir).await;
}

// ── A name another writer takes mid-upload ───────────────────────────

/// An upload no backend takes single-shot, so it is staged and lands through
/// the no-replace rename: over SMB's compound limit (the negotiated
/// `max_write`, 8 MiB on the fixture's Samba).
pub(super) const STAGED_ON_EVERY_BACKEND: usize = 12 * 1024 * 1024;

/// An upload SMB lands single-shot, straight at the final name.
pub(super) const FITS_ONE_SMB_WRITE: usize = CANCEL_PAYLOAD_BYTES;

/// A name another writer takes WHILE our upload is on the wire is never
/// replaced, and our bytes don't vanish without a word.
///
/// The name was free when the copy looked, so nothing asked anybody about it.
/// What guards it is the landing: the staged bytes take their name with a
/// rename that must not replace (`rename(temp, final, false)`, and
/// `staged_write.rs`'s `LandingName::ExpectedFree` refuses to clear the way),
/// which the SERVER evaluates atomically: WebDAV `MOVE` with `Overwrite: F`,
/// SFTP's plain rename, SMB's non-replacing rename. A landing that checked
/// first and then renamed with replace would clobber the other writer's file in
/// the gap.
///
/// The upload is held mid-body by the gated source, so "the other writer got
/// there first" is a fact the cell arranges rather than a race it hopes to win.
///
/// `ours_len` picks the write path: [`STAGED_ON_EVERY_BACKEND`] is staged
/// everywhere, which is what the landing guard covers; a size a backend lands
/// single-shot (SMB under its negotiated `max_write`) goes straight to the
/// final name and never meets that guard.
pub(super) async fn a_name_taken_mid_upload_is_never_replaced(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
    policy: ConflictResolution,
    ours_len: usize,
) {
    let ours = self_describing_bytes(ours_len, "ours");
    let theirs = self_describing_bytes(30_000, "someone-elses");
    let source = gated_upload(ours.clone()).await;

    let running = start_copy(
        "name-taken-mid-upload",
        Arc::clone(&source.volume),
        vec![PathBuf::from("/big.bin")],
        Arc::clone(&remote),
        dir.clone(),
        VolumeCopyConfig {
            conflict_resolution: policy,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    // Two chunks in, so the upload is provably on the wire to its staging
    // sibling while the user's filename is still free.
    source.gate.add_permits(2);
    crate::test_support::wait_until_async(
        budget(Duration::from_secs(6)),
        "the upload to get two chunks in",
        || source.handed_out.load(std::sync::atomic::Ordering::SeqCst) >= 2,
    )
    .await;
    remote
        .create_file(&dir.join("big.bin"), &theirs)
        .await
        .expect("the other writer takes the free name");
    source.gate.add_permits(10_000);
    running.settle().await;

    let names = names_in(remote.as_ref(), &dir).await;
    let errors: Vec<String> = running
        .events
        .errors
        .lock_ignore_poison()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    let completed = !running.events.complete.lock_ignore_poison().is_empty();
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("big.bin"))
            .await
            .map(|b| sha256(&b)),
        Some(sha256(&theirs)),
        "❗ under {policy:?}, a file another writer put at the name mid-upload must survive our landing byte \
         for byte (completed: {completed}, errors: {errors:?}, the server holds {names:?})"
    );
    let mut ours_kept_at = None;
    for name in names.iter().filter(|n| n.as_str() != "big.bin") {
        if try_read(remote.as_ref(), &dir.join(name)).await.as_deref() == Some(ours.as_slice()) {
            ours_kept_at = Some(name.clone());
        }
    }
    assert!(
        ours_kept_at.is_some() || !errors.is_empty(),
        "under {policy:?}, our bytes either stay on the server or the copy says it couldn't place them; \
         it did neither (the server holds {names:?})"
    );
    log::info!(
        target: "test",
        "name taken mid-upload under {policy:?}: ours kept at {ours_kept_at:?}, errors {errors:?}, server holds {names:?}"
    );
    // A landing refused with nothing cleared takes its complete temp away at
    // once (the source still holds those bytes), so nothing of ours waits in
    // the folder for the hourly stale-temp reap.
    assert_no_staging_litter(remote.as_ref(), &dir, "a copy whose name was taken mid-upload").await;

    clean_deep(remote.as_ref(), &dir).await;
}
