//! The `operationLog.enabled` switch, driven through an operation's journal on a
//! handle of the test's own, opened over a real temp DB.

use std::sync::Arc;

use crate::capture::FinalizeInputs;
use crate::rollback::{RollbackRefusal, VolumeLookup, rollback_operation};
use crate::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::types::{EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole};
use crate::writer::{JournalItem, OpenOperation, OperationLogWriter};
use crate::{OpJournal, OperationLog};
use cmdr_fs::volume::Volume;

struct Rig {
    _dir: tempfile::TempDir,
    log: OperationLog,
    writer: OperationLogWriter,
    path: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = operation_log_db_path(dir.path());
        let log = OperationLog::detached();
        log.open(&path, &|_| None).expect("open");
        let writer = log.writer().expect("an opened log has a writer");
        Self {
            _dir: dir,
            log,
            writer,
            path,
        }
    }

    fn row_count(&self, op_id: &str) -> Option<usize> {
        self.writer.flush_blocking().expect("flush");
        let conn = open_read_connection(&self.path).expect("read conn");
        read_operation(&conn, op_id).expect("read")?;
        Some(read_operation_items(&conn, op_id, 100).expect("items").len())
    }

    fn rollback_state(&self, op_id: &str) -> Option<RollbackState> {
        self.writer.flush_blocking().expect("flush");
        let conn = open_read_connection(&self.path).expect("read conn");
        read_operation(&conn, op_id)
            .expect("read")
            .map(|row| row.rollback_state)
    }

    fn open(&self, op_id: &str) -> OpJournal {
        self.log.journal_open(OpenOperation {
            op_id: op_id.into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("root".to_string()),
            dest_volume_id: Some("root".to_string()),
            item_count: 2,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
    }

    fn run_whole(&self, op_id: &str) -> u64 {
        let journal = self.open(op_id);
        record(&journal, "a.txt");
        record(&journal, "b.txt");
        finalize(journal)
    }
}

/// No volume is connected: what an unknown operation's gate never gets to ask.
struct NoVolumes;

impl VolumeLookup for NoVolumes {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        None
    }
}

fn record(journal: &OpJournal, name: &str) {
    journal.record_items(vec![JournalItem {
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
    }]);
}

fn finalize(journal: OpJournal) -> u64 {
    journal
        .finalize(FinalizeInputs {
            execution_status: ExecutionStatus::Done,
            kind: OpKind::Copy,
            archive_subkind: None,
            net_new: false,
            ended_at: 2,
            item_count: None,
            items_done: 2,
            bytes_total: 20,
            dev_summary: None,
        })
        .rollback_unit_rows
}

#[test]
fn an_operation_run_while_off_writes_no_rows() {
    let rig = Rig::new();
    rig.log.set_recording(false);

    assert_eq!(rig.run_whole("op-off"), 0);
    assert_eq!(rig.row_count("op-off"), None, "no header and no item rows");
}

#[test]
fn turning_recording_back_on_journals_the_next_operation() {
    let rig = Rig::new();
    rig.log.set_recording(false);
    rig.run_whole("op-off");
    rig.log.set_recording(true);

    assert_eq!(rig.run_whole("op-on"), 2);
    assert_eq!(rig.row_count("op-on"), Some(2));
    assert_eq!(rig.row_count("op-off"), None);
}

#[test]
fn an_operation_already_recording_when_switched_off_is_journaled_whole() {
    let rig = Rig::new();
    let journal = rig.open("op-mid");
    record(&journal, "a.txt");
    rig.log.set_recording(false);
    record(&journal, "b.txt");

    assert_eq!(finalize(journal), 2);
    assert_eq!(rig.row_count("op-mid"), Some(2), "every item, never half");
    assert_eq!(rig.rollback_state("op-mid"), Some(RollbackState::Rollbackable));
}

#[test]
fn an_operation_started_while_off_stays_unjournaled_when_switched_on() {
    let rig = Rig::new();
    rig.log.set_recording(false);
    let journal = rig.open("op-late");
    record(&journal, "a.txt");
    rig.log.set_recording(true);
    record(&journal, "b.txt");

    assert_eq!(finalize(journal), 0);
    assert_eq!(rig.row_count("op-late"), None, "no rows at all, never a tail");
}

#[test]
fn an_operation_run_while_off_is_not_offered_for_undo() {
    let rig = Rig::new();
    rig.log.set_recording(false);
    rig.run_whole("op-off");

    let refusal = rollback_operation(&NoVolumes, &rig.writer, "op-off", |_| Ok(())).unwrap_err();
    assert_eq!(refusal, RollbackRefusal::UnknownOperation);
}

#[test]
fn history_recorded_before_switching_off_stays_undoable() {
    let rig = Rig::new();
    rig.run_whole("op-before");
    rig.log.set_recording(false);

    assert_eq!(rig.row_count("op-before"), Some(2));
    assert_eq!(rig.rollback_state("op-before"), Some(RollbackState::Rollbackable));
}
