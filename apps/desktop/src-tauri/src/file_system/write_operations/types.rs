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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationType {
    Copy,
    Move,
    Delete,
    Trash,
}

/// Phase of the operation (for progress reporting).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
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
    /// Rolling back: deleting files created during a cancelled copy/move
    RollingBack,
}

// ============================================================================
// Conflict resolution
// ============================================================================

/// How to handle conflicts when destination files already exist.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, specta::Type)]
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
    /// Overwrite only when the destination is strictly smaller than the source.
    /// All other conflicts (equal or larger destination, or unknown sizes) are skipped.
    OverwriteSmaller,
    /// Overwrite only when the destination is strictly older than the source.
    /// All other conflicts (equal or newer destination, or unknown timestamps) are skipped.
    OverwriteOlder,
}

// ============================================================================
// Progress events
// ============================================================================

/// Progress event payload for write operations.
///
/// `bytes_per_second`, `files_per_second`, and `eta_seconds` are populated by
/// `eta::EtaEstimator` from `enrich_progress_event`. They're optional because
/// the estimator returns `None` for both rates and ETA during the warm-up
/// window (first ~800 ms of a phase or before the second sample lands).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProgressEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// Filename only, not full path.
    pub current_file: Option<String>,
    /// Absolute parent directory currently being scanned (Scanning phase only).
    /// Lets the UI show "in directory: …" alongside the filename so users
    /// get a sense of where in the tree the walker is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_dir: Option<String>,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Directories discovered so far (Scanning phase only; 0 outside scanning).
    /// `WriteProgressEvent` already carries `files_done`; some UIs want to show
    /// the dir count separately while the walker is mid-tree. Populated by
    /// `with_scan_meta`.
    #[serde(default)]
    pub dirs_done: usize,
    /// Smoothed bytes/second toward the phase target. `None` during warm-up.
    #[serde(default)]
    pub bytes_per_second: Option<u64>,
    /// Smoothed files/second toward the phase target. `None` during warm-up.
    #[serde(default)]
    pub files_per_second: Option<f32>,
    /// Seconds remaining, combining both axes via `max(ETA_bytes, ETA_files)`.
    /// `None` during warm-up or when both rates are zero (operation stalled).
    #[serde(default)]
    pub eta_seconds: Option<u32>,
    /// Index-derived expected file count, for rendering a progress bar during
    /// the scanning phase before the foolproof re-scan finishes. `None` when
    /// the index doesn't cover all sources, or outside the scanning phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_files_total: Option<u64>,
    /// Pairs with `expected_files_total`. See its doc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_bytes_total: Option<u64>,
}

impl WriteProgressEvent {
    /// Construct an event with the 8 core counter fields. Rate/ETA fields are
    /// filled in by `WriteOperationState::enrich_progress` right before the
    /// event is emitted. The scanning-only metadata (`current_dir`,
    /// `expected_files_total`, `expected_bytes_total`) defaults to `None` and
    /// is populated by the scan emit sites via `with_scan_meta`. Always go
    /// through this constructor at emit sites so the extra fields stay out of
    /// call sites as visual noise.
    #[allow(
        clippy::too_many_arguments,
        reason = "These are the natural fields of a progress event. Bundling into a struct adds ceremony without cleaning anything up."
    )]
    pub fn new(
        operation_id: String,
        operation_type: WriteOperationType,
        phase: WriteOperationPhase,
        current_file: Option<String>,
        files_done: usize,
        files_total: usize,
        bytes_done: u64,
        bytes_total: u64,
    ) -> Self {
        Self {
            operation_id,
            operation_type,
            phase,
            current_file,
            current_dir: None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            dirs_done: 0,
            bytes_per_second: None,
            files_per_second: None,
            eta_seconds: None,
            expected_files_total: None,
            expected_bytes_total: None,
        }
    }

    /// Attach scanning-phase metadata (current directory, running dirs count,
    /// and index-derived expected totals) to an event. Only emit sites in the
    /// scanning phase call this; everywhere else leaves the fields at their
    /// defaults (`None` / `0`).
    #[must_use]
    pub fn with_scan_meta(
        mut self,
        current_dir: Option<String>,
        dirs_done: usize,
        expected: Option<crate::indexing::expected_totals::ExpectedTotals>,
    ) -> Self {
        self.current_dir = current_dir;
        self.dirs_done = dirs_done;
        if let Some(e) = expected {
            self.expected_files_total = Some(e.files);
            self.expected_bytes_total = Some(e.bytes);
        }
        self
    }
}

