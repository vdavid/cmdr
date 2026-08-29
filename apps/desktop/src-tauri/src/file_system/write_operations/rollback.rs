//! Managed dispatch of an operation-log rollback.
//!
//! The rollback ENGINE (what an inverse does, and the data-safety rechecks) lives
//! in [`crate::operation_log::rollback`]; this thin glue is the only piece that
//! needs the [`OperationManager`](super::manager) (reachable only here, inside
//! `write_operations`), so it lives here. It spawns the inverse operation as a
//! MANAGED op — cancelable, lane-serialized, and shown in the queue like any
//! transfer — and bridges the manager's `OperationIntent` cancellation into the
//! engine's cancel predicate.
//!
//! Two entry points, for two different callers:
//!
//! - [`dispatch_rollback`] returns after DISPATCH, not after the reversal finishes:
//!   the inverse is an async managed op, so the caller polls the original op's
//!   `rollback_state` until it leaves `rolling_back` to observe the terminal result
//!   (the "dispatch then poll" contract). The MCP `operations_rollback` tool uses it.
//! - [`undo_operations`] reverses SEVERAL operations as one action, **newest first**,
//!   awaiting each before dispatching the next, and resolves with the whole tally. The
//!   frontend's undo (`commands::operation_log::undo_operations`) uses it: a user-facing
//!   Undo has to report what actually came back, which a dispatch can't say yet.
//!
//! This is also the ENGINE'S EXECUTOR ([`ReversalRunner`]). The engine plans (page
//! the journal, verify the snapshot, decide the act) and hands each decided act
//! here to be performed, because the cross-volume primitives and the managed op's
//! state live on this side of the boundary and `operation_log` must not import
//! them. The runner also owns the loop's three live answers: stop ([`StopMeans`]),
//! pause (the op's `PauseGate`), and progress (`write-progress` frames).

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use tokio::sync::oneshot;

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{DEFAULT_VOLUME_ID, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::rollback::{
    InverseAct, InversePlan, RollbackDispatch, RollbackProgress, RollbackRefusal, RollbackReport, RollbackRunner,
    SkipBreakdown, execute_rollback, inverse_kind, rollback_operation, undo_order,
};
use crate::operation_log::store::{open_read_connection, read_operation};
use crate::operation_log::types::{Initiator, OpKind, RollbackState};
use crate::operation_log::writer::OperationLogWriter;

use super::event_sinks::OperationEventSink;
use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText, manager};
use super::state::{StopMeans, WriteOperationState, update_operation_status};
use super::transfer::volume::move_file_across_volumes;
use super::types::{DEFAULT_PROGRESS_INTERVAL_MS, WriteOperationPhase, WriteOperationType, WriteProgressEvent};

/// Map the inverse op's journal kind to the manager's `WriteOperationType` (for
/// the queue row + busy registration). The inverse kind is only ever delete /
/// move / rename (see [`inverse_kind`]).
fn write_op_type(kind: OpKind) -> WriteOperationType {
    match kind {
        OpKind::Move => WriteOperationType::Move,
        OpKind::Rename => WriteOperationType::Rename,
        // Copy/create/compress undo is a delete; the fallthrough can't be reached
        // for a real inverse kind.
        _ => WriteOperationType::Delete,
    }
}

// ── The engine's executor: what a decided act actually does ──────────────────

/// The counters the newest frame went out with, plus when it went out. The
/// mid-file byte callback builds its frames off this stand, so a single large
/// file's bar moves without the planner having to hear about every chunk.
struct FrameStand {
    files_done: u64,
    files_total: u64,
    bytes_done: u64,
    bytes_total: u64,
    current_name: Option<String>,
    last_emit: Option<Instant>,
}

/// The rollback engine's executor: performs each decided act with the volume
/// primitives, answers "should I stop?" off the managed op's intent, parks on its
/// pause gate, and turns the engine's per-item position into `write-progress`.
pub(crate) struct ReversalRunner {
    operation_id: String,
    operation_type: WriteOperationType,
    state: Arc<WriteOperationState>,
    events: Arc<dyn OperationEventSink>,
    stop: StopMeans,
    stand: std::sync::Mutex<FrameStand>,
}

