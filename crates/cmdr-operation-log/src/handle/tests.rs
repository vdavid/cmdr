//! The handle's own guarantees: two handles share nothing, `open` runs once and
//! reconciles before anything can journal, and an operation is journaled wholly
//! or not at all, whichever way it reaches the journal.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{OpenError, OperationLog};
use crate::capture::{CapturingJournal, FinalizeInputs};
use crate::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, RowRole,
};
use crate::writer::{JournalItem, OpenOperation, OperationLogWriter};
use cmdr_fs::ignore_poison::IgnorePoison;

fn temp_db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = operation_log_db_path(dir.path());
    (dir, path)
}

fn opened(path: &Path) -> OperationLog {
    let log = OperationLog::detached();
    log.open(path, &|_| None).expect("open");
    log
}

fn header(op_id: &str) -> OpenOperation {
    OpenOperation {
        op_id: op_id.into(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("root".to_string()),
        dest_volume_id: Some("root".to_string()),
        item_count: 1,
        started_at: 1,
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    }
}

fn item(name: &str) -> Vec<JournalItem> {
    vec![JournalItem {
        seq: 0,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "root".to_string(),
        source_dir: "/src".to_string(),
        source_name: name.to_string(),
        dest_volume_id: Some("root".to_string()),
        dest_dir: Some("/dst".to_string()),
        dest_name: Some(name.to_string()),
        size: Some(10),
        mtime: Some(100),
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: None,
    }]
}

fn done() -> FinalizeInputs {
    FinalizeInputs {
        execution_status: ExecutionStatus::Done,
        kind: OpKind::Copy,
        archive_subkind: None,
        net_new: false,
        ended_at: 2,
        item_count: None,
        items_done: 1,
        bytes_total: 10,
        dev_summary: None,
    }
}

/// The operation's item count in `path`, or `None` when it has no header row.
fn rows(path: &Path, op_id: &str) -> Option<usize> {
    let conn = open_read_connection(path).expect("read conn");
    read_operation(&conn, op_id).expect("read")?;
    Some(read_operation_items(&conn, op_id, 100).expect("items").len())
}

#[test]
fn an_operation_journaled_in_one_handle_is_invisible_to_another() {
    let (_a_dir, a_path) = temp_db();
    let (_b_dir, b_path) = temp_db();
    let a = opened(&a_path);
    let b = opened(&b_path);

    let journal = a.journal_open(header("op-a"));
    journal.record_items(item("a.txt"));
    assert_eq!(journal.finalize(done()).rollback_unit_rows, 1);
    b.writer().expect("writer").flush_blocking().expect("flush");

    assert_eq!(rows(&a_path, "op-a"), Some(1));
    assert_eq!(
        rows(&b_path, "op-a"),
        None,
        "the other handle's journal never heard of it"
    );
}

#[test]
fn a_second_open_is_refused_and_starts_no_second_writer() {
    let (_dir, first) = temp_db();
    let (_other_dir, second) = temp_db();
    let log = opened(&first);

    assert!(matches!(log.open(&second, &|_| None), Err(OpenError::AlreadyOpen)));
    assert!(!second.exists(), "nothing opened, created, or migrated the second DB");
    assert_eq!(
        log.writer().expect("writer").db_path(),
        first,
        "the first writer stays the one"
    );
}

#[test]
fn open_resolves_an_operation_a_crash_left_rolling_back() {
    let (_dir, path) = temp_db();
    // A previous run's writer leaves "op" mid-rollback with no inverse ever opened.
    let crashed = OperationLogWriter::spawn(&path).expect("spawn writer");
    crashed.open_operation(header("op")).expect("open op");
    crashed
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("mark rolling back");
    crashed.shutdown();

    let log = opened(&path);

    log.writer().expect("writer").flush_blocking().expect("flush");
    let conn = open_read_connection(&path).expect("read conn");
    let state = read_operation(&conn, "op").expect("read").expect("row").rollback_state;
    assert_eq!(
        state,
        RollbackState::Rollbackable,
        "no inverse ran, so it's cleanly rollbackable again"
    );
}

/// What a journal that arrived mid-op heard about an operation opened before it.
fn heard_about(journal: &CapturingJournal) -> (usize, usize, usize) {
    (
        journal.opens.lock_ignore_poison().len(),
        journal.items.lock_ignore_poison().len(),
        journal.finalizes.lock_ignore_poison().len(),
    )
}

/// An operation that opened before the journal did stays unjournaled once it
/// arrives, rather than sending its tail and finalize to a journal with no row
/// for it.
#[test]
fn an_operation_opened_before_the_journal_journals_nothing_through_its_journal() {
    let log = OperationLog::detached();
    let op = log.journal_open(header("early"));

    let journal = Arc::new(CapturingJournal::new());
    log.set_journal(Some(journal.clone()));
    op.record_items(item("a.txt"));
    op.note_not_rollbackable(NotRollbackableReason::DirectoryMerge);

    assert_eq!(op.finalize(done()).rollback_unit_rows, 0);
    assert_eq!(heard_about(&journal), (0, 0, 0), "no tail and no finalize");
    assert!(journal.not_rollbackable_notes.lock_ignore_poison().is_empty());
}

/// The next operation after the journal arrives journals normally, whatever
/// the early one did.
#[test]
fn the_journal_hears_every_operation_opened_after_it_arrived() {
    let log = OperationLog::detached();
    let early = log.journal_open(header("early"));
    let journal = Arc::new(CapturingJournal::new());
    log.set_journal(Some(journal.clone()));
    early.finalize(done());

    let next = log.journal_open(header("next"));
    next.record_items(item("a.txt"));
    next.finalize(done());

    assert_eq!(heard_about(&journal), (1, 1, 1));
}
