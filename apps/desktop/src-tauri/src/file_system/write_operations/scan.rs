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
    ScanPreviewStartResult, ScanProgressEvent, SortColumn, SortOrder, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};

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
        for source in &sources {
            let source_root = source.parent().unwrap_or(source);
            scan_preview_recursive(
                source,
                source_root,
                &mut files,
                &mut dirs,
                &mut total_bytes,
                &state,
                &app,
                &preview_id,
                &mut last_progress_time,
                &mut visited,
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

/// Recursive helper for scan preview. Returns Err if cancelled.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
fn scan_preview_recursive(
    path: &Path,
    source_root: &Path,
    files: &mut Vec<FileInfo>,
    dirs: &mut Vec<PathBuf>,
    total_bytes: &mut u64,
    state: &Arc<ScanPreviewState>,
    app: &tauri::AppHandle,
    preview_id: &str,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    use tauri::Emitter;

    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }

    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;

    if metadata.is_symlink() || metadata.is_file() {
        *total_bytes += metadata.len();
        files.push(FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata));
    } else if metadata.is_dir() {
        // Check for symlink loop
        if is_symlink_loop(path, visited) {
            return Err(format!("Symlink loop detected: {}", path.display()));
        }

        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        dirs.push(path.to_path_buf());

        let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            scan_preview_recursive(
                &entry.path(),
                source_root,
                files,
                dirs,
                total_bytes,
                state,
                app,
                preview_id,
                last_progress_time,
                visited,
            )?;
        }
    } else {
        // Skip special files (sockets, FIFOs, char/block devices)
        log::warn!("scan_preview: skipping special file: {}", path.display());
    }

    // Emit progress periodically
    if last_progress_time.elapsed() >= state.progress_interval {
        let _ = app.emit(
            "scan-preview-progress",
            ScanPreviewProgressEvent {
                preview_id: preview_id.to_string(),
                files_found: files.len(),
                dirs_found: dirs.len(),
                bytes_found: *total_bytes,
                current_path: path.file_name().map(|n| n.to_string_lossy().to_string()),
            },
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
pub(super) fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
) -> Result<ScanResult, WriteOperationError> {
    use tauri::Emitter;

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    for source in sources {
        // source_root is the parent directory of the source file/folder
        // This is used to compute relative paths for the destination
        let source_root = source.parent().unwrap_or(source);
        scan_path_recursive(
            source,
            source_root,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            state,
            app,
            operation_id,
            operation_type,
            &state.progress_interval,
            &mut last_progress_time,
            &mut visited,
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

#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
fn scan_path_recursive(
    path: &Path,
    source_root: &Path,
    files: &mut Vec<FileInfo>,
    dirs: &mut Vec<PathBuf>,
    total_bytes: &mut u64,
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

    if metadata.is_symlink() {
        // For symlinks, just add the size and the file itself
        *total_bytes += metadata.len();
        files.push(FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata));
    } else if metadata.is_file() {
        *total_bytes += metadata.len();
        files.push(FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata));
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

        // Add directory to list (for deletion tracking)
        dirs.push(path.to_path_buf());

        // Scan contents
        let entries = fs::read_dir(path).map_err(|e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;

        for entry in entries.flatten() {
            scan_path_recursive(
                &entry.path(),
                source_root,
                files,
                dirs,
                total_bytes,
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
        log::warn!("scan: skipping special file: {}", path.display());
    }

    // Emit progress periodically
    if last_progress_time.elapsed() >= *progress_interval {
        let current_file = path.file_name().map(|n| n.to_string_lossy().to_string());
        log::debug!(
            "scan: emitting write-progress op={} phase=scanning files_found={} bytes_found={}",
            operation_id,
            files.len(),
            *total_bytes
        );
        let _ = app.emit(
            "write-progress",
            WriteProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type,
                phase: WriteOperationPhase::Scanning,
                current_file: current_file.clone(),
                files_done: files.len(),
                files_total: 0, // Unknown during scanning
                bytes_done: *total_bytes,
                bytes_total: 0, // Unknown during scanning
            },
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::Scanning,
            current_file,
            files.len(),
            0,
            *total_bytes,
            0,
        );
        *last_progress_time = Instant::now();
    }

    Ok(())
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
