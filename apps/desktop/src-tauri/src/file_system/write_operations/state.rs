//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use super::eta::EtaEstimator;
use super::types::{
    ConflictResolution, OperationEventSink, OperationStatus, OperationSummary, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};

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
    /// Per-operation ETA + throughput estimator. Fed by `enrich_progress_event`
    /// at every `write-progress` emit site, so every emitter (local copy/delete,
    /// volume copy/move, MTP, SMB) reports rates and ETA uniformly.
    pub estimator: std::sync::Mutex<EtaEstimator>,
}

impl WriteOperationState {
    /// Construct a fresh state for a new operation. Use this from every
    /// `*_files_start` entry point; keeps the field list out of every call
    /// site so adding new state members (like the estimator) is one-line.
    pub fn new(progress_interval: Duration) -> Self {
        Self {
            intent: Arc::new(AtomicU8::new(OperationIntent::Running as u8)),
            progress_interval,
            conflict_resolution_tx: std::sync::Mutex::new(None),
            estimator: std::sync::Mutex::new(EtaEstimator::new()),
        }
    }

    /// Populate `bytes_per_second`, `files_per_second`, and `eta_seconds` on a
    /// `WriteProgressEvent` before it's emitted. Call this from every
    /// `write-progress` emit site (local copy, local delete, trash, volume
    /// copy, volume move, MTP, SMB) so the FE sees uniform rates and ETA
    /// regardless of which backend produced the event.
    pub fn enrich_progress(&self, event: &mut WriteProgressEvent) {
        let stats = match self.estimator.lock() {
            Ok(mut est) => est.update(
                std::time::Instant::now(),
                event.phase,
                event.bytes_done,
                event.bytes_total,
                event.files_done,
                event.files_total,
            ),
            // Poisoned mutex (another thread panicked). Skip the enrichment
            // rather than propagating the panic; progress events are advisory.
            Err(_) => return,
        };
        event.bytes_per_second = Some(stats.bytes_per_second);
        event.files_per_second = Some(stats.files_per_second);
        event.eta_seconds = stats.eta_seconds;
    }

    /// Enrich and emit a `WriteProgressEvent` via a Tauri `AppHandle`. The
    /// canonical emit path for local copy/delete/trash, which don't go through
    /// the `OperationEventSink` indirection.
    pub fn emit_progress_via_app(&self, app: &tauri::AppHandle, mut event: WriteProgressEvent) {
        use tauri::Emitter;
        self.enrich_progress(&mut event);
        let _ = app.emit("write-progress", &event);
    }

    /// Enrich and emit a `WriteProgressEvent` via an `OperationEventSink`. The
    /// volume-copy/move pipeline uses this for testability: the test sink
    /// stores events in a `Vec` instead of calling `app.emit`.
    pub fn emit_progress_via_sink(&self, sink: &dyn OperationEventSink, mut event: WriteProgressEvent) {
        self.enrich_progress(&mut event);
        sink.emit_progress(event);
    }
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
        // Stopped is terminal; no further transitions.
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

#[cfg(test)]
mod tests {
    //! Targeted unit tests covering survivors from `cargo mutants` on this
    //! module (state machine transitions, status-cache CRUD, FileInfo
    //! sort-key derivation, and CopyTransaction commit/rollback/Drop).
    //!
    //! Tests that touch the global `WRITE_OPERATION_STATE` /
    //! `OPERATION_STATUS_CACHE` caches use unique operation IDs so they don't
    //! collide with concurrent test runs in the same process.
    use super::*;
    use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationType};
    use std::path::Path;
    use std::sync::atomic::Ordering;

    fn unique_id(label: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        format!("test-state-{label}-{n}-{:?}", std::thread::current().id())
    }

    // ---- OperationIntent::from_u8 ----