/// Completion event payload.
///
/// `files_processed` counts every source the operation considered (transferred + skipped),
/// matching the driver's `files_done`. `files_skipped` is the subset that was skipped via
/// conflict resolution (bulk pre-known-conflict skip, per-iter Skip from the resolver, or
/// closure-side Skip such as same-inode self-copy). For delete/trash, skipping isn't a
/// concept and the field is always 0. The FE uses both to compose user-facing summaries
/// like "Copy complete: 3 copied, 2 skipped" instead of the misleading "0 files".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCompleteEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub bytes_processed: u64,
}

/// Error event payload.
///
/// `error` is the typed variant for programmatic FE handling and tests; `friendly`
/// is the rendered user-facing copy (title + explanation + suggestion + category)
/// produced by the same `friendly_error_from_volume_error` + `enrich_with_provider`
/// pipeline the listing-error path uses. When `friendly` is `None`, the FE falls
/// back to variant-based messages (`transfer-error-messages.ts`). That branch
/// stays in place for local-FS error paths that bypass `VolumeError`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteErrorEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub error: WriteOperationError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly: Option<crate::file_system::volume::friendly_error::FriendlyError>,
}

impl WriteErrorEvent {
    /// Construct a `WriteErrorEvent`, automatically deriving the `FriendlyError`
    /// payload from the typed `error` variant via `friendly_from_write_error`.
    /// Every emit site goes through here by default, so every `write-error` event
    /// the FE receives carries a `friendly` payload it can render directly.
    pub fn new(operation_id: String, operation_type: WriteOperationType, error: WriteOperationError) -> Self {
        let friendly = Some(crate::file_system::volume::friendly_error::friendly_from_write_error(
            &error,
        ));
        Self {
            operation_id,
            operation_type,
            error,
            friendly,
        }
    }

