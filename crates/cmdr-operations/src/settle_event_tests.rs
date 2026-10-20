//! Tests for the `write-settled` per-op event. See § "Settle contract" in the
//! module CLAUDE.md for the full contract.
//!
//! The `WriteSettledGuard` RAII pattern emits exactly one `write-settled`
//! event when the spawned task fully returns — happy path, error path,
//! cancellation, or panic. These tests pin the contract.
//!
//! The guard emits via `sink.emit_settled(...)` from inside its `Drop`, using
//! the injected `Arc<dyn OperationEventSink>`. Production builds that sink at
//! the IPC edge (`TauriEventSink`); these tests pass a `CollectorEventSink`, so
//! the full lifecycle runs without a Tauri runtime.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_operation_log::store::{open_read_connection, read_operation};
use cmdr_operation_log::types::{ExecutionStatus, OperationId, RollbackState};
use tokio::sync::oneshot;

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::state::WriteSettledGuard;
use super::testing::{TestOperationLog, TestOperations};
use super::types::{CancelRollback, LifecycleStatus, WriteOperationConfig, WriteOperationType, WriteSettledEvent};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::LaneKey;

/// Bridge sink that keeps a direct handle to the underlying `CollectorEventSink`
/// for inspection. Needed because the guard takes `Arc<dyn OperationEventSink>`,
/// and `dyn OperationEventSink` isn't downcastable. An optional `probe` runs at
/// each terminal and settle emit, before the collector records it, so a test can
/// read the world as it stood at that moment.
struct Bridge {
    inner: Arc<CollectorEventSink>,
    probe: Option<Probe>,
}

/// The emit a probe is called at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Moment {
    Complete,
    Error,
    Settled,
}

type Probe = Arc<dyn Fn(Moment, &str) + Send + Sync>;

impl Bridge {
    fn probe(&self, moment: Moment, operation_id: &str) {
        if let Some(probe) = &self.probe {
            probe(moment, operation_id);
        }
    }
}

impl OperationEventSink for Bridge {
    fn emit_progress(&self, e: super::types::WriteProgressEvent) {
        self.inner.emit_progress(e);
    }
    fn emit_complete(&self, e: super::types::WriteCompleteEvent) {
        self.probe(Moment::Complete, &e.operation_id);
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: super::types::WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: super::types::WriteErrorEvent) {
        self.probe(Moment::Error, &e.operation_id);
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: crate::ArmedConflict) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: super::types::WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, e: super::types::WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(e);
    }
    fn emit_scan_progress(&self, e: super::types::ScanProgressEvent) {
        self.inner.emit_scan_progress(e);
    }
    fn emit_scan_conflict(&self, c: super::types::ConflictInfo) {
        self.inner.emit_scan_conflict(c);
    }
    fn emit_dry_run_complete(&self, r: super::types::DryRunResult) {
        self.inner.emit_dry_run_complete(r);
    }
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.probe(Moment::Settled, &e.operation_id);
        self.inner.emit_settled(e);
    }
}

fn pair() -> (Arc<dyn OperationEventSink>, Arc<CollectorEventSink>) {
    probed_pair(None)
}

fn probed_pair(probe: Option<Probe>) -> (Arc<dyn OperationEventSink>, Arc<CollectorEventSink>) {
    let inner = Arc::new(CollectorEventSink::new());
    let bridge: Arc<dyn OperationEventSink> = Arc::new(Bridge {
        inner: Arc::clone(&inner),
        probe,
    });
    (bridge, inner)
}

#[test]
fn settled_fires_once_on_normal_drop() {
    let (sink, collector) = pair();

    {
        let _guard = WriteSettledGuard::new(sink, "op-happy", WriteOperationType::Copy, Some("test-vol".to_string()));
        // Guard drops at end of scope.
    }

    let settled = collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1, "guard must fire exactly one write-settled event");
    assert_eq!(settled[0].operation_id, "op-happy");
    assert_eq!(settled[0].operation_type, WriteOperationType::Copy);
    assert_eq!(settled[0].volume_id.as_deref(), Some("test-vol"));
}

#[test]
fn settled_fires_with_none_volume_for_local_ops() {
    let (sink, collector) = pair();

    drop(WriteSettledGuard::new(
        sink,
        "op-local",
        WriteOperationType::Delete,
        None,
    ));

    let settled = collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1);
    assert!(
        settled[0].volume_id.is_none(),
        "local-FS settle event must carry volume_id=None"
    );
}

