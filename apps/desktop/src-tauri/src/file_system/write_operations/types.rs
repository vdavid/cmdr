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
    /// Whether partial files were rolled back (deleted)
    pub rolled_back: bool,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    /// Source file size in bytes
    pub source_size: u64,
    /// Destination file size in bytes
    pub destination_size: u64,
    /// Source modification time (Unix timestamp in seconds), if available
    pub source_modified: Option<i64>,
    /// Destination modification time (Unix timestamp in seconds), if available
    pub destination_modified: Option<i64>,
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
    /// Column to sort files by during copy (default: name)
    #[serde(default)]
    pub sort_column: SortColumn,
    /// Sort order (default: ascending)
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
    /// Number of files found so far
    pub files_found: usize,
    /// Number of directories found so far
    pub dirs_found: usize,
    /// Total bytes found so far
    pub bytes_found: u64,
    /// Current path being scanned (for activity indication)
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
    /// Progress update interval in milliseconds.
    pub progress_interval_ms: u64,
    /// How to handle conflicts (skip, overwrite, stop).
    pub conflict_resolution: ConflictResolution,
    /// Maximum number of conflicts to return in pre-flight scan.
    pub max_conflicts_to_show: usize,
}

impl Default for VolumeCopyConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: 200,
            conflict_resolution: ConflictResolution::Stop,
            max_conflicts_to_show: 100,
        }
    }
}

impl From<&WriteOperationConfig> for VolumeCopyConfig {
    fn from(config: &WriteOperationConfig) -> Self {
        Self {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
        }
    }
}

/// Result of a pre-flight scan for volume copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCopyScanResult {
    /// Total number of files to copy.
    pub file_count: usize,
    /// Total number of directories to create.
    pub dir_count: usize,
    /// Total bytes to copy.
    pub total_bytes: u64,
    /// Available space on destination.
    pub dest_space: SpaceInfo,
    /// Detected conflicts at destination.
    pub conflicts: Vec<ScanConflict>,
}
