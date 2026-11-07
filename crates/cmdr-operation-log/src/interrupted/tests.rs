//! Every op a crash left `queued` or `running` ends at the next open: `failed`, ended
//! when it started, and only as rollbackable as the facts it journaled allow.

use std::path::{Path, PathBuf};

use crate::OperationLog;
use crate::store::{OperationRow, open_read_connection, operation_log_db_path, read_operation};
use crate::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, RowRole,
};
use crate::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter, PruneRequest};

const STARTED: i64 = 1_000;

/// A journal a previous launch wrote and then crashed out of: each op opened (and
/// given a row) through the real writer, never finalized.
struct Crashed {
    _dir: tempfile::TempDir,
    path: PathBuf,
    writer: OperationLogWriter,
}

impl Crashed {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = operation_log_db_path(dir.path());
        drop(crate::store::OperationLogStore::open(&path, &|_| None).expect("store"));
        let writer = OperationLogWriter::spawn(&path).expect("writer");
        Crashed {
            _dir: dir,
            path,
            writer,
        }
    }

    fn left(&self, op_id: &str, kind: OpKind, status: ExecutionStatus, rolls_back: Option<&str>, overwrote: bool) {
        self.writer
            .open_operation(OpenOperation {
                op_id: op_id.into(),
                kind,
                initiator: Initiator::User,
                source_volume_id: Some("v".to_string()),
                dest_volume_id: Some("v".to_string()),
                item_count: 1,
                started_at: STARTED,
                rolls_back_op_id: rolls_back.map(Into::into),
                execution_status: status,
            })
            .expect("open");
        self.writer.record_items(op_id, vec![row(overwrote)]).expect("record");
    }

    /// The next launch: the handle opens the journal the crash left.
    fn relaunch(self) -> (OperationLog, PathBuf, tempfile::TempDir) {
        self.writer.flush_blocking().expect("flush");
        self.writer.shutdown();
        let log = OperationLog::detached();
        log.open(&self.path, &|_| None).expect("open");
        (log, self.path, self._dir)
    }
}

fn row(overwrote: bool) -> JournalItem {
    JournalItem {
        seq: 0,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "v".to_string(),
        source_dir: "/src".to_string(),
        source_name: "a.txt".to_string(),
        dest_volume_id: Some("v".to_string()),
        dest_dir: Some("/dst".to_string()),
        dest_name: Some("a.txt".to_string()),
        size: Some(1),
        mtime: Some(1),
        outcome: ItemOutcome::Done,
        overwrote,
        dest_identity: None,
        tags: None,
    }
}

fn read(path: &Path, op_id: &str) -> OperationRow {
    let conn = open_read_connection(path).expect("read connection");
    read_operation(&conn, op_id).expect("read").expect("the op is there")
}

#[test]
fn every_kind_ends_failed_at_its_start_with_only_the_rollback_its_rows_allow() {
    use NotRollbackableReason::*;
    let rollbackable = (RollbackState::Rollbackable, None);
    let refused = |reason| (RollbackState::NotRollbackable, Some(reason));
    let cases = [
        ("copy", OpKind::Copy, false, rollbackable),
        ("copy-over", OpKind::Copy, true, refused(Overwrote)),
        ("move", OpKind::Move, false, refused(JournalIncomplete)),
        ("compress", OpKind::ArchiveEdit, false, refused(JournalIncomplete)),
        ("delete", OpKind::Delete, false, refused(PermanentDelete)),
        ("trash", OpKind::Trash, false, rollbackable),
        ("rename", OpKind::Rename, false, rollbackable),
        ("mkdir", OpKind::CreateFolder, false, rollbackable),
        ("mkfile", OpKind::CreateFile, false, rollbackable),
        ("tag", OpKind::Tag, false, rollbackable),
    ];
    let crashed = Crashed::new();
    for (op_id, kind, overwrote, _) in &cases {
        crashed.left(op_id, *kind, ExecutionStatus::Running, None, *overwrote);
    }
    crashed.left("queued", OpKind::Copy, ExecutionStatus::Queued, None, false);

    let (_log, path, _dir) = crashed.relaunch();

    for (op_id, _, _, (state, reason)) in cases {
        let op = read(&path, op_id);
        assert_eq!(op.execution_status, ExecutionStatus::Failed, "{op_id}");
        assert_eq!(
            op.ended_at,
            Some(STARTED),
            "{op_id}: ended when it started, never at launch"
        );
        assert_eq!(
            (op.rollback_state, op.not_rollbackable_reason),
            (state, reason),
            "{op_id}"
        );
    }
    let queued = read(&path, "queued");
    assert_eq!(queued.execution_status, ExecutionStatus::Failed);
    assert_eq!(queued.rollback_state, RollbackState::Rollbackable);
}