#[test]
fn settled_fires_on_panic_unwind() {
    // The guard is in scope when the closure panics. `catch_unwind` swallows
    // the panic, but the guard's Drop still runs as part of stack unwinding.
    // This is the panic-safety property the FE relies on so the dialog can
    // always clear, even on a backend bug.
    let (sink, collector) = pair();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard = WriteSettledGuard::new(sink, "op-panic", WriteOperationType::Move, Some("v".to_string()));
        panic!("simulated handler panic");
    }));

    assert!(result.is_err(), "the closure must have panicked");
    let settled = collector.settled.lock().unwrap();
    assert_eq!(
        settled.len(),
        1,
        "settle event must fire even when the spawned task panicked: the FE would hang otherwise"
    );
    assert_eq!(settled[0].operation_id, "op-panic");
}

#[test]
fn settled_event_order_is_after_terminal_outcome_event() {
    // The FE depends on this ordering: it sees the terminal event
    // (write-cancelled / write-complete / write-error) first, knows the
    // outcome, then sees write-settled and knows the volume is ready.
    // Reversing would race: the dialog could try to close on settle before
    // it knows the outcome.
    //
    // Mirrors how production spawn tasks emit: handler emits the terminal
    // event, then state cleanup runs, then the guard's Drop emits settled.
    let (sink, collector) = pair();
    let op_id = "op-order".to_string();

    {
        let _guard = WriteSettledGuard::new(
            Arc::clone(&sink),
            op_id.clone(),
            WriteOperationType::Delete,
            Some("v1".to_string()),
        );
        // Simulate the handler's terminal emit.
        sink.emit_cancelled(super::types::WriteCancelledEvent {
            operation_id: op_id.clone(),
            operation_type: WriteOperationType::Delete,
            files_processed: 7,
            rollback: CancelRollback::none(),
        });
        // Guard's Drop runs at end of scope, AFTER the cancelled emit above.
    }

    let cancelled = collector.cancelled.lock().unwrap();
    let settled = collector.settled.lock().unwrap();
    assert_eq!(cancelled.len(), 1, "cancelled should fire first");
    assert_eq!(settled.len(), 1, "settled should fire second");
    assert_eq!(settled[0].operation_id, op_id);
}

#[test]
fn settled_fires_for_every_operation_type() {
    // Trivial coverage: each op-type variant flows through the same path,
    // but we pin it so a future refactor that branches on type won't
    // accidentally drop one.
    for op_type in [
        WriteOperationType::Copy,
        WriteOperationType::Move,
        WriteOperationType::Delete,
        WriteOperationType::Trash,
    ] {
        let (sink, collector) = pair();
        drop(WriteSettledGuard::new(sink, format!("op-{:?}", op_type), op_type, None));
        let settled = collector.settled.lock().unwrap();
        assert_eq!(settled.len(), 1, "settle must fire once per op_type={:?}", op_type);
        assert_eq!(settled[0].operation_type, op_type);
    }
}

// ============================================================================
// Injected event sink drives the managed spawn path end to end
// ============================================================================

fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("write_test_{}", name))
}

/// Drives `copy_files_start` through the operation manager with a
/// `CollectorEventSink` injected at the (test-stand-in) IPC edge, and asserts
/// the terminal `write-complete` and the `write-settled` events both arrive via
/// that sink for a real local copy. Before the sink lift these managed events
/// were only reachable through a live `TauriEventSink`; now the whole pipeline
/// takes the injected sink, so it's observable with no Tauri runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_sink_receives_complete_and_settled_for_local_copy() {
    let src_dir = create_temp_dir("managed_sink_src");
    let dst_dir = create_temp_dir("managed_sink_dst");
    let src_file = src_dir.join("hello.txt");
    fs::write(&src_file, b"managed sink test").expect("failed to write source file");

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();

    let result = TestOperations::new()
        .copy_files_start(
            events,
            vec![src_file.clone()],
            dst_dir.to_path_buf(),
            WriteOperationConfig::default(),
            vec![],
            None,
            cmdr_operation_log::types::Initiator::User,
            None,
            None,
        )
        .await
        .expect("copy_files_start should return Ok");

    // The deferred task runs asynchronously; wait until it settles.
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the write-settled event to arrive", || {
        !collector.settled.lock().unwrap().is_empty()
    })
    .await;
    {
        let settled = collector.settled.lock().unwrap();
        let ev = settled
            .first()
            .expect("write-settled must arrive via the injected sink");
        assert_eq!(ev.operation_id, result.operation_id);
        assert_eq!(ev.operation_type, WriteOperationType::Copy);
        assert!(ev.volume_id.is_none(), "a same-root local copy carries volume_id=None");
    }

    // The full managed pipeline ran through the injected sink, not just the
    // settle guard: the terminal write-complete arrived and the bytes landed.
    let complete = collector.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete must arrive via the sink");
    assert_eq!(complete[0].operation_id, result.operation_id);
    assert!(
        dst_dir.join("hello.txt").exists(),
        "the copied file must exist at the destination"
    );
}

