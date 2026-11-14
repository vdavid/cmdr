//! Undo on a real disk, through the real local copy and move, the real journal,
//! and the real rollback gate, now that a landed row names its node
//! (`crates/cmdr-operation-log/DETAILS.md` § "A landed item must still be the node
//! that landed").
//!
//! The replacement in each cell is the one a size-and-date snapshot can't see: an
//! app saving the same bytes through a temp and a rename, and pinning the same
//! modified time. A row with a node leaves it alone; a row without one (written
//! before nodes were kept, or on a filesystem that keeps none) undoes exactly as
//! it always did.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::CollectorEventSink;
use super::journal;
use super::transfer::copy::copy_files_with_progress_inner;
use super::transfer::move_op::move_files_with_progress_inner;
use super::types::WriteOperationConfig;
use crate::host::testing::FakeVolumes;
use crate::rollback::Reversal;
use crate::testing::{TestOperationLog, TestOperations};
use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::rollback::{RollbackReport, execute_rollback, rollback_operation};
use cmdr_operation_log::store::{open_read_connection, read_rollback_units_page};
use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind, RollbackState, SkipReason};

const OLD: i64 = 1_500_000_000;

/// Whether the copy's rows record nodes, as they do from now on, or record none,
/// as every row written before did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rows {
    WithNode,
    WithoutNode,
}

struct UndoRig {
    engine: TestOperations,
    log: TestOperationLog,
    vm: FakeVolumes,
}

impl UndoRig {
    fn new() -> Self {
        let log = TestOperationLog::new("landed-node-undo");
        let vm = FakeVolumes::default();
        vm.register(
            "root",
            Arc::new(LocalPosixVolume::local_folder(
                "Test root",
                "/",
                crate::host::testing::pane_order_host(),
            )) as Arc<dyn Volume>,
        );
        UndoRig {
            engine: TestOperations::journaling_into(&log),
            log,
            vm,
        }
    }

    /// Runs `kind` (a copy or a move) of `source` into `dest` on the local disk.
    fn run(&self, kind: OpKind, rows: Rows, source: &Path, dest: &Path) {
        let state = Arc::new(self.engine.new_operation_state(Duration::ZERO));
        journal::open_local_op(state.scope(), "op", kind, Initiator::User, 1, Some("root"));
        if rows == Rows::WithoutNode {
            // Settles the scope's once-per-operation answer the way a filesystem
            // with no lasting identity would.
            assert!(!state.scope().records_landed_nodes(|| false));
        }
        let sources = [source.to_path_buf()];
        let config = WriteOperationConfig::default();
        let sink = CollectorEventSink::new();
        let result = if kind == OpKind::Move {
            move_files_with_progress_inner(&sink, "op", &state, &sources, dest, &config)
        } else {
            copy_files_with_progress_inner(&sink, "op", &state, &sources, dest, &config)
        };
        journal::finalize_op(state.scope(), kind, ExecutionStatus::Done);
        result.expect("the operation ran");
        let conn = open_read_connection(self.log.db()).expect("read conn");
        let units = read_rollback_units_page(&conn, "op", i64::MAX, 100).expect("units");
        assert_eq!(
            units.iter().any(|unit| unit.dest_identity.is_some()),
            rows == Rows::WithNode && crate::local_disk::keeps_node_identity(dest),
            "{units:?}"
        );
    }

    /// Asks for the reversal the way the app does: the gate decides, then the engine runs.
    async fn undo(&self) -> RollbackReport {
        let writer = self.log.writer();
        let plan = rollback_operation(&self.vm, writer, "op", |_plan| Ok(())).expect("the operation reverses");
        let reversal = Reversal::on(&self.engine, "landed-node-undo");
        execute_rollback(
            &self.vm,
            writer,
            &plan.original,
            &plan.inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await
    }
}

fn write_old(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("mk parents");
    std::fs::write(path, bytes).expect("write fixture");
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(OLD, 0)).expect("pin mtime");
}

/// Saves `bytes` over `path` the way most apps do, a new file renamed into place,
/// with the same modified time the snapshot recorded.
fn save_over(path: &Path, bytes: &[u8]) {
    let temp = path.with_extension("saving");
    write_old(&temp, bytes);
    std::fs::rename(&temp, path).expect("rename into place");
}

