//! Scanning functionality for write operations.
//!
//! Contains file scanning, preview scanning, and dry-run operations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::helpers::{calculate_dest_path, create_conflict_info, is_symlink_loop, sample_conflicts};
use super::state::{
    CachedScanResult, FileInfo, SCAN_PREVIEW_RESULTS, SCAN_PREVIEW_STATE, ScanPreviewState, ScanResult,
    WriteOperationState, update_operation_status,
};
use super::types::{
    ConflictInfo, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
    ScanPreviewStartResult, ScanProgressEvent, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};
use crate::file_system::listing::{SortColumn, SortOrder};

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Starts a scan preview for the Copy dialog.
/// Returns a preview_id that can be used to cancel or to pass to copy_files.
pub fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: u64,
) -> ScanPreviewStartResult {
    let preview_id = Uuid::new_v4().to_string();
    let preview_id_clone = preview_id.clone();

    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(progress_interval_ms),
    });

    // Register state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.insert(preview_id.clone(), Arc::clone(&state));
    }

    // Spawn background task
    std::thread::spawn(move || {
        run_scan_preview(app, preview_id_clone, sources, sort_column, sort_order, state);
    });

    ScanPreviewStartResult { preview_id }
}

/// Cancels a running scan preview.
pub fn cancel_scan_preview(preview_id: &str) {
    if let Ok(cache) = SCAN_PREVIEW_STATE.read()
        && let Some(state) = cache.get(preview_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Internal function that runs the scan preview in a background thread.
fn run_scan_preview(
    app: tauri::AppHandle,
    preview_id: String,
    sources: Vec<PathBuf>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    state: Arc<ScanPreviewState>,
) {
    use tauri::Emitter;

    let mut files: Vec<FileInfo> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    let result: Result<(), String> = (|| {
        let ctx = WalkContext {
            progress_interval: state.progress_interval,
            is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
            on_io_error: &|_, e| e.to_string(),
            on_cancelled: &|| "Cancelled".to_string(),
            on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
            on_progress: &|files_found, dirs_found, bytes_found, current_path| {
                let _ = app.emit(
                    "scan-preview-progress",
                    ScanPreviewProgressEvent {
                        preview_id: preview_id.to_string(),
                        files_found,
                        dirs_found,
                        bytes_found,
                        current_path,
                    },
                );
            },
        };
        for source in &sources {
            let source_root = source.parent().unwrap_or(source);
            walk_dir_recursive(
                source,
                source_root,
                &mut files,
                &mut dirs,
                &mut total_bytes,
                &mut last_progress_time,
                &mut visited,
                &ctx,
            )?;
        }
        Ok(())
    })();

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(()) => {
            if state.cancelled.load(Ordering::Relaxed) {
                // Cancelled
                let _ = app.emit(
                    "scan-preview-cancelled",
                    ScanPreviewCancelledEvent {
                        preview_id: preview_id.clone(),
                    },
                );
            } else {
                // Sort files
                sort_files(&mut files, sort_column, sort_order);

                // Cache the results
                let file_count = files.len();
                let dirs_count = dirs.len();
                if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
                    cache.insert(
                        preview_id.clone(),
                        CachedScanResult {
                            files,
                            dirs,
                            file_count,
                            total_bytes,
                        },
                    );
                }

                // Emit completion
                let _ = app.emit(
                    "scan-preview-complete",
                    ScanPreviewCompleteEvent {
                        preview_id,
                        files_total: file_count,
                        dirs_total: dirs_count,
                        bytes_total: total_bytes,
                    },
                );
            }
        }
        Err(message) => {
            let _ = app.emit("scan-preview-error", ScanPreviewErrorEvent { preview_id, message });
        }
    }
}

/// Callbacks for customizing `walk_dir_recursive` behavior per caller.
struct WalkContext<'a, E> {
    progress_interval: Duration,
    is_cancelled: &'a dyn Fn() -> bool,
    on_io_error: &'a dyn Fn(&Path, std::io::Error) -> E,
    on_cancelled: &'a dyn Fn() -> E,
    on_symlink_loop: &'a dyn Fn(&Path) -> E,
    on_progress: &'a dyn Fn(usize, usize, u64, Option<String>),
}