// ============================================================================
// The full settle order, through a real managed op
// ============================================================================

/// What a probe saw at one emit: the op's journal row status, whether the
/// manager still holds its record, and whether its lane is still reserved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    moment: Moment,
    journal: Option<ExecutionStatus>,
    record_live: bool,
    lane_held: bool,
}

fn execution_status(db: &Path, op_id: &str) -> Option<ExecutionStatus> {
    let conn = open_read_connection(db).expect("read conn");
    read_operation(&conn, op_id)
        .expect("read operation")
        .map(|row| row.execution_status)
}

type SeenLog = Arc<std::sync::Mutex<Vec<Seen>>>;

/// A sink whose probe records [`Seen`] at every terminal and settle emit, asking
/// `engine`'s manager about the record and the lane.
fn recording_pair(
    engine: &TestOperations,
    journal_db: PathBuf,
    lane: String,
) -> (Arc<dyn OperationEventSink>, Arc<CollectorEventSink>, SeenLog) {
    let seen: SeenLog = Arc::default();
    let log = Arc::clone(&seen);
    let manager = Arc::clone(engine.manager());
    let probe: Probe = Arc::new(move |moment, op_id| {
        let journal = execution_status(&journal_db, op_id);
        log.lock_ignore_poison().push(Seen {
            moment,
            journal,
            record_live: manager.lifecycle_status(&OperationId::from(op_id)).is_some(),
            lane_held: manager.lane_use_snapshot().contains_key(&lane),
        });
    });
    let (sink, collector) = probed_pair(Some(probe));
    (sink, collector, seen)
}

async fn start_copy(
    engine: &TestOperations,
    events: Arc<dyn OperationEventSink>,
    source: PathBuf,
    dst: &Path,
    lane: &str,
) -> String {
    engine
        .copy_files_start(
            events,
            vec![source],
            dst.to_path_buf(),
            WriteOperationConfig::default(),
            vec![],
            Some(vec![LaneKey::new(lane)]),
            cmdr_operation_log::types::Initiator::User,
            None,
            None,
        )
        .await
        .expect("copy_files_start")
        .operation_id
}

async fn wait_settled(collector: &CollectorEventSink) {
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the write-settled event", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;
}

/// A handler that emits its own terminal event does so INSIDE the blocking
/// handler, before the journal finalizes: `write-complete` sees the row still
/// running, with the record live and the lane held. `write-settled` comes after
/// finalize and after `on_settled` freed both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_completed_op_emits_complete_then_finalizes_then_frees_then_settles() {
    let journal = TestOperationLog::new("settle_order_journal");
    let engine = TestOperations::journaling_into(&journal);
    let dir = create_temp_dir("settle_order_ok");
    fs::write(dir.join("a.txt"), b"a").expect("source");
    fs::create_dir(dir.join("dst")).expect("dst");
    let lane = "settle-order-ok".to_string();
    let (sink, collector, seen) = recording_pair(&engine, journal.db().to_path_buf(), lane.clone());

    let op_id = OperationId::from(start_copy(&engine, sink, dir.join("a.txt"), &dir.join("dst"), &lane).await);
    wait_settled(&collector).await;

    assert_eq!(
        *seen.lock_ignore_poison(),
        vec![
            Seen {
                moment: Moment::Complete,
                journal: Some(ExecutionStatus::Running),
                record_live: true,
                lane_held: true
            },
            Seen {
                moment: Moment::Settled,
                journal: Some(ExecutionStatus::Done),
                record_live: false,
                lane_held: false
            },
        ]
    );
    assert_eq!(
        collector.settled.lock_ignore_poison().len(),
        1,
        "write-settled fires once"
    );
    assert_eq!(
        execution_status(journal.db(), op_id.as_str()),
        Some(ExecutionStatus::Done)
    );
}

