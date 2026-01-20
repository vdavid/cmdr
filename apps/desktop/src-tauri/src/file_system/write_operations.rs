//! Write operations (copy, move, delete) with streaming progress.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.
//!
//! Safety features:
//! - macOS copyfile(3) for full metadata preservation (xattrs, ACLs, resource forks)
//! - Symlink preservation (not dereferenced)
//! - Symlink loop detection to prevent infinite recursion
//! - Copy rollback on failure (CopyTransaction)
//! - Atomic cross-filesystem moves using staging directory

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use super::macos_copy::{CopyProgressContext, copy_single_file_native, copy_symlink};

// ============================================================================
// Operation types
// ============================================================================

/// Type of write operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationType {
    Copy,
    Move,
    Delete,
}

/// Phase of the operation (for progress reporting).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationPhase {
    /// Scanning source files to calculate total size
    Scanning,
    /// Copying files (for copy and cross-filesystem move)
    Copying,
    /// Deleting files (for delete, and cleanup phase of cross-filesystem move)
    Deleting,
}

// ============================================================================
// Conflict resolution
// ============================================================================

/// How to handle conflicts when destination files already exist.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Stop operation on first conflict (default behavior)
    #[default]
    Stop,
    /// Skip conflicting files, continue with others
    Skip,
    /// Overwrite all conflicts
    Overwrite,
    /// Rename conflicting files (append " (1)", " (2)", etc.)
    Rename,
}

// ============================================================================
// Progress events
// ============================================================================

/// Progress event payload for write operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProgressEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// Current file being processed (filename only, not full path)
    pub current_file: Option<String>,
    /// Number of files processed
    pub files_done: usize,
    /// Total number of files
    pub files_total: usize,
    /// Bytes processed so far
    pub bytes_done: u64,
    /// Total bytes to process
    pub bytes_total: u64,
}

/// Completion event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCompleteEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    pub bytes_processed: u64,
}

/// Error event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteErrorEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub error: WriteOperationError,
}

/// Cancelled event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCancelledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Number of files processed before cancellation
    pub files_processed: usize,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    /// Whether destination is newer than source
    pub destination_is_newer: bool,
    /// Size difference (positive = destination is larger)
    pub size_difference: i64,
}

/// Progress event during scanning phase (emitted in dry-run mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Number of files found so far
    pub files_found: usize,
    /// Total bytes found so far
    pub bytes_found: u64,
    /// Number of conflicts detected so far
    pub conflicts_found: usize,
    /// Current path being scanned (for activity indication)
    pub current_path: Option<String>,
}

/// Detailed information about a single conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub source_path: String,
    pub destination_path: String,
    /// Source file size in bytes
    pub source_size: u64,
    /// Destination file size in bytes
    pub destination_size: u64,
    /// Source modification time (Unix timestamp in seconds)
    pub source_modified: Option<u64>,
    /// Destination modification time (Unix timestamp in seconds)
    pub destination_modified: Option<u64>,
    /// Whether destination is newer than source
    pub destination_is_newer: bool,
    /// Whether source is a directory
    pub is_directory: bool,
}

/// Result of a dry-run operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunResult {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Total number of files that would be processed
    pub files_total: usize,
    /// Total bytes that would be processed
    pub bytes_total: u64,
    /// Total number of conflicts detected
    pub conflicts_total: usize,
    /// Sampled conflicts (max 200 for large sets)
    pub conflicts: Vec<ConflictInfo>,
    /// Whether the conflicts list is a sample (true if conflicts_total > conflicts.len())
    pub conflicts_sampled: bool,
}

/// Maximum number of conflicts to include in DryRunResult
const MAX_CONFLICTS_IN_RESULT: usize = 200;

// ============================================================================
// Operation status (for query APIs)
// ============================================================================

/// Current status of an operation for query APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStatus {
    /// The operation ID
    pub operation_id: String,
    /// Type of operation
    pub operation_type: WriteOperationType,
    /// Current phase of the operation
    pub phase: WriteOperationPhase,
    /// Whether the operation is still running
    pub is_running: bool,
    /// Current file being processed (filename only)
    pub current_file: Option<String>,
    /// Number of files processed
    pub files_done: usize,
    /// Total number of files (0 if unknown/scanning)
    pub files_total: usize,
    /// Bytes processed so far
    pub bytes_done: u64,
    /// Total bytes to process (0 if unknown/scanning)
    pub bytes_total: u64,
    /// Operation start time (Unix timestamp in milliseconds)
    pub started_at: u64,
}