    /// Construct a `WriteErrorEvent` with the `FriendlyError` derived from the
    /// originating `VolumeError + path` via the richer
    /// `friendly_error_from_volume_error` + `enrich_with_provider` pipeline (the
    /// same one the listing-error path uses). Picks up provider-specific
    /// suggestions like "This folder is managed by **MacDroid**…" that the
    /// `WriteOperationError`-shaped `friendly_from_write_error` can't reach.
    ///
    /// Use this from volume-aware emit paths that still have the original
    /// `VolumeError + path` in scope (`volume_move`, `volume_copy`). When that
    /// context isn't available, fall back to `new`.
    pub fn with_friendly(
        operation_id: String,
        operation_type: WriteOperationType,
        error: WriteOperationError,
        volume_error: &crate::file_system::volume::VolumeError,
        path: &Path,
    ) -> Self {
        use crate::file_system::volume::friendly_error::{enrich_with_provider, friendly_error_from_volume_error};
        let mut friendly = friendly_error_from_volume_error(volume_error, path);
        enrich_with_provider(&mut friendly, path);
        Self {
            operation_id,
            operation_type,
            error,
            friendly: Some(friendly),
        }
    }
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

/// Settled event payload. Emitted exactly once per write operation, after the
/// spawned background task has fully returned (success, error, cancelled, or
/// panic). Pairs with the terminal outcome event (`write-complete` /
/// `write-cancelled` / `write-error`): the FE waits for `write-settled` before
/// clearing the "Cancelling…" dialog so the user isn't tempted to dispatch a
/// new op while the volume is still tearing down (USB session teardown on MTP,
/// for example).
///
/// Ordering contract: this event is emitted AFTER the terminal outcome event
/// for the same `operation_id`. The FE buffers any out-of-order delivery
/// defensively; the BE guarantees the BE-side emit order.
///
/// `volume_id` is populated when the source volume is known at the time the
/// guard is set up. Local-FS operations leave it `None` (they don't have a
/// volume_id concept beyond the implicit "root"). The FE doesn't currently
/// filter on volume_id — the per-op `operation_id` is the binding signal —
/// but it's carried for future diagnostics and consistency.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteSettledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Source volume id when known (MTP/SMB volume ops). `None` for local-FS
    /// operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<String>,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    /// Source size in bytes. Files use `metadata.len()`; folder sources use
    /// the recursive total from the pre-flight scan. Always known.
    pub source_size: u64,
    /// Destination size in bytes. `Some` for files (always from
    /// `metadata.len()`) and for folders covered by the drive index;
    /// `None` ("unknown") for folders the index doesn't cover (network mounts,
    /// MTP, paths outside the index scope). The FE renders `(unknown)` for
    /// `None` and disables the "Overwrite all smaller" bulk action.
    pub destination_size: Option<u64>,
    /// Unix timestamp in seconds.
    pub source_modified: Option<i64>,
    /// Unix timestamp in seconds.
    pub destination_modified: Option<i64>,
    pub destination_is_newer: bool,
    /// `destination_size - source_size` when both are known. `None` collapses
    /// the difference when `destination_size` is unknown.
    pub size_difference: Option<i64>,
    /// `true` when the source side is a directory. Lets the FE render the
    /// distinct "replace a folder with a file" / "replace a file with a folder"
    /// warning instead of the generic file-over-file dialog.
    #[serde(default)]
    pub source_is_directory: bool,
    /// `true` when the destination side is a directory. See
    /// `source_is_directory`.
    #[serde(default)]
    pub destination_is_directory: bool,
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

// ============================================================================
// Operation status (for query APIs)
// ============================================================================

/// Current status of an operation for query APIs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    /// The file is in `STATUS_DELETE_PENDING` on the server: a delete was requested
    /// but at least one open handle is keeping it alive. Transient — clears when the
    /// last handle closes. SMB-only today.
    DeletePending {
        path: String,
    },
    /// Catch-all for genuinely unexpected IO errors.
    IoError {
        path: String,
        message: String,
    },
}

/// Classifies a raw `std::io::Error` into a specific `WriteOperationError` variant.
///
/// Only `errno` and `ErrorKind` are consulted — never the formatted message.
/// Backend errors (SMB, MTP, etc.) are typed and flow through
/// `transfer/volume_copy.rs::map_volume_error`, so this function only sees
/// `std::io::Error` values produced by local-FS calls, which always carry a
/// `raw_os_error()` on Unix. Pre-fix the function had a lowercase-substring
/// fallback (`"disconnect"`, `"read-only"`, `"connection"`, `"operation not
/// permitted"`, …) that quietly misclassified errors on localized macOS
/// (the wording localizes; the substrings don't match) and was the exact
/// shape AGENTS.md bans.
fn classify_io_error(e: &std::io::Error, path: String) -> WriteOperationError {
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        match code {
            libc::EROFS => {
                return WriteOperationError::ReadOnlyDevice {
                    path,
                    device_name: None,
                };
            }
            libc::ENAMETOOLONG => return WriteOperationError::NameTooLong { path },
            libc::ENOTCONN | libc::ENETDOWN | libc::ENETUNREACH | libc::EHOSTUNREACH | libc::ETIMEDOUT => {
                return WriteOperationError::ConnectionInterrupted { path };
            }
            libc::ENODEV => return WriteOperationError::DeviceDisconnected { path },
            _ => {} // Fall through to ErrorKind classification
        }
    }

    match e.kind() {
        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path },
        std::io::ErrorKind::PermissionDenied => WriteOperationError::PermissionDenied {
            path,
            message: e.to_string(),
        },
        std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists { path },
        _ => WriteOperationError::IoError {
            path,
            message: e.to_string(),
        },
    }
}

