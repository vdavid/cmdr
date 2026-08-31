//! The write-operation vocabulary: the enums, error types, status shapes, and
//! configuration every other module here speaks in.
//!
//! ❌ Nothing in this module may `use` a sibling. It is the floor of
//! `write_operations`, and a single upward import welds eleven modules into
//! one cycle. `DETAILS.md` § "Why `types` imports nothing".
//!
//! The event payloads live one level down in `types/events.rs` and are
//! re-exported here, so every caller keeps its `types::WriteProgressEvent`
//! path.

use serde::{Deserialize, Serialize};

use crate::file_system::volume::{ScanConflict, SpaceInfo};

mod events;
pub use events::*;

// Re-export sort types from sorting module
pub use crate::file_system::listing::{SortColumn, SortOrder};

// ============================================================================
// Operation types
// ============================================================================

/// Type of write operation.
///
/// `Rename`, `CreateFolder`, and `CreateFile` are scan-free, near-instant,
/// result-returning metadata ops that flow through `manager::run_instant`
/// (registered + busy-marked, but NOT lane-queued), not the streaming
/// `spawn_managed` path the transfers/deletes use. They cross the wire as
/// `rename` / `create_folder` / `create_file` (snake_case).
///
/// `ArchiveEdit` is the zip-mutation op (add / delete / rename / mkdir / mkfile
/// inside a `.zip`, and copy/move INTO one): an O(archive) temp+rename rewrite
/// that flows through `spawn_managed` with a real progress bar and the parent
/// drive's lane, NOT the instant path (a rewrite is not a metadata syscall). It
/// crosses the wire as `archive_edit`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationType {
    Copy,
    Move,
    Delete,
    Trash,
    Rename,
    CreateFolder,
    CreateFile,
    ArchiveEdit,
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
    /// Flushing freshly-written destinations to disk (`fdatasync`) before
    /// reporting the copy/move complete. On slow media (USB sticks, SD cards)
    /// this is a real multi-second pause; the FE renders "Writing the last
    /// piece…" so the bar doesn't sit frozen at 100% pretending the work is
    /// done. See `transfer/CLAUDE.md` § "Durability".
    Flushing,
}

/// Lifecycle status of a managed operation, as shown in the queue window.
/// `Paused` is set only by the pause/resume path (`set_paused`); the rest flow
/// from admission and settle. Distinct from `WriteOperationPhase` (the progress
/// phase: Scanning/Copying/Flushing) and from `OperationIntent` (the
/// cancel/rollback machine) — a paused op is still `Running`-intent and may be
/// mid-`Copying`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    /// Registered, waiting for its lanes to free.
    Queued,
    /// Admitted; its deferred start has spawned the real work.
    Running,
    /// Running but pause-gated: the op is parked between files and still holds
    /// its lane slots. Set by the pause/resume path.
    Paused,
    /// Finished successfully.
    Done,
    /// Cancelled by the user (keep-partials).
    Cancelled,
    /// Could not complete.
    Failed,
}

// ============================================================================
// Conflict resolution
// ============================================================================

/// How to handle conflicts when destination files already exist.
// DEFAULT-OK: the zero value is `Stop`, the most conservative policy there is — it asks
// the user instead of deciding for them.
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

/// Which clash an answer is for.
///
/// An operation raises many Stop-mode clashes over its life, one at a time, and
/// the answer to each one travels out through a broadcast event, past a person,
/// and back through IPC. That round trip can outlast the clash: by the time the
/// answer arrives the operation may already be parked on the NEXT one. Without
/// an identity the two are indistinguishable and the late answer silently
/// decides a question nobody was shown.
///
/// So every clash carries one. `ConflictSlot` mints it as it arms, the
/// `write-conflict` event carries it to every surface, and
/// `resolve_write_conflict` requires it back. Ids count from 1 per operation;
/// an answer is only ever matched against the slot of the operation it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct ConflictId(pub u64);

/// What the backend did with one answer to a Stop-mode conflict. Produced by
/// `conflict_slot::ConflictSlot::answer`.
///
/// `write-conflict` broadcasts to every webview, so several surfaces can render
/// the same prompt and each of them can be answered. Exactly one answer reaches
/// the parked operation (the slot hands out its sender once, under its lock);
/// this is how the surfaces that lost learn they lost, and can take their own
/// prompt down instead of believing they did something.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionOutcome {
    /// This answer reached the parked operation, which carried on with it.
    Resolved,
    /// Somebody answered this conflict first. The operation carried on with
    /// THEIR answer; this one changed nothing.
    AlreadyResolved,
    /// This answer names a clash the operation has left behind. It was raised,
    /// it was settled (by an answer, an apply-to-all latch, or a cancel), and
    /// the operation has moved on — possibly onto a different clash, which this
    /// answer must NOT decide. Nothing changed.
    StaleAnswer,
    /// The operation is live but isn't waiting on a conflict: it hasn't raised
    /// one, or a cancel took the pending one away.
    NoPendingConflict,
    /// Nothing is registered under this operation id. It settled, it was
    /// cancelled, or it never existed.
    UnknownOperation,
}