/// Summary of an active operation for list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummary {
    /// The operation ID
    pub operation_id: String,
    /// Type of operation
    pub operation_type: WriteOperationType,
    /// Current phase of the operation
    pub phase: WriteOperationPhase,
    /// Percentage complete (0-100)
    pub percent_complete: u8,
    /// Operation start time (Unix timestamp in milliseconds)
    pub started_at: u64,
}

// ============================================================================
// Error enum (following MountError pattern)
// ============================================================================

/// Errors that can occur during write operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WriteOperationError {
    /// Source path not found
    SourceNotFound { path: String },
    /// Destination already exists (and overwrite not enabled)
    DestinationExists { path: String },
    /// Permission denied
    PermissionDenied { path: String, message: String },
    /// Not enough space on destination
    InsufficientSpace {
        required: u64,
        available: u64,
        volume_name: Option<String>,
    },
    /// Cannot move/copy to same location
    SameLocation { path: String },
    /// Destination is inside source (would cause infinite recursion)
    DestinationInsideSource { source: String, destination: String },
    /// Symlink loop detected
    SymlinkLoop { path: String },
    /// Operation was cancelled
    Cancelled { message: String },
    /// Generic I/O error
    IoError { path: String, message: String },
}

impl WriteOperationError {
    /// Returns a user-friendly error message.
    #[allow(dead_code, reason = "Public API for future error display UI")]
    pub fn user_message(&self) -> String {
        match self {
            WriteOperationError::SourceNotFound { path } => {
                format!("Cannot find \"{}\". It may have been moved or deleted.", path)
            }
            WriteOperationError::DestinationExists { path } => {
                let filename = Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default();
                format!("\"{}\" already exists at the destination.", filename)
            }
            WriteOperationError::PermissionDenied { path, .. } => {
                format!(
                    "Cannot write to \"{}\": permission denied. Check folder permissions in Finder.",
                    path
                )
            }
            WriteOperationError::InsufficientSpace {
                required,
                available,
                volume_name,
            } => {
                let volume = volume_name.as_deref().unwrap_or("the destination");
                format!(
                    "Not enough space on {}. Need {}, but only {} available.",
                    volume,
                    format_bytes(*required),
                    format_bytes(*available)
                )
            }
            WriteOperationError::SameLocation { path } => {
                format!("\"{}\" is already in this location.", path)
            }
            WriteOperationError::DestinationInsideSource { source, .. } => {
                format!("Cannot copy \"{}\" into itself.", source)
            }
            WriteOperationError::SymlinkLoop { path } => {
                format!("Symlink loop detected at \"{}\". Cannot continue.", path)
            }
            WriteOperationError::Cancelled { .. } => "Operation was cancelled.".to_string(),
            WriteOperationError::IoError { path, message } => {
                if path.is_empty() {
                    format!("An error occurred: {}", message)
                } else {
                    format!("Error with \"{}\": {}", path, message)
                }
            }
        }
    }
}

/// Formats bytes in human-readable form.
#[allow(dead_code, reason = "Utility kept for future progress display formatting")]
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

impl From<std::io::Error> for WriteOperationError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path: err.to_string() },
            std::io::ErrorKind::PermissionDenied => WriteOperationError::PermissionDenied {
                path: String::new(),
                message: err.to_string(),
            },
            std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists { path: err.to_string() },
            _ => WriteOperationError::IoError {
                path: String::new(),
                message: err.to_string(),
            },
        }
    }
}

// ============================================================================
// Result types
// ============================================================================

/// Result of starting a write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOperationStartResult {
    /// Unique operation ID for tracking and cancellation
    pub operation_id: String,
    /// Type of operation started
    pub operation_type: WriteOperationType,
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for write operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOperationConfig {
    /// Progress update interval in milliseconds (default: 200)
    #[serde(default = "default_progress_interval")]
    pub progress_interval_ms: u64,
    /// Whether to overwrite existing files (deprecated, use conflict_resolution)
    #[serde(default)]
    pub overwrite: bool,
    /// How to handle conflicts
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,
    /// If true, only scan and detect conflicts without executing the operation.
    /// Returns a DryRunResult with totals and conflicts.
    #[serde(default)]
    pub dry_run: bool,
}