/// Extension trait for converting `io::Result` to `Result<T, WriteOperationError>` with path
/// context.
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WriteOperationStartResult {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for write operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    /// Source filenames already known to conflict at the destination. See
    /// `VolumeCopyConfig::pre_known_conflicts` for the full rationale.
    #[serde(default)]
    pub pre_known_conflicts: Vec<String>,
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
            pre_known_conflicts: Vec::new(),
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
    /// Absolute parent directory currently being scanned. Lets the UI show
    /// "in directory: …" alongside the filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_dir: Option<String>,
    /// Index-derived expected file count, sampled once at scan start. Lets
    /// the FE render a real progress bar from second one of the scan.
    /// `None` when the index doesn't cover all sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_files_total: Option<u64>,
    /// Pairs with `expected_files_total`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_bytes_total: Option<u64>,
}

/// Completion event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewCompleteEvent {
    pub preview_id: String,
    pub files_total: usize,
    pub dirs_total: usize,
    /// Write footprint (un-dedup'd): the bytes a copy actually writes and the
    /// headline the Copy dialog shows. See `CopyScanResult::total_bytes`.
    pub bytes_total: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). Equals
    /// `bytes_total` when there are no hardlinks; when it's smaller, the
    /// dialog shows a "X will be written, source is Y" hint.
    pub dedup_bytes_total: u64,
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewStartResult {
    pub preview_id: String,
}

/// Cached scan-preview totals, returned by `check_scan_preview_status` when the
/// scan has already completed. Lets the FE recover from a race where events
/// fired between IPC dispatch and listener registration (M2a's watcher-backed
/// oracle can finish a scan in ~5 ms, so the FE sometimes registers its
/// listeners too late and never sees the progress/complete events).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreviewTotals {
    pub files_total: usize,
    pub dirs_total: usize,
    pub bytes_total: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). See
    /// `ScanPreviewCompleteEvent::dedup_bytes_total`.
    pub dedup_bytes_total: u64,
}

// ============================================================================
// Volume copy types
// ============================================================================

// ============================================================================
// Event sink trait (decouples write operations from Tauri)
// ============================================================================

/// Abstraction for emitting write operation events.
///
/// Decouples the copy/move/delete pipeline from `tauri::AppHandle`. The Tauri
/// layer provides `TauriEventSink` (calls `app.emit`). Tests use
/// `CollectorEventSink` (stores events in a `Vec` for assertions).
pub trait OperationEventSink: Send + Sync {
    fn emit_progress(&self, event: WriteProgressEvent);
    fn emit_complete(&self, event: WriteCompleteEvent);
    fn emit_cancelled(&self, event: WriteCancelledEvent);
    fn emit_error(&self, event: WriteErrorEvent);
    fn emit_conflict(&self, event: WriteConflictEvent);
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent);
    /// Per-iteration progress during dry-run scanning (separate from `write-progress`).
    fn emit_scan_progress(&self, event: ScanProgressEvent);
    /// One `ConflictInfo` per conflicting file during dry-run scanning.
    fn emit_scan_conflict(&self, conflict: ConflictInfo);
    /// Final dry-run result with conflict sample.
    fn emit_dry_run_complete(&self, result: DryRunResult);
    /// Emitted exactly once per op, after the spawned task fully returns
    /// (success, error, cancel, or panic). Default impl is a no-op so existing
    /// production sinks compile unchanged — `TauriEventSink` and
    /// `CollectorEventSink` override it. See `WriteSettledEvent` for the
    /// ordering contract.
    ///
    /// Production emit goes through `WriteSettledGuard` directly to
    /// `app.emit`; this trait method exists so tests can observe the same
    /// event without a Tauri runtime.
    #[allow(
        dead_code,
        reason = "Trait method consumed only by test sinks; production emits via WriteSettledGuard"
    )]
    fn emit_settled(&self, _event: WriteSettledEvent) {}
}

