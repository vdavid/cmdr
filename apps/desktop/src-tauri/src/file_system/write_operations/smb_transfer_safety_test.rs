//! Data-safety integration cells for the SMB backend, on a REAL share.
//!
//! The transfer-SEMANTICS cells (merge, same-share move, nested dest, compress,
//! volume replacement, the delete non-recursion contract) are
//! `smb_transfer_semantics_test.rs`, which also owns the fixtures these reuse. Same
//! gating: every test is `#[ignore]`d, so start the containers with
//! `./apps/desktop/test/smb-servers/start.sh` and run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::smb_test_support::*;
use cmdr_smb::volume::*;

// ============================================================================
// The four cells an in-memory double genuinely can't stand in for
// ============================================================================
//
// `write_operations/transfer/volume/safety_grid_tests.rs` covers the same axes
// against doubles, and says in its own doc comment why these four stay here: a
// real share publishes bytes at the write path, has real failure timing, and has
// a `create_directory` that really does signal a collision. "What a real backend
// does when asked to delete a non-empty directory" is the whole question in the
// last one, so a double answering it would be answering itself.
//
// All four drive the cache shape the production bug rode in on: a completed
// preview that counted files and recorded NO per-source result, which is what
// the LOCAL `std::fs` walk emitted for three months.

/// Reads a whole file off a volume, or `None` when it isn't there.
async fn try_read_volume_file(vol: &Arc<dyn Volume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// The read failure the two mid-op cells inject into the LOCAL source.
fn injected_read_failure() -> VolumeError {
    VolumeError::IoError {
        message: "Injected read failure".into(),
        raw_os_error: Some(5), // EIO
    }
}

/// Builds the local source tree both transfer cells use: `album` with three
/// files, one of which the fault will refuse to open.
fn local_album_source() -> (tempfile::TempDir, Arc<crate::file_system::volume::LocalPosixVolume>) {
    let dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::create_dir(dir.path().join("album")).expect("create the local album dir");
    std::fs::write(dir.path().join("album/one.bin"), vec![0xA1; 4096]).expect("seed album/one.bin");
    std::fs::write(dir.path().join("album/two.bin"), vec![0xA2; 4096]).expect("seed album/two.bin");
    std::fs::write(dir.path().join("album/three.bin"), vec![0xA3; 4096]).expect("seed album/three.bin");
    let vol = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        dir.path().to_path_buf(),
    ));
    (dir, vol)
}

/// Seeds the pre-existing destination tree on the share: an `album` the user
/// already had, holding files nothing in this operation may touch.
async fn seed_smb_album_with_sentinels(smb_vol: &Arc<SmbVolume>, base: &str) -> String {
    let album = format!("{base}/album");
    let sub = format!("{album}/sub");
    smb_vol
        .create_directory(Path::new(base))
        .await
        .expect("create the share base dir");
    smb_vol
        .create_directory(Path::new(&album))
        .await
        .expect("create the share album dir");
    smb_vol
        .create_directory(Path::new(&sub))
        .await
        .expect("create the share album subdir");
    smb_vol
        .create_file(Path::new(&format!("{album}/keep.txt")), b"DEST-keep")
        .await
        .expect("seed the album sentinel file");
    smb_vol
        .create_file(Path::new(&format!("{sub}/keep2.txt")), b"DEST-keep2")
        .await
        .expect("seed the subdir sentinel file");
    album
}