// ============================================================================
// Transfer activity
// ============================================================================

/// What a transfer is waiting on right now, derived from the live in-flight
/// table in `transfer::transfer_probe`.
///
/// The distinction that matters: parked ON PURPOSE (a user pause, a foreground
/// yield so the app stays responsive) is not the same as stuck, and calling a
/// deliberate yield a stall would train people to ignore the warning. The
/// backend classifies; the UI decides how long to wait before speaking.
///
/// Each variant names the QUESTION the operation is stuck on, ❌ never who may
/// answer it: who can answer is a property of which surfaces exist, and it
/// changes underneath this enum every time one ships (`resolve_conflict` made
/// [`Conflict`](Self::Conflict) agent-answerable without the operation changing
/// at all). `write_operations/DETAILS.md` § "Naming a wait".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TransferWaitReason {
    /// Bytes are moving.
    Moving,
    /// Paused by the user.
    Paused,
    /// Every in-flight task is parked waiting for the DESTINATION to accept
    /// writes (a busy share, a device doing foreground work).
    Destination,
    /// Every in-flight task is parked waiting for the SOURCE to produce bytes.
    Source,
    /// A conflict prompt is open and unanswered: the operation is parked until
    /// somebody decides skip / overwrite / rename. The clash itself, with the
    /// `ConflictId` an answer must name, rides `cmdr://state`'s
    /// `pendingConflict` block and `WriteConflictEvent`; a hand answers through
    /// the dialog, an agent through `resolve_conflict`, and the operation is
    /// indifferent to which.
    Conflict,
    /// Nothing is moving and no task explains why. This is the shape the
    /// 2026-07-31 wedge took.
    Unknown,
}

/// The live shape of a running transfer, on every progress event AND on
/// [`OperationStatus`], so both windows and an agent polling `cmdr://state` can
/// answer "why isn't this moving?" and "why does the counter say fewer files
/// than I can see at the destination?" from whichever one they hold.
///
/// Built from the in-flight table where there is one. An operation without one
/// still answers for the wait on a DECISION, off its own pause gate and conflict
/// slot (`WriteOperationState::activity`), with no count to report and no
/// stillness that isn't the answerer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferActivity {
    /// Files open in the concurrency window right now. These have bytes on the
    /// destination but aren't counted in `files_done` yet, which is why the
    /// counter can honestly read lower than what a user sees on the share.
    pub in_flight: u32,
    /// Whole seconds since the operation's aggregate byte counter last moved.
    /// `0` while bytes flow and while paused (a pause isn't time spent stalled).
    pub still_for_seconds: u32,
    /// What it's waiting on. See [`TransferWaitReason`].
    pub waiting_on: TransferWaitReason,
}

// ============================================================================
// Operation status (for query APIs)
// ============================================================================

