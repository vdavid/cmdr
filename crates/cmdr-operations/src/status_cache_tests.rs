//! Tests for the operation status cache and the busy-volume set it drives.
//!
//! Each test runs on an engine of its own (`TestOperations`), so the cache,
//! the busy set, and the events it reports hold that test's operations alone.

use super::*;
use crate::manager::{OperationDescriptor, OperationSummaryText, PauseOutcome};
use crate::state::OperationIntent;
use crate::test_support::placeholder_conflict;
use crate::testing::{RegisteredOperation, TestOperations};
use crate::types::{LifecycleStatus, TransferWaitReason, WriteOperationPhase, WriteOperationType};
use cmdr_fs::volume::LaneKey;
use cmdr_operation_log::types::OperationId;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::oneshot;

/// Registers a live state in `engine` with the given intent.
fn install_state(engine: &TestOperations, label: &str, initial: OperationIntent) -> RegisteredOperation {
    let op = engine.register(label);
    op.state().intent().set_for_test(initial);
    op
}

// ---- register / update / unregister + list / get ----

#[test]
fn register_then_get_status_roundtrip() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "reg-get", OperationIntent::Running);
    let id = op.id().clone();
    engine.status().register(id.as_str(), WriteOperationType::Copy, vec![]);

    let status = engine.get_operation_status(&id).expect("operation should be in cache");
    assert_eq!(status.operation_id, id.as_str());
    assert_eq!(status.operation_type, WriteOperationType::Copy);
    assert_eq!(status.phase, WriteOperationPhase::Scanning);
    // This op was never registered with the MANAGER, only with the state map, so
    // there is no lifecycle record to report. That is the honest answer: the
    // status cache does not invent one.
    assert_eq!(status.lifecycle, None);
    assert_eq!(status.files_done, 0);
    assert_eq!(status.files_total, 0);
    assert_eq!(status.bytes_done, 0);
    assert_eq!(status.bytes_total, 0);

    engine.registry().forget(&id);
    engine.status().unregister(id.as_str());
    assert!(engine.get_operation_status(&id).is_none());
}

/// A real managed operation on a lane of its own, so admission runs it
/// immediately and the manager holds a `Running` record for it. Its status-cache
/// row is registered too, which is what `get_operation_status` reads.
///
/// Dropping it drops `_release`, which lets the operation settle.
struct ManagedOperationFixture {
    id: OperationId,
    lane: LaneKey,
    _release: oneshot::Sender<()>,
}

impl ManagedOperationFixture {
    fn start(engine: &TestOperations, tag: &str) -> Self {
        let lane = LaneKey::new(format!("{tag}-lane"));
        Self::start_on_lane(engine, tag, lane)
    }

    /// Same, on a caller-supplied lane, so a second operation can be made to
    /// queue behind the first.
    fn start_on_lane(engine: &TestOperations, tag: &str, lane: LaneKey) -> Self {
        let id = OperationId::from(tag);
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let settle_id = id.clone();
        engine.manager().spawn_managed(
            OperationDescriptor {
                operation_id: id.clone(),
                operation_type: WriteOperationType::Copy,
                lanes: vec![lane.clone()],
                volume_ids: vec![],
                summary: OperationSummaryText::default(),
                rollback_support: crate::types::RollbackSupport::Supported,
                preview_id: None,
                reverses: None,
            },
            engine.new_state(),
            Box::new(move |ops: Operations| {
                Box::pin(async move {
                    let _ = release_rx.await;
                    ops.manager().on_settled(&settle_id);
                })
            }),
        );
        engine.status().register(id.as_str(), WriteOperationType::Copy, vec![]);
        Self {
            id,
            lane,
            _release: release_tx,
        }
    }
}

/// The bug the `lifecycle` field exists to prevent: a paused operation keeps its
/// registry entry (it still holds its lane slots and parks between
/// files), so the presence test this replaced answered "running" while parked.
/// Every surface that steers off the answer — the pause/resume toggle above all
/// — then tried to pause what was already parked.
#[test]
fn a_paused_operation_reports_paused_not_running() {
    let engine = TestOperations::new();
    let op = ManagedOperationFixture::start(&engine, "paused-lifecycle");
    assert_eq!(
        engine.manager().lifecycle_status(&op.id),
        Some(LifecycleStatus::Running),
        "precondition: a fresh lane admits the operation immediately"
    );

    assert_eq!(engine.manager().set_paused(&op.id, true), PauseOutcome::Applied);

    let status = engine
        .get_operation_status(&op.id)
        .expect("a paused operation is still in the status cache");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Paused));
    // The state-map entry is still there, which is exactly why the old boolean
    // got this backwards.
    assert!(engine.registry().contains(&op.id));
}

