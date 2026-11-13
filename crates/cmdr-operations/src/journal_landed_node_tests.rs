//! Which node each landed item's row records, through the real copy and move
//! bodies on a real disk, read back from the journal once the operation is over.
//!
//! The recorded identity has to be the node an `lstat` finds at the row's
//! destination AFTER the operation finished: everything the engine does to a file
//! once it lands (its dates, flags, a staged move's second rename) must leave that
//! pair alone, or every undo would read the file as a stranger. So each fixture
//! source carries an old modified time, the case where a filesystem that lowers a
//! birth time to an earlier modified time would show.
//!
//! The scratch disk decides what "records" means: where its filesystem keeps a
//! lasting identity (APFS on a Mac, ext4 on a Linux box) the row carries the node,
//! elsewhere (a container's overlay) it carries none, and `expected_at` follows it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::CollectorEventSink;
use super::journal;
use super::state::WriteOperationState;
use super::transfer::copy::copy_files_with_progress_inner;
use super::transfer::move_op::move_files_with_progress_inner;
use super::types::{ConflictResolution, VolumeCopyConfig, WriteOperationConfig};
use super::{copy_volumes_with_progress, move_volumes_with_progress};
use crate::testing::{TestOperationLog, TestOperations};

use cmdr_fs::volume::{InMemoryVolume, NodeIdentity, Volume};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::store::{RollbackUnit, open_read_connection, read_rollback_units_page};
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind};

const OLD: i64 = 1_500_000_000;

fn journaled() -> (TestOperations, TestOperationLog) {
    let log = TestOperationLog::new("journal-landed-node");
    (TestOperations::journaling_into(&log), log)
}

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::ZERO))
}

/// A fixture file with an old modified time.
fn write_old(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("mk parents");
    std::fs::write(path, bytes).expect("write fixture");
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(OLD, 0)).expect("pin mtime");
}

fn node_at(path: &Path) -> NodeIdentity {
    NodeIdentity::of(&std::fs::symlink_metadata(path).expect("the item is there")).expect("the disk keeps a birth time")
}

/// What the row of an item now at `path` should carry: its node where the scratch
/// disk keeps a lasting identity, nothing elsewhere.
fn expected_at(path: &Path) -> Option<NodeIdentity> {
    crate::local_disk::keeps_node_identity(path).then(|| node_at(path))
}

/// A Mac's scratch folder is on APFS, so there these cells always exercise a
/// recorded node, never the "records none" arm alone.
#[cfg(target_os = "macos")]
#[test]
fn the_scratch_disk_on_a_mac_keeps_a_lasting_identity() {
    let work = tempfile::tempdir().expect("work");
    assert!(crate::local_disk::keeps_node_identity(work.path()));
}

fn units(log: &TestOperationLog, op_id: &str) -> Vec<RollbackUnit> {
    let conn = open_read_connection(log.db()).expect("read conn");
    read_rollback_units_page(&conn, op_id, i64::MAX, 1_000).expect("units")
}

fn unit_landing_at<'a>(units: &'a [RollbackUnit], path: &Path) -> &'a RollbackUnit {
    units
        .iter()
        .find(|unit| unit.dest_path.as_deref() == Some(path))
        .unwrap_or_else(|| panic!("a row lands at {}: {units:?}", path.display()))
}

fn local_copy(engine: &TestOperations, op_id: &str, source: &Path, dest: &Path, config: &WriteOperationConfig) {
    let st = state(engine);
    journal::open_local_op(st.scope(), op_id, OpKind::Copy, Initiator::User, 0, Some("root"));
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        op_id,
        &st,
        &[source.to_path_buf()],
        dest,
        config,
    )
    .expect("copy");
    journal::finalize_op(st.scope(), OpKind::Copy, ExecutionStatus::Done);
}

fn local_move(engine: &TestOperations, op_id: &str, source: &Path, dest: &Path) {
    let st = state(engine);
    journal::open_local_op(st.scope(), op_id, OpKind::Move, Initiator::User, 0, Some("root"));
    move_files_with_progress_inner(
        &CollectorEventSink::new(),
        op_id,
        &st,
        &[source.to_path_buf()],
        dest,
        &WriteOperationConfig::default(),
    )
    .expect("move");
    journal::finalize_op(st.scope(), OpKind::Move, ExecutionStatus::Done);
}

// ── The local copy ───────────────────────────────────────────────────────────

