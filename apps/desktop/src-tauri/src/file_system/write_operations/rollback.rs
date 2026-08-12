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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use tokio::sync::oneshot;

use crate::file_system::volume::DEFAULT_VOLUME_ID;
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::rollback::{
    InversePlan, RollbackDispatch, RollbackRefusal, RollbackReport, SkipBreakdown, execute_rollback, inverse_kind,
    rollback_operation, undo_order,
};
use crate::operation_log::store::{open_read_connection, read_operation};
use crate::operation_log::types::{Initiator, OpKind, RollbackState};
use crate::operation_log::writer::OperationLogWriter;

use super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText, manager};
use super::state::{WriteOperationState, is_cancelled};
use super::types::WriteOperationType;

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

/// Roll back operation `op_id`: gate it, set it `rolling_back`, and spawn its
/// inverse as a managed op. Returns the inverse op's id (the reversal runs
/// asynchronously; poll the original op's `rollback_state` for the terminal
/// result). A refusal (unknown / already rolling back / not rollbackable / a
/// volume disconnected) surfaces typed; the entry resets `rolling_back` on a
/// synchronous spawn failure so a retry isn't wedged.
pub fn dispatch_rollback<R: Runtime>(
    app: &AppHandle<R>,
    op_id: &str,
    initiator: Initiator,
) -> Result<RollbackDispatch, RollbackRefusal> {
    dispatch_inverse(app, op_id, initiator, None)
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
) -> Result<oneshot::Receiver<RollbackReport>, RollbackRefusal> {
    let (report_tx, report_rx) = oneshot::channel();
    dispatch_inverse(app, op_id, initiator, Some(report_tx))?;
    Ok(report_rx)
}

fn dispatch_inverse<R: Runtime>(
    app: &AppHandle<R>,
    op_id: &str,
    initiator: Initiator,
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
        spawn_managed_inverse(&writer, plan, initiator, report)
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
        summary: OperationSummaryText::default(),
        // This IS the reversal. Offering to roll back a rollback would ask the
        // engine to re-apply what the person just chose to undo.
        supports_rollback: false,
        // No scan preview: nothing walked a tree to plan this op.
        preview_id: None,
    };
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

    let writer = writer.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let guard = ManagedTaskGuard::new(inverse_op_id.clone());
            // Bridge the manager's cancel machine into the engine's predicate: a
            // canceled rollback keeps what it reversed and records the rest.
            let is_canceled = || is_cancelled(&state_for_op.intent);
            let vm = get_volume_manager();
            let outcome = execute_rollback(vm, &writer, &original, &inverse_op_id, initiator, &is_canceled).await;
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
) -> UndoReport {
    let mut report = UndoReport {
        operations: Vec::with_capacity(operation_ids.len()),
        restored: 0,
        skipped: 0,
    };
    let (rows, mut unknown) = read_undoable_rows(app, operation_ids);

    for op in undo_order(rows) {
        let outcome = match dispatch_inverse_reported(app, &op.op_id, initiator) {
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