/// Current status of an operation for query APIs.
///
/// Two INDEPENDENT axes: [`Self::lifecycle`] is what the operation is doing,
/// [`Self::phase`] is what KIND of work. A paused op is mid-`Copying`; a scanning
/// one is `Running`. ❌ Neither may be inferred from the other.
///
/// A snapshot, ❌ not an event: a reader that never caught a `write-progress`
/// (an agent polling `cmdr://state`, a window that opened mid-transfer) gets the
/// same answers a subscriber does, [`activity`](Self::activity) included.
/// Otherwise "a slow copy", "a wedged mount", "parked on a conflict prompt", and
/// "queued behind a lane" all read as `running` with frozen counters.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// The manager's own lifecycle status. `None` once the operation has left the
    /// registry and only its status-cache row survives.
    ///
    /// ❌ Never re-derive one from `WRITE_OPERATION_STATE.contains` or any other
    /// presence test: the entry lands at spawn and survives a pause, so presence
    /// is `true` for queued, running, and parked alike. DETAILS § "Lifecycle
    /// status and `operations-changed`".
    pub lifecycle: Option<LifecycleStatus>,
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
    /// What the operation is waiting on right now, classified live at read time
    /// (`WriteOperationState::activity`) rather than cached: a stale wait is
    /// worse than none.
    ///
    /// `None` means the operation can't classify itself, ❌ never "it's moving":
    /// it has settled (the cache row outlives the state entry), or it's a backend
    /// that keeps no in-flight table and has nobody parked on a decision (a local
    /// copy, a delete, a trash).
    pub activity: Option<TransferActivity>,
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
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum WriteOperationError {
    SourceNotFound {
        path: String,
    },
    /// The destination folder isn't there (or the volume can't address it), so
    /// there was nowhere to put anything. Kept SEPARATE from `SourceNotFound`
    /// because the two send the user to opposite places: a missing source reads
    /// as "your file is gone" and starts a hunt for data loss, when the file is
    /// sitting untouched and it's the target folder that's missing. Which one a
    /// `VolumeError::NotFound` becomes is decided by the `PathRole` the caller
    /// passes to `map_volume_error`, never guessed from the path.
    DestinationNotFound {
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
    /// One or more files exceed the destination filesystem's per-file size
    /// limit (FAT32's 4 GiB cap). Detected during the pre-copy scan, before any
    /// bytes are written, so the whole operation is blocked all-or-nothing
    /// rather than failing partway through.
    FilesTooLargeForFilesystem {
        /// The destination filesystem, so the message can name it ("FAT32").
        filesystem: crate::file_system::filesystem_kind::FilesystemKind,
        /// The per-file ceiling in bytes (FAT32: 4 GiB − 1).
        max_size: u64,
        /// Up to 10 offending files (name + size), largest first.
        files: Vec<OversizedFile>,
        /// Total number of offending files (may exceed `files.len()`).
        total_count: usize,
    },
    /// Extracting from a password-protected archive needs a password. Raised when
    /// a copy/move source is inside an encrypted archive: `wrong_attempt` is
    /// `true` when the stored password was rejected (so the FE re-prompts rather
    /// than prompting fresh). The FE sets a per-archive password via
    /// `set_archive_password` and retries the operation.
    ArchiveNeedsPassword {
        path: String,
        wrong_attempt: bool,
    },
    /// Catch-all for genuinely unexpected IO errors.
    IoError {
        path: String,
        message: String,
    },
}

/// A file that exceeds the destination filesystem's per-file size limit.
/// Carried by [`WriteOperationError::FilesTooLargeForFilesystem`] so the dialog
/// can list the offenders.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OversizedFile {
    pub name: String,
    pub size: u64,
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

/// How often a running operation is allowed to speak: the cadence every
/// transfer's progress events are throttled to, and what any other long-running
/// op (an operation-log reversal) uses so its bar behaves like the rest.
pub(super) const DEFAULT_PROGRESS_INTERVAL_MS: u64 = 200;

fn default_progress_interval() -> u64 {
    DEFAULT_PROGRESS_INTERVAL_MS
}

fn default_max_conflicts_to_show() -> usize {
    100
}

// ============================================================================
// Scan preview results
// ============================================================================

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
    /// Estimated compressed size, mirroring
    /// `ScanPreviewCompleteEvent::estimated_compressed_bytes`, so the recovery
    /// path (`check_scan_preview_status`) hydrates the estimate too when the FE
    /// missed the complete event. `None` for non-compress or remote scans.
    #[serde(default)]
    pub estimated_compressed_bytes: Option<CompressedSizeEstimate>,
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
    /// Deflate level (1..=9) for zip writes this op produces (compress, or
    /// copy/move INTO an archive); `None` = the crate default (level 6). The
    /// frontend reads the `behavior.archiveCompressionLevel` setting at dispatch
    /// and passes it here; non-archive copies ignore it. The mutator clamps to
    /// 1..=9 (an out-of-range level hard-errors the edit, not clamps).
    #[serde(default)]
    pub compression_level: Option<i64>,
}

impl Default for VolumeCopyConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: DEFAULT_PROGRESS_INTERVAL_MS,
            conflict_resolution: ConflictResolution::Stop,
            max_conflicts_to_show: 100,
            preview_id: None,
            pre_known_conflicts: Vec::new(),
            compression_level: None,
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
            // `WriteOperationConfig` is the legacy local-only path (no archive
            // routing rides it), so the level has no source here.
            compression_level: None,
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
    /// What the destination reports it has room for, or `None` when the backend
    /// genuinely can't answer (SFTP: `statvfs@openssh.com` is out of reach). ❗
    /// `None` is "can't tell", ❌ never "no room" — a preview must still open.
    pub dest_space: Option<SpaceInfo>,
    pub conflicts: Vec<ScanConflict>,
}
