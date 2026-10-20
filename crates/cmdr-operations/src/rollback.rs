//! Managed dispatch of an operation-log rollback.
//!
//! The rollback ENGINE (what an inverse does, and the data-safety rechecks) lives
//! in [`cmdr_operation_log::rollback`]; this thin glue is the only piece that
//! needs the [`OperationManager`](super::manager) (reachable only here, inside
//! `cmdr-operations`), so it lives here. It spawns the inverse operation as a
//! MANAGED op — cancelable, lane-serialized, and shown in the queue like any
//! transfer — and bridges the manager's `OperationIntent` cancellation into the
//! engine's cancel predicate.
//!
//! Two entry points, for two different callers:
//!
//! - [`Operations::dispatch_rollback`](super::Operations::dispatch_rollback) returns after DISPATCH, not after the reversal finishes:
//!   the inverse is an async managed op, so the caller polls the original op's
//!   `rollback_state` until it leaves `rolling_back` to observe the terminal result
//!   (the "dispatch then poll" contract). The MCP `operations_rollback` tool uses it.
//! - [`Operations::undo_operations`](super::Operations::undo_operations) reverses SEVERAL operations as one action, **newest first**,
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
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::Handle;
use tokio::sync::oneshot;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{DEFAULT_VOLUME_ID, Volume};
use cmdr_operation_log::rollback::{
    ActRefusal, InverseAct, InversePlan, RollbackDispatch, RollbackProgress, RollbackRefusal, RollbackReport,
    RollbackRunner, SkipBreakdown, execute_rollback, inverse_kind, rollback_operation, undo_order,
};
use cmdr_operation_log::store::{open_read_connection, read_operation};
use cmdr_operation_log::types::{Initiator, OpKind, OperationId, RollbackState};
use cmdr_operation_log::writer::OperationLogWriter;