/// The other half of the same bug, and the one that shows how far the boolean
/// missed: `spawn_managed` inserts the state entry BEFORE admission, so an
/// operation still waiting for its lane — one that has not written a byte and
/// may never be admitted — was reported as running too.
#[test]
fn a_queued_operation_reports_queued() {
    let engine = TestOperations::new();
    let holder = ManagedOperationFixture::start(&engine, "queued-holder");
    // A second operation on the SAME lane can't be admitted until the first frees it.
    let queued = ManagedOperationFixture::start_on_lane(&engine, "queued-waiter", holder.lane.clone());

    assert_eq!(
        engine.manager().lifecycle_status(&queued.id),
        Some(LifecycleStatus::Queued)
    );
    let status = engine
        .get_operation_status(&queued.id)
        .expect("a queued operation has a status-cache row");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Queued));
    assert!(
        engine.registry().contains(&queued.id),
        "the state entry lands at spawn, not at admission — which is exactly why a presence test called this one running"
    );
}

#[test]
fn update_operation_status_overwrites_fields() {
    let engine = TestOperations::new();
    let id = OperationId::from("update");
    engine.status().register(id.as_str(), WriteOperationType::Move, vec![]);
    engine.status().update(
        id.as_str(),
        WriteOperationPhase::Copying,
        Some("a.txt".into()),
        3,
        10,
        500,
        1000,
    );
    let status = engine.get_operation_status(&id).unwrap();
    assert_eq!(status.phase, WriteOperationPhase::Copying);
    assert_eq!(status.current_file.as_deref(), Some("a.txt"));
    assert_eq!(status.files_done, 3);
    assert_eq!(status.files_total, 10);
    assert_eq!(status.bytes_done, 500);
    assert_eq!(status.bytes_total, 1000);
    engine.status().unregister(id.as_str());
}

// ---- activity on the snapshot (why isn't this moving?) ----

#[test]
fn a_parked_operation_names_its_wait_on_a_status_read() {
    // The sharp one: no `write-progress` event in flight, and none ever emitted.
    // A poller that missed the transient event — an agent reading `cmdr://state`
    // for the first time — still has to learn that this operation is parked on a
    // clash, and that it is a clash rather than a dead mount.
    let engine = TestOperations::new();
    let op = install_state(&engine, "parked-conflict", OperationIntent::Running);
    let id = op.id().clone();
    engine.status().register(id.as_str(), WriteOperationType::Copy, vec![]);

    // Nobody is being asked anything, and this operation keeps no in-flight
    // table, so it has nothing to claim. Absent, ❌ never a made-up `moving`.
    assert!(
        engine.get_operation_status(&id).unwrap().activity.is_none(),
        "an operation with no probe and nobody being asked reports no activity"
    );

    let (tx, _rx) = oneshot::channel();
    op.state().conflict_slot.arm(tx, placeholder_conflict);

    let activity = engine
        .get_operation_status(&id)
        .unwrap()
        .activity
        .expect("a parked operation answers for its own wait");
    assert_eq!(activity.waiting_on, TransferWaitReason::Conflict);
    assert_eq!(activity.in_flight, 0, "no in-flight table, so no honest count");
    assert_eq!(
        activity.still_for_seconds, 0,
        "a parked operation has been still for nobody's time but the answerer's"
    );

    engine.status().unregister(id.as_str());
}

#[test]
fn a_paused_operation_reads_as_paused_rather_than_stalled() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "parked-pause", OperationIntent::Running);
    let id = op.id().clone();
    engine.status().register(id.as_str(), WriteOperationType::Copy, vec![]);

    op.state().pause_gate.pause();

    let activity = engine
        .get_operation_status(&id)
        .unwrap()
        .activity
        .expect("paused is a wait");
    assert_eq!(activity.waiting_on, TransferWaitReason::Paused);

    engine.status().unregister(id.as_str());
}

