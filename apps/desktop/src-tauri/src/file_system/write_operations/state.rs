//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use super::types::{ConflictResolution, OperationStatus, OperationSummary, WriteOperationPhase, WriteOperationType};

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// Cancellation flag
    pub cancelled: AtomicBool,
    /// Skip rollback flag (when true, keep partial files on cancellation)
    pub skip_rollback: AtomicBool,
    /// Progress reporting interval
    pub progress_interval: Duration,
    /// Pending conflict resolution (set by resolve_write_conflict)
    pub pending_resolution: RwLock<Option<ConflictResolutionResponse>>,
    /// Condvar for waiting on conflict resolution
    pub conflict_condvar: std::sync::Condvar,
    /// Mutex for conflict condvar
    pub conflict_mutex: std::sync::Mutex<bool>,
}

/// Response to a conflict resolution request.
#[derive(Debug, Clone)]
pub struct ConflictResolutionResponse {
    /// The resolution to apply
    pub resolution: ConflictResolution,
    /// Whether to apply this resolution to all future conflicts
    pub apply_to_all: bool,
}

/// Global cache for in-progress write operation states.
pub(super) static WRITE_OPERATION_STATE: LazyLock<RwLock<HashMap<String, Arc<WriteOperationState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global cache for operation status (for query APIs).
static OPERATION_STATUS_CACHE: LazyLock<RwLock<HashMap<String, OperationStatusInternal>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Internal status tracking for operations.
#[derive(Debug, Clone)]
struct OperationStatusInternal {
    operation_type: WriteOperationType,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    started_at: u64,
}

// ============================================================================
// Status cache management
// ============================================================================

/// Updates the internal status for an operation.
pub(super) fn update_operation_status(
    operation_id: &str,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
) {
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write()
        && let Some(status) = cache.get_mut(operation_id)
    {
        status.phase = phase;
        status.current_file = current_file;
        status.files_done = files_done;
        status.files_total = files_total;
        status.bytes_done = bytes_done;
        status.bytes_total = bytes_total;
    }
}

/// Registers a new operation in the status cache.
pub(super) fn register_operation_status(operation_id: &str, operation_type: WriteOperationType) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.insert(
            operation_id.to_string(),
            OperationStatusInternal {
                operation_type,
                phase: WriteOperationPhase::Scanning,
                current_file: None,
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
                started_at: now,
            },
        );
    }
}

/// Removes an operation from the status cache.
pub(super) fn unregister_operation_status(operation_id: &str) {
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.remove(operation_id);
    }
}

// ============================================================================
// Public query functions
// ============================================================================

/// Lists all active write operations.
///
/// Returns a list of operation summaries for all currently running operations.
/// This is useful for showing a global progress view or managing multiple concurrent operations.
pub fn list_active_operations() -> Vec<OperationSummary> {
    let cache = match OPERATION_STATUS_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .map(|(id, status)| {
            let percent_complete = if status.bytes_total > 0 {
                ((status.bytes_done as f64 / status.bytes_total as f64) * 100.0).min(100.0) as u8
            } else if status.files_total > 0 {
                ((status.files_done as f64 / status.files_total as f64) * 100.0).min(100.0) as u8
            } else {
                0
            };

            OperationSummary {
                operation_id: id.clone(),
                operation_type: status.operation_type,
                phase: status.phase,
                percent_complete,
                started_at: status.started_at,
            }
        })
        .collect()
}

/// Gets the detailed status of a specific operation.
///
/// Returns `None` if the operation is not found (either never existed or already completed).
pub fn get_operation_status(operation_id: &str) -> Option<OperationStatus> {
    let cache = OPERATION_STATUS_CACHE.read().ok()?;
    let status = cache.get(operation_id)?;

    // Check if the operation is still running
    let is_running = WRITE_OPERATION_STATE
        .read()
        .ok()
        .map(|c| c.contains_key(operation_id))
        .unwrap_or(false);

    Some(OperationStatus {
        operation_id: operation_id.to_string(),
        operation_type: status.operation_type,
        phase: status.phase,
        is_running,
        current_file: status.current_file.clone(),
        files_done: status.files_done,
        files_total: status.files_total,
        bytes_done: status.bytes_done,
        bytes_total: status.bytes_total,
        started_at: status.started_at,
    })
}

/// Cancels an in-progress write operation.
///
/// # Arguments
/// * `operation_id` - The operation ID to cancel
/// * `rollback` - If true, delete any partial files created. If false, keep them.
pub fn cancel_write_operation(operation_id: &str, rollback: bool) {
    if let Ok(cache) = WRITE_OPERATION_STATE.read()
        && let Some(state) = cache.get(operation_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
        state.skip_rollback.store(!rollback, Ordering::Relaxed);
        // Wake up any waiting conflict resolution
        let _guard = state.conflict_mutex.lock();
        state.conflict_condvar.notify_all();
    }
}