impl ReversalRunner {
    pub(crate) fn new(
        operation_id: String,
        operation_type: WriteOperationType,
        state: Arc<WriteOperationState>,
        events: Arc<dyn OperationEventSink>,
        stop: StopMeans,
    ) -> Self {
        ReversalRunner {
            operation_id,
            operation_type,
            state,
            events,
            stop,
            stand: std::sync::Mutex::new(FrameStand {
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
                current_name: None,
                last_emit: None,
            }),
        }
    }

    /// Send a frame if the throttle allows it, or if it's one that must go out:
    /// the first (so a bar appears immediately) and the one that lands on the
    /// total (so a throttled run still ends full). `in_flight_bytes` are the bytes
    /// of the item currently streaming, which no counter has banked yet.
    fn frame(&self, in_flight_bytes: u64, must_send: bool) {
        let event = {
            let mut stand = self.stand.lock_ignore_poison();
            let due = must_send
                || stand
                    .last_emit
                    .is_none_or(|at| at.elapsed() >= self.state.progress_interval);
            if !due {
                return;
            }
            stand.last_emit = Some(Instant::now());
            WriteProgressEvent::new(
                self.operation_id.clone(),
                self.operation_type,
                WriteOperationPhase::RollingBack,
                stand.current_name.clone(),
                stand.files_done as usize,
                stand.files_total as usize,
                stand.bytes_done + in_flight_bytes,
                stand.bytes_total,
            )
        };
        update_operation_status(
            &self.operation_id,
            WriteOperationPhase::RollingBack,
            event.current_file.clone(),
            event.files_done,
            event.files_total,
            event.bytes_done,
            event.bytes_total,
        );
        self.state.emit_progress_via_sink(self.events.as_ref(), event);
    }
}

impl RollbackRunner for ReversalRunner {
    fn perform<'a>(
        &'a self,
        act: InverseAct<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            match act {
                // One node each, and the engine has already established that it may
                // go: a file verified unchanged, a directory verified empty. Neither
                // recurses.
                InverseAct::RemoveFile { volume, path } | InverseAct::RemoveDir { volume, path } => {
                    volume.delete(path).await
                }
                InverseAct::Restore {
                    from,
                    from_path,
                    to,
                    to_path,
                    same_volume,
                    force,
                } => {
                    if same_volume {
                        // A same-FS move / rename-back / trash-restore: one rename,
                        // atomic, nothing to stream.
                        from.rename(from_path, to_path, force).await
                    } else {
                        // Cross-volume: the staged per-file move, which is what buys
                        // mid-file cancel, byte progress, a `.cmdr-tmp-*` landing,
                        // retry, and stall detection. The callback carries BOTH: the
                        // stop travels to the backend as a `Break`, and the bytes so
                        // far move the bar inside one large file.
                        let on_progress = |written: u64, _total: u64| {
                            if self.should_stop() {
                                return ControlFlow::Break(());
                            }
                            self.frame(written, false);
                            ControlFlow::Continue(())
                        };
                        move_file_across_volumes(from, from_path, to, to_path, &self.state, &on_progress)
                            .await
                            .map(|_bytes| ())
                    }
                }
            }
        })
    }

    fn should_stop(&self) -> bool {
        self.stop.requested(&self.state.intent)
    }

    fn wait_while_paused(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.state
                .pause_gate
                .wait_while_paused_until(&|| self.should_stop())
                .await;
        })
    }

    fn report_progress(&self, progress: RollbackProgress<'_>) {
        let must_send = {
            let mut stand = self.stand.lock_ignore_poison();
            stand.files_done = progress.files_done;
            stand.files_total = progress.files_total;
            stand.bytes_done = progress.bytes_done;
            stand.bytes_total = progress.bytes_total;
            stand.current_name = progress.current_name.map(str::to_string);
            // The first frame, and the one that reaches the total: a bar that
            // appears at once and ends full, whatever the throttle did between.
            stand.last_emit.is_none() || (progress.files_total > 0 && progress.files_done >= progress.files_total)
        };
        self.frame(0, must_send);
    }
}