/// Tauri-backed event sink: calls `app.emit()` for each event.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl OperationEventSink for TauriEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-progress", &event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-complete", &event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-cancelled", &event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-error", &event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-conflict", &event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-source-item-done", &event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("scan-progress", &event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        use tauri::Emitter;
        let _ = self.app.emit("scan-conflict", &conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        use tauri::Emitter;
        let _ = self.app.emit("dry-run-complete", &result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("write-settled", &event);
    }
}

/// Test event sink: stores events for inspection.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "Fields are populated by emit_* methods; read in test assertions as needed"
)]
pub(crate) struct CollectorEventSink {
    pub progress: std::sync::Mutex<Vec<WriteProgressEvent>>,
    pub complete: std::sync::Mutex<Vec<WriteCompleteEvent>>,
    pub cancelled: std::sync::Mutex<Vec<WriteCancelledEvent>>,
    pub errors: std::sync::Mutex<Vec<WriteErrorEvent>>,
    pub conflicts: std::sync::Mutex<Vec<WriteConflictEvent>>,
    pub scan_progress: std::sync::Mutex<Vec<ScanProgressEvent>>,
    pub scan_conflicts: std::sync::Mutex<Vec<ConflictInfo>>,
    pub dry_run: std::sync::Mutex<Vec<DryRunResult>>,
    pub settled: std::sync::Mutex<Vec<WriteSettledEvent>>,
}

#[cfg(test)]
impl CollectorEventSink {
    pub fn new() -> Self {
        Self {
            progress: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
            errors: std::sync::Mutex::new(Vec::new()),
            conflicts: std::sync::Mutex::new(Vec::new()),
            scan_progress: std::sync::Mutex::new(Vec::new()),
            scan_conflicts: std::sync::Mutex::new(Vec::new()),
            dry_run: std::sync::Mutex::new(Vec::new()),
            settled: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl OperationEventSink for CollectorEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.progress.lock().unwrap().push(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.complete.lock().unwrap().push(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.cancelled.lock().unwrap().push(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.errors.lock().unwrap().push(event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.conflicts.lock().unwrap().push(event);
    }
    fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.scan_progress.lock().unwrap().push(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.scan_conflicts.lock().unwrap().push(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.dry_run.lock().unwrap().push(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.settled.lock().unwrap().push(event);
    }
}

// ============================================================================
// Volume copy types
// ============================================================================

/// Copy operation configuration for volume-to-volume copy.
#[derive(Debug, Clone, Deserialize, specta::Type)]
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
    /// Source filenames already known to conflict at the destination (from the
    /// pre-flight `scan_for_conflicts` call). When `conflict_resolution == Skip`,
    /// the copy pipeline bulk-skips these upfront so the progress bar jumps to
    /// reflect them immediately, rather than discovering each one serially via
    /// per-file `get_metadata` stats while non-conflict copies run in between.
    /// Ignored for other resolution modes (Stop still prompts; Overwrite still
    /// proceeds normally). Empty if the FE didn't pre-scan or found no
    /// conflicts.
    #[serde(default)]
    pub pre_known_conflicts: Vec<String>,
}

impl Default for VolumeCopyConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: 200,
            conflict_resolution: ConflictResolution::Stop,
            max_conflicts_to_show: 100,
            preview_id: None,
            pre_known_conflicts: Vec::new(),
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
            pre_known_conflicts: config.pre_known_conflicts.clone(),
        }
    }
}

/// Result of a pre-flight scan for volume copy.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCopyScanResult {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
    pub dest_space: SpaceInfo,
    pub conflicts: Vec<ScanConflict>,
}
