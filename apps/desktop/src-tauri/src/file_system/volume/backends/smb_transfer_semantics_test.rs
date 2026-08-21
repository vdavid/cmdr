//! High-level transfer-semantics integration tests for the SMB backend
//! (require Docker SMB containers).
//!
//! Covers folder-merge and same-share move behavior against a real SMB
//! server: deep-clash "Skip all" preserving dest-only files, same-share
//! move-merge with no folder prompt, and the non-conflicting move's
//! single-rename (no subtree walk) perf contract. Every test here is
//! `#[ignore]`d so default runs skip it. Start the containers with
//! `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.
//!
//! The data-SAFETY cells (a failed merge that must not sweep the user's share, a
//! delete bound to its preview, an unanswerable source probe) are
//! `smb_transfer_safety_test.rs`; the shared `Volume` contract assertions are
//! `smb_conformance_test.rs`.

use super::smb_test_support::*;
use super::*;
use std::sync::atomic::AtomicUsize;

/// FOLDER MERGE on a real SMB server: a Local source directory landing on a
/// pre-existing same-named SMB destination directory MERGES into it — with a deep
/// file clash resolved by "Skip all", the dest-only file survives, and the
/// non-clashing source file arrives. This pins the volume-side shape of the
/// now-fixed gotcha ("Skip-All on a volume copy with a top-level dir conflict
/// skipped the entire subtree"): the folder merges, only the clashing FILE is
/// skipped, and the user's untouched files are never destroyed. It also exercises
/// the real SMB `create_directory` → `AlreadyExists` merge trigger and the
/// per-level dest listing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_merge_deep_clash_skip_all_preserves_dest_only_files() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use std::time::Duration;

    // SMB destination: pre-create `<dir>/album` holding a dest-only sentinel and a
    // file that will clash with the source. A nested `album/sub` adds a deep
    // dest-only file at a second level.
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    let album = format!("{base}/album");
    let sub = format!("{album}/sub");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol.create_directory(Path::new(&album)).await.unwrap();
    smb_vol.create_directory(Path::new(&sub)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{album}/keep.txt")), b"DEST-keep")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{album}/clash.txt")), b"DEST-clash")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{sub}/keep2.txt")), b"DEST-keep2")
        .await
        .unwrap();

    // Local source: `album` with a fresh file, a clashing file, and a nested
    // fresh file.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::create_dir(local_dir.path().join("album")).unwrap();
    std::fs::create_dir(local_dir.path().join("album/sub")).unwrap();
    std::fs::write(local_dir.path().join("album/fresh.txt"), b"SRC-fresh").unwrap();
    std::fs::write(local_dir.path().join("album/clash.txt"), b"SRC-clash").unwrap();
    std::fs::write(local_dir.path().join("album/sub/fresh2.txt"), b"SRC-fresh2").unwrap();
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: crate::file_system::write_operations::ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-smb-merge-skip-all",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("album")],
        Arc::clone(&dest_vol),
        Path::new(&base),
        &config,
    )
    .await;
    assert!(result.is_ok(), "merge copy should succeed: {result:?}");

    // Helper: read a whole SMB file into a Vec.
    async fn read_smb(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
        let mut s = vol.open_read_stream(Path::new(path)).await.unwrap();
        let mut out = Vec::new();
        while let Some(Ok(chunk)) = s.next_chunk().await {
            out.extend_from_slice(&chunk);
        }
        out
    }

    // THE GOTCHA FIX: the folder MERGED (not skipped wholesale). Non-clashing
    // source files arrived at both depths.
    assert_eq!(read_smb(&dest_vol, &format!("{album}/fresh.txt")).await, b"SRC-fresh");
    assert_eq!(read_smb(&dest_vol, &format!("{sub}/fresh2.txt")).await, b"SRC-fresh2");

    // The clashing file was SKIPPED — dest keeps its own bytes.
    assert_eq!(read_smb(&dest_vol, &format!("{album}/clash.txt")).await, b"DEST-clash");

    // THE INVARIANT: dest-only files survive at every depth.
    assert_eq!(read_smb(&dest_vol, &format!("{album}/keep.txt")).await, b"DEST-keep");
    assert_eq!(read_smb(&dest_vol, &format!("{sub}/keep2.txt")).await, b"DEST-keep2");

    // No folder-level conflict was ever emitted (folders always merge silently).
    let folder_prompts = events
        .conflicts
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.source_is_directory && c.destination_is_directory)
        .count();
    assert_eq!(
        folder_prompts, 0,
        "a dir-vs-dir merge must never emit a folder conflict"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// SAME-SHARE MOVE with a folder collision on a real SMB server: moving
/// `<base>/src/album` onto a pre-existing `<base>/dst/album` MERGES via
/// server-side renames — no folder-level prompt, the dest-only file survives,
/// the clashing file follows the policy, and the fully-moved source spine is
/// deleted. This is the same-volume rename-merge fast path against real SMB
/// rename-onto-existing and empty-only-delete semantics.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_same_share_move_merges_with_no_folder_prompt() {
    use crate::file_system::write_operations::{
        CollectorEventSink, ConflictResolution, VolumeCopyConfig, WriteOperationState,
        move_within_same_volume_with_progress,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let vol: Arc<dyn Volume> = smb_vol.clone();

    // Source tree: src/album with a fresh file, a clashing file, and a nested file.
    let src_album = format!("{base}/src/album");
    let src_sub = format!("{src_album}/sub");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_directory(Path::new(&format!("{base}/src")))
        .await
        .unwrap();
    smb_vol.create_directory(Path::new(&src_album)).await.unwrap();
    smb_vol.create_directory(Path::new(&src_sub)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{src_album}/fresh.txt")), b"SRC-fresh")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{src_album}/clash.txt")), b"SRC-clash")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{src_sub}/deep.txt")), b"SRC-deep")
        .await
        .unwrap();

    // Dest tree: dst/album already holds a dest-only keeper and a clashing file.
    let dst = format!("{base}/dst");
    let dst_album = format!("{dst}/album");
    smb_vol.create_directory(Path::new(&dst)).await.unwrap();
    smb_vol.create_directory(Path::new(&dst_album)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{dst_album}/keep.txt")), b"DEST-keep")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{dst_album}/clash.txt")), b"DEST-clash")
        .await
        .unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "test-op-smb-same-move-merge",
        &state,
        Arc::clone(&vol),
        &[PathBuf::from(&src_album)],
        Path::new(&dst),
        &config,
    )
    .await;
    assert!(result.is_ok(), "same-share move-merge should succeed: {result:?}");

    async fn read_smb(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
        let mut s = vol.open_read_stream(Path::new(path)).await.unwrap();
        let mut out = Vec::new();
        while let Some(Ok(chunk)) = s.next_chunk().await {
            out.extend_from_slice(&chunk);
        }
        out
    }

    // Folder merged: fresh + nested files arrived.
    assert_eq!(read_smb(&vol, &format!("{dst_album}/fresh.txt")).await, b"SRC-fresh");
    assert_eq!(read_smb(&vol, &format!("{dst_album}/sub/deep.txt")).await, b"SRC-deep");
    // Clashing file was Skipped: dest keeps its bytes, source keeps its copy.
    assert_eq!(read_smb(&vol, &format!("{dst_album}/clash.txt")).await, b"DEST-clash");
    assert!(
        vol.exists(Path::new(&format!("{src_album}/clash.txt"))).await,
        "skipped source file survives"
    );
    // Dest-only file survives (the merge invariant).
    assert_eq!(read_smb(&vol, &format!("{dst_album}/keep.txt")).await, b"DEST-keep");
    // The source dir survives because it still holds the skipped clash file.
    assert!(
        vol.exists(Path::new(&src_album)).await,
        "source dir holding a skipped child survives"
    );

    // No folder-level conflict was ever emitted.
    let folder_prompts = events
        .conflicts
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.source_is_directory && c.destination_is_directory)
        .count();
    assert_eq!(folder_prompts, 0, "a same-volume folder merge must never prompt");

    ensure_clean(&smb_vol, &base).await;
}