impl Default for WriteOperationConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: default_progress_interval(),
            overwrite: false,
            conflict_resolution: ConflictResolution::Stop,
            dry_run: false,
        }
    }
}

fn default_progress_interval() -> u64 {
    200
}

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// Cancellation flag
    pub cancelled: AtomicBool,
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
static WRITE_OPERATION_STATE: LazyLock<RwLock<HashMap<String, Arc<WriteOperationState>>>> =
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

/// Updates the internal status for an operation.
fn update_operation_status(
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
fn register_operation_status(operation_id: &str, operation_type: WriteOperationType) {
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
fn unregister_operation_status(operation_id: &str) {
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.remove(operation_id);
    }
}

// ============================================================================
// Copy transaction for rollback
// ============================================================================

/// Tracks created files/directories for rollback on failure.
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CopyTransaction {
    /// Files created during the operation (in creation order)
    pub(crate) created_files: Vec<PathBuf>,
    /// Directories created during the operation (in creation order)
    pub(crate) created_dirs: Vec<PathBuf>,
}

impl CopyTransaction {
    pub(crate) fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
        }
    }

    pub(crate) fn record_file(&mut self, path: PathBuf) {
        self.created_files.push(path);
    }

    pub(crate) fn record_dir(&mut self, path: PathBuf) {
        self.created_dirs.push(path);
    }

    /// Rolls back all created files and directories.
    pub(crate) fn rollback(&self) {
        // Delete files first (in reverse order)
        for file in self.created_files.iter().rev() {
            let _ = fs::remove_file(file);
        }
        // Then directories (deepest first, already in reverse due to creation order)
        for dir in self.created_dirs.iter().rev() {
            let _ = fs::remove_dir(dir);
        }
    }

    /// Clears the transaction (call on success to prevent rollback).
    pub(crate) fn commit(self) {
        // Just drop without calling rollback
        drop(self.created_files);
        drop(self.created_dirs);
    }
}

// ============================================================================
// Public API functions
// ============================================================================

/// Starts a copy operation in the background.
///
/// # Arguments
/// * `app` - Tauri app handle for event emission
/// * `sources` - List of source file/directory paths (absolute)
/// * `destination` - Destination directory path (absolute)
/// * `config` - Operation configuration
///
/// # Events emitted
/// * `write-progress` - Every progress_interval_ms with WriteProgressEvent
/// * `write-complete` - On success with WriteCompleteEvent
/// * `write-error` - On error with WriteErrorEvent
/// * `write-cancelled` - If cancelled with WriteCancelledEvent
/// * `write-conflict` - When Stop mode encounters a conflict
pub async fn copy_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate inputs
    validate_sources(&sources)?;
    validate_destination(&destination)?;
    validate_not_same_location(&sources, &destination)?;
    validate_destination_not_inside_source(&sources, &destination)?;

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Copy);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            copy_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &destination, &config)
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Copy,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Copy,
    })
}

/// Starts a move operation in the background.
///
/// Uses instant rename() for same-filesystem moves.
/// Uses atomic staging pattern for cross-filesystem moves.
pub async fn move_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate inputs
    validate_sources(&sources)?;
    validate_destination(&destination)?;
    validate_not_same_location(&sources, &destination)?;
    validate_destination_not_inside_source(&sources, &destination)?;

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Move);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            move_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &destination, &config)
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Move,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

/// Starts a delete operation in the background.
///
/// Recursively deletes files and directories.
pub async fn delete_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate inputs
    validate_sources(&sources)?;

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Delete);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            delete_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &config)
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Delete,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Delete,
    })
}

