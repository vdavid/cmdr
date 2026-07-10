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
//! [`dispatch_rollback`] is the backend entry point a rollback caller invokes.
//! The MCP `operations_rollback` tool is the first consumer; a FE-facing
//! tauri command lands with the alpha UI. It returns after DISPATCH, not
//! after the reversal finishes: the inverse is an async managed op, so the caller
//! polls the original op's `rollback_state` until it leaves `rolling_back` to
//! observe the terminal result (the "dispatch then poll" contract).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::file_system::get_volume_manager;
use crate::file_system::volume::DEFAULT_VOLUME_ID;
use crate::operation_log::rollback::{
    InversePlan, RollbackDispatch, RollbackRefusal, execute_rollback, inverse_kind, rollback_operation,
};
use crate::operation_log::types::{Initiator, OpKind};
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
    // The writer lives in managed state (the durable store). Its absence means the journal never
    // opened, so there's nothing to roll back.
    let writer = app
        .try_state::<OperationLogWriter>()
        .map(|s| s.inner().clone())
        .ok_or(RollbackRefusal::UnknownOperation)?;
    let vm = get_volume_manager();

    let plan = rollback_operation(vm, &writer, op_id, |plan| {
        spawn_managed_inverse(&writer, plan, initiator)
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
            execute_rollback(vm, &writer, &original, &inverse_op_id, initiator, &is_canceled).await;
            guard.disarm();
            manager().on_settled(&inverse_op_id);
        })
    };

    manager().spawn_managed(descriptor, state, Box::new(deferred));
    Ok(())
}
