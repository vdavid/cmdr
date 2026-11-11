//! A symlink is an opaque entry to the same-volume rename-merge.
//!
//! `rename_merge_tests.rs` pins the easy half (a file symlink with nothing in
//! its way rides across on one rename). This suite pins the half that loses
//! data: a link whose TARGET is a directory, meeting a same-named directory at
//! the destination. The listing reports such a link as `is_directory: true`
//! (`cmdr-local`'s `reading.rs`: `metadata.is_dir() || target_is_dir`), so a merge that
//! believes it descends into the link, lists the target, and renames the
//! target's real children out of a folder the user never selected.
//!
//! `FileEntry::is_symlink` is the answer, and the rule is the local engines':
//! a link is a LEAF, and a link facing a real directory is a cross-type clash
//! for the file policy. `transfer/DETAILS.md` § "Symlinks are opaque to a move".
//!
//! Runs on the real `LocalPosixVolume` rig (`rename_merge_test_support.rs`), the
//! only one where a symlink is a symlink.

#![cfg(unix)]

use super::super::conflict_responder_test_support::ConflictResponderSink;
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, local_volume, make_state, mkdir, read, write_file};
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{ConflictResolution, VolumeCopyConfig};
use cmdr_fs::volume::Volume;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Builds the shared fixture: a target directory holding one file, OUTSIDE both
/// `src` and `dst`, so anything that reaches it visibly reached past the move.
fn plant_target(root: &Path) {
    write_file(root, "outside/target/inside.txt", b"OUTSIDE THE SELECTION");
}

fn link(root: &Path, at: &str, to: &str) {
    cmdr_fs::testing::symlink(root.join(to), root.join(at)).expect("symlink");
}

fn is_link(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

async fn run_merge(
    engine: &TestOperations,
    volume: &Arc<dyn Volume>,
    op_id: &str,
    resolution: ConflictResolution,
) -> Arc<CollectorEventSink> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(engine);
    let config = VolumeCopyConfig {
        conflict_resolution: resolution,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let result = move_within_same_volume_with_progress(
        events.clone(),
        op_id,
        &state,
        Arc::clone(volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "{op_id}: expected Ok, got {:?}", result);
    events
}

/// The same merge under Stop, with every prompt answered `resolution` for that
/// one pair — a person clicking, rather than a policy picked once for the whole
/// transfer. The two are no longer interchangeable: a blanket Overwrite refuses
/// a cross-type clash, so this is how the destructive branch gets exercised.
async fn run_merge_answering(
    engine: &TestOperations,
    volume: &Arc<dyn Volume>,
    op_id: &str,
    resolution: ConflictResolution,
) {
    let state = make_state(engine);
    let events = Arc::new(ConflictResponderSink::new(&state, resolution, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let result = move_within_same_volume_with_progress(
        events,
        op_id,
        &state,
        Arc::clone(volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "{op_id}: expected Ok, got {:?}", result);
}

/// Skip: the link stays in the source and its target keeps every byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dir_link_child_meeting_a_real_dir_is_skipped_not_merged() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    write_file(root, "src/album/plain.txt", b"PLAIN");
    mkdir(root, "dst/album/link");

    run_merge(&engine, &volume, "op-merge-symlink-dir-skip", ConflictResolution::Skip).await;

    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "a merge must never rename entries out of a symlink's target"
    );
    assert!(
        is_link(root, "src/album/link"),
        "the skipped link stays in the source, still a link"
    );
    assert_eq!(
        std::fs::read_dir(root.join("dst/album/link")).unwrap().count(),
        0,
        "the destination's real directory stays empty"
    );
    assert_eq!(read(root, "dst/album/plain.txt"), b"PLAIN", "the sibling still moves");
}

/// Rename: the link lands beside the destination directory, as a link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dir_link_child_meeting_a_real_dir_lands_aside_on_rename() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    mkdir(root, "dst/album/link");

    run_merge(
        &engine,
        &volume,
        "op-merge-symlink-dir-rename",
        ConflictResolution::Rename,
    )
    .await;

    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "the link's target is untouched"
    );
    assert!(
        is_link(root, "dst/album/link (1)"),
        "the incoming link lands aside AS a link"
    );
    assert_eq!(
        std::fs::read_link(root.join("dst/album/link (1)")).unwrap(),
        root.join("outside/target"),
        "and still points where it always did"
    );
    assert_eq!(
        std::fs::read_dir(root.join("dst/album/link")).unwrap().count(),
        0,
        "the destination's real directory is kept, untouched"
    );
}

/// The mirror: a real source directory meeting a LINK at the destination. A
/// merge here renames the source's files THROUGH the link, into a folder the
/// user never chose as the destination.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_dir_child_meeting_a_dir_link_at_the_destination_is_not_merged() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    run_merge(&engine, &volume, "op-merge-symlink-dest-skip", ConflictResolution::Skip).await;

    assert!(
        !exists(root, "outside/target/mine.txt"),
        "nothing may land inside the destination link's target"
    );
    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert_eq!(
        read(root, "src/album/sub/mine.txt"),
        b"MINE",
        "the skipped source directory keeps its file"
    );
    assert!(is_link(root, "dst/album/sub"), "the destination link is untouched");
}