    #[test]
    fn from_u8_maps_each_known_variant() {
        // Kills: replace from_u8 → Default::default(), delete match arm 1, delete match arm 2.
        assert_eq!(OperationIntent::from_u8(0), OperationIntent::Running);
        assert_eq!(OperationIntent::from_u8(1), OperationIntent::RollingBack);
        assert_eq!(OperationIntent::from_u8(2), OperationIntent::Stopped);
    }

    #[test]
    fn from_u8_unknown_values_fall_back_to_running() {
        // Pins the catch-all arm. If a future variant is added, this should fail.
        assert_eq!(OperationIntent::from_u8(3), OperationIntent::Running);
        assert_eq!(OperationIntent::from_u8(255), OperationIntent::Running);
    }

    // ---- load_intent / is_cancelled ----

    #[test]
    fn load_intent_reflects_atomic_value() {
        let atom = AtomicU8::new(OperationIntent::RollingBack as u8);
        assert_eq!(load_intent(&atom), OperationIntent::RollingBack);
        atom.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
        assert_eq!(load_intent(&atom), OperationIntent::Stopped);
        atom.store(OperationIntent::Running as u8, Ordering::Relaxed);
        assert_eq!(load_intent(&atom), OperationIntent::Running);
    }

    #[test]
    fn is_cancelled_is_true_for_any_non_running_value() {
        // Kills: replace is_cancelled → true / → false, replace != with ==.
        let running = AtomicU8::new(OperationIntent::Running as u8);
        assert!(!is_cancelled(&running), "Running must not be reported as cancelled");

        let rolling = AtomicU8::new(OperationIntent::RollingBack as u8);
        assert!(is_cancelled(&rolling), "RollingBack must be reported as cancelled");

        let stopped = AtomicU8::new(OperationIntent::Stopped as u8);
        assert!(is_cancelled(&stopped), "Stopped must be reported as cancelled");
    }

    // ---- cancel_write_operation state-machine transitions ----
    //
    // Helper: install a fresh state into the global cache under `op_id`, run
    // the cancellation, then read back the resulting intent. Cleans up after
    // itself so the global cache isn't polluted.

    fn install_state(op_id: &str, initial: OperationIntent) -> Arc<WriteOperationState> {
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
        state.intent.store(initial as u8, Ordering::Relaxed);
        WRITE_OPERATION_STATE
            .write()
            .unwrap()
            .insert(op_id.to_string(), Arc::clone(&state));
        state
    }

    fn uninstall_state(op_id: &str) {
        WRITE_OPERATION_STATE.write().unwrap().remove(op_id);
    }

    #[test]
    fn cancel_running_with_rollback_goes_to_rolling_back() {
        let id = unique_id("cancel-running-rollback");
        let state = install_state(&id, OperationIntent::Running);
        cancel_write_operation(&id, true);
        assert_eq!(load_intent(&state.intent), OperationIntent::RollingBack);
        uninstall_state(&id);
    }

    #[test]
    fn cancel_running_without_rollback_goes_to_stopped() {
        let id = unique_id("cancel-running-stop");
        let state = install_state(&id, OperationIntent::Running);
        cancel_write_operation(&id, false);
        assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
        uninstall_state(&id);
    }

    #[test]
    fn cancel_rolling_back_with_rollback_is_a_noop() {
        // Only RollingBack → Stopped is valid; RollingBack → RollingBack is a no-op.
        let id = unique_id("cancel-rb-rb");
        let state = install_state(&id, OperationIntent::RollingBack);
        cancel_write_operation(&id, true);
        assert_eq!(
            load_intent(&state.intent),
            OperationIntent::RollingBack,
            "RollingBack → RollingBack is not a valid transition; intent must not change"
        );
        uninstall_state(&id);
    }

    #[test]
    fn cancel_rolling_back_without_rollback_goes_to_stopped() {
        let id = unique_id("cancel-rb-stop");
        let state = install_state(&id, OperationIntent::RollingBack);
        cancel_write_operation(&id, false);
        assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
        uninstall_state(&id);
    }