/// THE PRODUCTION BUG, ON THE WIRE. A local directory copied onto a same-named
/// directory that already exists on the share, with a cache hit whose `per_path`
/// is empty, forced to fail mid-copy.
///
/// That's the exact intersection: an empty `per_path` made the driver believe
/// the source was a FILE, and the cleanup guard keyed on that same belief then
/// swept the MERGED destination root. Against a real share the sweep would take
/// the user's files off the NAS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_failed_dir_copy_onto_a_merged_share_folder_keeps_the_users_files() {
    use crate::file_system::write_operations::{
        CollectorEventSink, ConflictResolution, FaultyOp, FaultyVolume, VolumeCopyConfig, WriteOperationState,
        copy_volumes_with_progress, seed_incoherent_scan_result_for_test,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();
    let album = seed_smb_album_with_sentinels(&smb_vol, &base).await;

    let (_local_dir, local_vol) = local_album_source();
    // The second read gives up: the copy dies with the merged destination root
    // already in play, which is the only way to reach the partial sweep.
    let source_vol: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&local_vol))
        .failing_call(FaultyOp::OpenReadStream, 2, injected_read_failure())
        .arc();

    let preview_id = format!("smb-safety-copy-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from("album")], 3, 12_288);

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events,
        "test-op-smb-safety-copy-fail",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("album")],
        Arc::clone(&dest_vol),
        Path::new(&base),
        &config,
    )
    .await;
    assert!(result.is_err(), "the injected read failure must fail the copy");

    // ❗ The share's pre-existing files survive at every depth.
    assert_eq!(
        try_read_volume_file(&dest_vol, &format!("{album}/keep.txt"))
            .await
            .as_deref(),
        Some(&b"DEST-keep"[..]),
        "a failed copy swept a dest-only file off the share"
    );
    assert_eq!(
        try_read_volume_file(&dest_vol, &format!("{album}/sub/keep2.txt"))
            .await
            .as_deref(),
        Some(&b"DEST-keep2"[..]),
        "a failed copy reached into the merged folder's subtree"
    );

    // And the source is intact: a copy never takes anything.
    let local: Arc<dyn Volume> = Arc::clone(&local_vol) as Arc<dyn Volume>;
    for name in ["one.bin", "two.bin", "three.bin"] {
        assert!(
            local.exists(Path::new(&format!("album/{name}"))).await,
            "a failed copy removed the source file album/{name}"
        );
    }

    ensure_clean(&smb_vol, &base).await;
}

/// The same cell for the cross-volume MOVE, where the stakes are higher: the
/// move's source sweep runs after the copy phase, so a wrong belief about what
/// landed costs the only remaining copy of the data.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_failed_dir_move_onto_a_merged_share_folder_loses_no_byte() {
    use crate::file_system::write_operations::{
        CollectorEventSink, ConflictResolution, FaultyOp, FaultyVolume, VolumeCopyConfig, WriteOperationState,
        move_volumes_with_progress, seed_incoherent_scan_result_for_test,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();
    let album = seed_smb_album_with_sentinels(&smb_vol, &base).await;

    let (_local_dir, local_vol) = local_album_source();
    let source_vol: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&local_vol))
        .failing_call(FaultyOp::OpenReadStream, 2, injected_read_failure())
        .arc();

    let preview_id = format!("smb-safety-move-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from("album")], 3, 12_288);

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events,
        "test-op-smb-safety-move-fail",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("album")],
        Arc::clone(&dest_vol),
        Path::new(&base),
        &config,
    )
    .await;
    assert!(result.is_err(), "the injected read failure must fail the move");

    // ❗ The share's pre-existing files survive.
    assert_eq!(
        try_read_volume_file(&dest_vol, &format!("{album}/keep.txt"))
            .await
            .as_deref(),
        Some(&b"DEST-keep"[..]),
        "a failed move swept a dest-only file off the share"
    );

    // ❗ NO BYTE LOST: every source file is readable from one side or the other.
    let local: Arc<dyn Volume> = Arc::clone(&local_vol) as Arc<dyn Volume>;
    for name in ["one.bin", "two.bin", "three.bin"] {
        let at_source = local.exists(Path::new(&format!("album/{name}"))).await;
        let at_dest = dest_vol.exists(Path::new(&format!("{album}/{name}"))).await;
        assert!(
            at_source || at_dest,
            "album/{name} is gone from BOTH sides after a failed move — data destroyed"
        );
    }

    ensure_clean(&smb_vol, &base).await;
}