/// The same mirror under an Overwrite a person answered for THIS pair, which is
/// where the last "is this a directory?" probe lives: `apply_child_decision`
/// asks `Volume::is_directory` about the write path, and that follows links too.
/// Answering yes would merge the source subtree INTO the link's target.
///
/// Driven through a Stop prompt rather than the Overwrite policy, because a
/// BLANKET Overwrite refuses a cross-type clash outright (the cell below) and
/// would never reach the probe this pins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_dir_child_overwriting_a_dir_link_never_lands_in_the_target() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    run_merge_answering(
        &engine,
        &volume,
        "op-merge-symlink-dest-overwrite",
        ConflictResolution::Overwrite,
    )
    .await;

    assert!(
        !exists(root, "outside/target/mine.txt"),
        "an Overwrite must replace the link, never merge through it"
    );
    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "the link's target keeps its own file"
    );
    assert_eq!(
        read(root, "dst/album/sub/mine.txt"),
        b"MINE",
        "the source subtree lands at the destination itself"
    );
}

/// The blanket policy's answer to the same pair: a link is a leaf, so this is a
/// cross-type clash, and `Overwrite` picked once for a whole transfer never
/// replaces one kind of entry with another. Both sides stay as they were, and
/// the source subtree — the only copy of it, on a move — stays home.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_leaves_a_dir_link_and_the_source_dir_alone() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    run_merge(
        &engine,
        &volume,
        "op-merge-symlink-dest-blanket-overwrite",
        ConflictResolution::Overwrite,
    )
    .await;

    assert!(
        !exists(root, "outside/target/mine.txt"),
        "nothing may land inside the destination link's target"
    );
    assert!(
        is_link(root, "dst/album/sub"),
        "the destination link must survive a blanket Overwrite"
    );
    assert_eq!(
        read(root, "src/album/sub/mine.txt"),
        b"MINE",
        "the refused source directory keeps its file"
    );
}

/// No clash at all: a link to a directory rides across on one rename, as a link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dir_link_child_with_no_clash_moves_as_a_link() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    mkdir(root, "dst/album");

    run_merge(
        &engine,
        &volume,
        "op-merge-symlink-dir-no-clash",
        ConflictResolution::Skip,
    )
    .await;

    assert!(
        is_link(root, "dst/album/link"),
        "the link moves as a link, not as a copy of its target"
    );
    assert_eq!(
        std::fs::read_link(root.join("dst/album/link")).unwrap(),
        root.join("outside/target")
    );
    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
}

// ---------------------------------------------------------------------------
// The TOP-LEVEL source is the link (#140). The preflight scan and the
// resolver's type hint both answered "directory" for it, so the move merged
// the link's target into the destination folder.
// ---------------------------------------------------------------------------

