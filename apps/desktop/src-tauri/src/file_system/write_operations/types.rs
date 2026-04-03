//! Type definitions for write operations.
//!
//! Contains enums, event structs, error types, and configuration.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::file_system::volume::{ScanConflict, SpaceInfo};

// Re-export sort types from sorting module
pub use crate::file_system::listing::{SortColumn, SortOrder};

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
    Trash,
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
    /// Moving items to trash
    Trashing,
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
    /// Filename only, not full path.
    pub current_file: Option<String>,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
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

/// Emitted when all files belonging to a top-level source item have been processed.
/// Used by the frontend for gradual deselection during operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteSourceItemDoneEvent {
    pub operation_id: String,
    pub source_path: String,
}

/// Cancelled event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCancelledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    /// Whether partial files were rolled back (deleted).
    pub rolled_back: bool,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    /// In bytes.
    pub source_size: u64,
    /// In bytes.
    pub destination_size: u64,
    /// Unix timestamp in seconds.
    pub source_modified: Option<i64>,
    /// Unix timestamp in seconds.
    pub destination_modified: Option<i64>,
    pub destination_is_newer: bool,
    /// Positive = destination is larger.
    pub size_difference: i64,
}

/// Progress event during scanning phase (emitted in dry-run mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_found: usize,
    pub bytes_found: u64,
    pub conflicts_found: usize,
    /// For activity indication.
    pub current_path: Option<String>,
}

/// Detailed information about a single conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub source_path: String,
    pub destination_path: String,
    /// In bytes.
    pub source_size: u64,
    /// In bytes.
    pub destination_size: u64,
    /// Unix timestamp in seconds.
    pub source_modified: Option<u64>,
    /// Unix timestamp in seconds.
    pub destination_modified: Option<u64>,
    pub destination_is_newer: bool,
    pub is_directory: bool,
}

/// Result of a dry-run operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunResult {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_total: usize,
    pub bytes_total: u64,
    pub conflicts_total: usize,
    /// Sampled subset (max 200 for large sets).
    pub conflicts: Vec<ConflictInfo>,
    /// True if `conflicts` is a sample (`conflicts_total > conflicts.len()`).
    pub conflicts_sampled: bool,
}

/// Legacy constant, kept for backward compatibility.
/// The actual value is now configurable via WriteOperationConfig.max_conflicts_to_show.
#[allow(dead_code, reason = "Kept for backward compatibility")]
pub const MAX_CONFLICTS_IN_RESULT: usize = 200;

// ============================================================================
// Operation status (for query APIs)
// ============================================================================

/// Current status of an operation for query APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    pub is_running: bool,
    /// Filename only.
    pub current_file: Option<String>,
    pub files_done: usize,
    /// 0 if unknown/scanning.
    pub files_total: usize,
    pub bytes_done: u64,
    /// 0 if unknown/scanning.
    pub bytes_total: u64,
    /// Unix timestamp in milliseconds.
    pub started_at: u64,
}

/// Summary of an active operation for list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummary {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// 0-100.
    pub percent_complete: u8,
    /// Unix timestamp in milliseconds.
    pub started_at: u64,
}

// ============================================================================
// Error enum (following MountError pattern)
// ============================================================================

/// Errors that can occur during write operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WriteOperationError {
    SourceNotFound {
        path: String,
    },
    /// Overwrite not enabled.
    DestinationExists {
        path: String,
    },
    PermissionDenied {
        path: String,
        message: String,
    },
    InsufficientSpace {
        required: u64,
        available: u64,
        volume_name: Option<String>,
    },
    SameLocation {
        path: String,
    },
    /// Would cause infinite recursion.
    DestinationInsideSource {
        source: String,
        destination: String,
    },
    SymlinkLoop {
        path: String,
    },
    Cancelled {
        message: String,
    },
    /// Device was disconnected during the operation (USB, MTP, etc.).
    DeviceDisconnected {
        path: String,
    },
    /// Target device or volume is read-only.
    ReadOnlyDevice {
        path: String,
        device_name: Option<String>,
    },
    /// File is locked (macOS immutable flag, "Operation not permitted" on delete).
    FileLocked {
        path: String,
    },
    /// Volume doesn't support trash (network mounts, FAT, etc.).
    TrashNotSupported {
        path: String,
    },
    /// Network connection was interrupted or timed out.
    ConnectionInterrupted {
        path: String,
    },
    /// Couldn't read from the source.
    ReadError {
        path: String,
        message: String,
    },
    /// Couldn't write to the destination.
    WriteError {
        path: String,
        message: String,
    },
    /// File name exceeds the destination filesystem's length limit.
    NameTooLong {
        path: String,
    },
    /// File name contains characters not allowed at the destination.
    InvalidName {
        path: String,
        message: String,
    },
    /// Catch-all for genuinely unexpected IO errors.
    IoError {
        path: String,
        message: String,
    },
}