#[test]
fn a_settled_operation_reports_no_activity() {
    // The status cache outlives registry, and once the state entry
    // is gone there is nothing left to classify.
    let engine = TestOperations::new();
    let op = install_state(&engine, "settled", OperationIntent::Running);
    let id = op.id().clone();
    engine.status().register(id.as_str(), WriteOperationType::Copy, vec![]);
    let (tx, _rx) = oneshot::channel();
    op.state().conflict_slot.arm(tx, placeholder_conflict);
    engine.registry().forget(&id);

    let status = engine
        .get_operation_status(&id)
        .expect("the cache row outlives the state");
    assert!(status.activity.is_none(), "a settled operation waits on nothing");

    engine.status().unregister(id.as_str());
}

#[test]
fn update_unknown_id_is_a_silent_noop() {
    // Pins the `&& get_mut` short-circuit. If `&&` becomes `||`, this would
    // dereference a None and panic.
    let engine = TestOperations::new();
    engine
        .status()
        .update("no-such-op-xyzzy", WriteOperationPhase::Copying, None, 0, 0, 0, 0);
}

#[test]
fn indeterminate_compress_phases_clear_both_percent_fallback_axes() {
    let engine = TestOperations::new();
    let id = OperationId::from("compress-finishing");
    engine
        .status()
        .register(id.as_str(), WriteOperationType::Compress, vec![]);
    for phase in [
        WriteOperationPhase::FinishingCompression,
        WriteOperationPhase::FinishingTransfer,
    ] {
        engine.status().update(id.as_str(), phase, None, 0, 0, 0, 0);
        let status = engine.get_operation_status(&id).expect("operation status");
        assert_eq!((status.files_done, status.files_total), (0, 0));
        assert_eq!((status.bytes_done, status.bytes_total), (0, 0));
    }
    engine.status().unregister(id.as_str());
}

// ---- `volumes-busy-changed` and the transfer gauge ----

/// `volumes-busy-changed` fires once per distinct change of the busy SET: a
/// progress update, a second op on an already-busy volume, a `root`-only op,
/// and releasing one of two holders all leave membership alone and stay quiet.
///
/// Its subject is the engine-wide dedupe (the busy set's last-reported value),
/// so it runs on an engine of its own.
#[test]
fn volumes_busy_changed_fires_once_per_distinct_change() {
    let engine = TestOperations::new();
    let volume = "busy-dedupe-volume".to_string();
    let (first, second, local) = ("busy-1".to_string(), "busy-2".to_string(), "busy-root".to_string());
    let emits = engine.status().busy_emit_count();

    engine
        .status()
        .register(&first, WriteOperationType::Copy, vec![volume.clone()]);
    assert_eq!(engine.status().busy_emit_count(), emits + 1, "the volume became busy");
    engine
        .status()
        .update(&first, WriteOperationPhase::Copying, None, 1, 2, 10, 20);
    engine
        .status()
        .register(&second, WriteOperationType::Copy, vec![volume.clone()]);
    engine
        .status()
        .register(&local, WriteOperationType::Copy, vec!["root".to_string()]);
    engine.status().unregister(&first);
    assert_eq!(
        engine.status().busy_emit_count(),
        emits + 1,
        "nothing changed the set's membership"
    );
    engine.status().unregister(&second);
    assert_eq!(engine.status().busy_emit_count(), emits + 2, "the last holder left");
    engine.status().unregister(&local);
    assert_eq!(
        engine.status().busy_emit_count(),
        emits + 2,
        "`root` was never in the set"
    );
}

