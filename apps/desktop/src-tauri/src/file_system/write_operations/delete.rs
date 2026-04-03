//! Delete implementation for write operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::helpers::spawn_async_sync;
use super::scan::{SourceItemTracker, scan_sources};
use super::state::{WriteOperationState, update_operation_status};
use super::types::{
    DryRunResult, IoResultExt, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use super::volume_copy::map_volume_error;
use crate::file_system::volume::Volume;

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

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    // Delete files
    for file_info in &scan_result.files {
        // Check cancellation
        if super::state::is_cancelled(&state.intent) {
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

        fs::remove_file(&file_info.path).with_path(&file_info.path)?;

        files_done += 1;
        bytes_done += file_size;

        if let Some(source_path) = tracker.record(file_info) {
            let _ = app.emit(
                "write-source-item-done",
                WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                },
            );
        }

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
        if super::state::is_cancelled(&state.intent) {
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

// ============================================================================
// Volume-aware delete implementation (for MTP and other non-local volumes)
// ============================================================================

/// Entry collected during the volume scan phase: path, size, and whether it's a directory.
struct VolumeDeleteEntry {
    path: PathBuf,
    size: u64,
    is_dir: bool,
}

/// Recursively enumerates a directory tree via `volume.list_directory()`, collecting
/// files and directories. Directories are appended after their children, so the
/// resulting list is already in deepest-first order for safe deletion.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
fn scan_volume_recursive(
    volume: &dyn Volume,
    path: &Path,
    entries: &mut Vec<VolumeDeleteEntry>,
    total_bytes: &mut u64,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    last_progress_time: &mut Instant,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    if super::state::is_cancelled(&state.intent) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    let is_dir = volume
        .is_directory(path)
        .map_err(|e| map_volume_error(&path.display().to_string(), e))?;

    if is_dir {
        // Recurse into children first — list_directory returns FileEntry with size,
        // so we use child.size directly instead of calling get_metadata (which returns
        // NotSupported on MTP).
        let children = volume
            .list_directory(path)
            .map_err(|e| map_volume_error(&path.display().to_string(), e))?;

        for child in &children {
            let child_path = PathBuf::from(&child.path);
            if child.is_directory {
                scan_volume_recursive(
                    volume,
                    &child_path,
                    entries,
                    total_bytes,
                    state,
                    app,
                    operation_id,
                    last_progress_time,
                )?;
            } else {
                let size = child.size.unwrap_or(0);
                *total_bytes += size;
                entries.push(VolumeDeleteEntry {
                    path: child_path,
                    size,
                    is_dir: false,
                });
            }
        }

        // Add directory after its children (already deepest-first order)
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            size: 0,
            is_dir: true,
        });
    } else {
        // Top-level file without listing context — size unknown, use 0.
        // Progress still tracks file count accurately.
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            size: 0,
            is_dir: false,
        });
    }

    // Emit scan progress periodically
    if last_progress_time.elapsed() >= state.progress_interval {
        let file_count = entries.iter().filter(|e| !e.is_dir).count();
        let current_file = path.file_name().map(|n| n.to_string_lossy().to_string());
        let _ = app.emit(
            "write-progress",
            WriteProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                phase: WriteOperationPhase::Scanning,
                current_file: current_file.clone(),
                files_done: file_count,
                files_total: 0,
                bytes_done: *total_bytes,
                bytes_total: 0,
            },
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::Scanning,
            current_file,
            file_count,
            0,
            *total_bytes,
            0,
        );
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Deletes files on a non-local volume (like MTP) with progress reporting.
///
/// Uses `volume.list_directory()` for scanning and `volume.delete()` per item.
/// Emits the same events as `delete_files_with_progress` so the frontend progress
/// dialog works unchanged.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(super) fn delete_volume_files_with_progress(
    volume: Arc<dyn Volume>,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan — recursively enumerate the tree via volume.list_directory()
    let mut entries: Vec<VolumeDeleteEntry> = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();

    for source in sources {
        // Check if the source itself is a file or directory
        let is_dir = volume.is_directory(source).unwrap_or(false);

        if is_dir {
            scan_volume_recursive(
                &*volume,
                source,
                &mut entries,
                &mut total_bytes,
                state,
                app,
                operation_id,
                &mut last_progress_time,
            )?;
        } else {
            // Top-level file — size unknown without listing the parent, use 0.
            // Progress still tracks file count accurately, and individual file
            // deletes are near-instant on MTP.
            entries.push(VolumeDeleteEntry {
                path: source.to_path_buf(),
                size: 0,
                is_dir: false,
            });
        }
    }

    let file_count = entries.iter().filter(|e| !e.is_dir).count();

    // Emit final scan progress
    let _ = app.emit(
        "write-progress",
        WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            phase: WriteOperationPhase::Scanning,
            current_file: None,
            files_done: file_count,
            files_total: file_count,
            bytes_done: total_bytes,
            bytes_total: total_bytes,
        },
    );

    // Handle dry-run mode
    if config.dry_run {
        let result = DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: file_count,
            bytes_total: total_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        };

        let _ = app.emit("dry-run-complete", result);
        return Ok(());
    }

    // Phase 2: Delete — files first, then directories deepest-first
    // entries are already in order: children before parents (due to recursive scan).
    // We process files first, then dirs in reverse order.
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Delete files
    for entry in entries.iter().filter(|e| !e.is_dir) {
        if super::state::is_cancelled(&state.intent) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rolled_back: false,
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        volume
            .delete(&entry.path)
            .map_err(|e| map_volume_error(&entry.path.display().to_string(), e))?;

        files_done += 1;
        bytes_done += entry.size;

        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = entry.path.file_name().map(|n| n.to_string_lossy().to_string());
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    phase: WriteOperationPhase::Deleting,
                    current_file: current_file.clone(),
                    files_done,
                    files_total: file_count,
                    bytes_done,
                    bytes_total: total_bytes,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                file_count,
                bytes_done,
                total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (already in deepest-first order from scan_volume_recursive)
    for entry in entries.iter().filter(|e| e.is_dir) {
        if super::state::is_cancelled(&state.intent) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rolled_back: false,
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Best-effort directory removal (may fail if not empty due to partial delete)
        let _ = volume.delete(&entry.path);
    }

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
