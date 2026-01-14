//! Write operations (copy, move, delete) with streaming progress.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

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
    InsufficientSpace { required: u64, available: u64 },
    /// Cannot move/copy to same location
    SameLocation { path: String },
    /// Destination is inside source (would cause infinite recursion)
    DestinationInsideSource { source: String, destination: String },
    /// Operation was cancelled
    Cancelled { message: String },
    /// Generic I/O error
    IoError { path: String, message: String },
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
    /// Whether to overwrite existing files
    #[serde(default)]
    pub overwrite: bool,
}

impl Default for WriteOperationConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: default_progress_interval(),
            overwrite: false,
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
}

/// Global cache for in-progress write operation states.
static WRITE_OPERATION_STATE: LazyLock<RwLock<HashMap<String, Arc<WriteOperationState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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
/// Falls back to copy+delete for cross-filesystem moves.
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
        && let Some(state) = cache.get(operation_id) {
            state.cancelled.store(true, Ordering::Relaxed);
        }
}

// ============================================================================
// Validation helpers
// ============================================================================

fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    for source in sources {
        if !source.exists() {
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
            && parent == destination {
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
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    if path.is_file() || path.is_symlink() {
        if let Ok(meta) = fs::metadata(path) {
            *total_bytes += meta.len();
        }
        files.push(path.to_path_buf());
    } else if path.is_dir() {
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

    // Phase 2: Copy files
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

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
            config.overwrite,
        )?;
    }

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
    overwrite: bool,
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

    if source.is_file() || source.is_symlink() {
        // Check if destination exists
        if dest_path.exists() && !overwrite {
            return Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            });
        }

        // Copy file (std::fs::copy uses clonefile on APFS automatically)
        let bytes = fs::copy(source, &dest_path).map_err(|e| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: e.to_string(),
        })?;

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
    } else if source.is_dir() {
        // Create destination directory
        if !dest_path.exists() {
            fs::create_dir(&dest_path).map_err(|e| WriteOperationError::IoError {
                path: dest_path.display().to_string(),
                message: e.to_string(),
            })?;
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
                overwrite,
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

    // Check if all sources are on the same filesystem as destination
    let same_fs = sources
        .iter()
        .all(|s| is_same_filesystem(s, destination).unwrap_or(false));

    if same_fs {
        // Use instant rename for each source
        move_with_rename(app, operation_id, state, sources, destination, config)
    } else {
        // Fall back to copy + delete
        // Phase 1: Scan
        let scan_result = scan_sources(sources, state, app, operation_id, WriteOperationType::Move)?;

        // Phase 2: Copy
        let mut files_done = 0;
        let mut bytes_done = 0u64;
        let mut last_progress_time = Instant::now();

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
                config.overwrite,
            )?;
        }

        // Phase 3: Delete sources
        delete_sources_after_move(app, operation_id, state, sources, files_done)?;

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

        // Check if destination exists
        if dest_path.exists() && !config.overwrite {
            return Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            });
        }

        // Use rename (instant on same filesystem)
        fs::rename(source, &dest_path).map_err(|e| WriteOperationError::IoError {
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

        let file_size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);

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