use super::event_sinks::OperationEventSink;
use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::state::{StopMeans, WriteOperationState};
use super::transfer::volume::move_file_across_volumes;
use super::types::{DEFAULT_PROGRESS_INTERVAL_MS, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::unblocked_removal::{NodeKind, RemovalRefused};

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
///
/// The reversal is its OWN managed operation, opened `Running`, so any move off
/// `Running` is somebody stopping it ([`StopMeans::IntentLeavesRunning`]). That
/// reading is also a forward transfer's, which is what lets it hand its state to
/// the staged per-file move. A cancelled transfer's own wind-down reads
/// differently, and goes through `WindDown`.
pub struct ReversalRunner {
    operation_id: String,
    operation_type: WriteOperationType,
    state: Arc<WriteOperationState>,
    events: Arc<dyn OperationEventSink>,
    stand: std::sync::Mutex<FrameStand>,
}

impl ReversalRunner {
    pub(crate) fn new(
        operation_id: String,
        operation_type: WriteOperationType,
        state: Arc<WriteOperationState>,
        events: Arc<dyn OperationEventSink>,
    ) -> Self {
        ReversalRunner {
            operation_id,
            operation_type,
            state,
            events,
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
        self.state.scope().status().update(
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
    fn perform<'a>(&'a self, act: InverseAct<'a>) -> Pin<Box<dyn Future<Output = Result<(), ActRefusal>> + Send + 'a>> {
        Box::pin(async move {
            match act {
                // One node each, and the engine has already established that it may
                // go: a file verified unchanged, a directory verified empty. Neither
                // recurses.
                InverseAct::RemoveFile { volume, path } => {
                    delete_clearing_blockers(&self.state.scope().host().runtime(), volume, path, NodeKind::File).await
                }
                InverseAct::RemoveDir { volume, path } => {
                    let runtime = self.state.scope().host().runtime();
                    delete_clearing_blockers(&runtime, volume, path, NodeKind::Directory).await
                }
                InverseAct::Restore {
                    from,
                    from_path,
                    to,
                    to_path,
                    same_volume,
                    force,
                } => {
                    // Put the folder back before the file. A move of a FOLDER
                    // empties its source tree and removes it, so the original
                    // location no longer exists — and a rename into a missing
                    // parent fails with ENOENT, which reads as "already gone" and
                    // counts the item as REVERSED. A reversal reporting success
                    // having restored nothing is the one outcome this engine may
                    // never produce. Creating a directory only ever adds, so it
                    // can't take anything away from the user.
                    if let Some(parent) = to_path.parent() {
                        to.create_directory_all(parent).await?;
                    }
                    // ❗ On an object store a rename of a big file isn't one call
                    // (`Volume::rename_work`), and `rename` refuses it: that one
                    // goes back the way a cross-volume one does, copied (on the
                    // server, where it can) and then deleted.
                    let renames_in_one_call = same_volume
                        && matches!(
                            from.rename_work(from_path).await,
                            Ok(cmdr_fs::volume::RenameWork::OneCall)
                        );
                    if renames_in_one_call {
                        // A same-FS move / rename-back / trash-restore: one rename,
                        // atomic, nothing to stream.
                        Ok(from.rename(from_path, to_path, force).await?)
                    } else {
                        // Cross-volume, or a rename that copies: the staged per-file move, which is what buys
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
                        Ok(
                            move_file_across_volumes(from, from_path, to, to_path, &self.state, &on_progress)
                                .await
                                .map(|_bytes| ())?,
                        )
                    }
                }
            }
        })
    }

    fn should_stop(&self) -> bool {
        StopMeans::IntentLeavesRunning.requested(self.state.intent())
    }

    fn pace_between_items(&self) -> Option<Duration> {
        self.state.scope().host().pacing().rollback_pace()
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

/// The planner verified the item as this operation's own right before this
/// act, so what the removal clears is what the copy carried over
/// (`unblocked_removal.rs`).
async fn delete_clearing_blockers(
    runtime: &Handle,
    volume: &Arc<dyn Volume>,
    path: &Path,
    kind: NodeKind,
) -> Result<(), ActRefusal> {
    super::unblocked_removal::delete_clearing_blockers(runtime, volume, path, kind)
        .await
        .map_err(|refused| match refused {
            RemovalRefused::Volume(e) => ActRefusal::Volume(e),
            RemovalRefused::Locked => ActRefusal::Locked,
        })
}

impl crate::Operations {
    /// Roll back operation `op_id`: gate it, set it `rolling_back`, and spawn its
    /// inverse as a managed op. Returns the inverse op's id (the reversal runs
    /// asynchronously; poll the original op's `rollback_state` for the terminal
    /// result). A refusal (unknown / already rolling back / not rollbackable / a
    /// volume disconnected) surfaces typed; the entry resets `rolling_back` on a
    /// synchronous spawn failure so a retry isn't wedged.
    ///
    /// `events` is built at the IPC/MCP edge like every other managed op's sink: the
    /// pipeline in here never constructs one (see `mod.rs`).
    pub fn dispatch_rollback(
        &self,
        op_id: &OperationId,
        initiator: Initiator,
        events: Arc<dyn OperationEventSink>,
    ) -> Result<RollbackDispatch, RollbackRefusal> {
        self.dispatch_inverse(op_id.as_str(), initiator, events, None)
    }

    /// [`Self::dispatch_rollback`] plus a channel that resolves with the inverse's
    /// [`RollbackReport`] once the reversal finishes.
    ///
    /// This is what lets a caller reverse several operations IN ORDER (see
    /// [`Self::undo_operations`]) and report a complete tally, rather than dispatching them
    /// all and hoping the queue reverses them in the right sequence. The reversal is
    /// still a normal managed op — queued, lane-serialized, and cancelable — so
    /// awaiting it also waits out anything already holding the lane.
    fn dispatch_inverse_reported(
        &self,
        op_id: &str,
        initiator: Initiator,
        events: Arc<dyn OperationEventSink>,
    ) -> Result<oneshot::Receiver<RollbackReport>, RollbackRefusal> {
        let (report_tx, report_rx) = oneshot::channel();
        self.dispatch_inverse(op_id, initiator, events, Some(report_tx))?;
        Ok(report_rx)
    }

    fn dispatch_inverse(
        &self,
        op_id: &str,
        initiator: Initiator,
        events: Arc<dyn OperationEventSink>,
        report: Option<oneshot::Sender<RollbackReport>>,
    ) -> Result<RollbackDispatch, RollbackRefusal> {
        // No writer means the journal never opened, so there's nothing to roll back.
        let writer = self.operation_log().writer().ok_or(RollbackRefusal::UnknownOperation)?;
        let vm = self.host().volumes();

        let plan = rollback_operation(vm, &writer, op_id, |plan| {
            self.spawn_managed_inverse(&writer, plan, initiator, Arc::clone(&events), report)
        })?;
        Ok(RollbackDispatch {
            inverse_op_id: plan.inverse_op_id,
        })
    }

    /// Register the inverse operation with the manager. Runs synchronously inside
    /// [`rollback_operation`]'s spawn hook: a volume that dropped between the gate and
    /// here is a synchronous spawn failure — returned typed so the entry
    /// resets `rolling_back`.
    fn spawn_managed_inverse(
        &self,
        writer: &OperationLogWriter,
        plan: &InversePlan,
        initiator: Initiator,
        events: Arc<dyn OperationEventSink>,
        report: Option<oneshot::Sender<RollbackReport>>,
    ) -> Result<(), RollbackRefusal> {
        let vm = self.host().volumes();
        let original = plan.original.clone();
        let inverse_op_id = OperationId::from(plan.inverse_op_id.clone());

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
            lanes.push(cmdr_fs::volume::LaneKey::new(DEFAULT_VOLUME_ID));
        }

        let op_type = write_op_type(inverse_kind(original.kind));
        let descriptor = OperationDescriptor {
            operation_id: inverse_op_id.clone(),
            operation_type: op_type,
            lanes,
            volume_ids,
            // Where the reversal will act, read off the newest journal row at dispatch
            // (`cmdr_operation_log::rollback::summarize_inverse`), so the queue row isn't
            // nameless while it works.
            summary: OperationSummaryText {
                source: plan.summary.from.clone(),
                destination: plan.summary.to.clone(),
                items: None,
            },
            // This IS the reversal. Offering to roll back a rollback would ask the
            // engine to re-apply what the person just chose to undo.
            rollback_support: crate::types::RollbackSupport::IsReversal,
            // No scan preview: nothing walked a tree to plan this op.
            preview_id: None,
            // The ORIGINAL kind, so every surface naming this op words it by what the
            // reversal will DO ("Putting files back" for a move, not "Moving"). The
            // inverse kind alone can't say that: `Move` covers undoing a move AND
            // undoing a trash, and `Delete` covers undoing a copy, a new folder, and
            // a compress.
            reverses: Some(original.kind),
        };
        // The cadence every other transfer's bar runs at. A zero interval would mean
        // no throttle at all, and a million-item reversal emitting an event per item.
        let state = Arc::new(self.new_operation_state(Duration::from_millis(DEFAULT_PROGRESS_INTERVAL_MS)));

        let writer = writer.clone();
        let state_for_op = Arc::clone(&state);
        let deferred = move |ops: crate::Operations| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let guard = ManagedTaskGuard::new(ops.manager(), inverse_op_id.clone());
                // The engine's hands and its three live answers. The reversal is its OWN
                // managed operation, opened `Running`, so any move off `Running` is
                // somebody stopping the reversal itself.
                let runner = ReversalRunner::new(inverse_op_id.to_string(), op_type, state_for_op, events);
                let vm = ops.host().volumes();
                let outcome =
                    execute_rollback(vm, &writer, &original, inverse_op_id.as_str(), initiator, &runner).await;
                // Hand the tally to a caller awaiting it (a multi-operation undo). A
                // dropped receiver (the caller went away) is not a problem: the reversal
                // already happened and is journaled.
                if let Some(report) = report {
                    let _ = report.send(outcome);
                }
                guard.disarm();
                ops.manager().on_settled(&inverse_op_id);
            })
        };

        self.manager().spawn_managed(descriptor, state, Box::new(deferred));
        Ok(())
    }
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
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct UndoReport {
    /// **In the order the operations were reversed: newest first** (see
    /// [`undo_order`]).
    pub operations: Vec<OperationUndoOutcome>,
    pub restored: u64,
    pub skipped: u64,
}

impl UndoReport {
    /// Every one of `operation_ids` refused for `refusal`, nothing reversed: what
    /// a host with no engine to undo on answers.
    pub fn all_refused(operation_ids: &[OperationId], refusal: RollbackRefusal) -> Self {
        Self {
            operations: operation_ids
                .iter()
                .map(|id| refused(id.as_str(), refusal.clone()))
                .collect(),
            restored: 0,
            skipped: 0,
        }
    }
}

impl crate::Operations {
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
    pub async fn undo_operations(
        &self,
        operation_ids: &[OperationId],
        initiator: Initiator,
        events: Arc<dyn OperationEventSink>,
    ) -> UndoReport {
        let mut report = UndoReport {
            operations: Vec::with_capacity(operation_ids.len()),
            restored: 0,
            skipped: 0,
        };
        let (rows, mut unknown) = self.read_undoable_rows(operation_ids);

        for op in undo_order(rows) {
            let outcome = match self.dispatch_inverse_reported(&op.op_id, initiator, Arc::clone(&events)) {
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

impl crate::Operations {
    /// Read each requested operation's journal row, keeping the caller's order.
    /// Anything unreadable becomes an `UnknownOperation` outcome instead of vanishing.
    fn read_undoable_rows(
        &self,
        operation_ids: &[OperationId],
    ) -> (Vec<cmdr_operation_log::store::OperationRow>, Vec<OperationUndoOutcome>) {
        let Some(writer) = self.operation_log().writer() else {
            let missing = operation_ids
                .iter()
                .map(|id| refused(id.as_str(), RollbackRefusal::UnknownOperation))
                .collect();
            return (Vec::new(), missing);
        };
        let Ok(conn) = open_read_connection(writer.db_path()) else {
            let missing = operation_ids
                .iter()
                .map(|id| refused(id.as_str(), RollbackRefusal::UnknownOperation))
                .collect();
            return (Vec::new(), missing);
        };
        let mut rows = Vec::with_capacity(operation_ids.len());
        let mut unknown = Vec::new();
        for op_id in operation_ids {
            match read_operation(&conn, op_id.as_str()) {
                Ok(Some(row)) => rows.push(row),
                _ => unknown.push(refused(op_id.as_str(), RollbackRefusal::UnknownOperation)),
            }
        }
        (rows, unknown)
    }
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
/// spot. `pub(crate)` because the planner's end-to-end suite (`planner_tests`)
/// drives the planner and this executor as the pair they are.
#[cfg(any(test, feature = "testing"))]
pub struct Reversal {
    /// The engine its state is registered in, which the controls ask.
    engine: super::Operations,
    operation_id: OperationId,
    events: Arc<super::CollectorEventSink>,
    runner: ReversalRunner,
}

#[cfg(any(test, feature = "testing"))]
impl Reversal {
    /// A reversal that behaves like a dispatched one: its own `Running` operation,
    /// reporting every frame it's given.
    ///
    /// The unthrottled interval is deliberate: a test that has to CATCH a reversal
    /// mid-file needs to see the bytes move, and an in-memory volume finishes a
    /// megabyte inside any real interval. The throttle itself is pinned separately.
    ///
    /// Registered under `tag` on `engine`, the engine the operation it reverses
    /// ran on; the registration lives as long as that engine does.
    pub fn on(engine: &super::Operations, tag: &str) -> Self {
        Self::emitting_every(engine, tag, Duration::ZERO)
    }

    /// [`Self::on`] with an explicit progress interval.
    pub(crate) fn emitting_every(engine: &super::Operations, tag: &str, progress_interval: Duration) -> Self {
        let state = Arc::new(engine.new_operation_state(progress_interval));
        engine.registry().insert(tag.into(), Arc::clone(&state));
        Self::running(super::Operations::clone(engine), tag.to_string(), state)
    }

    fn running(engine: super::Operations, operation_id: String, state: Arc<WriteOperationState>) -> Self {
        let events = Arc::new(super::CollectorEventSink::new());
        let runner = ReversalRunner::new(
            operation_id.clone(),
            WriteOperationType::Move,
            state,
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
        );
        Reversal {
            engine,
            operation_id: operation_id.into(),
            events,
            runner,
        }
    }

    /// The executor, for `execute_rollback`.
    pub fn runner(&self) -> &ReversalRunner {
        &self.runner
    }

    /// The id the reversal is registered under: what a test hands to
    /// `cancel_write_operation`, `pause_operation`, or `resume_operation`.
    pub fn op_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Every progress frame emitted so far, oldest first.
    pub fn frames(&self) -> Vec<WriteProgressEvent> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.events.progress.lock_ignore_poison().clone()
    }

    /// Stop the reversal, through the call the queue window's Cancel makes.
    pub fn stop(&self) {
        self.engine.cancel_write_operation(self.op_id(), false);
    }

    /// Park the reversal, through the call `pause_operation` makes on the live
    /// state (the manager half of that pair only flips the row's status).
    pub fn pause(&self) {
        assert!(
            self.engine.pause_write_operation(self.op_id()),
            "the reversal's state must be registered for a pause to reach it"
        );
    }

    /// Let it go again, through `resume_operation`'s half of the same pair.
    pub fn resume(&self) {
        assert!(
            self.engine.resume_write_operation(self.op_id()),
            "the reversal's state must be registered for a resume to reach it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestOperations;

    /// Two rules govern which frames go out, and both matter: the throttle keeps a
    /// million-item reversal from emitting an event per item, and the "landed on
    /// the total" exemption keeps a throttled run from ending on a stale frame that
    /// says 1 of 3.
    #[test]
    fn the_throttle_holds_frames_back_but_never_the_one_that_lands_on_the_total() {
        let engine = TestOperations::new();
        let reversal = Reversal::emitting_every(&engine, "progress-throttle", Duration::from_secs(600));
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

    /// ❗ Undoing a move on an object store, where a big file's rename is a copy
    /// (`Volume::rename_work`) and `rename` refuses it: the file goes back
    /// copied and then deleted, never through the `rename` that would fail.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_same_volume_restore_that_renames_by_copy_goes_back_by_copy() {
        use cmdr_fs::volume::{InMemoryVolume, Volume};
        use std::path::Path;

        let volume: Arc<dyn Volume> = Arc::new(
            InMemoryVolume::new("Bucket")
                .with_whole_publish()
                .with_renames_by_copy()
                .with_space_info(1_000_000, 1_000_000),
        );
        volume.create_directory(Path::new("/moved")).await.unwrap();
        volume
            .create_file(Path::new("/moved/big.mov"), b"frames")
            .await
            .unwrap();
        let engine = TestOperations::new();
        let reversal = Reversal::on(&engine, "restore-by-copy");

        reversal
            .runner()
            .perform(InverseAct::Restore {
                from: &volume,
                from_path: Path::new("/moved/big.mov"),
                to: &volume,
                to_path: Path::new("/home/big.mov"),
                same_volume: true,
                force: false,
            })
            .await
            .expect("the restore lands");

        assert!(volume.exists(Path::new("/home/big.mov")).await, "back where it started");
        assert!(
            !volume.exists(Path::new("/moved/big.mov")).await,
            "and gone from where it was"
        );
    }
}

// The operation log's rollback planner, driven end to end by this executor. The
// planner lives in `cmdr-operation-log`; its pure helpers are tested there.
#[cfg(test)]
mod planner_tests;