/// Roll back operation `op_id`: gate it, set it `rolling_back`, and spawn its
/// inverse as a managed op. Returns the inverse op's id (the reversal runs
/// asynchronously; poll the original op's `rollback_state` for the terminal
/// result). A refusal (unknown / already rolling back / not rollbackable / a
/// volume disconnected) surfaces typed; the entry resets `rolling_back` on a
/// synchronous spawn failure so a retry isn't wedged.
///
/// `events` is built at the IPC/MCP edge like every other managed op's sink: the
/// pipeline in here never constructs one (see `mod.rs`).
pub fn dispatch_rollback<R: Runtime>(
    app: &AppHandle<R>,
    op_id: &str,
    initiator: Initiator,
    events: Arc<dyn OperationEventSink>,
) -> Result<RollbackDispatch, RollbackRefusal> {
    dispatch_inverse(app, op_id, initiator, events, None)
}

/// [`dispatch_rollback`] plus a channel that resolves with the inverse's
/// [`RollbackReport`] once the reversal finishes.
///
/// This is what lets a caller reverse several operations IN ORDER (see
/// [`undo_operations`]) and report a complete tally, rather than dispatching them
/// all and hoping the queue reverses them in the right sequence. The reversal is
/// still a normal managed op — queued, lane-serialized, and cancelable — so
/// awaiting it also waits out anything already holding the lane.
fn dispatch_inverse_reported<R: Runtime>(
    app: &AppHandle<R>,
    op_id: &str,
    initiator: Initiator,
    events: Arc<dyn OperationEventSink>,
) -> Result<oneshot::Receiver<RollbackReport>, RollbackRefusal> {
    let (report_tx, report_rx) = oneshot::channel();
    dispatch_inverse(app, op_id, initiator, events, Some(report_tx))?;
    Ok(report_rx)
}

fn dispatch_inverse<R: Runtime>(
    app: &AppHandle<R>,
    op_id: &str,
    initiator: Initiator,
    events: Arc<dyn OperationEventSink>,
    report: Option<oneshot::Sender<RollbackReport>>,
) -> Result<RollbackDispatch, RollbackRefusal> {
    // The writer lives in managed state (the durable store). Its absence means the journal never
    // opened, so there's nothing to roll back.
    let writer = app
        .try_state::<OperationLogWriter>()
        .map(|s| s.inner().clone())
        .ok_or(RollbackRefusal::UnknownOperation)?;
    let vm = get_volume_manager();

    let plan = rollback_operation(vm, &writer, op_id, |plan| {
        spawn_managed_inverse(&writer, plan, initiator, Arc::clone(&events), report)
    })?;
    Ok(RollbackDispatch {
        inverse_op_id: plan.inverse_op_id,
    })
}