/// Recursively walks a directory tree, collecting files and directories.
///
/// Shared walker used by both scan preview and write operation scanning.
/// Behavior is customized via `WalkContext` callbacks for error handling and progress reporting.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
fn walk_dir_recursive<E>(
    path: &Path,
    source_root: &Path,
    files: &mut Vec<FileInfo>,
    dirs: &mut Vec<PathBuf>,
    total_bytes: &mut u64,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
    ctx: &WalkContext<'_, E>,
) -> Result<(), E> {
    if (ctx.is_cancelled)() {
        return Err((ctx.on_cancelled)());
    }

    let metadata = fs::symlink_metadata(path).map_err(|e| (ctx.on_io_error)(path, e))?;

    if metadata.is_symlink() || metadata.is_file() {
        *total_bytes += metadata.len();
        files.push(FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata));
    } else if metadata.is_dir() {
        if is_symlink_loop(path, visited) {
            return Err((ctx.on_symlink_loop)(path));
        }

        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        dirs.push(path.to_path_buf());

        let entries = fs::read_dir(path).map_err(|e| (ctx.on_io_error)(path, e))?;
        for entry in entries.flatten() {
            walk_dir_recursive(
                &entry.path(),
                source_root,
                files,
                dirs,
                total_bytes,
                last_progress_time,
                visited,
                ctx,
            )?;
        }
    } else {
        log::warn!("scan: skipping special file: {}", path.display());
    }

    if last_progress_time.elapsed() >= ctx.progress_interval {
        (ctx.on_progress)(
            files.len(),
            dirs.len(),
            *total_bytes,
            path.file_name().map(|n| n.to_string_lossy().to_string()),
        );
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Tries to get cached scan results for a preview, removing them from cache.
pub(super) fn take_cached_scan_result(preview_id: &str) -> Option<ScanResult> {
    if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
        cache.remove(preview_id).map(|cached| ScanResult {
            files: cached.files,
            dirs: cached.dirs,
            file_count: cached.file_count,
            total_bytes: cached.total_bytes,
        })
    } else {
        None
    }
}

// ============================================================================
// Scanning helpers
// ============================================================================

/// Interval for checking cancellation while waiting for scan results.
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Sorts files according to the specified column and order.
pub(super) fn sort_files(files: &mut [FileInfo], column: SortColumn, order: SortOrder) {
    files.sort_by(|a, b| {
        let cmp = match column {
            SortColumn::Name => a.name_lower().cmp(&b.name_lower()),
            SortColumn::Extension => a
                .extension()
                .cmp(&b.extension())
                .then_with(|| a.name_lower().cmp(&b.name_lower())),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Created => a.created.cmp(&b.created),
        };
        match order {
            SortOrder::Ascending => cmp,
            SortOrder::Descending => cmp.reverse(),
        }
    });
}

/// Scans source paths recursively, returns file list and totals.
/// Files are sorted according to the specified column and order.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
pub(super) fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
) -> Result<ScanResult, WriteOperationError> {
    use std::sync::mpsc;

    // Clone data for the background thread
    let sources = sources.to_vec();
    let state_clone = Arc::clone(state);
    let app_clone = app.clone();
    let operation_id_owned = operation_id.to_string();
    let progress_interval = state.progress_interval;

    // Channel for receiving scan results
    let (tx, rx) = mpsc::channel();

    // Spawn scanning thread
    std::thread::spawn(move || {
        let result = scan_sources_internal(
            &sources,
            &state_clone,
            &app_clone,
            &operation_id_owned,
            operation_type,
            sort_column,
            sort_order,
            progress_interval,
        );
        let _ = tx.send(result);
    });

    // Poll for results, checking cancellation flag between polls.
    // This ensures we respond quickly to cancellation even if filesystem I/O is blocked.
    loop {
        // Check cancellation before waiting
        if state.cancelled.load(Ordering::Relaxed) {
            log::debug!("scan: cancellation detected during scan polling op={}", operation_id);
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
                // Thread panicked or was unexpectedly terminated
                return Err(WriteOperationError::IoError {
                    path: "scan".to_string(),
                    message: "Scan thread terminated unexpectedly".to_string(),
                });
            }
        }
    }
}