/// A handler that returns an error without emitting one gets the safety net's
/// `write-error`, which fires AFTER the journal finalized the op as failed, and
/// still before `on_settled`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_op_finalizes_then_emits_the_safety_net_error_then_frees_then_settles() {
    let journal = TestOperationLog::new("settle_order_fail_journal");
    let engine = TestOperations::journaling_into(&journal);
    let dir = create_temp_dir("settle_order_fail");
    fs::create_dir(dir.join("dst")).expect("dst");
    let lane = "settle-order-fail".to_string();
    let (sink, collector, seen) = recording_pair(&engine, journal.db().to_path_buf(), lane.clone());

    // A missing source fails validation inside the handler, which emits nothing.
    start_copy(&engine, sink, dir.join("missing.txt"), &dir.join("dst"), &lane).await;
    wait_settled(&collector).await;

    assert_eq!(
        *seen.lock_ignore_poison(),
        vec![
            Seen {
                moment: Moment::Error,
                journal: Some(ExecutionStatus::Failed),
                record_live: true,
                lane_held: true
            },
            Seen {
                moment: Moment::Settled,
                journal: Some(ExecutionStatus::Failed),
                record_live: false,
                lane_held: false
            },
        ]
    );
    assert_eq!(
        collector.errors.lock_ignore_poison().len(),
        1,
        "exactly one write-error"
    );
}

/// The next op on a lane is admitted only by `on_settled`, which runs after the
/// journal finalized the op ahead of it: an op queued behind a copy reads that
/// copy's row already finalized, eligibility computed, when its turn comes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_op_queued_behind_a_copy_reads_its_row_finalized_when_admitted() {
    let journal = TestOperationLog::new("settle_order_queue_journal");
    let engine = TestOperations::journaling_into(&journal);
    let dir = create_temp_dir("settle_order_queue");
    fs::write(dir.join("a.txt"), b"a").expect("source");
    fs::create_dir(dir.join("dst")).expect("dst");
    let lane = "settle-order-queue".to_string();

    // A blocker holds the lane so the copy queues; the reader queues behind it.
    let blocker = OperationId::from("settle-order-blocker");
    let (release_blocker, blocker_released) = oneshot::channel::<()>();
    spawn_on_lane(&engine, &blocker, &lane, || {}, async move {
        let _ = blocker_released.await;
    });

    let (sink, collector) = pair();
    let copy_id = OperationId::from(start_copy(&engine, sink, dir.join("a.txt"), &dir.join("dst"), &lane).await);
    assert_eq!(
        engine.manager().lifecycle_status(&copy_id),
        Some(LifecycleStatus::Queued)
    );

    let reader = OperationId::from("settle-order-reader");
    let (row_tx, row_rx) = oneshot::channel::<Option<(ExecutionStatus, RollbackState)>>();
    let db = journal.db().to_path_buf();
    let read_id = copy_id.clone();
    // Read at the admission instant itself, inside the admission pass, so a
    // finalize that ran after `on_settled` can't win a race to the row.
    let at_admission = move || {
        let conn = open_read_connection(&db).expect("read conn");
        let row = read_operation(&conn, read_id.as_str()).expect("read");
        let _ = row_tx.send(row.map(|r| (r.execution_status, r.rollback_state)));
    };
    spawn_on_lane(&engine, &reader, &lane, at_admission, async {});
    assert_eq!(
        engine.manager().lifecycle_status(&reader),
        Some(LifecycleStatus::Queued)
    );

    let _ = release_blocker.send(());
    let row = tokio::time::timeout(Duration::from_secs(5), row_rx)
        .await
        .expect("the reader to be admitted")
        .expect("the reader to report");
    assert_eq!(
        row,
        Some((ExecutionStatus::Done, RollbackState::Rollbackable)),
        "the op ahead was finalized, eligibility included, before the next one was admitted"
    );
    wait_settled(&collector).await;
}

/// Registers a synthetic op on `engine`'s `lane` whose body is `work`, settling
/// on that engine when it ends. `at_admission` runs synchronously inside the
/// admission pass that admits it, where its deferred start is built.
fn spawn_on_lane(
    engine: &TestOperations,
    op_id: &OperationId,
    lane: &str,
    at_admission: impl FnOnce() + Send + 'static,
    work: impl Future<Output = ()> + Send + 'static,
) {
    let descriptor = OperationDescriptor {
        operation_id: op_id.clone(),
        operation_type: WriteOperationType::Copy,
        lanes: vec![LaneKey::new(lane)],
        volume_ids: vec![],
        summary: OperationSummaryText::default(),
        rollback_support: crate::types::RollbackSupport::Supported,
        preview_id: None,
        reverses: None,
    };
    let id = op_id.clone();
    engine.manager().spawn_managed(
        descriptor,
        Arc::new(engine.new_operation_state(Duration::from_millis(200))),
        Box::new(move |ops: crate::Operations| {
            at_admission();
            Box::pin(async move {
                let guard = ManagedTaskGuard::new(ops.manager(), id.clone());
                work.await;
                guard.disarm();
                ops.manager().on_settled(&id);
            })
        }),
    );
}
