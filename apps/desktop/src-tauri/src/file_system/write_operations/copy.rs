//! Copy implementation for write operations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::file_system::macos_copy::{CopyProgressContext, copy_single_file_native, copy_symlink};

use super::chunked_copy::{ChunkedCopyProgressFn, chunked_copy_with_metadata, is_network_filesystem};
use super::helpers::{
    is_same_file, resolve_conflict, safe_overwrite_file, spawn_async_sync, validate_disk_space, validate_path_length,
};
use super::scan::{handle_dry_run, scan_sources, take_cached_scan_result};
use super::state::{CopyTransaction, WriteOperationState, update_operation_status};
use super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};

// ============================================================================
// Cancellation-aware helpers
// ============================================================================

/// Interval for checking cancellation while waiting for blocking operations.
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs `validate_disk_space` with polling-based cancellation.
/// This ensures we respond quickly to cancellation even if `statvfs` blocks on slow network drives.
fn validate_disk_space_cancellable(
    destination: &Path,
    required_bytes: u64,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
) -> Result<(), WriteOperationError> {
    use std::sync::mpsc;

    let destination = destination.to_path_buf();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = validate_disk_space(&destination, required_bytes);
        let _ = tx.send(result);
    });

    // Poll for results, checking cancellation flag between polls
    loop {
        if state.cancelled.load(Ordering::Relaxed) {
            log::debug!(
                "copy: cancellation detected during disk space check polling op={}",
                operation_id
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Continue polling
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(WriteOperationError::IoError {
                    path: "disk_space_check".to_string(),
                    message: "Disk space check thread terminated unexpectedly".to_string(),
                });
            }
        }
    }
}

// ============================================================================
// Copy implementation
// ============================================================================