    #[test]
    fn cancel_stopped_is_terminal_for_any_target() {
        // Stopped is terminal; no transition is valid from it.
        let id = unique_id("cancel-stopped");
        let state = install_state(&id, OperationIntent::Stopped);
        cancel_write_operation(&id, true);
        assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
        cancel_write_operation(&id, false);
        assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
        uninstall_state(&id);
    }

    #[test]
    fn cancel_drops_the_conflict_resolution_sender() {
        // After cancel, any pending receiver should observe a closed channel.
        let id = unique_id("cancel-drops-tx");
        let state = install_state(&id, OperationIntent::Running);
        let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
        *state.conflict_resolution_tx.lock().unwrap() = Some(tx);
        cancel_write_operation(&id, false);
        // The receiver should now be closed (sender dropped).
        match rx.try_recv() {
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {} // good
            other => panic!("expected sender to be dropped, got {other:?}"),
        }
        uninstall_state(&id);
    }

    #[test]
    fn cancel_unknown_operation_is_a_silent_noop() {
        // No installed state; must not panic, must not affect anything.
        cancel_write_operation("does-not-exist-xyzzy", true);
        cancel_write_operation("does-not-exist-xyzzy", false);
    }

    // ---- cancel_all_write_operations ----

    #[test]
    fn cancel_all_stops_running_but_does_not_re_stop_already_stopped() {
        // Pins the `current != OperationIntent::Stopped` guard. If the guard
        // flips to `==`, running operations would NOT be stopped; they'd
        // remain running.
        let running_id = unique_id("cancel-all-running");
        let stopped_id = unique_id("cancel-all-stopped");
        let rb_id = unique_id("cancel-all-rb");

        let running = install_state(&running_id, OperationIntent::Running);
        let stopped = install_state(&stopped_id, OperationIntent::Stopped);
        let rb = install_state(&rb_id, OperationIntent::RollingBack);

        cancel_all_write_operations();

        assert_eq!(load_intent(&running.intent), OperationIntent::Stopped);
        assert_eq!(load_intent(&stopped.intent), OperationIntent::Stopped);
        assert_eq!(
            load_intent(&rb.intent),
            OperationIntent::Stopped,
            "RollingBack should also be force-stopped on teardown"
        );

        uninstall_state(&running_id);
        uninstall_state(&stopped_id);
        uninstall_state(&rb_id);
    }

    // ---- resolve_write_conflict ----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resolve_write_conflict_delivers_response_to_waiter() {
        let id = unique_id("resolve-conflict");
        let state = install_state(&id, OperationIntent::Running);

        let (tx, rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
        *state.conflict_resolution_tx.lock().unwrap() = Some(tx);

        resolve_write_conflict(&id, ConflictResolution::Overwrite, true);

        let resp = rx.await.expect("sender should have delivered the response");
        assert_eq!(resp.resolution, ConflictResolution::Overwrite);
        assert!(resp.apply_to_all);

        uninstall_state(&id);
    }

    #[test]
    fn resolve_write_conflict_without_pending_sender_is_a_noop() {
        let id = unique_id("resolve-no-tx");
        let _state = install_state(&id, OperationIntent::Running);
        // No sender stashed; must not panic.
        resolve_write_conflict(&id, ConflictResolution::Skip, false);
        uninstall_state(&id);
    }

    // ---- register / update / unregister + list / get ----

    #[test]
    fn register_then_get_status_roundtrip() {
        let id = unique_id("reg-get");
        register_operation_status(&id, WriteOperationType::Copy);
        let _state = install_state(&id, OperationIntent::Running);

        let status = get_operation_status(&id).expect("operation should be in cache");
        assert_eq!(status.operation_id, id);
        assert_eq!(status.operation_type, WriteOperationType::Copy);
        assert_eq!(status.phase, WriteOperationPhase::Scanning);
        assert!(
            status.is_running,
            "is_running must reflect WRITE_OPERATION_STATE presence"
        );
        assert_eq!(status.files_done, 0);
        assert_eq!(status.files_total, 0);
        assert_eq!(status.bytes_done, 0);
        assert_eq!(status.bytes_total, 0);

        // is_running flips when WRITE_OPERATION_STATE entry is removed.
        uninstall_state(&id);
        let status = get_operation_status(&id).expect("status cache still has it");
        assert!(!status.is_running);

        unregister_operation_status(&id);
        assert!(get_operation_status(&id).is_none());
    }

