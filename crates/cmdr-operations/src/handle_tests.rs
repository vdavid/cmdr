//! Two engines share nothing: the proof the engine's state is its own and no
//! longer process-wide.

use cmdr_operation_log::types::OperationId;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::LaneKey;
use tokio::sync::oneshot;

use super::Operations;
use super::host::testing::{FakeVolumes, RecordingEngineEvents};
use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::types::{LifecycleStatus, WriteOperationType};
use cmdr_fs::testing::wait_until_async;

/// An operation registered in one engine is invisible to another: not in its
/// queue, its registry, its status cache, or its busy set.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_operation_registered_in_one_engine_is_invisible_to_another() {
    let (a, b) = (Operations::detached(), Operations::detached());
    let op = OperationId::from(format!("side-by-side-{}", uuid::Uuid::new_v4()));
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let id = op.clone();
    a.manager().spawn_managed(
        OperationDescriptor {
            operation_id: op.clone(),
            operation_type: WriteOperationType::Copy,
            lanes: vec![LaneKey::new(format!("{op}-lane"))],
            volume_ids: vec!["side-by-side-volume".to_string()],
            summary: OperationSummaryText::default(),
            rollback_support: crate::types::RollbackSupport::Supported,
            preview_id: None,
            reverses: None,
        },
        Arc::new(a.new_operation_state(Duration::from_millis(50))),
        Box::new(move |ops: Operations| {
            Box::pin(async move {
                let guard = ManagedTaskGuard::new(ops.manager(), id.clone());
                let _ = started_tx.send(());
                let _ = release_rx.await;
                guard.disarm();
                ops.manager().on_settled(&id);
            })
        }),
    );
    started_rx.await.expect("the operation runs on its own engine");

    assert_eq!(a.manager().lifecycle_status(&op), Some(LifecycleStatus::Running));
    assert_eq!(b.manager().lifecycle_status(&op), None, "not in the other queue");
    assert!(a.manager().list().iter().any(|row| row.operation_id == op.as_str()));
    assert!(b.manager().list().is_empty(), "the other engine lists nothing");
    assert!(a.registry().get(&op).is_some());
    assert!(b.registry().get(&op).is_none(), "not in the other registry");
    assert!(a.get_operation_status(&op).is_some());
    assert!(b.get_operation_status(&op).is_none(), "not in the other status cache");
    assert_eq!(a.status().busy_volume_ids(), vec!["side-by-side-volume".to_string()]);
    assert!(b.status().busy_volume_ids().is_empty(), "the other busy set is empty");

    let _ = release_tx.send(());
    // A wait, not an assert: the record goes first, the busy mark just after.
    wait_until_async(Duration::from_secs(5), "the operation to settle on its engine", || {
        a.manager().lifecycle_status(&op).is_none() && a.status().busy_volume_ids().is_empty()
    })
    .await;
}

/// Each engine journals into the operation log it was built with, and only
/// that one.
#[test]
fn each_engine_journals_into_its_own_log() {
    use cmdr_operation_log::OperationLog;
    use cmdr_operation_log::capture::CapturingJournal;
    use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind};
    use cmdr_operation_log::writer::OpenOperation;

    let (log_a, log_b) = (OperationLog::detached(), OperationLog::detached());
    let (journal_a, journal_b) = (Arc::new(CapturingJournal::new()), Arc::new(CapturingJournal::new()));
    log_a.set_journal(Some(journal_a.clone()));
    log_b.set_journal(Some(journal_b.clone()));
    let builder = || {
        Operations::builder(
            Arc::new(FakeVolumes::default()),
            Arc::new(RecordingEngineEvents::default()),
        )
    };
    let a = builder().operation_log(log_a).build();
    let _b = builder().operation_log(log_b).build();

    a.op_scope().open_journal(OpenOperation {
        op_id: "journaled-in-a".into(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: None,
        dest_volume_id: None,
        item_count: 1,
        started_at: 0,
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });

    assert_eq!(journal_a.opens.lock_ignore_poison().len(), 1);
    assert!(
        journal_b.opens.lock_ignore_poison().is_empty(),
        "the other log heard nothing"
    );
}

/// An engine reports its engine-wide facts to its own events seam: the busy set
/// and the transfer pair an operation raises reach this engine's events, and
/// another engine's hear nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_engine_reports_its_facts_to_its_own_events() {
    let (events_a, events_b) = (
        Arc::new(RecordingEngineEvents::default()),
        Arc::new(RecordingEngineEvents::default()),
    );
    let a = Operations::builder(Arc::new(FakeVolumes::default()), events_a.clone()).build();
    let _b = Operations::builder(Arc::new(FakeVolumes::default()), events_b.clone()).build();
    let volume = vec!["reported-volume".to_string()];

    a.status()
        .register("reported-op", WriteOperationType::Copy, volume.clone());
    a.status().unregister("reported-op");

    assert_eq!(*events_a.transfers_started.lock_ignore_poison(), vec![volume.clone()]);
    assert_eq!(*events_a.transfers_finished.lock_ignore_poison(), vec![volume.clone()]);
    assert_eq!(
        *events_a.volumes_busy_changed.lock_ignore_poison(),
        vec![volume, Vec::new()]
    );
    assert!(events_b.transfers_started.lock_ignore_poison().is_empty());
    assert!(events_b.volumes_busy_changed.lock_ignore_poison().is_empty());
}