pub(super) fn copy_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    log::debug!(
        "copy_files_with_progress: starting operation_id={}, {} sources",
        operation_id,
        sources.len()
    );

    // Handle dry-run mode
    if handle_dry_run(
        config.dry_run,
        sources,
        destination,
        state,
        app,
        operation_id,
        WriteOperationType::Copy,
        state.progress_interval,
        config.max_conflicts_to_show,
    )? {
        return Ok(());
    }

    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Try to reuse cached scan results from preview
        if let Some(cached) = take_cached_scan_result(preview_id) {
            log::info!(
                "copy_files_with_progress: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
            // Cache miss or expired, do normal scan
            log::debug!(
                "copy_files_with_progress: preview_id={} cache miss, starting fresh scan for operation_id={}",
                preview_id,
                operation_id
            );
            scan_sources(
                sources,
                state,
                app,
                operation_id,
                WriteOperationType::Copy,
                config.sort_column,
                config.sort_order,
            )?
        }
    } else {
        // No preview ID, do normal scan
        log::debug!(
            "copy_files_with_progress: starting scan phase for operation_id={}",
            operation_id
        );
        scan_sources(
            sources,
            state,
            app,
            operation_id,
            WriteOperationType::Copy,
            config.sort_column,
            config.sort_order,
        )?
    };
    log::info!(
        "copy_files_with_progress: scan complete for operation_id={}, files={}, bytes={}",
        operation_id,
        scan_result.file_count,
        scan_result.total_bytes
    );

    // Pre-flight disk space check: verify destination has enough free space
    // Use polling-based cancellation to remain responsive on slow network drives
    log::info!(
        "copy_files_with_progress: starting disk space check for operation_id={}",
        operation_id
    );
    validate_disk_space_cancellable(destination, scan_result.total_bytes, state, operation_id)?;
    log::info!(
        "copy_files_with_progress: disk space check complete for operation_id={}",
        operation_id
    );

    // Phase 2: Copy files in sorted order with rollback support
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();

    // Emit initial copying phase event (important when reusing cached scan - no scanning events were emitted)
    let _ = app.emit(
        "write-progress",
        WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            phase: WriteOperationPhase::Copying,
            current_file: None,
            files_done: 0,
            files_total: scan_result.file_count,
            bytes_done: 0,
            bytes_total: scan_result.total_bytes,
        },
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        0,
        scan_result.file_count,
        0,
        scan_result.total_bytes,
    );

    log::info!(
        "copy_files_with_progress: starting copy loop for operation_id={}, {} files",
        operation_id,
        scan_result.files.len()
    );

    let result: Result<(), WriteOperationError> = (|| {
        for file_info in &scan_result.files {
            log::debug!(
                "copy_files_with_progress: copying file {} ({} bytes)",
                file_info.path.display(),
                file_info.size
            );
            copy_single_file_sorted(
                &file_info.path,
                file_info.dest_path(destination),
                file_info.is_symlink,
                &mut files_done,
                &mut bytes_done,
                scan_result.file_count,
                scan_result.total_bytes,
                state,
                app,
                operation_id,
                &state.progress_interval,
                &mut last_progress_time,
                config,
                &mut transaction,
                &mut apply_to_all_resolution,
                &mut created_dirs,
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            // Success - commit transaction (don't rollback)
            transaction.commit();

            // Spawn async sync for durability (non-blocking)
            spawn_async_sync();

            log::info!(
                "copy_files_with_progress: completed op={} files={} bytes={}",
                operation_id,
                files_done,
                bytes_done
            );

            // Emit completion
            let _ = app.emit(
                "write-complete",
                WriteCompleteEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    files_processed: files_done,
                    bytes_processed: bytes_done,
                },
            );
            Ok(())
        }
        Err(e) => {
            if matches!(e, WriteOperationError::Cancelled { .. }) {
                // Cancellation - check if user wants to keep partial files
                let skip_rollback = state.skip_rollback.load(Ordering::Relaxed);
                let rolled_back = !skip_rollback;

                if skip_rollback {
                    log::info!(
                        "copy_files_with_progress: cancelled op={}, keeping {} partial files",
                        operation_id,
                        transaction.created_files.len()
                    );
                    transaction.commit();
                } else {
                    log::info!(
                        "copy_files_with_progress: cancelled op={}, rolling back {} files",
                        operation_id,
                        transaction.created_files.len()
                    );
                    transaction.rollback();
                }

                let _ = app.emit(
                    "write-cancelled",
                    WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        files_processed: files_done,
                        rolled_back,
                    },
                );
            } else {
                // Non-cancellation error - always rollback
                log::error!(
                    "copy_files_with_progress: failed op={} error={:?}, rolling back",
                    operation_id,
                    e
                );
                transaction.rollback();

                let _ = app.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        error: e.clone(),
                    },
                );
            }
            Err(e)
        }
    }
}