/// Reports the busy set like [`RecordingEngineEvents`], but holds the first
/// report inside the call until the test releases it, so a second update can
/// race it.
///
/// [`RecordingEngineEvents`]: super::super::host::testing::RecordingEngineEvents
struct GatedBusyEvents {
    reports: std::sync::Mutex<Vec<Vec<String>>>,
    first_entered: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    first_release: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl EngineEvents for GatedBusyEvents {
    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        let entered = self.first_entered.lock_ignore_poison().take();
        if let Some(entered) = entered {
            let release = self
                .first_release
                .lock_ignore_poison()
                .take()
                .expect("set with the sender");
            let _ = entered.send(());
            let _ = release.recv();
        }
        self.reports.lock_ignore_poison().push(volume_ids);
    }
    fn operations_changed(&self, _snapshot: crate::types::OperationsChanged) {}
    fn transfer_started(&self, _volume_ids: &[String]) {}
    fn transfer_finished(&self, _volume_ids: &[String]) {}
    fn leftovers_kept(&self, _notice: crate::types::LeftoversKeptEvent) {}
    fn instant_mutation_settled(&self, _fact: super::super::host::InstantMutationFact) {}
    fn error_report(&self, _report: super::super::host::OperationErrorReport) {}
    fn scan_wait_progress(&self, _event: crate::types::WriteProgressEvent) {}
}

/// The last busy set reported is the current one, however two updates race.
/// Here the first register's report is held mid-call while a second register
/// runs: if the second could report around it, the first's older set (missing
/// the second volume) would land last, and the volume picker would offer Eject
/// on a drive a transfer just started on.
#[test]
fn racing_updates_report_the_current_busy_set_last() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let events = Arc::new(GatedBusyEvents {
        reports: std::sync::Mutex::new(Vec::new()),
        first_entered: std::sync::Mutex::new(Some(entered_tx)),
        first_release: std::sync::Mutex::new(Some(release_rx)),
    });
    let cache = Arc::new(StatusCache::new(Arc::clone(&events) as Arc<dyn EngineEvents>));

    let first = {
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || cache.register("first-op", WriteOperationType::Copy, vec!["usb-a".to_string()]))
    };
    entered_rx.recv().expect("the first register reaches its report");

    let (second_done_tx, second_done_rx) = std::sync::mpsc::channel();
    let second = {
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || {
            cache.register("second-op", WriteOperationType::Copy, vec!["usb-b".to_string()]);
            let _ = second_done_tx.send(());
        })
    };
    // A racy cache lets the second register finish while the first is held, so
    // wait for that; an atomic one keeps it waiting behind the first report,
    // and the timeout only moves the release along. Neither way can it turn a
    // correct cache red.
    let _ = second_done_rx.recv_timeout(Duration::from_millis(500));
    release_tx.send(()).expect("the first report is waiting");
    first.join().expect("first register");
    second.join().expect("second register");

    let reports = events.reports.lock_ignore_poison();
    assert_eq!(
        reports.last(),
        Some(&vec!["usb-a".to_string(), "usb-b".to_string()]),
        "the busy set reported last must be the current one, got {reports:?}"
    );
}

/// How a synthetic managed op ends.
#[derive(Debug, Clone, Copy)]
enum Ending {
    Completes,
    Cancels,
    Fails,
    HandlerPanics,
}

/// Every transfer start an op reports has exactly one finish, however the op
/// ends: the engine's events hear one `transfer_started` while the handler
/// runs, and exactly one `transfer_finished` by the time it settles. (The app
/// turns the pair into indexing's "a transfer trumps me" gauge;
/// `operations_host/events.rs` pins that.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_transfer_start_has_exactly_one_finish_however_the_op_ends() {
    use crate::types::WriteOperationError;
    use crate::{CollectorEventSink, OperationEventSink};
    use std::sync::atomic::AtomicBool;

    for ending in [Ending::Completes, Ending::Cancels, Ending::Fails, Ending::HandlerPanics] {
        let engine = TestOperations::new();
        let volume = vec!["gauge-volume".to_string()];

        let events = Arc::new(CollectorEventSink::new());
        let started_inside = Arc::new(AtomicBool::new(false));
        let (seen, recorder, probe) = (Arc::clone(&started_inside), Arc::clone(engine.events()), volume.clone());
        engine
            .start_write_operation(
                Arc::clone(&events) as Arc<dyn OperationEventSink>,
                WriteOperationType::Copy,
                cmdr_operation_log::types::Initiator::User,
                200,
                volume.clone(),
                vec![LaneKey::new("gauge-lane".to_string())],
                OperationSummaryText::default(),
                None,
                1,
                None,
                None,
                move |_events, _op_id, _state| {
                    let started = *recorder.transfers_started.lock_ignore_poison() == vec![probe];
                    let finished = !recorder.transfers_finished.lock_ignore_poison().is_empty();
                    seen.store(started && !finished, Ordering::SeqCst);
                    match ending {
                        Ending::Completes => Ok(()),
                        Ending::Cancels => Err(WriteOperationError::Cancelled {
                            message: "stopped".into(),
                        }),
                        Ending::Fails => Err(WriteOperationError::IoError {
                            path: "/x".to_string(),
                            message: "gone".into(),
                        }),
                        Ending::HandlerPanics => panic!("simulated handler panic"),
                    }
                },
            )
            .await
            .expect("start");
        cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the op to settle", || {
            !events.settled.lock_ignore_poison().is_empty()
        })
        .await;

        assert!(started_inside.load(Ordering::SeqCst), "{ending:?}: busy while it ran");
        assert_eq!(
            *engine.events().transfers_started.lock_ignore_poison(),
            vec![volume.clone()],
            "{ending:?}: one start"
        );
        assert_eq!(
            *engine.events().transfers_finished.lock_ignore_poison(),
            vec![volume.clone()],
            "{ending:?}: its own start was finished, exactly once"
        );
    }
}

