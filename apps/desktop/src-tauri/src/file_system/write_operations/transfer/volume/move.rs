//! Volume move operations.
//!
//! Move operations across different volume types:
//! - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
//! - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
//! - Cross-volume: copy to destination then delete sources

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::super::event_sinks::OperationEventSink;
use super::super::super::journal;
use super::super::super::manager;
use super::super::super::source_binding::{ExpectedSources, retain_bound_sources_on};
use super::super::super::state::WriteOperationState;
use super::super::super::types::{
    VolumeCopyConfig, WriteOperationConfig, WriteOperationError, WriteOperationStartResult, WriteOperationType,
};
use super::super::transfer_driver::{ConflictDecision, TransferOutcome};
// The same-volume rename path lives in `volume::move_same`; the dispatcher below
// routes to its entry point.
use super::move_same::move_within_same_volume;
use super::transfer_error::{WriteFailure, write_error_event_from};
use crate::file_system::volume::Volume;
use crate::operation_log::types::OpKind;

// The driver-closure future-shape aliases are shared with `volume::move_same`
// (which imports them from here), so they're `pub(super)` rather than private.
/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
pub(super) type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Per-call future shape for the driver's `conflict_resolver` closure.
pub(super) type ResolveFut<'a> =
    Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for the driver's `transfer_one` closure.
pub(super) type TransferFut<'a> =
    Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

/// Unified move across volume types.
///
/// Determines the best strategy based on volume relationship:
/// - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
/// - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
/// - Cross-volume: copy to destination then delete sources
///
/// Emits the standard write events (`write-progress`, `write-complete`, `write-error`).
#[allow(
    clippy::too_many_arguments,
    reason = "each volume travels with its ID (for the busy set) plus its Arc; bundling them would just shuffle the same fields into a struct at every call site"
)]
pub async fn move_between_volumes(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
    initiator: crate::operation_log::types::Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Same volume: use native rename/move (instant for MTP)
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        return move_within_same_volume(
            events,
            source_volume_id,
            source_volume,
            source_paths,
            dest_path,
            config,
            initiator,
            expected_sources,
        )
        .await;
    }

    // Both local: delegate to the battle-tested move implementation
    if let (Some(src_root), Some(dest_root)) = (source_volume.local_path(), dest_volume.local_path()) {
        log::debug!(
            "move_between_volumes: both volumes are local, delegating to native move (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        // Anchored, not joined: the IPC boundary already anchors what the
        // transfer dialog sends, and a raw join would re-root an absolute dest
        // under itself (`/Volumes/USB/sub` → `/Volumes/USB/Volumes/USB/sub`).
        let absolute_dest = cmdr_fs::volume::root_anchored(&dest_root, &dest_path);

        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            pre_known_conflicts: config.pre_known_conflicts,
            ..Default::default()
        };

        // Pass both volume IDs so a local→USB / DMG move still marks the
        // ejectable destination busy while it runs, plus the real
        // `Volume::lane_key()`s so the manager serializes against the mount.
        let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
        return super::super::super::move_files_start(
            events,
            absolute_sources,
            absolute_dest,
            write_config,
            vec![source_volume_id, dest_volume_id],
            Some(lanes),
            initiator,
            expected_sources,
        )
        .await;
    }

    // Cross-volume: copy each file to destination, then delete source
    log::info!(
        "move_between_volumes: cross-volume move, {} -> {}, {} sources",
        source_volume.name(),
        dest_volume.name(),
        source_paths.len()
    );

    let operation_id = crate::operation_log::new_operation_id();

    // The per-leaf record points inside `move_volumes_with_progress` journal under
    // these REAL volume ids (carried on the op state so the test call sites stay
    // unchanged); the deferred's open/finalize bracket uses them directly.
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(config.progress_interval_ms))
            .with_journal_volumes(source_volume_id.clone(), dest_volume_id.clone()),
    );
    let journal_source_volume_id = source_volume_id.clone();
    let journal_dest_volume_id = dest_volume_id.clone();

    // Occupies both volumes' lanes (source AND destination). Both volume IDs go
    // in `volume_ids` for the eject guard.
    let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
    let source_volume_name = source_volume.name().to_string();
    let summary = manager::OperationSummaryText {
        source: Some(source_volume.name().to_string()),
        destination: Some(dest_volume.name().to_string()),
    };
    let descriptor = manager::OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Move,
        lanes,
        volume_ids: vec![source_volume_id, dest_volume_id],
        summary,
        // A cross-volume move copies and deletes the source PER FILE, and this
        // driver never reverses: its `PostLoopIntent::Cancelled` arm treats
        // Stopped and RollingBack alike, leaving what's at the destination
        // alone and reporting `rolled_back: false` (see the arm below). Undoing
        // it would also mean re-creating source files it has already deleted.
        supports_rollback: false,
        preview_id: config.preview_id.clone(),
        reverses: None,
    };

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            // Settle guard: emits `write-settled` at end of scope, after the
            // terminal event and after `on_settled`'s cache cleanup.
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Move,
                Some(source_volume_name),
            );

            // Wait out the confirming dialog's scan before journaling or
            // touching either device; see `write_operations::start_write_operation`.
            if crate::file_system::write_operations::scan_bridge::await_claimed_preview(
                &*events,
                &op_id,
                WriteOperationType::Move,
                &state,
            )
            .await
            .stopped()
            {
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            // Journal the cross-volume move under the REAL volume ids (per-leaf
            // rows land inside `move_volumes_with_progress`; this brackets the op).
            journal::open_volume_op(
                &op_id,
                OpKind::Move,
                initiator,
                &journal_source_volume_id,
                Some(&journal_dest_volume_id),
                source_paths.len() as u64,
            );

            // Hold the operation to what its caller was promised, at the latest
            // moment there is: an approved operation can sit queued behind a long
            // transfer, so the check belongs after admission, not at approval.
            // `../../source_binding.rs`.
            let bound = retain_bound_sources_on(
                source_volume.as_ref(),
                &*events,
                &op_id,
                WriteOperationType::Move,
                expected_sources.as_ref(),
                source_paths,
            )
            .await;

            // An emptied binding is a finished operation, and takes the Ok arm:
            // `announce_empty_batch` has already emitted its `write-complete`.
            let result: Result<(), WriteFailure> = match bound {
                None => Ok(()),
                Some(source_paths) => {
                    move_volumes_with_progress(
                        Arc::clone(&events),
                        &op_id,
                        &state,
                        source_volume,
                        &source_paths,
                        dest_volume,
                        &dest_path,
                        &config,
                    )
                    .await
                }
            };

            journal::finalize_op(
                &op_id,
                OpKind::Move,
                journal::execution_status_from_error(result.as_ref().err().map(|f| &f.error)),
            );

            match result {
                Ok(()) => {}
                Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                    log::info!("move_between_volumes: operation {} cancelled", op_id);
                }
                Err(failure) => {
                    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, failure.error);
                    events.emit_error(write_error_event_from(op_id.clone(), WriteOperationType::Move, failure));
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

// The cross-volume engine sits in its own file: this one is the dispatcher, and
// holding both made a 980-line file out of two separate decisions. Re-exported
// rather than reached directly, so `volume::mod`'s facade names it exactly where
// it always did. The move test suite lives beside the engine, in `move_cross`.
pub(crate) use super::move_cross::move_volumes_with_progress;