    #[test]
    fn update_operation_status_overwrites_fields() {
        let id = unique_id("update");
        register_operation_status(&id, WriteOperationType::Move);
        update_operation_status(
            &id,
            WriteOperationPhase::Copying,
            Some("a.txt".into()),
            3,
            10,
            500,
            1000,
        );
        let status = get_operation_status(&id).unwrap();
        assert_eq!(status.phase, WriteOperationPhase::Copying);
        assert_eq!(status.current_file.as_deref(), Some("a.txt"));
        assert_eq!(status.files_done, 3);
        assert_eq!(status.files_total, 10);
        assert_eq!(status.bytes_done, 500);
        assert_eq!(status.bytes_total, 1000);
        unregister_operation_status(&id);
    }

    #[test]
    fn update_unknown_id_is_a_silent_noop() {
        // Pins the `&& get_mut` short-circuit. If `&&` becomes `||`, this would
        // dereference a None and panic.
        update_operation_status("no-such-op-xyzzy", WriteOperationPhase::Copying, None, 0, 0, 0, 0);
    }

    #[test]
    fn list_active_operations_percent_uses_bytes_when_available() {
        // bytes_total > 0 → percent comes from bytes axis, not files.
        let id = unique_id("list-bytes");
        register_operation_status(&id, WriteOperationType::Copy);
        update_operation_status(
            &id,
            WriteOperationPhase::Copying,
            None,
            1,    // files_done
            100,  // files_total (would give 1% if used)
            500,  // bytes_done
            1000, // bytes_total → 50%
        );
        let summary = list_active_operations()
            .into_iter()
            .find(|s| s.operation_id == id)
            .expect("operation present in summary");
        assert_eq!(
            summary.percent_complete, 50,
            "percent must be derived from bytes axis when bytes_total > 0"
        );
        unregister_operation_status(&id);
    }

    #[test]
    fn list_active_operations_percent_falls_back_to_files() {
        // bytes_total == 0, files_total > 0 → use files axis.
        let id = unique_id("list-files");
        register_operation_status(&id, WriteOperationType::Delete);
        update_operation_status(&id, WriteOperationPhase::Deleting, None, 3, 4, 0, 0);
        let summary = list_active_operations()
            .into_iter()
            .find(|s| s.operation_id == id)
            .unwrap();
        assert_eq!(summary.percent_complete, 75);
        unregister_operation_status(&id);
    }

    #[test]
    fn list_active_operations_percent_is_zero_when_nothing_known() {
        // Both totals == 0 → percent_complete == 0 (not the files-axis path).
        let id = unique_id("list-zero");
        register_operation_status(&id, WriteOperationType::Copy);
        let summary = list_active_operations()
            .into_iter()
            .find(|s| s.operation_id == id)
            .unwrap();
        assert_eq!(summary.percent_complete, 0);
        unregister_operation_status(&id);
    }

    #[test]
    fn list_active_operations_percent_clamps_to_100() {
        // Pin the `.min(100.0)` clamp. If bytes_done > bytes_total (which can
        // happen in flight due to over-counting), the UI must never see > 100.
        let id = unique_id("list-clamp");
        register_operation_status(&id, WriteOperationType::Copy);
        update_operation_status(&id, WriteOperationPhase::Copying, None, 0, 0, 1500, 1000);
        let summary = list_active_operations()
            .into_iter()
            .find(|s| s.operation_id == id)
            .unwrap();
        assert_eq!(summary.percent_complete, 100);
        unregister_operation_status(&id);
    }

    // ---- FileInfo derived sort keys ----