/// The panic net (`ManagedTaskGuard`, no `on_settled`) finishes the transfer
/// too, exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panicking_op_task_finishes_its_transfer_start_exactly_once() {
    use crate::manager::ManagedTaskGuard;

    let engine = TestOperations::new();
    let volume = vec!["gauge-panic-volume".to_string()];
    let op = OperationId::from("gauge-panic-op");
    let id = op.clone();
    engine.manager().spawn_managed(
        OperationDescriptor {
            operation_id: op.clone(),
            operation_type: WriteOperationType::Copy,
            lanes: vec![LaneKey::new("gauge-panic-lane".to_string())],
            volume_ids: volume.clone(),
            summary: OperationSummaryText::default(),
            rollback_support: crate::types::RollbackSupport::Supported,
            preview_id: None,
            reverses: None,
        },
        Arc::new(engine.new_operation_state(Duration::from_millis(200))),
        Box::new(move |ops: Operations| {
            Box::pin(async move {
                let _guard = ManagedTaskGuard::new(ops.manager(), id);
                panic!("simulated task panic");
            })
        }),
    );
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the panic net to unregister the op", || {
        engine.manager().lifecycle_status(&op).is_none() && engine.get_operation_status(&op).is_none()
    })
    .await;

    // A wait, not an assert: the net reports the finish just after it drops the
    // status row the wait above watched.
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the net to finish the op's own start", || {
        !engine.events().transfers_finished.lock_ignore_poison().is_empty()
    })
    .await;
    assert_eq!(
        *engine.events().transfers_started.lock_ignore_poison(),
        vec![volume.clone()]
    );
    assert_eq!(
        *engine.events().transfers_finished.lock_ignore_poison(),
        vec![volume],
        "the net finishes the op's start exactly once"
    );
}

/// A cache a panic poisoned still marks a transfer's volume busy, so Eject
/// stays refused mid-transfer. Each entry is a plain value: a panic can lose at
/// most the one update it interrupted, never tear the map.
#[test]
fn a_poisoned_cache_still_marks_a_running_transfers_volume_busy() {
    let cache = StatusCache::new(Arc::new(super::super::host::testing::RecordingEngineEvents::default()));
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = cache.entries.write_ignore_poison();
        panic!("a status update panicked, on purpose");
    }));
    assert!(poisoned.is_err() && cache.entries.is_poisoned());

    cache.register("poisoned-op", WriteOperationType::Copy, vec!["usb-stick".to_string()]);
    assert_eq!(
        cache.busy_volume_ids(),
        vec!["usb-stick".to_string()],
        "the volume a transfer touches is busy, poison or not"
    );
    cache.update("poisoned-op", WriteOperationPhase::Copying, None, 1, 2, 10, 20);
    assert_eq!(
        cache.snapshot("poisoned-op", None).map(|status| status.files_done),
        Some(1),
        "and its status still reads"
    );

    cache.unregister("poisoned-op");
    assert!(cache.busy_volume_ids().is_empty(), "until it's done");
}
