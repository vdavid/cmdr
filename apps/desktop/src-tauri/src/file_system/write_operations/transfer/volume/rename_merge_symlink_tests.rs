//! A symlink is an opaque entry to the same-volume rename-merge.
//!
//! `rename_merge_tests.rs` pins the easy half (a file symlink with nothing in
//! its way rides across on one rename). This suite pins the half that loses
//! data: a link whose TARGET is a directory, meeting a same-named directory at
//! the destination. The listing reports such a link as `is_directory: true`
//! (`listing/reading.rs`: `metadata.is_dir() || target_is_dir`), so a merge that
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

use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, local_volume, make_state, mkdir, read, write_file};
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Builds the shared fixture: a target directory holding one file, OUTSIDE both
/// `src` and `dst`, so anything that reaches it visibly reached past the move.
fn plant_target(root: &Path) {
    write_file(root, "outside/target/inside.txt", b"OUTSIDE THE SELECTION");
}

fn link(root: &Path, at: &str, to: &str) {
    std::os::unix::fs::symlink(root.join(to), root.join(at)).expect("symlink");
}

fn is_link(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

async fn run_merge(volume: &Arc<dyn Volume>, op_id: &str, resolution: ConflictResolution) -> Arc<CollectorEventSink> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
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

/// Skip: the link stays in the source and its target keeps every byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dir_link_child_meeting_a_real_dir_is_skipped_not_merged() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    write_file(root, "src/album/plain.txt", b"PLAIN");
    mkdir(root, "dst/album/link");

    run_merge(&volume, "op-merge-symlink-dir-skip", ConflictResolution::Skip).await;

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
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    mkdir(root, "dst/album/link");

    run_merge(&volume, "op-merge-symlink-dir-rename", ConflictResolution::Rename).await;

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
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    run_merge(&volume, "op-merge-symlink-dest-skip", ConflictResolution::Skip).await;

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

/// The same mirror under Overwrite, which is where the last "is this a
/// directory?" probe lives: `apply_child_decision` asks `Volume::is_directory`
/// about the write path, and that follows links too. Answering yes would merge
/// the source subtree INTO the link's target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_dir_child_overwriting_a_dir_link_never_lands_in_the_target() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    run_merge(
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

/// No clash at all: a link to a directory rides across on one rename, as a link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dir_link_child_with_no_clash_moves_as_a_link() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src/album");
    link(root, "src/album/link", "outside/target");
    mkdir(root, "dst/album");

    run_merge(&volume, "op-merge-symlink-dir-no-clash", ConflictResolution::Skip).await;

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
