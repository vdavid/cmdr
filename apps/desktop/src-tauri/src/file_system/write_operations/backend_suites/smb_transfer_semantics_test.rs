//! What a transfer MEANS on a real SMB server (require Docker SMB containers):
//! merges under every policy, moves in both directions, same-share moves and
//! copies, a destination addressed the way the transfer dialog addresses it, and
//! a copy that survives its volume being replaced mid-transfer.
//!
//! The backend-blind scenarios live in `network_semantics_test_support.rs`,
//! shared with `sftp_transfer_semantics_test.rs`; the last two cells here are
//! SMB's own. Every test is `#[ignore]`d so default runs skip it. Start the
//! containers with `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.
//!
//! The data-SAFETY cells are `smb_transfer_safety_test.rs`; the archive and
//! compress cells `smb_archive_integration_test.rs`.

use super::network_dates_test_support::{
    a_copy_off_the_server_keeps_the_source_date, a_copy_onto_the_server_keeps_the_source_date,
    copied_folders_off_the_server_keep_their_dates, copied_folders_onto_the_server_keep_their_dates,
};
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
    a_same_server_move_without_a_clash_moves_the_folder_whole, many_files_at_full_concurrency_land_intact,
    overwrite_smaller_replaces_only_the_smaller_destination,
};
use super::smb_test_support::*;
use cmdr_smb::volume::*;
use std::sync::atomic::AtomicUsize;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_merge_deep_clash_skip_all_preserves_dest_only_files() {
    let (remote, dir) = fixture().await;
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_deep_clash_merge_under_overwrite_replaces_only_the_clash() {
    let (remote, dir) = fixture().await;
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_rename_policy_lands_the_clash_beside_the_users_file() {
    let (remote, dir) = fixture().await;
    a_rename_policy_lands_the_clash_beside_the_users_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_overwrite_smaller_replaces_only_the_smaller_destination() {
    let (remote, dir) = fixture().await;
    overwrite_smaller_replaces_only_the_smaller_destination(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_move_merge_onto_the_share_spares_what_it_skipped() {
    let (remote, dir) = fixture().await;
    a_move_merge_onto_the_server_spares_what_it_skipped(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_folder_moved_onto_the_share_leaves_no_source() {
    let (remote, dir) = fixture().await;
    a_folder_moved_onto_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_folder_moved_off_the_share_leaves_no_source() {
    let (remote, dir) = fixture().await;
    a_folder_moved_off_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_file_saved_over_mid_move_off_the_share_stays() {
    let (remote, dir) = fixture().await;
    a_file_saved_over_mid_move_off_the_server_stays(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_file_added_mid_move_off_the_share_stays() {
    let (remote, dir) = fixture().await;
    a_file_added_mid_move_off_the_server_stays(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_same_share_move_merges_with_no_folder_prompt() {
    let (remote, dir) = fixture().await;
    a_same_server_move_merges_without_a_folder_prompt(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_same_share_nonconflicting_move_no_subtree_walk() {
    let (remote, dir) = fixture().await;
    a_same_server_move_without_a_clash_moves_the_folder_whole(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_same_share_copy_duplicates_a_tree() {
    let (remote, dir) = fixture().await;
    a_same_server_copy_duplicates_a_tree(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_creates_missing_nested_dest() {
    let (remote, dir) = fixture().await;
    a_copy_into_a_missing_nested_destination_makes_every_level(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_multi_megabyte_file_round_trips_byte_exact() {
    let (remote, dir) = fixture().await;
    a_multi_megabyte_file_round_trips_byte_exact(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_many_files_at_full_concurrency_round_trip_intact() {
    let (remote, dir) = fixture().await;
    many_files_at_full_concurrency_land_intact(remote, dir).await;
}

/// MOVE into a share SUBFOLDER addressed the way the transfer dialog addresses
/// it: a VOLUME-RELATIVE destination (`/<base>/photos`, leading slash and no
/// mount root, because the volume is a separate dropdown), anchored the way the
/// IPC boundary anchors it.
///
/// Every other test here spells its destination share-relative, which is why
/// none of them caught the shipped failure: unanchored, `to_smb_path` reads that
/// leading slash as absolute, finds the path outside the mount, and answers
/// `NotFound` before a single packet leaves the machine, so a 360 GB move died
/// in 2 ms reporting the DESTINATION as a missing source (ERR-XCP5Q). Anchoring
/// at the boundary is what makes the two dialects one path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_move_into_a_subfolder_addressed_the_way_the_dialog_addresses_it() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, move_volumes_with_progress,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    // The destination subfolder exists already, as `_todo_pics/…` did.
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_directory(Path::new(&format!("{base}/photos")))
        .await
        .unwrap();

    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::write(local_dir.path().join("clip.mp4"), b"footage").unwrap();
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    // What the dialog puts on the wire, and what the boundary makes of it.
    let dialog_dest = format!("/{base}/photos");
    let dest_path = cmdr_fs::volume::root_anchored(smb_vol.root(), Path::new(&dialog_dest));
    assert_eq!(
        dest_path,
        Path::new(&share_path(&format!("{base}/photos"))),
        "anchoring puts the mount root in front of the dialog's volume-relative path"
    );

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events.clone(),
        "test-op-smb-dialog-dest",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("clip.mp4")],
        Arc::clone(&dest_vol),
        &dest_path,
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(
        result.is_ok(),
        "a move into an existing share subfolder should succeed: {result:?}"
    );

    // The file is on the share, under the subfolder the dialog named, and the
    // move took the source with it.
    let mut stream = dest_vol
        .open_read_stream(Path::new(&format!("{base}/photos/clip.mp4")))
        .await
        .expect("the moved file is readable at the destination");
    let mut landed = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        landed.extend_from_slice(&chunk);
    }
    assert_eq!(landed, b"footage");
    assert!(
        !local_dir.path().join("clip.mp4").exists(),
        "a completed move leaves no source behind"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// THE regression, end to end on a real server: a running copy survives the
/// destination volume being replaced mid-transfer.
///
/// This is the 2026-08-01 failure verbatim. A 3 GB copy to the NAS was underway,
/// holding `Arc<dyn Volume>` clones of its source and destination
/// (`volume/copy.rs` clones them into every per-file task). A redundant SMB
/// upgrade pass fired, `register_replacing_predecessor` called `on_unmount` on
/// the predecessor, the smb2 session was dropped out from under the copy, and the
/// operation died with `DeviceDisconnected` on a connection that was demonstrably
/// healthy (`fs_info` and `stat` had completed normally moments earlier).
///
/// Here a genuine second connection replaces the volume through the same helper
/// the upgrade paths use, while the copy is in flight. The copy must finish, and
/// every byte must land.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_running_copy_survives_the_volume_being_replaced() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use crate::test_support::wait_until_async;
    use std::time::Duration;

    // Big enough that the copy is still running when the swap lands: the test
    // asserts that below, so an under-sized payload fails loudly rather than
    // passing vacuously.
    const FILE_COUNT: usize = 120;
    const FILE_BYTES: usize = 512 * 1024;

    let smb_vol = Arc::new(make_docker_volume().await);
    let volume_id = smb_vol.volume_id().to_string();
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();

    // Enough files that the replace lands in the middle rather than before or
    // after: each one is a separate round trip to the server.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    let mut names = Vec::new();
    for i in 0..FILE_COUNT {
        let name = format!("file-{i:03}.bin");
        // Distinct per-file byte so a mixed-up or truncated write can't pass.
        std::fs::write(local_dir.path().join(&name), vec![i as u8; FILE_BYTES]).unwrap();
        names.push(PathBuf::from(&name));
    }
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    // What the running operation holds: an `Arc` taken before the swap.
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(&volume_id, Arc::clone(&dest_vol));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();
    let dest = base.clone();

    let copy = tokio::spawn({
        let source_vol = Arc::clone(&source_vol);
        let dest_vol = Arc::clone(&dest_vol);
        let dest = dest.clone();
        async move {
            copy_volumes_with_progress(
                events,
                "test-op-smb-replace-mid-copy",
                &state,
                source_vol,
                &names,
                dest_vol,
                Path::new(&dest),
                &config,
            )
            .await
        }
    });

    // Wait until the copy is genuinely under way (some files landed) but not
    // finished, so the replace hits it mid-flight.
    // A background probe feeds an atomic that the (sync) wait condition reads;
    // the condition can't await, and blocking on a future inside the runtime
    // would deadlock.
    let landed = Arc::new(AtomicUsize::new(0));
    let probe = tokio::spawn({
        let vol = Arc::clone(&smb_vol);
        let dest = dest.clone();
        let landed = Arc::clone(&landed);
        async move {
            loop {
                if let Ok(entries) = vol.list_directory(Path::new(&dest), None).await {
                    landed.store(entries.len(), Ordering::Relaxed);
                }
                // allowed-test-sleep: the probe's own polling interval; the test's
                // WAIT is `wait_until_async` on the atomic this feeds.
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    });
    wait_until_async(Duration::from_secs(60), "the copy to start landing files", || {
        landed.load(Ordering::Relaxed) > 0
    })
    .await;
    probe.abort();

    // The swap, through the exact helper every upgrade path uses. A real second
    // smb2 connection takes the volume id.
    let in_flight_at_swap = landed.load(Ordering::Relaxed);
    let successor = connect_smb_volume(
        "public",
        MountAnchor::at_share_root("/tmp/smb-test-mount"),
        &volume_id,
        docker_guest_params(),
        crate::volume_host::host(),
    )
    .await
    .expect("second connection to the Docker SMB container");
    crate::network::smb_upgrade::register_replacing_predecessor(&volume_id, Arc::new(successor)).await;

    assert!(
        in_flight_at_swap < FILE_COUNT,
        "the swap has to land MID-copy or this test proves nothing; already landed: {in_flight_at_swap} of {FILE_COUNT}"
    );

    let result = copy.await.expect("the copy task itself must not panic");
    assert!(
        result.is_ok(),
        "a copy must survive its volume being replaced: an upgrade is not a disconnect. Got {result:?}"
    );

    // Every file landed, whole and correct. A dropped session mid-copy would
    // leave the tail missing or truncated.
    let entries = smb_vol.list_directory(Path::new(&base), None).await.unwrap();
    assert_eq!(entries.len(), FILE_COUNT, "every file must have landed");
    for i in [0usize, FILE_COUNT / 2, FILE_COUNT - 1] {
        let path = format!("{base}/file-{i:03}.bin");
        let mut stream = dest_vol.open_read_stream(Path::new(&path)).await.unwrap();
        let mut out = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            out.extend_from_slice(&chunk);
        }
        assert_eq!(out, vec![i as u8; FILE_BYTES], "{path} must be byte-identical");
    }

    manager.unregister(&volume_id);
    ensure_clean(&smb_vol, &base).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_onto_a_share_keeps_the_source_date() {
    let (remote, dir) = fixture().await;
    a_copy_onto_the_server_keeps_the_source_date(remote, dir, Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_off_a_share_keeps_the_source_date() {
    let (remote, dir) = fixture().await;
    a_copy_off_the_server_keeps_the_source_date(remote, dir, Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_folders_copied_onto_a_share_keep_their_dates() {
    let (remote, dir) = fixture().await;
    copied_folders_onto_the_server_keep_their_dates(remote, dir, Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_folders_copied_off_a_share_keep_their_dates() {
    let (remote, dir) = fixture().await;
    copied_folders_off_the_server_keep_their_dates(remote, dir).await;
}