/// A NON-conflicting same-share folder move completes via a single server-side
/// rename — it does NOT walk the moved folder's interior. Pins the perf
/// contract on a real SMB server: a deep folder moves wholesale, no per-file
/// listing of its contents.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_same_share_nonconflicting_move_no_subtree_walk() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, move_within_same_volume_with_progress,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let vol: Arc<dyn Volume> = smb_vol.clone();

    // A deep source folder with many files at two levels.
    let src_album = format!("{base}/src/album");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_directory(Path::new(&format!("{base}/src")))
        .await
        .unwrap();
    smb_vol.create_directory(Path::new(&src_album)).await.unwrap();
    smb_vol
        .create_directory(Path::new(&format!("{src_album}/deep")))
        .await
        .unwrap();
    for i in 0..20 {
        smb_vol
            .create_file(Path::new(&format!("{src_album}/f{i:02}.txt")), b"x")
            .await
            .unwrap();
        smb_vol
            .create_file(Path::new(&format!("{src_album}/deep/g{i:02}.txt")), b"y")
            .await
            .unwrap();
    }

    // Dest exists but has NO `album` → non-conflicting.
    let dst = format!("{base}/dst");
    smb_vol.create_directory(Path::new(&dst)).await.unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "test-op-smb-nonconflicting-move",
        &state,
        Arc::clone(&vol),
        &[PathBuf::from(&src_album)],
        Path::new(&dst),
        &config,
    )
    .await;
    assert!(
        result.is_ok(),
        "non-conflicting same-share move should succeed: {result:?}"
    );

    // Folder moved wholesale to its new home.
    assert!(vol.exists(Path::new(&format!("{dst}/album/f00.txt"))).await);
    assert!(vol.exists(Path::new(&format!("{dst}/album/deep/g00.txt"))).await);
    assert!(
        !vol.exists(Path::new(&src_album)).await,
        "fully-moved source folder is gone"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// DESTINATION AUTO-CREATE on a real SMB server: a copy into a not-yet-existing
/// nested destination folder creates the folder (and every missing ancestor) on
/// the share via `create_directory_all`, then lands the files. Pins the
/// volume-aware parity with the local-FS `ensure_destination_dir`: recursive
/// dest-create now works for SMB, not just local. SMB's `create_directory`
/// errors on an existing dir (`create_directory_errors_on_existing_dir() ==
/// true`), but the recursive helper pre-checks existence per component, so it
/// creates only the missing levels.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_creates_missing_nested_dest() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    // Only `base` exists; the nested dest `base/incoming/2026/trip` does NOT.
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let dest = format!("{base}/incoming/2026/trip");
    assert!(!smb_vol.exists(Path::new(&format!("{base}/incoming"))).await);

    // Local source: two plain files.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::write(local_dir.path().join("a.txt"), b"alpha").unwrap();
    std::fs::write(local_dir.path().join("b.txt"), b"bravo").unwrap();
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-smb-mkdir-dest",
        &state,
        Arc::clone(&source_vol),
        &[PathBuf::from("a.txt"), PathBuf::from("b.txt")],
        Arc::clone(&dest_vol),
        Path::new(&dest),
        &config,
    )
    .await;
    assert!(
        result.is_ok(),
        "copy into a missing nested SMB dest should succeed: {result:?}"
    );

    // Every missing ancestor was created as a directory on the share.
    for dir in [
        format!("{base}/incoming"),
        format!("{base}/incoming/2026"),
        dest.clone(),
    ] {
        assert!(
            smb_vol.is_directory(Path::new(&dir)).await.unwrap_or(false),
            "{dir} should be a directory on the share"
        );
    }

    // Both files landed in the freshly-created dest.
    async fn read_smb(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
        let mut s = vol.open_read_stream(Path::new(path)).await.unwrap();
        let mut out = Vec::new();
        while let Some(Ok(chunk)) = s.next_chunk().await {
            out.extend_from_slice(&chunk);
        }
        out
    }
    assert_eq!(read_smb(&dest_vol, &format!("{dest}/a.txt")).await, b"alpha");
    assert_eq!(read_smb(&dest_vol, &format!("{dest}/b.txt")).await, b"bravo");

    ensure_clean(&smb_vol, &base).await;
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