/// Whether the temp dir's filesystem keeps a lasting node identity, which a row's
/// node check needs. A filesystem without birth times (the Linux container's
/// overlayfs) records none, so its rows undo by size and date as before
/// (`undoing_a_copy_whose_row_names_no_node_removes_what_matches_its_snapshot_as_before`),
/// and the cells that need a node have nothing to check there.
fn keeps_nodes(dir: &Path) -> bool {
    crate::local_disk::keeps_node_identity(dir)
}

fn skipped_for_drift(report: &RollbackReport) -> bool {
    report.reversed == 0
        && report.skipped == 1
        && report.skips.len() == 1
        && report.skips[0].reason == SkipReason::Drift
        && report.final_state == RollbackState::PartiallyRolledBack
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_copy_removes_the_file_it_landed() {
    for rows in [Rows::WithNode, Rows::WithoutNode] {
        let rig = UndoRig::new();
        let work = tempfile::tempdir().expect("work");
        let source = work.path().join("src/report.pdf");
        write_old(&source, b"the report");
        let dest = work.path().join("dst");
        std::fs::create_dir_all(&dest).expect("mk dst");

        rig.run(OpKind::Copy, rows, &source, &dest);
        let report = rig.undo().await;

        assert_eq!((report.reversed, report.skipped), (1, 0), "{rows:?}: {report:?}");
        assert!(!dest.join("report.pdf").exists(), "{rows:?}: the copy is gone");
        assert_eq!(std::fs::read(&source).expect("source"), b"the report", "{rows:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_copy_keeps_a_same_looking_file_saved_over_it_once_rows_name_the_node() {
    let rig = UndoRig::new();
    let work = tempfile::tempdir().expect("work");
    if !keeps_nodes(work.path()) {
        return;
    }
    let source = work.path().join("src/report.pdf");
    write_old(&source, b"the report");
    let dest = work.path().join("dst");
    std::fs::create_dir_all(&dest).expect("mk dst");
    rig.run(OpKind::Copy, Rows::WithNode, &source, &dest);
    let landed = dest.join("report.pdf");
    save_over(&landed, b"THE REPORT");

    let report = rig.undo().await;

    assert!(skipped_for_drift(&report), "{report:?}");
    assert_eq!(
        std::fs::read(&landed).expect("kept"),
        b"THE REPORT",
        "the stranger stays"
    );
}

/// ❗ The behavior every row written before keeps: size and date are all it can
/// check, so the same replacement is removed as the copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_copy_whose_row_names_no_node_removes_what_matches_its_snapshot_as_before() {
    let rig = UndoRig::new();
    let work = tempfile::tempdir().expect("work");
    let source = work.path().join("src/report.pdf");
    write_old(&source, b"the report");
    let dest = work.path().join("dst");
    std::fs::create_dir_all(&dest).expect("mk dst");
    rig.run(OpKind::Copy, Rows::WithoutNode, &source, &dest);
    save_over(&dest.join("report.pdf"), b"THE REPORT");

    let report = rig.undo().await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert!(!dest.join("report.pdf").exists());
}

/// A moved file saved over since stays where it is: undoing the move never carries
/// a stranger back to the source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_move_leaves_a_same_looking_file_saved_over_the_moved_one() {
    let rig = UndoRig::new();
    let work = tempfile::tempdir().expect("work");
    if !keeps_nodes(work.path()) {
        return;
    }
    let source = work.path().join("src/report.pdf");
    write_old(&source, b"the report");
    let dest = work.path().join("dst");
    std::fs::create_dir_all(&dest).expect("mk dst");
    rig.run(OpKind::Move, Rows::WithNode, &source, &dest);
    let landed = dest.join("report.pdf");
    save_over(&landed, b"THE REPORT");

    let report = rig.undo().await;

    assert!(skipped_for_drift(&report), "{report:?}");
    assert_eq!(std::fs::read(&landed).expect("kept"), b"THE REPORT");
    assert!(!source.exists(), "nothing went back to the source");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_move_takes_back_the_file_it_moved() {
    let rig = UndoRig::new();
    let work = tempfile::tempdir().expect("work");
    let source = work.path().join("src/report.pdf");
    write_old(&source, b"the report");
    let dest = work.path().join("dst");
    std::fs::create_dir_all(&dest).expect("mk dst");
    rig.run(OpKind::Move, Rows::WithNode, &source, &dest);

    let report = rig.undo().await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(std::fs::read(&source).expect("back"), b"the report");
    assert!(!dest.join("report.pdf").exists());
}