/// Resolves a pending conflict for an in-progress write operation.
///
/// When an operation encounters a conflict in Stop mode, it emits a WriteConflictEvent
/// and waits for this function to be called. The operation will then proceed with the
/// chosen resolution.
///
/// # Arguments
/// * `operation_id` - The operation ID that has a pending conflict
/// * `resolution` - How to resolve the conflict (Skip, Overwrite, or Rename)
/// * `apply_to_all` - If true, apply this resolution to all future conflicts in this operation
pub fn resolve_write_conflict(operation_id: &str, resolution: ConflictResolution, apply_to_all: bool) {
    if let Ok(cache) = WRITE_OPERATION_STATE.read()
        && let Some(state) = cache.get(operation_id)
    {
        // Set the pending resolution
        if let Ok(mut pending) = state.pending_resolution.write() {
            *pending = Some(ConflictResolutionResponse {
                resolution,
                apply_to_all,
            });
        }
        // Wake up the waiting operation
        let _guard = state.conflict_mutex.lock();
        state.conflict_condvar.notify_all();
    }
}

// ============================================================================
// Scan preview state
// ============================================================================

/// State for a scan preview operation.
pub(super) struct ScanPreviewState {
    /// Cancellation flag
    pub cancelled: AtomicBool,
    /// Progress reporting interval
    pub progress_interval: Duration,
}

/// Cached result from a completed scan preview.
#[allow(dead_code, reason = "Fields read via take_cached_scan_result")]
pub(super) struct CachedScanResult {
    pub files: Vec<FileInfo>,
    pub dirs: Vec<PathBuf>,
    pub file_count: usize,
    pub total_bytes: u64,
}

/// Global cache for scan preview states.
pub(super) static SCAN_PREVIEW_STATE: LazyLock<RwLock<HashMap<String, Arc<ScanPreviewState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global cache for completed scan preview results.
pub(super) static SCAN_PREVIEW_RESULTS: LazyLock<RwLock<HashMap<String, CachedScanResult>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// FileInfo (used for scanning and sorting)
// ============================================================================

/// File info collected during scan (used for sorting).
#[derive(Debug, Clone)]
pub(super) struct FileInfo {
    pub path: PathBuf,
    /// Parent of the original source (used to compute relative path for destination)
    pub source_root: PathBuf,
    pub size: u64,
    pub modified: u64, // Unix timestamp in seconds
    pub created: u64,  // Unix timestamp in seconds
    pub is_symlink: bool,
}

impl FileInfo {
    pub fn new(path: PathBuf, source_root: PathBuf, metadata: &std::fs::Metadata) -> Self {
        use std::time::UNIX_EPOCH;
        Self {
            path,
            source_root,
            size: metadata.len(),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            is_symlink: metadata.is_symlink(),
        }
    }

    /// Get extension for sorting (lowercase, empty string if none).
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Get filename for sorting (lowercase).
    pub fn name_lower(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Compute the destination path for this file given the destination root.
    pub fn dest_path(&self, destination: &std::path::Path) -> PathBuf {
        // Strip source_root from path to get relative path, then join with destination
        if let Ok(relative) = self.path.strip_prefix(&self.source_root) {
            destination.join(relative)
        } else {
            // Fallback: just use the filename
            destination.join(self.path.file_name().unwrap_or_default())
        }
    }
}

/// Information about files to be processed.
pub(super) struct ScanResult {
    /// All files to process with metadata for sorting
    pub files: Vec<FileInfo>,
    /// Directories to process (for deletion, in reverse order - deepest first)
    pub dirs: Vec<PathBuf>,
    /// Total file count (not including directories)
    pub file_count: usize,
    /// Total byte size of all files
    pub total_bytes: u64,
}

// ============================================================================
// Copy transaction for rollback
// ============================================================================

/// Tracks created files/directories for rollback on failure.
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CopyTransaction {
    /// Files created during the operation (in creation order)
    pub created_files: Vec<PathBuf>,
    /// Directories created during the operation (in creation order)
    pub created_dirs: Vec<PathBuf>,
}

impl CopyTransaction {
    pub fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
        }
    }

    pub fn record_file(&mut self, path: PathBuf) {
        self.created_files.push(path);
    }

    pub fn record_dir(&mut self, path: PathBuf) {
        self.created_dirs.push(path);
    }

    /// Rolls back all created files and directories.
    pub fn rollback(&self) {
        // Delete files first (in reverse order)
        for file in self.created_files.iter().rev() {
            let _ = std::fs::remove_file(file);
        }
        // Then directories (deepest first, already in reverse due to creation order)
        for dir in self.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }

    /// Clears the transaction (call on success to prevent rollback).
    pub fn commit(self) {
        // Just drop without calling rollback
        drop(self.created_files);
        drop(self.created_dirs);
    }
}