/// Skip: a selected link meeting a real folder is a type mismatch, so the
/// policy leaves both where they are and the target keeps every byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_dir_link_meeting_a_real_dir_is_skipped_not_merged() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    mkdir(root, "dst/album");

    run_merge(&engine, &volume, "op-top-symlink-dir-skip", ConflictResolution::Skip).await;

    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "a move must never carry entries out of a selected link's target"
    );
    assert!(
        is_link(root, "src/album"),
        "the skipped link stays in the source, still a link"
    );
    assert_eq!(
        std::fs::read_dir(root.join("dst/album")).unwrap().count(),
        0,
        "the destination's real directory stays empty"
    );
}

/// Rename: the selected link lands beside the destination folder, as a link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_dir_link_meeting_a_real_dir_lands_aside_on_rename() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    mkdir(root, "dst/album");

    run_merge(
        &engine,
        &volume,
        "op-top-symlink-dir-rename",
        ConflictResolution::Rename,
    )
    .await;

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(
        is_link(root, "dst/album (1)"),
        "the incoming link lands aside AS a link"
    );
    assert_eq!(
        std::fs::read_link(root.join("dst/album (1)")).unwrap(),
        root.join("outside/target")
    );
    assert_eq!(std::fs::read_dir(root.join("dst/album")).unwrap().count(), 0);
}

/// A blanket Overwrite never crosses types, so both sides stay put.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_leaves_a_top_level_dir_link_and_the_dest_dir_alone() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    write_file(root, "dst/album/theirs.txt", b"THEIRS");

    run_merge(
        &engine,
        &volume,
        "op-top-symlink-dir-blanket-overwrite",
        ConflictResolution::Overwrite,
    )
    .await;

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(
        !exists(root, "dst/album/inside.txt"),
        "nothing from the target may land at the destination"
    );
    assert_eq!(read(root, "dst/album/theirs.txt"), b"THEIRS");
    assert!(is_link(root, "src/album"));
}

/// No clash: the selected link rides across on one rename, as a link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_dir_link_with_no_clash_moves_as_a_link() {
    let engine = TestOperations::new();
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    mkdir(root, "dst");

    run_merge(
        &engine,
        &volume,
        "op-top-symlink-dir-no-clash",
        ConflictResolution::Skip,
    )
    .await;

    assert!(is_link(root, "dst/album"), "the link moves as a link");
    assert!(!is_link(root, "src/album") && !exists(root, "src/album"));
    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
}

/// The same clash reached with a TransferDialog preview in hand, which is how a
/// move from the dialog arrives. A copy scan follows the link (it answers "does
/// this stream as a folder"), so the preview calls the link a directory; the
/// move must still take it for the leaf it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_dir_link_is_not_merged_when_a_preview_scanned_it_as_a_folder() {
    use super::super::super::scan_cache::CachedScanResult;

    let engine = TestOperations::new();

    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    mkdir(root, "dst/album");

    let sources = vec![PathBuf::from("src/album")];
    let batch = volume.scan_for_copy_batch(&sources).await.expect("preview scan");
    assert!(
        batch.per_path[0].1.top_level_is_directory,
        "precondition: the preview's scan follows the link"
    );
    let preview_id = "preview-top-symlink-dir".to_string();
    engine.stores().previews().insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_volume_batch(
            crate::types::ScanPreviewPurpose::Transfer,
            sources.clone(),
            batch.aggregate.file_count,
            batch.aggregate.total_bytes,
            batch.aggregate.dedup_bytes,
            batch.per_path,
        ),
    );

    let state = make_state(&engine);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };
    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-top-symlink-dir-preview",
        &state,
        Arc::clone(&volume),
        &sources,
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "a preview's answer must never turn a link into a folder to merge"
    );
    assert!(is_link(root, "src/album"));
    assert_eq!(std::fs::read_dir(root.join("dst/album")).unwrap().count(), 0);
}