/// Register the inverse operation with the manager. Runs synchronously inside
/// [`rollback_operation`]'s spawn hook: a volume that dropped between the gate and
/// here is a synchronous spawn failure (Finding 3) — returned typed so the entry
/// resets `rolling_back`.
fn spawn_managed_inverse(
    writer: &OperationLogWriter,
    plan: &InversePlan,
    initiator: Initiator,
    events: Arc<dyn OperationEventSink>,
    report: Option<oneshot::Sender<RollbackReport>>,
) -> Result<(), RollbackRefusal> {
    let vm = get_volume_manager();
    let original = plan.original.clone();
    let inverse_op_id = plan.inverse_op_id.clone();

    // Resolve the lanes + ejectable volume ids from the volumes the op touches.
    // A missing volume here is the sync spawn failure.
    let mut lanes = Vec::new();
    let mut volume_ids = Vec::new();
    for id in [original.source_volume_id.as_deref(), original.dest_volume_id.as_deref()]
        .into_iter()
        .flatten()
    {
        match vm.get(id) {
            Some(volume) => {
                let lane = volume.lane_key();
                if !lanes.contains(&lane) {
                    lanes.push(lane);
                }
                if id != DEFAULT_VOLUME_ID && !volume_ids.contains(&id.to_string()) {
                    volume_ids.push(id.to_string());
                }
            }
            None => {
                return Err(RollbackRefusal::VolumeUnavailable {
                    volume_id: id.to_string(),
                });
            }
        }
    }
    if lanes.is_empty() {
        lanes.push(crate::file_system::volume::LaneKey::new(DEFAULT_VOLUME_ID));
    }

    let op_type = write_op_type(inverse_kind(original.kind));
    let descriptor = OperationDescriptor {
        operation_id: inverse_op_id.clone(),
        operation_type: op_type,
        lanes,
        volume_ids,
        // Where the reversal will act, read off the newest journal row at dispatch
        // (`operation_log::rollback::summarize_inverse`), so the queue row isn't
        // nameless while it works.
        summary: OperationSummaryText {
            source: plan.summary.from.clone(),
            destination: plan.summary.to.clone(),
        },
        // This IS the reversal. Offering to roll back a rollback would ask the
        // engine to re-apply what the person just chose to undo.
        supports_rollback: false,
        // No scan preview: nothing walked a tree to plan this op.
        preview_id: None,
    };
    // The cadence every other transfer's bar runs at. A zero interval would mean
    // no throttle at all, and a million-item reversal emitting an event per item.
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(
        DEFAULT_PROGRESS_INTERVAL_MS,
    )));

    let writer = writer.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let guard = ManagedTaskGuard::new(inverse_op_id.clone());
            // The engine's hands and its three live answers. The reversal is its OWN
            // managed operation, opened `Running`, so any move off `Running` is
            // somebody stopping the reversal itself.
            let runner = ReversalRunner::new(
                inverse_op_id.clone(),
                op_type,
                state_for_op,
                events,
                StopMeans::IntentLeavesRunning,
            );
            let vm = get_volume_manager();
            let outcome = execute_rollback(vm, &writer, &original, &inverse_op_id, initiator, &runner).await;
            // Hand the tally to a caller awaiting it (a multi-operation undo). A
            // dropped receiver (the caller went away) is not a problem: the reversal
            // already happened and is journaled.
            if let Some(report) = report {
                let _ = report.send(outcome);
            }
            guard.disarm();
            manager().on_settled(&inverse_op_id);
        })
    };

    manager().spawn_managed(descriptor, state, Box::new(deferred));
    Ok(())
}

// ── Undoing a job: several operations, newest first ───────────────────────────

/// What one operation contributed to a multi-operation undo. `refusal` is set when
/// the operation never ran its inverse at all (already undone, a volume gone), in
/// which case both counts are zero — a refusal is never reported as a silent zero.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationUndoOutcome {
    pub operation_id: String,
    /// Items restored (or already back where they belong — an idempotent re-issue).
    pub restored: u64,
    /// Items left alone: they changed since, or their old name is taken. Never a
    /// forced overwrite.
    pub skipped: u64,
    /// WHICH reason left which file alone, one group per typed reason, each with its
    /// complete count and one example file name. Lets the UI say what happened to a
    /// specific file instead of naming a reason class for the whole batch. Empty on a
    /// refusal (nothing was examined) and on a clean undo.
    pub skips: Vec<SkipBreakdown>,
    /// The state the operation resolves to, absent on a refusal.
    pub final_state: Option<RollbackState>,
    pub refusal: Option<RollbackRefusal>,
}

/// The whole job's undo result: a per-operation breakdown plus the totals the UI
/// leads with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    /// **In the order the operations were reversed: newest first** (see
    /// [`undo_order`]).
    pub operations: Vec<OperationUndoOutcome>,
    pub restored: u64,
    pub skipped: u64,
}