/// Internal scan implementation (runs in background thread).
#[allow(
    clippy::too_many_arguments,
    reason = "Internal helper passes through all required context"
)]
fn scan_sources_internal(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval: Duration,
) -> Result<ScanResult, WriteOperationError> {
    use tauri::Emitter;

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    let ctx = WalkContext {
        progress_interval,
        is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
        on_io_error: &|path, e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        },
        on_cancelled: &|| WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        on_symlink_loop: &|path| WriteOperationError::SymlinkLoop {
            path: path.display().to_string(),
        },
        on_progress: &|files_done, _, bytes_done, current_file| {
            log::debug!(
                "scan: emitting write-progress op={} phase=scanning files_found={} bytes_found={}",
                operation_id,
                files_done,
                bytes_done
            );
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type,
                    phase: WriteOperationPhase::Scanning,
                    current_file: current_file.clone(),
                    files_done,
                    files_total: 0,
                    bytes_done,
                    bytes_total: 0,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Scanning,
                current_file,
                files_done,
                0,
                bytes_done,
                0,
            );
        },
    };

    for source in sources {
        let source_root = source.parent().unwrap_or(source);
        walk_dir_recursive(
            source,
            source_root,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            &mut last_progress_time,
            &mut visited,
            &ctx,
        )?;
    }

    // Sort files according to configuration
    sort_files(&mut files, sort_column, sort_order);

    // Emit final scanning progress
    log::debug!(
        "scan: emitting final write-progress op={} phase=scanning files={} bytes={}",
        operation_id,
        files.len(),
        total_bytes
    );
    let _ = app.emit(
        "write-progress",
        WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type,
            phase: WriteOperationPhase::Scanning,
            current_file: None,
            files_done: files.len(),
            files_total: files.len(),
            bytes_done: total_bytes,
            bytes_total: total_bytes,
        },
    );

    Ok(ScanResult {
        file_count: files.len(),
        files,
        dirs,
        total_bytes,
    })
}

// ============================================================================
// Dry-run scanning (with conflict detection)
// ============================================================================

/// Result of a dry-run scan including conflicts.
pub(super) struct DryRunScanResult {
    /// Total number of files
    pub file_count: usize,
    /// Total bytes
    pub total_bytes: u64,
    /// All detected conflicts
    pub conflicts: Vec<ConflictInfo>,
}

/// Performs a dry-run scan: scans sources, detects conflicts at destination.
/// Emits ScanProgressEvent during scanning with conflict counts.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn dry_run_scan(
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
) -> Result<DryRunScanResult, WriteOperationError> {
    use std::sync::mpsc;

    // Clone data for the background thread
    let sources = sources.to_vec();
    let destination = destination.to_path_buf();
    let state_clone = Arc::clone(state);
    let app_clone = app.clone();
    let operation_id_owned = operation_id.to_string();

    // Channel for receiving scan results
    let (tx, rx) = mpsc::channel();

    // Spawn scanning thread
    std::thread::spawn(move || {
        let result = dry_run_scan_internal(
            &sources,
            &destination,
            &state_clone,
            &app_clone,
            &operation_id_owned,
            operation_type,
            progress_interval,
        );
        let _ = tx.send(result);
    });

    // Poll for results, checking cancellation flag between polls
    loop {
        if state.cancelled.load(Ordering::Relaxed) {
            log::debug!(
                "scan: cancellation detected during dry-run scan polling op={}",
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
                    path: "dry_run_scan".to_string(),
                    message: "Scan thread terminated unexpectedly".to_string(),
                });
            }
        }
    }
}

/// Internal dry-run scan implementation (runs in background thread).
fn dry_run_scan_internal(
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
) -> Result<DryRunScanResult, WriteOperationError> {
    use tauri::Emitter;

    let mut files_found = 0usize;
    let mut bytes_found = 0u64;
    let mut conflicts = Vec::new();
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    for source in sources {
        dry_run_scan_recursive(
            source,
            source,
            destination,
            &mut files_found,
            &mut bytes_found,
            &mut conflicts,
            state,
            app,
            operation_id,
            operation_type,
            &progress_interval,
            &mut last_progress_time,
            &mut visited,
        )?;
    }

    // Emit final scan progress
    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type,
            files_found,
            bytes_found,
            conflicts_found: conflicts.len(),
            current_path: None,
        },
    );

    Ok(DryRunScanResult {
        file_count: files_found,
        total_bytes: bytes_found,
        conflicts,
    })
}