/// Classifies a raw `std::io::Error` into a specific `WriteOperationError` variant based on its
/// errno code, `ErrorKind`, and message content. Used by both `IoResultExt::with_path` and the
/// `From` impl.
fn classify_io_error(e: &std::io::Error, path: String) -> WriteOperationError {
    // Prefer errno when available (local FS operations always have one)
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        match code {
            libc::EROFS => {
                return WriteOperationError::ReadOnlyDevice {
                    path,
                    device_name: None,
                }
            }
            libc::ENAMETOOLONG => return WriteOperationError::NameTooLong { path },
            libc::ENOTCONN | libc::ENETDOWN | libc::ENETUNREACH | libc::EHOSTUNREACH
            | libc::ETIMEDOUT => return WriteOperationError::ConnectionInterrupted { path },
            libc::ENODEV => return WriteOperationError::DeviceDisconnected { path },
            _ => {} // Fall through to ErrorKind/message-based classification
        }
    }

    let msg = e.to_string();
    let lower = msg.to_lowercase();

    // Message-based heuristics as fallback for errors without raw OS codes
    // (synthetic/wrapped errors from libraries)
    if lower.contains("disconnect") || lower.contains("no such device") {
        return WriteOperationError::DeviceDisconnected { path };
    }
    if lower.contains("read-only") || lower.contains("read only") {
        return WriteOperationError::ReadOnlyDevice {
            path,
            device_name: None,
        };
    }
    if lower.contains("connection") || lower.contains("timed out") || lower.contains("timeout") {
        return WriteOperationError::ConnectionInterrupted { path };
    }
    if lower.contains("name too long") || lower.contains("file name too long") {
        return WriteOperationError::NameTooLong { path };
    }
    if lower.contains("invalid") && lower.contains("name") {
        return WriteOperationError::InvalidName {
            path,
            message: msg,
        };
    }

    // ErrorKind-based fallback, with one kind-specific heuristic
    match e.kind() {
        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path },
        std::io::ErrorKind::PermissionDenied => {
            // macOS immutable flag manifests as PermissionDenied + "operation not permitted"
            if lower.contains("immutable") || lower.contains("operation not permitted") {
                return WriteOperationError::FileLocked { path };
            }
            WriteOperationError::PermissionDenied {
                path,
                message: msg,
            }
        }
        std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists { path },
        _ => WriteOperationError::IoError {
            path,
            message: msg,
        },
    }
}

/// Extension trait for converting `io::Result` to `Result<T, WriteOperationError>` with path context.
pub(super) trait IoResultExt<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError> {
        self.map_err(|e| classify_io_error(&e, path.display().to_string()))
    }
}

impl From<std::io::Error> for WriteOperationError {
    fn from(err: std::io::Error) -> Self {
        classify_io_error(&err, String::new())
    }
}

// ============================================================================
// Result types
// ============================================================================

/// Result of starting a write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOperationStartResult {
    pub operation_id: String,
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
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,
    /// If true, only scan and detect conflicts without executing the operation.
    /// Returns a DryRunResult with totals and conflicts.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub sort_column: SortColumn,
    #[serde(default)]
    pub sort_order: SortOrder,
    /// Preview scan ID to reuse cached scan results (from start_scan_preview)
    #[serde(default)]
    pub preview_id: Option<String>,
    /// Maximum number of conflicts to include in DryRunResult (default: 100)
    #[serde(default = "default_max_conflicts_to_show")]
    pub max_conflicts_to_show: usize,
}

impl Default for WriteOperationConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: default_progress_interval(),
            overwrite: false,
            conflict_resolution: ConflictResolution::Stop,
            dry_run: false,
            sort_column: SortColumn::default(),
            sort_order: SortOrder::default(),
            preview_id: None,
            max_conflicts_to_show: default_max_conflicts_to_show(),
        }
    }
}

fn default_progress_interval() -> u64 {
    200
}

fn default_max_conflicts_to_show() -> usize {
    100
}

// ============================================================================
// Scan preview events
// ============================================================================

/// Progress event for scan preview (shown in Copy dialog).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewProgressEvent {
    pub preview_id: String,
    pub files_found: usize,
    pub dirs_found: usize,
    pub bytes_found: u64,
    /// For activity indication.
    pub current_path: Option<String>,
}

/// Completion event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewCompleteEvent {
    pub preview_id: String,
    pub files_total: usize,
    pub dirs_total: usize,
    pub bytes_total: u64,
}

/// Error event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewErrorEvent {
    pub preview_id: String,
    pub message: String,
}

/// Cancelled event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewCancelledEvent {
    pub preview_id: String,
}

/// Result of starting a scan preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewStartResult {
    pub preview_id: String,
}

// ============================================================================
// Volume copy types
// ============================================================================

/// Copy operation configuration for volume-to-volume copy.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCopyConfig {
    /// In milliseconds.
    pub progress_interval_ms: u64,
    pub conflict_resolution: ConflictResolution,
    /// Maximum returned in pre-flight scan.
    pub max_conflicts_to_show: usize,
    /// Preview scan ID to reuse cached scan results (from start_scan_preview).
    #[serde(default)]
    pub preview_id: Option<String>,
}

impl Default for VolumeCopyConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: 200,
            conflict_resolution: ConflictResolution::Stop,
            max_conflicts_to_show: 100,
            preview_id: None,
        }
    }
}

impl From<&WriteOperationConfig> for VolumeCopyConfig {
    fn from(config: &WriteOperationConfig) -> Self {
        Self {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id.clone(),
        }
    }
}

/// Result of a pre-flight scan for volume copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCopyScanResult {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
    pub dest_space: SpaceInfo,
    pub conflicts: Vec<ScanConflict>,
}