    fn make_file_info(path: &str, source_root: &str) -> FileInfo {
        FileInfo {
            path: PathBuf::from(path),
            source_root: PathBuf::from(source_root),
            size: 0,
            modified: 0,
            created: 0,
            is_symlink: false,
        }
    }

    #[test]
    fn extension_is_lowercased() {
        // Kills: replace extension → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/Photo.JPG", "/x").extension(), "jpg");
        assert_eq!(make_file_info("/x/archive.TAR.GZ", "/x").extension(), "gz");
    }

    #[test]
    fn extension_is_empty_for_no_extension() {
        assert_eq!(make_file_info("/x/README", "/x").extension(), "");
    }

    #[test]
    fn name_lower_is_lowercased_filename_only() {
        // Kills: replace name_lower → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/y/Foo.Bar", "/x").name_lower(), "foo.bar");
    }

    #[test]
    fn dest_path_preserves_relative_layout_under_destination_root() {
        // Kills: replace dest_path → Default::default().
        let info = make_file_info("/src/dir/sub/leaf.txt", "/src");
        assert_eq!(
            info.dest_path(Path::new("/dst")),
            PathBuf::from("/dst/dir/sub/leaf.txt")
        );
    }

    #[test]
    fn dest_path_falls_back_to_filename_when_prefix_does_not_match() {
        // The fallback branch: when strip_prefix fails, just place the file
        // by name at the destination root.
        let info = make_file_info("/elsewhere/file.bin", "/different-root");
        assert_eq!(info.dest_path(Path::new("/dst")), PathBuf::from("/dst/file.bin"));
    }

    // ---- CopyTransaction ----

    #[test]
    fn copy_transaction_rollback_deletes_files_and_dirs_in_reverse() {
        // Build a real on-disk transaction: nested dirs + a file, then roll
        // back. Both removals must happen. The rollback must walk dirs in
        // reverse-creation order so the leaf is removed before its parent.
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir(&outer).unwrap();
        std::fs::create_dir(&inner).unwrap();
        let file = inner.join("data.bin");
        std::fs::write(&file, b"hello").unwrap();

        let mut tx = CopyTransaction::new();
        tx.record_dir(outer.clone());
        tx.record_dir(inner.clone());
        tx.record_file(file.clone());

        tx.rollback();

        assert!(!file.exists(), "file must be removed on rollback");
        assert!(!inner.exists(), "inner dir must be removed (leaf-first)");
        assert!(!outer.exists(), "outer dir must be removed");
    }

    #[test]
    fn copy_transaction_commit_prevents_drop_rollback() {
        // Kills: replace CopyTransaction::commit with (), and the `!self.committed`
        // guard in Drop. After commit(), files must survive Drop.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("kept.txt");
        std::fs::write(&file, b"persist").unwrap();

        {
            let mut tx = CopyTransaction::new();
            tx.record_file(file.clone());
            tx.commit();
        } // Drop runs here.

        assert!(file.exists(), "commit() must prevent the Drop-based rollback");
    }

    #[test]
    fn copy_transaction_drop_rolls_back_when_not_committed() {
        // Kills: replace <impl Drop>::drop with (), and `delete !` in Drop.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("ephemeral.txt");
        std::fs::write(&file, b"will be gone").unwrap();

        {
            let mut tx = CopyTransaction::new();
            tx.record_file(file.clone());
            // No commit; Drop should roll back.
        }

        assert!(!file.exists(), "Drop-on-uncommitted must remove recorded files");
    }

    #[test]
    fn copy_transaction_record_methods_push_in_order() {
        // Kills: replace record_file/record_dir with ().
        let mut tx = CopyTransaction::new();
        tx.record_file(PathBuf::from("/a"));
        tx.record_file(PathBuf::from("/b"));
        tx.record_dir(PathBuf::from("/d1"));
        assert_eq!(tx.created_files, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(tx.created_dirs, vec![PathBuf::from("/d1")]);
        tx.commit(); // suppress Drop rollback (paths don't exist anyway, but be tidy)
    }
}