/// Copies a single file from the sorted file list to its destination.
/// Ensures parent directories exist before copying.
#[allow(
    clippy::too_many_arguments,
    reason = "File copy requires passing state through multiple levels"
)]
fn copy_single_file_sorted(
    source: &Path,
    dest_path: PathBuf,
    is_symlink: bool,
    files_done: &mut usize,
    bytes_done: &mut u64,
    files_total: usize,
    bytes_total: u64,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    progress_interval: &Duration,
    last_progress_time: &mut Instant,
    config: &WriteOperationConfig,
    transaction: &mut CopyTransaction,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        log::debug!(
            "copy: cancellation detected op={} files_done={}",
            operation_id,
            *files_done
        );
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Ensure parent directories exist
    if let Some(parent) = dest_path.parent()
        && !created_dirs.contains(parent)
        && !parent.exists()
    {
        // Collect directories that don't exist BEFORE creating them
        // (so we know exactly which ones we're creating for rollback)
        let mut dirs_to_create: Vec<PathBuf> = Vec::new();
        let mut dir = parent.to_path_buf();
        while !dir.exists() && !created_dirs.contains(&dir) {
            dirs_to_create.push(dir.clone());
            match dir.parent() {
                Some(p) => dir = p.to_path_buf(),
                None => break,
            }
        }

        // Create all directories
        fs::create_dir_all(parent).map_err(|e| WriteOperationError::IoError {
            path: parent.display().to_string(),
            message: format!("Failed to create directory: {}", e),
        })?;

        // Record only the directories we actually created (in creation order: deepest last)
        // dirs_to_create is in reverse order (deepest first), so iterate in reverse
        for created_dir in dirs_to_create.into_iter().rev() {
            transaction.record_dir(created_dir.clone());
            created_dirs.insert(created_dir);
        }
    }

    // Get metadata for size tracking
    let metadata = fs::symlink_metadata(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: e.to_string(),
    })?;

    let file_name = source.file_name().unwrap_or_default();

    if is_symlink {
        // Handle symlink
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file but still count it toward progress
                    *files_done += 1;
                    *bytes_done += metadata.len();
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        if needs_safe_overwrite {
            if actual_dest.is_dir() {
                fs::remove_dir_all(&actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: actual_dest.display().to_string(),
                    message: e.to_string(),
                })?;
            } else {
                fs::remove_file(&actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: actual_dest.display().to_string(),
                    message: e.to_string(),
                })?;
            }
        }

        #[cfg(target_os = "macos")]
        {
            copy_symlink(source, &actual_dest)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let target = fs::read_link(source).map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: format!("Failed to read symlink: {}", e),
            })?;
            std::os::unix::fs::symlink(&target, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: actual_dest.display().to_string(),
                message: format!("Failed to create symlink: {}", e),
            })?;
        }

        transaction.record_file(actual_dest);
        *files_done += 1;
        *bytes_done += metadata.len();
    } else {
        // Handle regular file
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file but still count it toward progress
                    *files_done += 1;
                    *bytes_done += metadata.len();
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        // Prevent copying a file over itself via symlinks (same inode + device)
        if is_same_file(source, &actual_dest) {
            log::warn!(
                "copy: skipping {}: source and destination resolve to the same file",
                source.display()
            );
            *files_done += 1;
            *bytes_done += metadata.len();
            return Ok(());
        }

        // Check cancellation before copy
        if state.cancelled.load(Ordering::Relaxed) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Copy file using platform-specific method
        // For network filesystems, use chunked copy for responsive cancellation
        let bytes = if is_network_filesystem(&actual_dest) {
            log::debug!(
                "copy: using chunked copy for network destination {}",
                actual_dest.display()
            );

            // Create progress callback for intra-file progress reporting
            let base_bytes_done = *bytes_done;
            let current_file_name = file_name.to_string_lossy().to_string();
            let last_emit_time = std::cell::Cell::new(Instant::now());

            let progress_cb: ChunkedCopyProgressFn = &|chunk_bytes: u64, _total: u64| {
                // Check if enough time has passed to emit a progress event
                if last_emit_time.get().elapsed() >= *progress_interval {
                    let effective_bytes_done = base_bytes_done + chunk_bytes;
                    log::debug!(
                        "copy: emitting chunked progress op={} files={}/{} bytes={}/{}",
                        operation_id,
                        *files_done,
                        files_total,
                        effective_bytes_done,
                        bytes_total
                    );
                    let _ = app.emit(
                        "write-progress",
                        WriteProgressEvent {
                            operation_id: operation_id.to_string(),
                            operation_type: WriteOperationType::Copy,
                            phase: WriteOperationPhase::Copying,
                            current_file: Some(current_file_name.clone()),
                            files_done: *files_done,
                            files_total,
                            bytes_done: effective_bytes_done,
                            bytes_total,
                        },
                    );
                    update_operation_status(
                        operation_id,
                        WriteOperationPhase::Copying,
                        Some(current_file_name.clone()),
                        *files_done,
                        files_total,
                        effective_bytes_done,
                        bytes_total,
                    );
                    last_emit_time.set(Instant::now());
                }
            };

            chunked_copy_with_metadata(source, &actual_dest, &state.cancelled, Some(progress_cb))?
        } else if needs_safe_overwrite {
            #[cfg(target_os = "macos")]
            {
                let context = CopyProgressContext {
                    cancelled: Arc::clone(&state.cancelled),
                    ..Default::default()
                };
                safe_overwrite_file(source, &actual_dest, Some(&context))?
            }
            #[cfg(not(target_os = "macos"))]
            {
                safe_overwrite_file(source, &actual_dest)?
            }
        } else {
            #[cfg(target_os = "macos")]
            {
                let context = CopyProgressContext {
                    cancelled: Arc::clone(&state.cancelled),
                    ..Default::default()
                };
                copy_single_file_native(source, &actual_dest, false, Some(&context))?
            }
            #[cfg(not(target_os = "macos"))]
            {
                fs::copy(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?
            }
        };

        transaction.record_file(actual_dest.clone());
        *files_done += 1;
        *bytes_done += bytes;

        // Emit progress
        if last_progress_time.elapsed() >= *progress_interval {
            let current_file_name = file_name.to_string_lossy().to_string();
            log::debug!(
                "copy: emitting write-progress op={} phase=copying files={}/{} bytes={}/{}",
                operation_id,
                *files_done,
                files_total,
                *bytes_done,
                bytes_total
            );
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    phase: WriteOperationPhase::Copying,
                    current_file: Some(current_file_name.clone()),
                    files_done: *files_done,
                    files_total,
                    bytes_done: *bytes_done,
                    bytes_total,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Copying,
                Some(current_file_name),
                *files_done,
                files_total,
                *bytes_done,
                bytes_total,
            );
            *last_progress_time = Instant::now();
        }
    }

    Ok(())
}