/// Cancels an in-progress write operation.
pub fn cancel_write_operation(operation_id: &str) {
    if let Ok(cache) = WRITE_OPERATION_STATE.read()
        && let Some(state) = cache.get(operation_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
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

// ============================================================================
// Async sync for durability
// ============================================================================

/// Spawns a background thread to call sync() for durability.
/// This ensures writes are flushed to disk without blocking the completion event.
fn spawn_async_sync() {
    std::thread::spawn(|| {
        // On Unix, call sync() to flush all filesystem buffers
        #[cfg(unix)]
        unsafe {
            libc::sync();
        }
        // On other platforms, this is a no-op (sync is not easily available)
    });
}

// ============================================================================
// Validation helpers
// ============================================================================

pub(crate) fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    for source in sources {
        // Use symlink_metadata to check existence without following symlinks
        if fs::symlink_metadata(source).is_err() {
            return Err(WriteOperationError::SourceNotFound {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination(destination: &Path) -> Result<(), WriteOperationError> {
    // Destination must exist and be a directory
    if !destination.exists() {
        return Err(WriteOperationError::SourceNotFound {
            path: destination.display().to_string(),
        });
    }
    if !destination.is_dir() {
        return Err(WriteOperationError::IoError {
            path: destination.display().to_string(),
            message: "Destination must be a directory".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_not_same_location(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
    for source in sources {
        if let Some(parent) = source.parent()
            && parent == destination
        {
            return Err(WriteOperationError::SameLocation {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination_not_inside_source(
    sources: &[PathBuf],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    for source in sources {
        if source.is_dir() && destination.starts_with(source) {
            return Err(WriteOperationError::DestinationInsideSource {
                source: source.display().to_string(),
                destination: destination.display().to_string(),
            });
        }
    }
    Ok(())
}

// ============================================================================
// Symlink loop detection
// ============================================================================

/// Checks if a path creates a symlink loop.
fn is_symlink_loop(path: &Path, visited: &HashSet<PathBuf>) -> bool {
    if let Ok(canonical) = path.canonicalize() {
        visited.contains(&canonical)
    } else {
        false
    }
}

// ============================================================================
// Filesystem detection
// ============================================================================

/// Checks if two paths are on the same filesystem using device IDs.
#[cfg(unix)]
pub(crate) fn is_same_filesystem(source: &Path, destination: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source_meta = fs::metadata(source)?;
    let dest_meta = fs::metadata(destination)?;

    Ok(source_meta.dev() == dest_meta.dev())
}

#[cfg(not(unix))]
pub(crate) fn is_same_filesystem(_source: &Path, _destination: &Path) -> std::io::Result<bool> {
    // On non-Unix, assume different filesystem to be safe (will use copy+delete)
    Ok(false)
}

// ============================================================================
// Scanning helpers
// ============================================================================

/// Information about files to be processed.
struct ScanResult {
    /// All files to process (in order: files first for copying, then dirs for deletion)
    files: Vec<PathBuf>,
    /// Directories to process (for deletion, in reverse order - deepest first)
    dirs: Vec<PathBuf>,
    /// Total file count (not including directories)
    file_count: usize,
    /// Total byte size of all files
    total_bytes: u64,
}

/// Scans source paths recursively, returns file list and totals.
fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    app: &tauri::AppHandle,
    operation_id: &str,
    operation_type: WriteOperationType,
) -> Result<ScanResult, WriteOperationError> {
    use tauri::Emitter;

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    for source in sources {
        scan_path_recursive(
            source,
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

    // Emit final scanning progress
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

#[allow(clippy::too_many_arguments, reason = "Recursive fn requires passing state through multiple levels")]
fn scan_path_recursive(
    path: &Path,
    files: &mut Vec<PathBuf>,
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
        files.push(path.to_path_buf());
    } else if metadata.is_file() {
        *total_bytes += metadata.len();
        files.push(path.to_path_buf());
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
    }

    // Emit progress periodically
    if last_progress_time.elapsed() >= *progress_interval {
        let current_file = path.file_name().map(|n| n.to_string_lossy().to_string());
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
struct DryRunScanResult {
    /// Total number of files
    file_count: usize,
    /// Total bytes
    total_bytes: u64,
    /// All detected conflicts
    conflicts: Vec<ConflictInfo>,
}

/// Performs a dry-run scan: scans sources, detects conflicts at destination.
/// Emits ScanProgressEvent during scanning with conflict counts.
#[allow(clippy::too_many_arguments, reason = "Recursive fn requires passing state through multiple levels")]
fn dry_run_scan(
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
#[allow(clippy::too_many_arguments, reason = "Recursive fn requires passing state through multiple levels")]
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

/// Calculates destination path for a source file relative to source root.
fn calculate_dest_path(path: &Path, source_root: &Path, dest_root: &Path) -> Result<PathBuf, WriteOperationError> {
    // If path is the source root itself, use the file name in dest_root
    if path == source_root {
        let file_name = path.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        return Ok(dest_root.join(file_name));
    }

    // Otherwise, strip the source root's parent and join with dest_root
    let source_parent = source_root.parent().unwrap_or(source_root);
    let relative = path
        .strip_prefix(source_parent)
        .map_err(|_| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Failed to calculate relative path".to_string(),
        })?;

    Ok(dest_root.join(relative))
}

/// Creates ConflictInfo for a source/destination pair.
fn create_conflict_info(
    source: &Path,
    dest: &Path,
    source_metadata: &fs::Metadata,
) -> Result<Option<ConflictInfo>, WriteOperationError> {
    let dest_metadata = match fs::symlink_metadata(dest) {
        Ok(m) => m,
        Err(_) => return Ok(None), // No conflict if dest doesn't exist
    };

    let source_modified = source_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let dest_modified = dest_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let destination_is_newer = match (source_modified, dest_modified) {
        (Some(s), Some(d)) => d > s,
        _ => false,
    };

    Ok(Some(ConflictInfo {
        source_path: source.display().to_string(),
        destination_path: dest.display().to_string(),
        source_size: source_metadata.len(),
        destination_size: dest_metadata.len(),
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        is_directory: source_metadata.is_dir(),
    }))
}

/// Samples conflicts if there are too many, using reservoir sampling.
fn sample_conflicts(conflicts: Vec<ConflictInfo>, max_count: usize) -> (Vec<ConflictInfo>, bool) {
    if conflicts.len() <= max_count {
        return (conflicts, false);
    }

    // Use reservoir sampling for uniform random selection
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut sampled: Vec<ConflictInfo> = conflicts.iter().take(max_count).cloned().collect();

    for (i, conflict) in conflicts.iter().enumerate().skip(max_count) {
        // Deterministic "random" based on path hash for reproducibility
        let mut hasher = DefaultHasher::new();
        conflict.source_path.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash = hasher.finish();
        let j = (hash as usize) % (i + 1);

        if j < max_count {
            sampled[j] = conflict.clone();
        }
    }

    (sampled, true)
}

// ============================================================================
// Conflict handling helpers
// ============================================================================

/// Resolves a file conflict based on the configured resolution mode.
/// Returns the resolved destination info, or None if the file should be skipped.
/// Also returns whether the resolution should be applied to all future conflicts.
#[allow(clippy::too_many_arguments, reason = "Recursive fn requires passing state through multiple levels")]
fn resolve_conflict(
    source: &Path,
    dest_path: &Path,
    config: &WriteOperationConfig,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    use tauri::Emitter;

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_resolution {
        // Use saved "apply to all" resolution
        *saved_resolution
    } else if config.overwrite {
        ConflictResolution::Overwrite
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Emit conflict event for frontend to handle
            let source_meta = fs::metadata(source).ok();
            let dest_meta = fs::metadata(dest_path).ok();

            let destination_is_newer = match (&source_meta, &dest_meta) {
                (Some(s), Some(d)) => {
                    let src_time = s.modified().ok();
                    let dst_time = d.modified().ok();
                    matches!((src_time, dst_time), (Some(src), Some(dst)) if dst > src)
                }
                _ => false,
            };

            let size_difference = match (&source_meta, &dest_meta) {
                (Some(s), Some(d)) => d.len() as i64 - s.len() as i64,
                _ => 0,
            };

            let _ = app.emit(
                "write-conflict",
                WriteConflictEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source.display().to_string(),
                    destination_path: dest_path.display().to_string(),
                    destination_is_newer,
                    size_difference,
                },
            );

            // Wait for user to call resolve_write_conflict
            let guard = state.conflict_mutex.lock().unwrap();
            let _guard = state
                .conflict_condvar
                .wait_while(guard, |_| {
                    // Keep waiting while:
                    // 1. No pending resolution
                    // 2. Not cancelled
                    let has_resolution = state.pending_resolution.read().map(|r| r.is_some()).unwrap_or(false);
                    let is_cancelled = state.cancelled.load(Ordering::Relaxed);
                    !has_resolution && !is_cancelled
                })
                .unwrap();

            // Check if cancelled
            if state.cancelled.load(Ordering::Relaxed) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            // Get the resolution
            let response = state.pending_resolution.write().ok().and_then(|mut r| r.take());

            if let Some(response) = response {
                // Save for future conflicts if apply_to_all
                if response.apply_to_all {
                    *apply_to_all_resolution = Some(response.resolution);
                }

                // Now apply the chosen resolution
                apply_resolution(response.resolution, dest_path)
            } else {
                // No resolution provided, treat as error
                Err(WriteOperationError::DestinationExists {
                    path: dest_path.display().to_string(),
                })
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => apply_resolution(ConflictResolution::Overwrite, dest_path),
        ConflictResolution::Rename => apply_resolution(ConflictResolution::Rename, dest_path),
    }
}

/// Result of applying a conflict resolution.
#[derive(Debug)]
struct ResolvedDestination {
    /// The path to write to
    path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    needs_safe_overwrite: bool,
}

/// Applies a specific conflict resolution to a destination path.
/// Returns None for Skip, or ResolvedDestination with path and overwrite flag.
fn apply_resolution(
    resolution: ConflictResolution,
    dest_path: &Path,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Don't delete here - the copy function will use safe overwrite pattern
            Ok(Some(ResolvedDestination {
                path: dest_path.to_path_buf(),
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name by appending " (1)", " (2)", etc.
            let unique_path = find_unique_name(dest_path);
            Ok(Some(ResolvedDestination {
                path: unique_path,
                needs_safe_overwrite: false,
            }))
        }
    }
}

/// Performs a safe overwrite using temp+rename pattern.
/// This ensures the original file is preserved if the copy fails.
///
/// Steps:
/// 1. Copy source to `dest.cmdr-tmp-{uuid}` (temp file in same directory)
/// 2. Rename original dest to `dest.cmdr-backup-{uuid}`
/// 3. Rename temp to final dest path
/// 4. Delete backup
///
/// If any step fails before step 3 completes, the original dest is intact.
fn safe_overwrite_file(
    source: &Path,
    dest: &Path,
    #[cfg(target_os = "macos")] context: Option<&CopyProgressContext>,
) -> Result<u64, WriteOperationError> {
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let temp_path = parent.join(format!("{}.cmdr-tmp-{}", file_name, uuid));
    let backup_path = parent.join(format!("{}.cmdr-backup-{}", file_name, uuid));

    // Step 1: Copy source to temp
    #[cfg(target_os = "macos")]
    let bytes = copy_single_file_native(source, &temp_path, false, context)?;
    #[cfg(not(target_os = "macos"))]
    let bytes = fs::copy(source, &temp_path).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: e.to_string(),
    })?;

    // Step 2: Rename original dest to backup
    if let Err(e) = fs::rename(dest, &backup_path) {
        // Failed to backup - clean up temp and return error
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to backup existing file: {}", e),
        });
    }

    // Step 3: Rename temp to final dest
    if let Err(e) = fs::rename(&temp_path, dest) {
        // Failed to rename - restore backup and clean up
        let _ = fs::rename(&backup_path, dest);
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to finalize overwrite: {}", e),
        });
    }

    // Step 4: Delete backup (non-critical, ignore errors)
    let _ = fs::remove_file(&backup_path);

    Ok(bytes)
}

/// Performs a safe overwrite for directories using temp+rename pattern.
fn safe_overwrite_dir(dest: &Path) -> Result<PathBuf, WriteOperationError> {
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let backup_path = parent.join(format!("{}.cmdr-backup-{}", file_name, uuid));

    // Rename original dest to backup
    fs::rename(dest, &backup_path).map_err(|e| WriteOperationError::IoError {
        path: dest.display().to_string(),
        message: format!("Failed to backup existing directory: {}", e),
    })?;

    // Return the backup path so caller can delete it after successful copy
    Ok(backup_path)
}

/// Finds a unique filename by appending " (1)", " (2)", etc.
fn find_unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
    }
}

// ============================================================================
// Copy implementation
// ============================================================================

fn copy_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Handle dry-run mode
    if config.dry_run {
        let scan_result = dry_run_scan(
            sources,
            destination,
            state,
            app,
            operation_id,
            WriteOperationType::Copy,
            state.progress_interval,
        )?;

        let conflicts_count = scan_result.conflicts.len();
        let (sampled_conflicts, conflicts_sampled) = sample_conflicts(scan_result.conflicts, MAX_CONFLICTS_IN_RESULT);

        let result = DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            files_total: scan_result.file_count,
            bytes_total: scan_result.total_bytes,
            conflicts_total: conflicts_count,
            conflicts: sampled_conflicts,
            conflicts_sampled,
        };

        let _ = app.emit("dry-run-complete", result);
        return Ok(());
    }

    // Phase 1: Scan
    let scan_result = scan_sources(sources, state, app, operation_id, WriteOperationType::Copy)?;

    // Phase 2: Copy files with rollback support
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    let result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            copy_path_recursive(
                source,
                destination,
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
            // Failure - rollback created files
            transaction.rollback();

            // Emit error
            let _ = app.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    error: e.clone(),
                },
            );
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "Recursive fn requires passing state through multiple levels")]
fn copy_path_recursive(
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

    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        let _ = app.emit(
            "write-cancelled",
            WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                files_processed: *files_done,
            },
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

        // Check cancellation before copy
        if state.cancelled.load(Ordering::Relaxed) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Copy file using platform-specific method
        let bytes = if needs_safe_overwrite {
            // Use safe overwrite pattern (temp + rename)
            #[cfg(target_os = "macos")]
            {
                let context = CopyProgressContext {
                    cancelled: Arc::new(AtomicBool::new(false)),
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
                    cancelled: Arc::new(AtomicBool::new(false)),
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
    }

    Ok(())
}

// ============================================================================
// Move implementation
// ============================================================================

fn move_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Handle dry-run mode
    if config.dry_run {
        let scan_result = dry_run_scan(
            sources,
            destination,
            state,
            app,
            operation_id,
            WriteOperationType::Move,
            state.progress_interval,
        )?;

        let conflicts_count = scan_result.conflicts.len();
        let (sampled_conflicts, conflicts_sampled) = sample_conflicts(scan_result.conflicts, MAX_CONFLICTS_IN_RESULT);

        let result = DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_total: scan_result.file_count,
            bytes_total: scan_result.total_bytes,
            conflicts_total: conflicts_count,
            conflicts: sampled_conflicts,
            conflicts_sampled,
        };

        let _ = app.emit("dry-run-complete", result);
        return Ok(());
    }

    // Check if all sources are on the same filesystem as destination
    let same_fs = sources
        .iter()
        .all(|s| is_same_filesystem(s, destination).unwrap_or(false));

    if same_fs {
        // Use instant rename for each source
        move_with_rename(app, operation_id, state, sources, destination, config)
    } else {
        // Use atomic staging pattern for cross-filesystem move
        move_with_staging(app, operation_id, state, sources, destination, config)
    }
}

fn move_with_rename(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    let mut files_done = 0;
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    for source in sources {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_done,
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        let dest_path = destination.join(file_name);

        // Handle conflicts
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                &mut apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file
                    continue;
                }
            }
        } else {
            (dest_path, false)
        };

        // For same-FS move with overwrite:
        // - For files: rename() atomically replaces the destination
        // - For directories: need to remove dest first (rename fails on non-empty dirs)
        if needs_safe_overwrite && actual_dest.is_dir() {
            // Safe directory overwrite: backup, then rename
            let backup_path = safe_overwrite_dir(&actual_dest)?;
            if let Err(e) = fs::rename(source, &actual_dest) {
                // Restore backup on failure
                let _ = fs::rename(&backup_path, &actual_dest);
                return Err(WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                });
            }
            // Remove backup
            let _ = fs::remove_dir_all(&backup_path);
        } else {
            // For files or non-overwrite: rename() handles it (atomic for files)
            fs::rename(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: e.to_string(),
            })?;
        }

        files_done += 1;
    }

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion (instant, no progress needed)
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            bytes_processed: 0, // Rename doesn't track bytes
        },
    );

    Ok(())
}

