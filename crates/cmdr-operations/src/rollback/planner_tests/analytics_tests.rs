//! `EngineEvents::rollback_settled`, which the app reports as `operation_rollback_run`:
//! every reversal that RUNS reports exactly once, from the managed inverse's single
//! exit, with who asked, whether the gate narrowed it, and the run's own report. A
//! refusal at the gate ran nothing and reports nothing.

use std::sync::Arc;
use std::time::Duration;

use super::test_support::*;
use crate::event_sinks::OperationEventSink;
use crate::testing::{CollectorEventSink, TestOperations};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::query::SelectionToggle::Item;
use cmdr_operation_log::store::operation_log_db_path;
use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind, RollbackState, SearchCoverage};
use cmdr_operation_log::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

/// A finished rename of `items` on volume `v`, journaled through `writer`.
fn seed_rename(writer: &OperationLogWriter, op_id: &str, items: Vec<JournalItem>) {
    let n = items.len() as u64;
    writer
        .open_operation(OpenOperation {
            op_id: op_id.into(),
            kind: OpKind::Rename,
            initiator: Initiator::User,
            source_volume_id: Some("v".to_string()),
            dest_volume_id: Some("v".to_string()),
            item_count: n,
            started_at: 100,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    writer.record_items(op_id, items).expect("record");
    writer
        .finalize_operation(FinalizeOperation {
            op_id: op_id.to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 200,
            item_count: None,
            items_done: n,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    writer.flush_blocking().expect("flush");
}

/// An engine over an OPENED log, so a dispatch finds a writer (a journal set on a
/// detached log has none), plus that log's writer to seed with and a volume `v`.
async fn rig() -> (
    TestOperations,
    OperationLogWriter,
    Arc<InMemoryVolume>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = OperationLog::detached();
    log.open(&operation_log_db_path(dir.path()), &|_| None)
        .expect("open the log");
    let writer = log.writer().expect("an opened log has a writer");
    let engine = TestOperations::with(|builder| builder.operation_log(log));
    let v = Arc::new(InMemoryVolume::new("V"));
    engine.volumes().register("v", v.clone());
    (engine, writer, v, dir)
}

fn sink() -> Arc<dyn OperationEventSink> {
    Arc::new(CollectorEventSink::new())
}

#[tokio::test]
async fn a_whole_reversal_reports_once_unnarrowed_with_its_report() {
    let (engine, writer, v, _dir) = rig().await;
    put(&v, "/b.txt", b"data").await;
    seed_rename(&writer, "op", vec![file_unit(0, "v", "/a.txt", "v", "/b.txt", 4)]);

    let undo = engine.undo_operations(&["op".into()], Initiator::User, sink()).await;
    assert_eq!(undo.restored, 1, "the rename came back: {undo:?}");

    let recorded = engine.events().rollbacks.lock_ignore_poison().clone();
    assert_eq!(recorded.len(), 1, "one report per reversal that ran");
    let (initiator, narrowed, report) = &recorded[0];
    assert_eq!(*initiator, Initiator::User);
    assert!(!narrowed);
    assert_eq!((report.reversed, report.skipped, report.canceled), (1, 0, false));
    assert_eq!(report.final_state, RollbackState::RolledBack);
}

#[tokio::test]
async fn a_narrowed_dispatch_reports_narrowed() {
    let (engine, writer, v, _dir) = rig().await;
    put(&v, "/b.txt", b"data").await;
    put(&v, "/d.txt", b"data").await;
    seed_rename(
        &writer,
        "op",
        vec![
            file_unit(0, "v", "/a.txt", "v", "/b.txt", 4),
            file_unit(1, "v", "/c.txt", "v", "/d.txt", 4),
        ],
    );

    engine
        .dispatch_rollback(
            &"op".into(),
            Initiator::Agent,
            Some(&only(vec![Item { seq: 0, on: true }])),
            sink(),
        )
        .expect("the gate lets it through");
    wait_until_async(Duration::from_secs(10), "the reversal reports", || {
        !engine.events().rollbacks.lock_ignore_poison().is_empty()
    })
    .await;

    let recorded = engine.events().rollbacks.lock_ignore_poison().clone();
    let (initiator, narrowed, report) = &recorded[0];
    assert_eq!(*initiator, Initiator::Agent);
    assert!(narrowed, "a selection the gate kept is a partial scope");
    assert_eq!(
        report.final_state,
        RollbackState::PartiallyRolledBack,
        "the other row is left out"
    );
}

#[tokio::test]
async fn a_refused_rollback_ran_nothing_and_reports_nothing() {
    let (engine, _writer, _v, _dir) = rig().await;

    let undo = engine.undo_operations(&["nope".into()], Initiator::User, sink()).await;
    assert!(undo.operations[0].refusal.is_some());

    assert!(engine.events().rollbacks.lock_ignore_poison().is_empty());
}