/// Recursively scans a path for dry-run, detecting conflicts.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
fn dry_run_scan_recursive(
    path: &Path,
    source_root: &Path,
    dest_root: &Path,
    files_found: &mut usize,
    bytes_found: &mut u64,
    conflicts: &mut Vec<ConflictInfo>,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: &Duration,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Use symlink_metadata to not follow symlinks
    let metadata = fs::symlink_metadata(path).map_err(|e| WriteOperationError::IoError {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;

    // Calculate destination path
    let dest_path = calculate_dest_path(path, source_root, dest_root)?;

    if metadata.is_symlink() || metadata.is_file() {
        *bytes_found += metadata.len();
        *files_found += 1;

        // Check for conflict
        if (dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok())
            && let Some(conflict) = create_conflict_info(path, &dest_path, &metadata)?
        {
            // Emit conflict event for streaming
            let _ = app.emit("scan-conflict", conflict.clone());
            conflicts.push(conflict);
        }
    } else if metadata.is_dir() {
        // Check for symlink loop before recursing
        if is_symlink_loop(path, visited) {
            return Err(WriteOperationError::SymlinkLoop {
                path: path.display().to_string(),
            });
        }

        // Track this directory
        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        // Check if destination exists and is not a directory (type conflict)
        if dest_path.exists()
            && !dest_path.is_dir()
            && let Some(conflict) = create_conflict_info(path, &dest_path, &metadata)?
        {
            let _ = app.emit("scan-conflict", conflict.clone());
            conflicts.push(conflict);
        }

        // Scan contents
        let entries = fs::read_dir(path).map_err(|e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;

        for entry in entries.flatten() {
            dry_run_scan_recursive(
                &entry.path(),
                source_root,
                dest_root,
                files_found,
                bytes_found,
                conflicts,
                state,
                app,
                operation_id,
                operation_type,
                progress_interval,
                last_progress_time,
                visited,
            )?;
        }
    } else {
        // Skip special files (sockets, FIFOs, char/block devices)
        log::warn!("dry_run_scan: skipping special file: {}", path.display());
    }

    // Emit progress periodically
    if last_progress_time.elapsed() >= *progress_interval {
        let _ = app.emit(
            "scan-progress",
            ScanProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type,
                files_found: *files_found,
                bytes_found: *bytes_found,
                conflicts_found: conflicts.len(),
                current_path: path.file_name().map(|n| n.to_string_lossy().to_string()),
            },
        );
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Handles dry-run mode for copy/move operations.
/// Returns Ok(true) if dry-run was performed, Ok(false) if not dry-run mode.
#[allow(
    clippy::too_many_arguments,
    reason = "Dry-run requires all operation context parameters"
)]
pub(super) fn handle_dry_run(
    config_dry_run: bool,
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
    max_conflicts_to_show: usize,
) -> Result<bool, WriteOperationError> {
    use super::types::DryRunResult;
    use tauri::Emitter;

    if !config_dry_run {
        return Ok(false);
    }

    let scan_result = dry_run_scan(
        sources,
        destination,
        state,
        app,
        operation_id,
        operation_type,
        progress_interval,
    )?;

    let conflicts_count = scan_result.conflicts.len();
    let (sampled_conflicts, conflicts_sampled) = sample_conflicts(scan_result.conflicts, max_conflicts_to_show);

    let result = DryRunResult {
        operation_id: operation_id.to_string(),
        operation_type,
        files_total: scan_result.file_count,
        bytes_total: scan_result.total_bytes,
        conflicts_total: conflicts_count,
        conflicts: sampled_conflicts,
        conflicts_sampled,
    };

    let _ = app.emit("dry-run-complete", result);
    Ok(true)
}