/// Performs cross-filesystem move using atomic staging pattern.
/// This ensures source files remain intact if the operation fails.
fn move_with_staging(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan
    let scan_result = scan_sources(sources, state, app, operation_id, WriteOperationType::Move)?;

    // Create staging directory
    let staging_dir = destination.join(format!(".cmdr-staging-{}", operation_id));
    fs::create_dir(&staging_dir).map_err(|e| WriteOperationError::IoError {
        path: staging_dir.display().to_string(),
        message: format!("Failed to create staging directory: {}", e),
    })?;

    // Phase 2: Copy to staging directory
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    let copy_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            copy_path_recursive(
                source,
                &staging_dir,
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
            )?;
        }
        Ok(())
    })();

    if let Err(e) = copy_result {
        // Cleanup staging directory on failure
        let _ = fs::remove_dir_all(&staging_dir);
        let _ = app.emit(
            "write-error",
            WriteErrorEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                error: e.clone(),
            },
        );
        return Err(e);
    }

    // Phase 3: Atomic rename from staging to final destination
    let rename_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".to_string(),
            })?;

            let staged_path = staging_dir.join(file_name);
            let final_path = destination.join(file_name);

            // Handle conflicts at final destination
            let (actual_dest, needs_safe_overwrite) = if final_path.exists() {
                match resolve_conflict(
                    source,
                    &final_path,
                    config,
                    app,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                    None => {
                        // Skip - remove from staging
                        if staged_path.is_dir() {
                            let _ = fs::remove_dir_all(&staged_path);
                        } else {
                            let _ = fs::remove_file(&staged_path);
                        }
                        continue;
                    }
                }
            } else {
                (final_path, false)
            };

            // Rename from staging to final (atomic on same filesystem)
            // For overwrite of directories, need to backup first
            if needs_safe_overwrite && actual_dest.is_dir() {
                let backup_path = safe_overwrite_dir(&actual_dest)?;
                if let Err(e) = fs::rename(&staged_path, &actual_dest) {
                    // Restore backup on failure
                    let _ = fs::rename(&backup_path, &actual_dest);
                    return Err(WriteOperationError::IoError {
                        path: staged_path.display().to_string(),
                        message: format!("Failed to move from staging: {}", e),
                    });
                }
                let _ = fs::remove_dir_all(&backup_path);
            } else {
                // For files: rename atomically replaces
                fs::rename(&staged_path, &actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: staged_path.display().to_string(),
                    message: format!("Failed to move from staging: {}", e),
                })?;
            }
        }
        Ok(())
    })();

    if let Err(e) = rename_result {
        // Cleanup staging directory on failure
        let _ = fs::remove_dir_all(&staging_dir);
        let _ = app.emit(
            "write-error",
            WriteErrorEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                error: e.clone(),
            },
        );
        return Err(e);
    }

    // Phase 4: Delete source files (only after successful copy+rename)
    delete_sources_after_move(app, operation_id, state, sources, files_done)?;

    // Phase 5: Remove empty staging directory
    let _ = fs::remove_dir(&staging_dir);

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            bytes_processed: bytes_done,
        },
    );

    Ok(())
}

fn delete_sources_after_move(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    for source in sources {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_done,
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use symlink_metadata to check if it still exists
        if fs::symlink_metadata(source).is_ok() {
            if source.is_dir() {
                fs::remove_dir_all(source).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?;
            } else {
                fs::remove_file(source).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?;
            }
        }
    }

    Ok(())
}

// ============================================================================
// Delete implementation
// ============================================================================

fn delete_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan to get file count
    let scan_result = scan_sources(sources, state, app, operation_id, WriteOperationType::Delete)?;

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
    for file in &scan_result.files {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use symlink_metadata for accurate size (don't follow symlinks)
        let file_size = fs::symlink_metadata(file).map(|m| m.len()).unwrap_or(0);

        fs::remove_file(file).map_err(|e| WriteOperationError::IoError {
            path: file.display().to_string(),
            message: e.to_string(),
        })?;

        files_done += 1;
        bytes_done += file_size;

        // Emit progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = file.file_name().map(|n| n.to_string_lossy().to_string());
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