/// An SMB delete consuming a LOCAL preview's cache: it must remove the requested
/// tree and nothing else. Delete is the one operation with no rollback, and the
/// cache binding is what stops a `preview_id` from authorizing work on a
/// different set of paths.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_delete_with_a_local_shaped_preview_removes_only_the_requested_tree() {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::write_operations::{
        CollectorEventSink, WriteOperationConfig, WriteOperationState, delete_volume_files_for_test,
        seed_incoherent_scan_result_for_test,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let volume: Arc<dyn Volume> = smb_vol.clone();

    let requested = format!("{base}/requested");
    let other = format!("{base}/other");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol.create_directory(Path::new(&requested)).await.unwrap();
    smb_vol.create_directory(Path::new(&other)).await.unwrap();
    for i in 0..4 {
        smb_vol
            .create_file(Path::new(&format!("{requested}/r{i}.bin")), b"requested")
            .await
            .unwrap();
    }
    smb_vol
        .create_file(Path::new(&format!("{other}/untouched.txt")), b"OTHER-untouched")
        .await
        .unwrap();

    let volume_id = format!("smb-safety-delete-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(&volume_id, Arc::clone(&volume));

    let preview_id = format!("smb-safety-delete-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from(&requested)], 4, 36);

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = Arc::new(CollectorEventSink::new());
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_volume_files_for_test(
        Arc::clone(&volume),
        &volume_id,
        events.as_ref(),
        "test-op-smb-safety-delete",
        &state,
        &[PathBuf::from(&requested)],
        &config,
    )
    .await;
    get_volume_manager().unregister(&volume_id);
    assert!(result.is_ok(), "the delete should succeed: {result:?}");

    // It did the work it was asked for...
    assert!(
        !volume.exists(Path::new(&format!("{requested}/r0.bin"))).await,
        "the requested tree survived the delete"
    );
    // ...and nothing outside the requested set is gone.
    assert_eq!(
        try_read_volume_file(&volume, &format!("{other}/untouched.txt"))
            .await
            .as_deref(),
        Some(&b"OTHER-untouched"[..]),
        "the delete removed a sibling it was never asked to touch"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// A cross-type clash where the SOURCE type is unknown, on the wire: a local
/// FILE onto a same-named DIRECTORY on the share, policy Overwrite, with the
/// source's `is_directory` failing.
///
/// This is the one combination the in-memory grid can't reach honestly, because
/// "what a real backend does when asked to delete a non-empty directory" is the
/// whole question. A wrong `false` here makes the resolver see a file→folder
/// clash and reach for the recursive delete of the user's share folder; the
/// resolver must fail the item instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_unknown_source_type_never_clears_a_share_folder() {
    use crate::file_system::write_operations::{
        CollectorEventSink, ConflictResolution, FaultyOp, FaultyVolume, VolumeCopyConfig, WriteOperationState,
        copy_volumes_with_progress, seed_incoherent_scan_result_for_test,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    // The share holds a DIRECTORY named `swap`, with a file inside it.
    let swap = format!("{base}/swap");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol.create_directory(Path::new(&swap)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{swap}/inner.txt")), b"DEST-inner")
        .await
        .unwrap();

    // The source holds a FILE of the same name, and can't answer what it is.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::write(local_dir.path().join("swap"), b"SRC-swap-file").unwrap();
    let local_vol = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));
    let faulty = Arc::new(FaultyVolume::wrapping(Arc::clone(&local_vol)).failing_call(
        FaultyOp::IsDirectory,
        1,
        injected_read_failure(),
    ));
    let source_vol: Arc<dyn Volume> = Arc::clone(&faulty) as Arc<dyn Volume>;

    // The empty-`per_path` cache hit is what makes the probe happen at all: with a
    // real scan the preflight hands the resolver a confident `is_directory: false`
    // and `resolve_volume_conflict` never asks the source. Then the armed fault
    // wouldn't fire and this cell would quietly assert the UNFAULTED behavior — a
    // plain cross-type Overwrite, which is documented to clear the folder.
    let preview_id = format!("smb-unknown-type-{}", uuid::Uuid::new_v4());
    seed_incoherent_scan_result_for_test(preview_id.clone(), vec![PathBuf::from("swap")], 1, 13);

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };

    let _ = copy_volumes_with_progress(
        events,
        "test-op-smb-unknown-source-type",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("swap")],
        Arc::clone(&dest_vol),
        Path::new(&base),
        &config,
    )
    .await;

    // The whole cell rests on the source being ASKED and refusing to answer.
    assert!(
        faulty.fault_fired(FaultyOp::IsDirectory),
        "the source type was never probed, so this cell proved nothing about an unanswerable probe"
    );

    // ❗ Whatever the operation decided, the user's folder and its contents are
    // still there. An unanswerable probe is never a licence to clear a folder.
    assert!(
        dest_vol.is_directory(Path::new(&swap)).await.unwrap_or(false),
        "the share's directory was replaced after an unanswerable source probe"
    );
    assert_eq!(
        try_read_volume_file(&dest_vol, &format!("{swap}/inner.txt"))
            .await
            .as_deref(),
        Some(&b"DEST-inner"[..]),
        "the share folder's contents were cleared after an unanswerable source probe"
    );

    ensure_clean(&smb_vol, &base).await;
}
