//! The managed lifecycle of a delete that runs through the `Volume` trait.
//!
//! A local delete rides `start_write_operation`'s generic spawn, but a volume
//! delete's body is `async` (the trait's I/O is), so it owns its own deferred
//! start: settle guard, scan-wait, journal open, the walk, the terminal event,
//! and `on_settled`. That lifecycle lives here rather than inline in the module
//! facade, beside the walker it drives.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use super::super::event_sinks::OperationEventSink;
use super::super::manager::{self, OperationDescriptor};
use super::super::source_binding::{ExpectedSources, retain_bound_sources_on};
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::types::{
    WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationStartResult, WriteOperationType,
};
use super::super::{journal, path_summary, scan_bridge};
use super::delete_volume_files_with_progress_inner;
use crate::file_system::volume::LaneKey;
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::types::{ExecutionStatus, Initiator, OpKind};

/// Registers a volume-aware delete with the manager and hands it a deferred
/// async start.
///
/// The lane is the volume's own `Volume::lane_key()`, falling back to the volume
/// id when the volume isn't registered yet (the not-found surfaces on admission).
/// The manager owns lifecycle, cache cleanup, and the busy registration; the
/// deferred owns the op body, its terminal event, and settle.
pub(in crate::file_system::write_operations) fn start_volume_delete(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<std::path::PathBuf>,
    config: WriteOperationConfig,
    volume_id: String,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> WriteOperationStartResult {
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(
        config.progress_interval_ms,
    )));

    let lane = get_volume_manager()
        .get(&volume_id)
        .map(|volume| volume.lane_key())
        .unwrap_or_else(|| LaneKey::new(volume_id.clone()));

    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Delete,
        lanes: vec![lane],
        volume_ids: vec![volume_id.clone()],
        summary: path_summary(&sources, None),
        // Deleted is deleted; there's nothing for a rollback to put back.
        supports_rollback: false,
        preview_id: config.preview_id.clone(),
    };

    let deferred_state = Arc::clone(&state);
    let deferred_id = operation_id.clone();
    // A named future rather than a giant inline `async move` block: the body is
    // the whole lifecycle, and it reads better next to the walker than nested
    // four levels deep inside a closure inside a starter.
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(drive_volume_delete(DeferredDelete {
            events,
            operation_id: deferred_id,
            state: deferred_state,
            volume_id,
            sources,
            config,
            initiator,
            expected_sources,
        }))
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

    WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Delete,
    }
}

/// Everything one volume delete's deferred start owns, moved in as a unit so the
/// closure captures one value instead of eight.
struct DeferredDelete {
    events: Arc<dyn OperationEventSink>,
    operation_id: String,
    state: Arc<WriteOperationState>,
    volume_id: String,
    sources: Vec<std::path::PathBuf>,
    config: WriteOperationConfig,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
}

/// The whole lifecycle of an admitted volume delete: settle guard, scan-wait,
/// journal open, the walk, the terminal event, and `on_settled`.
async fn drive_volume_delete(op: DeferredDelete) {
    let DeferredDelete {
        events,
        operation_id,
        state,
        volume_id,
        sources,
        config,
        initiator,
        expected_sources,
    } = op;
    let task_guard = manager::ManagedTaskGuard::new(operation_id.clone());
    // Fires `write-settled` at end of scope, AFTER the terminal event and AFTER
    // `on_settled`'s cache cleanup (this guard drops last). The FE ordering
    // contract depends on it.
    let _settled_guard = WriteSettledGuard::new(
        Arc::clone(&events),
        operation_id.clone(),
        WriteOperationType::Delete,
        Some(volume_id.clone()),
    );

    // Wait out the confirming dialog's scan before journaling or touching the
    // device; see `write_operations::start_write_operation`.
    if scan_bridge::await_claimed_preview(&*events, &operation_id, WriteOperationType::Delete, &state)
        .await
        .stopped()
    {
        task_guard.disarm();
        manager::manager().on_settled(&operation_id);
        return;
    }

    // Journal under the REAL volume id (not the hardcoded `"root"` the local
    // helpers bake in). Per-leaf rows are recorded inside the walker.
    journal::open_volume_op(
        &operation_id,
        OpKind::Delete,
        initiator,
        &volume_id,
        None,
        sources.len() as u64,
    );

    let execution_status = run_volume_delete(
        &*events,
        &operation_id,
        &state,
        &volume_id,
        sources,
        &config,
        expected_sources.as_ref(),
    )
    .await;
    journal::finalize_op(&operation_id, OpKind::Delete, execution_status);

    task_guard.disarm();
    manager::manager().on_settled(&operation_id);
}

/// The op body: resolve the volume, hold it to the caller's source binding, walk.
/// Returns the `ExecutionStatus` the journal finalizes with, and emits the
/// terminal event for every outcome that owes one.
#[allow(clippy::too_many_arguments, reason = "the op's own inputs plus its emit context")]
async fn run_volume_delete(
    events: &dyn OperationEventSink,
    op_id: &str,
    state: &Arc<WriteOperationState>,
    volume_id: &str,
    sources: Vec<std::path::PathBuf>,
    config: &WriteOperationConfig,
    expected_sources: Option<&ExpectedSources>,
) -> ExecutionStatus {
    let Some(volume) = get_volume_manager().get(volume_id) else {
        events.emit_error(WriteErrorEvent::new(
            op_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationError::IoError {
                path: volume_id.to_string(),
                message: format!("Volume '{}' not found", volume_id),
            },
        ));
        return ExecutionStatus::Failed;
    };

    let bound = retain_bound_sources_on(
        volume.as_ref(),
        events,
        op_id,
        WriteOperationType::Delete,
        expected_sources,
        sources,
    )
    .await;
    // The binding left nothing to delete. Each source went out as a `Skipped`
    // item and the complete event with it, so the op is over and it didn't fail.
    let Some(sources) = bound else {
        return ExecutionStatus::Done;
    };

    match delete_volume_files_with_progress_inner(volume, volume_id, events, op_id, state, &sources, config).await {
        Ok(()) => ExecutionStatus::Done,
        Err(ref error) if matches!(error, WriteOperationError::Cancelled { .. }) => ExecutionStatus::Canceled,
        Err(error) => {
            events.emit_error(WriteErrorEvent::new(
                op_id.to_string(),
                WriteOperationType::Delete,
                error,
            ));
            ExecutionStatus::Failed
        }
    }
}