#[test]
fn a_local_copy_records_the_node_each_file_landed_as() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("album");
    write_old(&src.join("a.jpg"), b"aaa");
    write_old(&src.join("sub/b.jpg"), b"bbbb");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    local_copy(&engine, "op", &src, &dst, &WriteOperationConfig::default());

    let units = units(&log, "op");
    for landed in [dst.join("album/a.jpg"), dst.join("album/sub/b.jpg")] {
        let unit = unit_landing_at(&units, &landed);
        assert_eq!(unit.dest_identity, expected_at(&landed), "{}", landed.display());
        assert_ne!(
            unit.dest_identity.map(|node| node.inode),
            Some(node_at(&src.join(landed.strip_prefix(dst.join("album")).expect("under the copy"))).inode),
            "the copy's node, never its original's"
        );
    }
    // A folder the copy created names no node: its row is source == dest, and the
    // undo removes it only when it's empty.
    let folder = unit_landing_at(&units, &dst.join("album"));
    assert_eq!(folder.entry_type, EntryType::Dir);
    assert_eq!(folder.dest_identity, None);
}

/// An overwrite lands through a temp and a rename, so the node at the destination
/// is a new one: the row names it, never the file it replaced.
#[test]
fn an_overwriting_copy_records_the_node_that_replaced_the_old_file() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("a.txt");
    write_old(&src, b"new");
    let dst = work.path().join("dst");
    write_old(&dst.join("a.txt"), b"old");
    let replaced = node_at(&dst.join("a.txt"));

    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..Default::default()
    };
    local_copy(&engine, "op", &src, &dst, &config);

    let landed = dst.join("a.txt");
    assert_eq!(std::fs::read(&landed).expect("read"), b"new");
    let units = units(&log, "op");
    let unit = unit_landing_at(&units, &landed);
    assert_eq!(unit.dest_identity, expected_at(&landed));
    assert_ne!(node_at(&landed), replaced, "the overwrite put a new node there");
}

#[cfg(unix)]
#[test]
fn a_copied_link_records_the_link_never_what_it_points_at() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let target = work.path().join("target.txt");
    write_old(&target, b"target");
    let link = work.path().join("link");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    local_copy(&engine, "op", &link, &dst, &WriteOperationConfig::default());

    let landed = dst.join("link");
    let units = units(&log, "op");
    let unit = unit_landing_at(&units, &landed);
    assert_eq!(unit.dest_identity, expected_at(&landed));
    assert_ne!(node_at(&landed), node_at(&target));
}

/// A destination whose filesystem keeps no lasting identity records none, and the
/// copy is the same copy. The scope's answer is settled before the copy asks, the
/// way a FAT stick or a mounted share would settle it.
#[test]
fn a_copy_onto_a_filesystem_with_no_lasting_identity_records_none() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("a.txt");
    write_old(&src, b"aaa");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let st = state(&engine);
    journal::open_local_op(st.scope(), "op", OpKind::Copy, Initiator::User, 0, Some("root"));
    assert!(!st.scope().records_landed_nodes(|| false));
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op",
        &st,
        std::slice::from_ref(&src),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("copy");
    journal::finalize_op(st.scope(), OpKind::Copy, ExecutionStatus::Done);

    assert_eq!(std::fs::read(dst.join("a.txt")).expect("read"), b"aaa");
    let units = units(&log, "op");
    assert_eq!(unit_landing_at(&units, &dst.join("a.txt")).dest_identity, None);
}

/// An operation that journals nothing never asks about the destination at all.
#[test]
fn an_unjournaled_operation_never_probes_the_destination() {
    let engine = TestOperations::new();
    let st = state(&engine);
    let recorded = st
        .scope()
        .records_landed_nodes(|| panic!("nothing is journaled, so nothing is asked"));
    assert!(!recorded);
}

// ── The local move ───────────────────────────────────────────────────────────

/// A rename carries the node across, so the row names the very node that was moved,
/// for a file and for a folder (whose one row stands for its whole subtree).
#[test]
fn a_same_filesystem_move_records_the_node_it_moved() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let file = work.path().join("notes.txt");
    write_old(&file, b"notes");
    let folder = work.path().join("album");
    write_old(&folder.join("a.jpg"), b"aaa");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");
    let (file_before, folder_before) = (node_at(&file), node_at(&folder));

    local_move(&engine, "op-file", &file, &dst);
    local_move(&engine, "op-folder", &folder, &dst);

    let moved_file = dst.join("notes.txt");
    assert_eq!(node_at(&moved_file), file_before, "a rename keeps the node");
    assert_eq!(
        unit_landing_at(&units(&log, "op-file"), &moved_file).dest_identity,
        expected_at(&moved_file)
    );
    let moved_folder = dst.join("album");
    assert_eq!(node_at(&moved_folder), folder_before);
    assert_eq!(
        unit_landing_at(&units(&log, "op-folder"), &moved_folder).dest_identity,
        expected_at(&moved_folder)
    );
}