#[test]
fn an_interrupted_rollback_ends_but_never_becomes_rollbackable_and_its_original_is_the_reconciles_call() {
    let crashed = Crashed::new();
    crashed.left("op", OpKind::Move, ExecutionStatus::Done, None, false);
    crashed
        .writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("rolling back");
    // The inverse reversed the one row and crashed before it finalized.
    crashed.left("undo", OpKind::Move, ExecutionStatus::Running, Some("op"), false);

    let (_log, path, _dir) = crashed.relaunch();

    let undo = read(&path, "undo");
    assert_eq!(undo.execution_status, ExecutionStatus::Failed);
    assert_eq!(undo.ended_at, Some(STARTED));
    assert_eq!(
        (undo.rollback_state, undo.not_rollbackable_reason),
        (RollbackState::NotRollbackable, None),
        "it keeps what it opened with"
    );
    let op = read(&path, "op");
    assert_eq!(
        op.execution_status,
        ExecutionStatus::Done,
        "a finished op is never touched"
    );
    assert_eq!(
        op.rollback_state,
        RollbackState::PartiallyRolledBack,
        "the reconcile read the inverse's `done` row"
    );
}

#[test]
fn an_op_this_launch_journals_is_never_taken_for_an_interrupted_one() {
    // The step runs inside `open`, before the journal is published: an op the engine
    // opens afterwards stays running until it finalizes itself.
    let crashed = Crashed::new();
    crashed.left("stale", OpKind::Copy, ExecutionStatus::Running, None, false);
    let (log, path, _dir) = crashed.relaunch();

    let journal = log.journal_open(OpenOperation {
        op_id: "fresh".into(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("v".to_string()),
        dest_volume_id: Some("v".to_string()),
        item_count: 1,
        started_at: STARTED + 5,
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
    journal.record_items(vec![row(false)]);
    log.writer().expect("published").flush_blocking().expect("flush");

    assert_eq!(read(&path, "fresh").execution_status, ExecutionStatus::Running);
    assert_eq!(read(&path, "fresh").ended_at, None);
    assert_eq!(read(&path, "stale").execution_status, ExecutionStatus::Failed);
}

#[test]
fn a_finalize_that_landed_first_wins() {
    // `end_interrupted` only ever touches an op still queued or running.
    let crashed = Crashed::new();
    crashed.left("op", OpKind::Copy, ExecutionStatus::Running, None, false);
    crashed
        .writer
        .finalize_operation(FinalizeOperation {
            op_id: "op".to_string(),
            execution_status: ExecutionStatus::Canceled,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: crate::types::SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: STARTED + 30,
            item_count: None,
            items_done: 1,
            bytes_total: 1,
            dev_summary: None,
        })
        .expect("finalize");

    crashed
        .writer
        .end_interrupted("op", Some((RollbackState::NotRollbackable, None)))
        .expect("end");

    let op = read(&crashed.path, "op");
    assert_eq!(op.execution_status, ExecutionStatus::Canceled);
    assert_eq!(op.ended_at, Some(STARTED + 30));
    assert_eq!(op.rollback_state, RollbackState::Rollbackable);
}

#[test]
fn an_interrupted_op_ages_out_from_when_it_started() {
    let crashed = Crashed::new();
    crashed.left("op", OpKind::Copy, ExecutionStatus::Running, None, false);
    let (log, path, _dir) = crashed.relaunch();
    let writer = log.writer().expect("published");

    // A day past its start, a week's retention keeps it; a week and a day past, it goes.
    let day = 86_400;
    for (now, kept) in [(STARTED as u64 + day, true), (STARTED as u64 + 8 * day, false)] {
        writer
            .prune(PruneRequest {
                max_age_secs: Some(7 * day),
                max_size_bytes: None,
                now_secs: now,
                vacuum: false,
            })
            .expect("prune");
        writer.flush_blocking().expect("flush");
        let conn = open_read_connection(&path).expect("read connection");
        assert_eq!(read_operation(&conn, "op").expect("read").is_some(), kept, "at {now}");
    }
}