/// Recursively copies a path to a destination directory.
/// Used by move_with_staging for cross-filesystem moves.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn copy_path_recursive(
    source: &Path,
    dest_dir: &Path,
    files_done: &mut usize,
    bytes_done: &mut u64,
    files_total: usize,
    bytes_total: u64,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    progress_interval: &Duration,
    last_progress_time: &mut Instant,
    config: &WriteOperationConfig,
    transaction: &mut CopyTransaction,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Check cancellation (event will be emitted from error handler after rollback decision)
    if state.cancelled.load(Ordering::Relaxed) {
        log::debug!(
            "copy: cancellation detected op={} files_done={}",
            operation_id,
            *files_done
        );
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: "Invalid source path".to_string(),
    })?;
    let dest_path = dest_dir.join(file_name);

    // Use symlink_metadata to check type without following symlinks
    let metadata = fs::symlink_metadata(source).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: e.to_string(),
    })?;

    if metadata.is_symlink() {
        // Handle symlink - copy as symlink, not target
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        // For symlink overwrite, remove the existing symlink/file first (safe since we can recreate)
        if needs_safe_overwrite {
            if actual_dest.is_dir() {
                fs::remove_dir_all(&actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: actual_dest.display().to_string(),
                    message: e.to_string(),
                })?;
            } else {
                fs::remove_file(&actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: actual_dest.display().to_string(),
                    message: e.to_string(),
                })?;
            }
        }

        #[cfg(target_os = "macos")]
        {
            copy_symlink(source, &actual_dest)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let target = fs::read_link(source).map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: format!("Failed to read symlink: {}", e),
            })?;
            std::os::unix::fs::symlink(&target, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: actual_dest.display().to_string(),
                message: format!("Failed to create symlink: {}", e),
            })?;
        }

        transaction.record_file(actual_dest);
        *files_done += 1;
        *bytes_done += metadata.len();
    } else if metadata.is_file() {
        // Handle regular file
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        // Prevent copying a file over itself via symlinks (same inode + device)
        if is_same_file(source, &actual_dest) {
            log::warn!(
                "copy: skipping {}: source and destination resolve to the same file",
                source.display()
            );
            *files_done += 1;
            *bytes_done += metadata.len();
            return Ok(());
        }

        // Check cancellation before copy
        if state.cancelled.load(Ordering::Relaxed) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Copy file using platform-specific method
        // For network filesystems, use chunked copy for responsive cancellation
        let bytes = if is_network_filesystem(&actual_dest) {
            log::debug!(
                "copy: using chunked copy for network destination {}",
                actual_dest.display()
            );

            // Create progress callback for intra-file progress reporting
            let base_bytes_done = *bytes_done;
            let current_file_name = file_name.to_string_lossy().to_string();
            let last_emit_time = std::cell::Cell::new(Instant::now());

            let progress_cb: ChunkedCopyProgressFn = &|chunk_bytes: u64, _total: u64| {
                // Check if enough time has passed to emit a progress event
                if last_emit_time.get().elapsed() >= *progress_interval {
                    let effective_bytes_done = base_bytes_done + chunk_bytes;
                    log::debug!(
                        "copy: emitting chunked progress (recursive) op={} files={}/{} bytes={}/{}",
                        operation_id,
                        *files_done,
                        files_total,
                        effective_bytes_done,
                        bytes_total
                    );
                    let _ = app.emit(
                        "write-progress",
                        WriteProgressEvent {
                            operation_id: operation_id.to_string(),
                            operation_type: WriteOperationType::Copy,
                            phase: WriteOperationPhase::Copying,
                            current_file: Some(current_file_name.clone()),
                            files_done: *files_done,
                            files_total,
                            bytes_done: effective_bytes_done,
                            bytes_total,
                        },
                    );
                    update_operation_status(
                        operation_id,
                        WriteOperationPhase::Copying,
                        Some(current_file_name.clone()),
                        *files_done,
                        files_total,
                        effective_bytes_done,
                        bytes_total,
                    );
                    last_emit_time.set(Instant::now());
                }
            };

            chunked_copy_with_metadata(source, &actual_dest, &state.cancelled, Some(progress_cb))?
        } else if needs_safe_overwrite {
            // Use safe overwrite pattern (temp + rename)
            #[cfg(target_os = "macos")]
            {
                let context = CopyProgressContext {
                    cancelled: Arc::clone(&state.cancelled),
                    ..Default::default()
                };
                safe_overwrite_file(source, &actual_dest, Some(&context))?
            }
            #[cfg(not(target_os = "macos"))]
            {
                safe_overwrite_file(source, &actual_dest)?
            }
        } else {
            // Normal copy to new location
            #[cfg(target_os = "macos")]
            {
                let context = CopyProgressContext {
                    cancelled: Arc::clone(&state.cancelled),
                    ..Default::default()
                };
                copy_single_file_native(source, &actual_dest, false, Some(&context))?
            }
            #[cfg(not(target_os = "macos"))]
            {
                fs::copy(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?
            }
        };

        transaction.record_file(actual_dest.clone());
        *files_done += 1;
        *bytes_done += bytes;

        // Emit progress
        if last_progress_time.elapsed() >= *progress_interval {
            let current_file_name = file_name.to_string_lossy().to_string();
            log::debug!(
                "copy: emitting write-progress op={} phase=copying files={}/{} bytes={}/{}",
                operation_id,
                *files_done,
                files_total,
                *bytes_done,
                bytes_total
            );
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    phase: WriteOperationPhase::Copying,
                    current_file: Some(current_file_name.clone()),
                    files_done: *files_done,
                    files_total,
                    bytes_done: *bytes_done,
                    bytes_total,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Copying,
                Some(current_file_name),
                *files_done,
                files_total,
                *bytes_done,
                bytes_total,
            );
            *last_progress_time = Instant::now();
        }
    } else if metadata.is_dir() {
        // Handle directory
        let dir_created = if !dest_path.exists() {
            fs::create_dir(&dest_path).map_err(|e| WriteOperationError::IoError {
                path: dest_path.display().to_string(),
                message: e.to_string(),
            })?;
            true
        } else {
            false
        };

        if dir_created {
            transaction.record_dir(dest_path.clone());
        }

        // Recursively copy contents
        let entries = fs::read_dir(source).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: e.to_string(),
        })?;

        for entry in entries.flatten() {
            copy_path_recursive(
                &entry.path(),
                &dest_path,
                files_done,
                bytes_done,
                files_total,
                bytes_total,
                state,
                app,
                operation_id,
                progress_interval,
                last_progress_time,
                config,
                transaction,
                apply_to_all_resolution,
            )?;
        }
    } else {
        // Skip special files (sockets, FIFOs, char/block devices)
        log::warn!("copy: skipping special file: {}", source.display());
    }

    Ok(())
}
