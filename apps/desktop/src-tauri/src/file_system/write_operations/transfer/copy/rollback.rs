//! Tracked rollback: deletes the files a copy created, emitting reverse progress.

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use crate::file_system::write_operations::state::{
    CopyTransaction, OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use crate::file_system::write_operations::types::{
    OperationEventSink, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};

/// Rolls back created files with progress events, checking for cancellation between deletions.
///
/// Emits progress events with _decreasing_ `files_done` / `bytes_done` so the frontend's
/// progress bars count backwards from the cancellation point toward zero (no UI flicker,
/// no separate rollback view).
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it
/// (intent transitioned to `Stopped`). Does NOT call `transaction.rollback()` or
/// `transaction.commit()`. The caller must commit unconditionally (this function
/// already deleted whatever it deleted).
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
pub(super) fn rollback_with_progress(
    transaction: &CopyTransaction,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    operation_type: WriteOperationType,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    let files_to_delete = transaction.created_files.len();
    let mut files_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event (same values as cancellation point)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::RollingBack,
            None,
            files_at_cancel,
            files_total,
            bytes_at_cancel,
            bytes_total,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::RollingBack,
        None,
        files_at_cancel,
        files_total,
        bytes_at_cancel,
        bytes_total,
    );

    // Delete files in reverse order (newest first), checking for cancellation
    for file in transaction.created_files.iter().rev() {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "rollback_with_progress: rollback cancelled at {}/{} files, keeping remaining",
                files_deleted,
                files_to_delete,
            );
            return false;
        }

        if let Err(e) = fs::remove_file(file) {
            log::warn!("rollback: failed to remove {}: {}", file.display(), e);
        }
        files_deleted += 1;

        // Throttled progress events with decreasing values
        if last_progress_time.elapsed() >= state.progress_interval {
            // Linearly interpolate bytes based on file deletion progress
            let remaining_files = files_at_cancel.saturating_sub(files_deleted);
            let remaining_bytes = if files_to_delete > 0 {
                bytes_at_cancel - (bytes_at_cancel as f64 * files_deleted as f64 / files_to_delete as f64) as u64
            } else {
                0
            };

            let current_file_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    operation_type,
                    WriteOperationPhase::RollingBack,
                    Some(current_file_name.clone()),
                    remaining_files,
                    files_total,
                    remaining_bytes,
                    bytes_total,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::RollingBack,
                Some(current_file_name),
                remaining_files,
                files_total,
                remaining_bytes,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete created directories (no progress events; this is fast)
    for dir in transaction.created_dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }

    true
}
