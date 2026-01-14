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
use super::macos_copy::{copy_single_file_native, copy_symlink, CopyProgressContext};

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
    #[allow(dead_code)]
    pub fn user_message(&self) -> String {
        match self {
            WriteOperationError::SourceNotFound { path } => {
                format!("Cannot find \"{}\". It may have been moved or deleted.", path)
            }
            WriteOperationError::DestinationExists { path } => {
                let filename = Path::new(path).file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
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
#[allow(dead_code)]
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
            std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound {
                path: err.to_string(),
            },
            std::io::ErrorKind::PermissionDenied => WriteOperationError::PermissionDenied {
                path: String::new(),
                message: err.to_string(),
            },
            std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists {
                path: err.to_string(),
            },
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
}

impl Default for WriteOperationConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: default_progress_interval(),
            overwrite: false,
            conflict_resolution: ConflictResolution::Stop,
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

// ============================================================================
// Copy transaction for rollback
// ============================================================================

/// Tracks created files/directories for rollback on failure.
struct CopyTransaction {
    /// Files created during the operation (in creation order)
    created_files: Vec<PathBuf>,
    /// Directories created during the operation (in creation order)
    created_dirs: Vec<PathBuf>,
}

impl CopyTransaction {
    fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
        }
    }

    fn record_file(&mut self, path: PathBuf) {
        self.created_files.push(path);
    }

    fn record_dir(&mut self, path: PathBuf) {
        self.created_dirs.push(path);
    }

    /// Rolls back all created files and directories.
    fn rollback(&self) {
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
    fn commit(self) {
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
            *pending = Some(ConflictResolutionResponse { resolution, apply_to_all });
        }
        // Wake up the waiting operation
        let _guard = state.conflict_mutex.lock();
        state.conflict_condvar.notify_all();
    }
}

// ============================================================================
// Validation helpers
// ============================================================================

fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
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

fn validate_destination(destination: &Path) -> Result<(), WriteOperationError> {
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

fn validate_not_same_location(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
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

fn validate_destination_not_inside_source(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
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
fn is_same_filesystem(source: &Path, destination: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source_meta = fs::metadata(source)?;
    let dest_meta = fs::metadata(destination)?;

    Ok(source_meta.dev() == dest_meta.dev())
}

#[cfg(not(unix))]
fn is_same_filesystem(_source: &Path, _destination: &Path) -> std::io::Result<bool> {
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

#[allow(clippy::too_many_arguments)]
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
        let _ = app.emit(
            "write-progress",
            WriteProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type,
                phase: WriteOperationPhase::Scanning,
                current_file: path.file_name().map(|n| n.to_string_lossy().to_string()),
                files_done: files.len(),
                files_total: 0, // Unknown during scanning
                bytes_done: *total_bytes,
                bytes_total: 0, // Unknown during scanning
            },
        );
        *last_progress_time = Instant::now();
    }

    Ok(())
}

// ============================================================================
// Conflict handling helpers
// ============================================================================

/// Resolves a file conflict based on the configured resolution mode.
/// Returns the actual destination path to use, or None if the file should be skipped.
/// Also returns whether the resolution should be applied to all future conflicts.
#[allow(clippy::too_many_arguments)]
fn resolve_conflict(
    source: &Path,
    dest_path: &Path,
    config: &WriteOperationConfig,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<Option<PathBuf>, WriteOperationError> {
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

/// Applies a specific conflict resolution to a destination path.
fn apply_resolution(resolution: ConflictResolution, dest_path: &Path) -> Result<Option<PathBuf>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Remove existing file/dir first
            if dest_path.is_dir() {
                fs::remove_dir_all(dest_path).map_err(|e| WriteOperationError::IoError {
                    path: dest_path.display().to_string(),
                    message: e.to_string(),
                })?;
            } else {
                fs::remove_file(dest_path).map_err(|e| WriteOperationError::IoError {
                    path: dest_path.display().to_string(),
                    message: e.to_string(),
                })?;
            }
            Ok(Some(dest_path.to_path_buf()))
        }
        ConflictResolution::Rename => {
            // Find a unique name by appending " (1)", " (2)", etc.
            let unique_path = find_unique_name(dest_path);
            Ok(Some(unique_path))
        }
    }
}

/// Finds a unique filename by appending " (1)", " (2)", etc.
fn find_unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
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

#[allow(clippy::too_many_arguments)]
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
        let actual_dest = if dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok() {
            match resolve_conflict(source, &dest_path, config, app, operation_id, state, apply_to_all_resolution)? {
                Some(path) => path,
                None => {
                    // Skip this file
                    return Ok(());
                }
            }
        } else {
            dest_path.clone()
        };

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
        let actual_dest = if dest_path.exists() {
            match resolve_conflict(source, &dest_path, config, app, operation_id, state, apply_to_all_resolution)? {
                Some(path) => path,
                None => {
                    // Skip this file
                    return Ok(());
                }
            }
        } else {
            dest_path.clone()
        };

        // Copy file using platform-specific method
        #[cfg(target_os = "macos")]
        let bytes = {
            let context = CopyProgressContext {
                cancelled: Arc::new(AtomicBool::new(false)),
                ..Default::default()
            };
            // Check cancellation before copy
            if state.cancelled.load(Ordering::Relaxed) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }
            copy_single_file_native(source, &actual_dest, actual_dest != dest_path, Some(&context))?
        };

        #[cfg(not(target_os = "macos"))]
        let bytes = fs::copy(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: e.to_string(),
        })?;

        transaction.record_file(actual_dest);
        *files_done += 1;
        *bytes_done += bytes;

        // Emit progress
        if last_progress_time.elapsed() >= *progress_interval {
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    phase: WriteOperationPhase::Copying,
                    current_file: Some(file_name.to_string_lossy().to_string()),
                    files_done: *files_done,
                    files_total,
                    bytes_done: *bytes_done,
                    bytes_total,
                },
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
    // Check if all sources are on the same filesystem as destination
    let same_fs = sources.iter().all(|s| is_same_filesystem(s, destination).unwrap_or(false));

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
        let actual_dest = if dest_path.exists() {
            match resolve_conflict(source, &dest_path, config, app, operation_id, state, &mut apply_to_all_resolution)? {
                Some(path) => path,
                None => {
                    // Skip this file
                    continue;
                }
            }
        } else {
            dest_path
        };

        // Use rename (instant on same filesystem)
        fs::rename(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: e.to_string(),
        })?;

        files_done += 1;
    }

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
            let actual_dest = if final_path.exists() {
                match resolve_conflict(source, &final_path, config, app, operation_id, state, &mut apply_to_all_resolution)? {
                    Some(path) => path,
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
                final_path
            };

            // Rename from staging to final (atomic on same filesystem)
            fs::rename(&staged_path, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: staged_path.display().to_string(),
                message: format!("Failed to move from staging: {}", e),
            })?;
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
    _config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan to get file count
    let scan_result = scan_sources(sources, state, app, operation_id, WriteOperationType::Delete)?;

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
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    phase: WriteOperationPhase::Deleting,
                    current_file: file.file_name().map(|n| n.to_string_lossy().to_string()),
                    files_done,
                    files_total: scan_result.file_count,
                    bytes_done,
                    bytes_total: scan_result.total_bytes,
                },
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
