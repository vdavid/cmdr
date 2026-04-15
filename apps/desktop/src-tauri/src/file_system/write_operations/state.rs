//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use super::types::{ConflictResolution, OperationStatus, OperationSummary, WriteOperationPhase, WriteOperationType};

// ============================================================================
// Operation intent (state machine for cancellation)
// ============================================================================

/// What the operation should do next.
///
/// State machine: `Running` → `RollingBack` or `Stopped`, `RollingBack` → `Stopped`.
/// No reverse transitions. Encoded as `AtomicU8` for lock-free sharing with native
/// copy callbacks (macOS `copyfile`, chunked copy, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum OperationIntent {
    /// Continue the operation normally.
    Running = 0,
    /// Stop the forward operation and delete created files.
    RollingBack = 1,
    /// Stop immediately, keep partial files.
    Stopped = 2,
}

impl OperationIntent {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::RollingBack,
            2 => Self::Stopped,
            _ => Self::Running,
        }
    }
}

/// Loads the current intent from an `AtomicU8`.
pub(crate) fn load_intent(intent: &AtomicU8) -> OperationIntent {
    OperationIntent::from_u8(intent.load(Ordering::Relaxed))
}

/// Returns true if the operation should stop (intent is not `Running`).
/// Use this for the common cancellation check in copy/delete/move loops.
pub(crate) fn is_cancelled(intent: &AtomicU8) -> bool {
    intent.load(Ordering::Relaxed) != OperationIntent::Running as u8
}

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// Shared with native copy operations for cancellation checks.
    /// Encodes `OperationIntent` as a `u8`. Use `is_cancelled()` / `load_intent()` to read.
    pub intent: Arc<AtomicU8>,
    pub progress_interval: Duration,
    /// Sender for conflict resolution. Created on demand when a conflict occurs;
    /// the receiver is held by the waiting operation. `resolve_write_conflict` takes
    /// the sender and sends the resolution. Dropping the sender unblocks the receiver
    /// with an error, which the waiting code interprets as cancellation.
    pub conflict_resolution_tx: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionResponse>>>,
}

/// Response to a conflict resolution request.
#[derive(Debug, Clone)]
pub struct ConflictResolutionResponse {
    pub resolution: ConflictResolution,
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
/// State transitions: `Running → RollingBack` (rollback=true), `Running → Stopped` (rollback=false),
/// `RollingBack → Stopped` (cancel during rollback). Other transitions are no-ops.
///
/// # Arguments
/// * `operation_id` - The operation ID to cancel
/// * `rollback` - If true, roll back (delete created files). If false, stop and keep partial files.
pub fn cancel_write_operation(operation_id: &str, rollback: bool) {
    if let Ok(cache) = WRITE_OPERATION_STATE.read()
        && let Some(state) = cache.get(operation_id)
    {
        let target = if rollback {
            OperationIntent::RollingBack
        } else {
            OperationIntent::Stopped
        };
        let current = OperationIntent::from_u8(state.intent.load(Ordering::Relaxed));

        // Valid transitions: Running → RollingBack/Stopped, RollingBack → Stopped.
        // Stopped is terminal — no further transitions.
        let valid = matches!(
            (current, target),
            (OperationIntent::Running, _) | (OperationIntent::RollingBack, OperationIntent::Stopped)
        );
        if !valid {
            return;
        }

        state.intent.store(target as u8, Ordering::Relaxed);
        // Drop the conflict resolution sender to unblock any waiting receiver
        let _ = state.conflict_resolution_tx.lock().unwrap().take();
    }
}

/// Stops all in-progress write operations without rollback.
///
/// Used as a safety net when the frontend is tearing down (beforeunload, hot-reload).
/// Transitions to `Stopped` (not `RollingBack`) because teardown must never silently
/// delete files in the background without visual feedback.
pub fn cancel_all_write_operations() {
    if let Ok(cache) = WRITE_OPERATION_STATE.read() {
        for (id, state) in cache.iter() {
            let current = load_intent(&state.intent);
            if current != OperationIntent::Stopped {
                log::info!("cancel_all_write_operations: stopping op={id}");
                state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
                // Drop the conflict resolution sender to unblock any waiting receiver
                let _ = state.conflict_resolution_tx.lock().unwrap().take();
            }
        }
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
        // Take the sender and send the resolution through the oneshot channel
        let tx = state.conflict_resolution_tx.lock().unwrap().take();
        if let Some(tx) = tx {
            let _ = tx.send(ConflictResolutionResponse {
                resolution,
                apply_to_all,
            });
        }
    }
}

// ============================================================================
// Scan preview state
// ============================================================================

/// State for a scan preview operation.
pub(super) struct ScanPreviewState {
    pub cancelled: AtomicBool,
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
    pub files: Vec<FileInfo>,
    /// For deletion: in reverse order, deepest first.
    pub dirs: Vec<PathBuf>,
    /// Not including directories.
    pub file_count: usize,
    pub total_bytes: u64,
}

// ============================================================================
// Copy transaction for rollback
// ============================================================================

/// Tracks created files/directories for rollback on failure.
///
/// If dropped without calling `commit()`, automatically rolls back
/// (deletes) all recorded files and directories. This ensures cleanup
/// even if a thread panics during the copy loop.
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CopyTransaction {
    /// In creation order.
    pub created_files: Vec<PathBuf>,
    /// In creation order.
    pub created_dirs: Vec<PathBuf>,
    /// Set to `true` by `commit()` to prevent rollback on drop.
    committed: bool,
}

impl CopyTransaction {
    pub fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
            committed: false,
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

    /// Marks the transaction as committed, preventing rollback on drop.
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for CopyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            log::warn!(
                "CopyTransaction dropped without commit, rolling back {} files and {} dirs",
                self.created_files.len(),
                self.created_dirs.len()
            );
            self.rollback();
        }
    }
}
