//! Delete implementation for write operations.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::helpers::spawn_async_sync;
use super::scan::scan_sources;
use super::state::{WriteOperationState, update_operation_status};
use super::types::{
    DryRunResult, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};

// ============================================================================
// Delete implementation
// ============================================================================

pub(super) fn delete_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan to get file count (delete uses default sorting)
    let scan_result = scan_sources(
        sources,
        state,
        app,
        operation_id,
        WriteOperationType::Delete,
        config.sort_column,
        config.sort_order,
    )?;

    // Handle dry-run mode (delete has no conflicts)
    if config.dry_run {
        let result = DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: scan_result.file_count,
            bytes_total: scan_result.total_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        };

        let _ = app.emit("dry-run-complete", result);
        return Ok(());
    }

    // Phase 2: Delete files first (deepest first)
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Delete files
    for file_info in &scan_result.files {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rolled_back: false, // Delete operations can't be rolled back
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use the size from FileInfo (already captured during scan)
        let file_size = file_info.size;

        fs::remove_file(&file_info.path).map_err(|e| WriteOperationError::IoError {
            path: file_info.path.display().to_string(),
            message: e.to_string(),
        })?;

        files_done += 1;
        bytes_done += file_size;

        // Emit progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = file_info.path.file_name().map(|n| n.to_string_lossy().to_string());
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    phase: WriteOperationPhase::Deleting,
                    current_file: current_file.clone(),
                    files_done,
                    files_total: scan_result.file_count,
                    bytes_done,
                    bytes_total: scan_result.total_bytes,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                scan_result.file_count,
                bytes_done,
                scan_result.total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (in reverse order - deepest first)
    for dir in scan_result.dirs.iter().rev() {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rolled_back: false, // Delete operations can't be rolled back
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Only remove if empty (files should already be deleted)
        let _ = fs::remove_dir(dir);
    }

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_processed: files_done,
            bytes_processed: bytes_done,
        },
    );

    Ok(())
}