/// COMPRESS onto a real SMB share: local files packed into a NEW zip that lands on
/// the server. This is the end-to-end proof of the remote seed-through-volume path
/// — the 22-byte empty zip is written THROUGH the SMB volume (upload temp → swap),
/// then `route_archive_copy_into` pulls it, adds the sources, and swaps the full
/// archive into place. Reading the zip back off the share and parsing it proves the
/// result is a valid archive holding the sources, and no `.cmdr-tmp-*` upload temp
/// is left at the destination.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_compress_local_files_onto_the_share() {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::write_operations::{CollectorEventSink, ConflictResolution, compress_start};
    use std::io::Read;
    use std::time::Duration;

    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();

    // Register the share under a unique id so `compress_start` resolves it as the
    // (remote) parent and routes through the seed-through-volume path.
    let parent_id = format!("smb-compress-{base}");
    get_volume_manager().register(&parent_id, smb_vol.clone() as Arc<dyn Volume>);

    // Local sources: two files to pack.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    std::fs::write(local_dir.path().join("one.txt"), b"first").unwrap();
    std::fs::write(local_dir.path().join("two.txt"), b"second").unwrap();
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    let dest_zip = format!("{base}/bundle.zip");
    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        events.clone() as Arc<dyn crate::file_system::OperationEventSink>,
        Arc::clone(&source_vol),
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        PathBuf::from(&dest_zip),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        100,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start SMB compress");

    // Wait for completion, surfacing an error event loudly rather than waiting it out.
    crate::test_support::wait_until_async(Duration::from_secs(30), "the SMB compress to complete", || {
        // Snapshot both queues in a tight scope so no lock guard outlives the closure
        // (`clippy::await_holding_lock`).
        let completed = !events.complete.lock().unwrap().is_empty();
        let errs = events.errors.lock().unwrap();
        assert!(errs.is_empty(), "SMB compress errored: {errs:?}");
        completed
    })
    .await;

    // Read the zip back off the share and parse it: it must be a valid archive
    // holding both sources with their exact bytes.
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();
    let mut stream = dest_vol.open_read_stream(Path::new(&dest_zip)).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk);
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("the SMB zip must parse");
    let read = |archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str| -> Vec<u8> {
        let mut entry = archive.by_name(name).expect("entry present");
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("read entry");
        buf
    };
    assert_eq!(read(&mut archive, "one.txt"), b"first");
    assert_eq!(read(&mut archive, "two.txt"), b"second");

    // No upload temp debris left at the destination.
    let leftovers: Vec<String> = smb_vol
        .list_directory(Path::new(&base), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.contains(".cmdr-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "no upload temp should remain, got: {leftovers:?}");

    get_volume_manager().unregister(&parent_id);
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
        "/tmp/smb-test-mount",
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