/// A folder merged into one that was already there didn't land: the folder at the
/// destination is the one that was there before, so its row claims no node.
#[test]
fn a_merged_folder_move_records_no_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let folder = work.path().join("album");
    write_old(&folder.join("new.jpg"), b"new");
    let dst = work.path().join("dst");
    write_old(&dst.join("album/kept.jpg"), b"kept");

    local_move(&engine, "op", &folder, &dst);

    assert!(dst.join("album/new.jpg").exists() && dst.join("album/kept.jpg").exists());
    assert_eq!(
        unit_landing_at(&units(&log, "op"), &dst.join("album")).dest_identity,
        None
    );
}

// ── The volume engine ────────────────────────────────────────────────────────

fn in_memory(name: &str) -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new(name).with_space_info(1_000_000, 900_000))
}

fn local_disk(root: &Path) -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        "Disk",
        root,
        crate::host::testing::pane_order_host(),
    ))
}

async fn share_with_an_album() -> Arc<InMemoryVolume> {
    let share = in_memory("Share");
    share.create_directory(Path::new("/album")).await.expect("album");
    share.create_file(Path::new("/album/a.jpg"), b"aaa").await.expect("a");
    share
        .create_file(Path::new("/notes.txt"), b"notes")
        .await
        .expect("notes");
    share.set_modified_at(Path::new("/album/a.jpg"), Some(OLD as u64));
    share.set_modified_at(Path::new("/notes.txt"), Some(OLD as u64));
    share
}

/// A copy, or a move, off a share onto a local disk: `kind` picks which.
async fn volume_transfer(
    engine: &TestOperations,
    op_id: &str,
    kind: OpKind,
    source: Arc<dyn Volume>,
    sources: &[PathBuf],
    dest: Arc<dyn Volume>,
    dest_path: &Path,
) {
    let state = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_journal_volumes("share".into(), "disk".into()),
    );
    journal::open_volume_op(state.scope(), op_id, kind, Initiator::User, "share", Some("disk"), 0);
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();
    if kind == OpKind::Move {
        move_volumes_with_progress(events, op_id, &state, source, sources, dest, dest_path, &config)
            .await
            .expect("volume move");
    } else {
        copy_volumes_with_progress(events, op_id, &state, source, sources, dest, dest_path, &config)
            .await
            .expect("volume copy");
    }
    journal::finalize_op(state.scope(), kind, ExecutionStatus::Done);
}

/// A file source and a folder source, copied off a share onto a local disk: each
/// landed file's row names its node, read after its dates went on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_from_a_share_onto_a_local_disk_records_each_landed_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let share = share_with_an_album().await;

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        share as Arc<dyn Volume>,
        &[PathBuf::from("/album"), PathBuf::from("/notes.txt")],
        local_disk(work.path()),
        work.path(),
    )
    .await;

    let units = units(&log, "op");
    for landed in [work.path().join("album/a.jpg"), work.path().join("notes.txt")] {
        let unit = unit_landing_at(&units, &landed);
        assert_eq!(unit.dest_identity, expected_at(&landed), "{}", landed.display());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_from_a_share_onto_a_local_disk_records_each_landed_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let share = share_with_an_album().await;

    volume_transfer(
        &engine,
        "op",
        OpKind::Move,
        share as Arc<dyn Volume>,
        &[PathBuf::from("/album"), PathBuf::from("/notes.txt")],
        local_disk(work.path()),
        work.path(),
    )
    .await;

    let units = units(&log, "op");
    for landed in [work.path().join("album/a.jpg"), work.path().join("notes.txt")] {
        let unit = unit_landing_at(&units, &landed);
        assert_eq!(unit.dest_identity, expected_at(&landed), "{}", landed.display());
    }
}

/// A destination that isn't a local disk records no node and is never asked for
/// one, though this double would answer: a share or a phone pays no call per file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_onto_a_share_records_no_node() {
    let (engine, log) = journaled();
    let share = share_with_an_album().await;
    let other = in_memory("Other");
    other.create_directory(Path::new("/in")).await.expect("in");

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        share as Arc<dyn Volume>,
        &[PathBuf::from("/album"), PathBuf::from("/notes.txt")],
        other.clone() as Arc<dyn Volume>,
        Path::new("/in"),
    )
    .await;

    assert!(
        other
            .node_identity(Path::new("/in/notes.txt"))
            .await
            .expect("stat")
            .is_some(),
        "the double has an identity to give"
    );
    let units = units(&log, "op");
    assert!(
        units.iter().all(|unit| unit.dest_identity.is_none()),
        "no row names a node: {units:?}"
    );
    assert_eq!(
        units.iter().filter(|unit| unit.entry_type == EntryType::File).count(),
        2
    );
}