/// Undo a job's operations — a multi-batch rename run, most importantly — as one
/// action.
///
/// **Reverses NEWEST FIRST, one at a time**, per [`undo_order`]: a later batch can
/// have renamed a file into a name an earlier batch freed, and oldest-first would
/// then hit an occupied restore target and skip that file silently. Each inverse is
/// awaited before the next is dispatched, so the order is this function's
/// guarantee rather than the operation queue's admission order.
///
/// `operation_ids` arrives in the order the caller APPLIED the operations (that's
/// what breaks a same-second tie). An id with no journal row is reported as an
/// `UnknownOperation` refusal rather than dropped, so the caller's count of
/// operations always matches what came back (invariant 9).
pub async fn undo_operations<R: Runtime>(
    app: &AppHandle<R>,
    operation_ids: &[String],
    initiator: Initiator,
    events: Arc<dyn OperationEventSink>,
) -> UndoReport {
    let mut report = UndoReport {
        operations: Vec::with_capacity(operation_ids.len()),
        restored: 0,
        skipped: 0,
    };
    let (rows, mut unknown) = read_undoable_rows(app, operation_ids);

    for op in undo_order(rows) {
        let outcome = match dispatch_inverse_reported(app, &op.op_id, initiator, Arc::clone(&events)) {
            // A dropped sender means the inverse task died without reporting (a
            // panic). The reversal's own journaling is authoritative either way, so
            // report nothing reversed rather than inventing a tally.
            Ok(report_rx) => match report_rx.await {
                Ok(run) => OperationUndoOutcome {
                    operation_id: op.op_id.clone(),
                    restored: run.reversed,
                    skipped: run.skipped,
                    skips: run.skips,
                    final_state: Some(run.final_state),
                    refusal: None,
                },
                Err(_) => refused(&op.op_id, RollbackRefusal::UnknownOperation),
            },
            Err(refusal) => refused(&op.op_id, refusal),
        };
        report.restored += outcome.restored;
        report.skipped += outcome.skipped;
        report.operations.push(outcome);
    }
    // Unknown ids carry no start time to order by, so they land last; they reversed
    // nothing, so their position changes no outcome.
    report.operations.append(&mut unknown);
    report
}

fn refused(op_id: &str, refusal: RollbackRefusal) -> OperationUndoOutcome {
    OperationUndoOutcome {
        operation_id: op_id.to_string(),
        restored: 0,
        skipped: 0,
        // A refused operation examined no items, so it has no per-file reasons — the
        // typed `refusal` is the whole story.
        skips: Vec::new(),
        final_state: None,
        refusal: Some(refusal),
    }
}

/// Read each requested operation's journal row, keeping the caller's order.
/// Anything unreadable becomes an `UnknownOperation` outcome instead of vanishing.
fn read_undoable_rows<R: Runtime>(
    app: &AppHandle<R>,
    operation_ids: &[String],
) -> (
    Vec<crate::operation_log::store::OperationRow>,
    Vec<OperationUndoOutcome>,
) {
    let Some(writer) = app.try_state::<OperationLogWriter>() else {
        let missing = operation_ids
            .iter()
            .map(|id| refused(id, RollbackRefusal::UnknownOperation))
            .collect();
        return (Vec::new(), missing);
    };
    let Ok(conn) = open_read_connection(writer.db_path()) else {
        let missing = operation_ids
            .iter()
            .map(|id| refused(id, RollbackRefusal::UnknownOperation))
            .collect();
        return (Vec::new(), missing);
    };
    let mut rows = Vec::with_capacity(operation_ids.len());
    let mut unknown = Vec::new();
    for op_id in operation_ids {
        match read_operation(&conn, op_id) {
            Ok(Some(row)) => rows.push(row),
            _ => unknown.push(refused(op_id, RollbackRefusal::UnknownOperation)),
        }
    }
    (rows, unknown)
}

// ── The test fixture for a live reversal ─────────────────────────────────────

/// A reversal a test can watch, stop, and park.
///
/// It carries what the manager would have registered — an operation state under a
/// unique id — plus a sink that keeps every frame and the PRODUCTION executor over
/// both. Registering the state is what lets a test stop or pause a running
/// reversal through the very calls the queue window makes
/// (`cancel_write_operation` / `pause_operation`) instead of reaching into the
/// intent atom, which `docs/testing.md` names as the anti-pattern for this hot
/// spot. `pub(crate)` because the engine's own suite (in `operation_log::rollback`)
/// drives the planner and this executor as the pair they are.
#[cfg(test)]
pub(crate) struct Reversal {
    guard: super::test_support::TestOperationGuard,
    events: Arc<super::CollectorEventSink>,
    runner: ReversalRunner,
}

#[cfg(test)]
impl Reversal {
    /// A reversal that behaves like a dispatched one: its own `Running` operation,
    /// reporting every frame it's given.
    ///
    /// The unthrottled interval is deliberate: a test that has to CATCH a reversal
    /// mid-file needs to see the bytes move, and an in-memory volume finishes a
    /// megabyte inside any real interval. The throttle itself is pinned separately.
    pub(crate) fn new(tag: &str) -> Self {
        Self::emitting_every(tag, Duration::ZERO, StopMeans::IntentLeavesRunning)
    }

    /// [`Self::new`] with an explicit progress interval and reading of "stop".
    pub(crate) fn emitting_every(tag: &str, progress_interval: Duration, stop: StopMeans) -> Self {
        let state = Arc::new(WriteOperationState::new(progress_interval));
        let guard = super::test_support::TestOperationGuard::register_state(tag, Arc::clone(&state));
        let events = Arc::new(super::CollectorEventSink::new());
        let runner = ReversalRunner::new(
            guard.id().to_string(),
            WriteOperationType::Move,
            state,
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            stop,
        );
        Reversal { guard, events, runner }
    }

    /// The executor, for `execute_rollback`.
    pub(crate) fn runner(&self) -> &ReversalRunner {
        &self.runner
    }

    /// The id the reversal is registered under: what a test hands to
    /// `cancel_write_operation`, `pause_operation`, or `resume_operation`.
    pub(crate) fn op_id(&self) -> &str {
        self.guard.id()
    }

    /// Every progress frame emitted so far, oldest first.
    pub(crate) fn frames(&self) -> Vec<WriteProgressEvent> {
        use crate::ignore_poison::IgnorePoison;
        self.events.progress.lock_ignore_poison().clone()
    }

    /// Stop the reversal, through the call the queue window's Cancel makes.
    pub(crate) fn stop(&self) {
        super::state::cancel_write_operation(self.op_id(), false);
    }

    /// Park the reversal, through the call `pause_operation` makes on the live
    /// state (the manager half of that pair only flips the row's status).
    pub(crate) fn pause(&self) {
        assert!(
            super::state::pause_write_operation(self.op_id()),
            "the reversal's state must be registered for a pause to reach it"
        );
    }

    /// Let it go again, through `resume_operation`'s half of the same pair.
    pub(crate) fn resume(&self) {
        assert!(
            super::state::resume_write_operation(self.op_id()),
            "the reversal's state must be registered for a resume to reach it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two rules govern which frames go out, and both matter: the throttle keeps a
    /// million-item reversal from emitting an event per item, and the "landed on
    /// the total" exemption keeps a throttled run from ending on a stale frame that
    /// says 1 of 3.
    #[test]
    fn the_throttle_holds_frames_back_but_never_the_one_that_lands_on_the_total() {
        let reversal = Reversal::emitting_every(
            "progress-throttle",
            Duration::from_secs(600),
            StopMeans::IntentLeavesRunning,
        );
        let runner = reversal.runner();
        let report = |files_done: u64| RollbackProgress {
            files_done,
            files_total: 3,
            bytes_done: files_done,
            bytes_total: 3,
            current_name: None,
        };

        runner.report_progress(report(0));
        runner.report_progress(report(1));
        runner.report_progress(report(2));
        assert_eq!(
            reversal.frames().len(),
            1,
            "the first frame goes out at once; the interval holds the rest back"
        );

        runner.report_progress(report(3));
        let frames = reversal.frames();
        assert_eq!(frames.len(), 2, "the frame that reaches the total ignores the throttle");
        assert_eq!(frames[1].files_done, 3);
        assert_eq!(frames[1].phase, WriteOperationPhase::RollingBack);
    }
}
