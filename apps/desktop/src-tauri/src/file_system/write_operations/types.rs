//! Type definitions for write operations.
//!
//! Contains enums, event structs, error types, and configuration.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use crate::file_system::volume::{ScanConflict, SpaceInfo};

// Re-export sort types from sorting module
pub use crate::file_system::listing::{SortColumn, SortOrder};

// Behavior that used to live here now lives in sibling modules. These re-exports
// keep every existing `types::…` path valid so callers don't change. The event
// sinks (`event_sinks`), analytics (`analytics`), and IO-error classification
// (`error_classification`) all depend on the DTOs below, never the reverse.
pub(super) use super::error_classification::IoResultExt;
#[cfg(test)]
pub(crate) use super::event_sinks::CollectorEventSink;
pub use super::event_sinks::OperationEventSink;
// The lifecycle vocabulary is the manager's, and `OperationStatus` carries it
// rather than growing a second answer to the same question.
use super::manager::LifecycleStatus;

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
// Progress events
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

/// Progress event payload for write operations.
///
/// `bytes_per_second`, `files_per_second`, and `eta_seconds` are populated by
/// `eta::EtaEstimator` from `enrich_progress_event`. They're optional because
/// the estimator returns `None` for both rates and ETA during the warm-up
/// window (first ~800 ms of a phase or before the second sample lands).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-progress")]
pub struct WriteProgressEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// Filename only, not full path.
    pub current_file: Option<String>,
    /// Absolute parent directory currently being scanned (Scanning phase only).
    /// Lets the UI show "in directory: …" alongside the filename so users
    /// get a sense of where in the tree the walker is.
    #[serde(default)]
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
    #[serde(default)]
    pub expected_files_total: Option<u64>,
    /// Pairs with `expected_files_total`. See its doc.
    #[serde(default)]
    pub expected_bytes_total: Option<u64>,
    /// Live in-flight count + stall classification, from the transfer probe —
    /// or, for an operation that keeps no in-flight table (local copy, delete,
    /// trash), just the wait on a person. `None` means the operation is moving
    /// and has nothing extra to say, so the UI shows nothing extra.
    #[serde(default)]
    pub activity: Option<TransferActivity>,
}

/// Completion event payload.
///
/// `files_processed` counts every source the operation considered (transferred + skipped),
/// matching the driver's `files_done`. `files_skipped` is the subset that was skipped via
/// conflict resolution (bulk pre-known-conflict skip, per-iter Skip from the resolver, or
/// closure-side Skip such as same-inode self-copy). For delete/trash, skipping isn't a
/// concept and the field is always 0. The FE uses both to compose user-facing summaries
/// like "Copy complete: 3 copied, 2 skipped" instead of the misleading "0 files".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-complete")]
pub struct WriteCompleteEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub bytes_processed: u64,
}

/// Error event payload.
///
/// `error` is the typed `WriteOperationError` variant. The FE renders all
/// user-facing copy (title, explanation, suggestion) plus the category/retry
/// classification from this typed variant via `transfer-error-messages.ts`.
/// No rendered prose crosses IPC.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-error")]
pub struct WriteErrorEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub error: WriteOperationError,
}

/// How one top-level source item ENDED.
///
/// `Done` is not "every byte moved": a directory merge that skipped three of a
/// hundred children still lands as `Done`, because the source as a whole was
/// carried out. `Skipped` means the operation deliberately left the item alone
/// and wrote nothing for it; `Failed` means it tried and couldn't.
///
/// ⚠️ **The LAST event a source gets is its verdict**, because a cross-filesystem
/// move speaks twice for one source and staging succeeding says nothing about where
/// the item ended up. A consumer recording a per-source status overwrites rather
/// than accumulating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceItemOutcome {
    Done,
    Skipped,
    Failed,
}

/// Emitted when a top-level source item is FINISHED WITH, whichever way it ended:
/// carried out, deliberately skipped, or failed. So this is the per-path OUTCOME
/// stream, not a restatement of intent, and it costs one event per top-level item
/// rather than one list per operation. An item the operation never reached before a
/// cancel still emits nothing — nothing was decided about it.
///
/// The frontend uses it for gradual deselection during an operation, and
/// `source_removed` additionally drives the search-snapshot purge
/// (`$lib/search/snapshot-purge.ts`). Why the outcome rides on this event rather
/// than a sibling one, and where each non-`Done` verdict comes from: `DETAILS.md`
/// § "Per-source outcomes".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-source-item-done")]
pub struct WriteSourceItemDoneEvent {
    pub operation_id: String,
    pub source_path: String,
    /// Whether `source_path` is GONE from its original location now.
    ///
    /// ⚠️ Not "is this a move or a delete": a cross-FS move emits once when the
    /// item finishes STAGING (the source is still there, and a Skip in the
    /// rename phase may mean it stays) and again when the source-delete phase
    /// removes it. Anything that acts on a vanished file must read this flag,
    /// not infer removal from the operation type. ⚠️ Nor is it implied by
    /// `outcome`: a source skipped because it vanished under us reports
    /// `Skipped` AND `source_removed: true`.
    pub source_removed: bool,
    /// How the item ended. See [`SourceItemOutcome`].
    pub outcome: SourceItemOutcome,
}

/// Cancelled event payload.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-cancelled")]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-settled")]
pub struct WriteSettledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Source volume id when known (MTP/SMB volume ops). `None` for local-FS
    /// operations.
    #[serde(default)]
    pub volume_id: Option<String>,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-conflict")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    /// Which clash this is, within its operation. The answer must name it back
    /// (`resolve_write_conflict`), so an answer for a clash the operation has
    /// already left behind can't land on the one it is parked on now. See
    /// [`ConflictId`].
    pub conflict_id: ConflictId,
    pub source_path: String,
    pub destination_path: String,
    /// Source size in bytes. Files use `metadata.len()`; folder sources use
    /// the recursive total from the pre-flight scan when known. `None`
    /// ("unknown") for a folder source on a path that ran no pre-flight scan
    /// (the same-volume move fast path), which the FE renders as `(unknown)`,
    /// mirroring `destination_size`.
    pub source_size: Option<u64>,
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
    /// the difference when either `destination_size` or `source_size` is
    /// unknown.
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

/// A Stop-mode clash is over: the operation took an answer for it and carried
/// on. The counterpart to [`WriteConflictEvent`], and it closes the same loop
/// the id opened.
///
/// The prompt goes out to EVERY webview, so several surfaces can be showing one
/// clash, and only the surface whose own `resolve_write_conflict` call returned
/// learns what happened to it. Every other one — the queue window's copy of the
/// prompt, the main window's host, a surface that never called anything because
/// an AGENT answered over MCP — would sit there asking a question that has no
/// answer left to give, and (being a modal) refusing to let anything new start.
/// So the operation says so as it resumes, and each surface drops the clash it
/// names.
///
/// Emitted only when an answer actually reached the operation. A cancel takes
/// the clash away without one, and the surfaces drop it because the operation
/// itself is gone.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-conflict-resolved")]
pub struct WriteConflictResolvedEvent {
    pub operation_id: String,
    /// The clash that is over. Named, ❌ never implied: by the time this lands,
    /// the operation may already be parked on the NEXT one, and a surface that
    /// dropped "whatever it is showing" would throw away a live question.
    pub conflict_id: ConflictId,
}

/// Progress event during scanning phase (emitted in dry-run mode).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-progress")]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-conflict")]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "dry-run-complete")]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-preview-progress")]
pub struct ScanPreviewProgressEvent {
    pub preview_id: String,
    pub files_found: usize,
    pub dirs_found: usize,
    pub bytes_found: u64,
    /// For activity indication.
    pub current_path: Option<String>,
    /// Absolute parent directory currently being scanned. Lets the UI show
    /// "in directory: …" alongside the filename.
    #[serde(default)]
    pub current_dir: Option<String>,
    /// Index-derived expected file count, sampled once at scan start. Lets
    /// the FE render a real progress bar from second one of the scan.
    /// `None` when the index doesn't cover all sources.
    #[serde(default)]
    pub expected_files_total: Option<u64>,
    /// Pairs with `expected_files_total`.
    #[serde(default)]
    pub expected_bytes_total: Option<u64>,
}

/// Estimated compressed output size for a Compress operation, split by
/// compressibility class so the frontend can re-scale to the selected deflate
/// level via its baked per-class curve without a re-scan. Each field is
/// estimated **level-6** deflate bytes; at level 6 the shown estimate is their
/// sum. `None` on the carrying event when unavailable (non-compress scan, or a
/// remote source where sampling is suppressed). Built by
/// `compress_estimate::CompressEstimator`; see
/// `docs/notes/compress-size-estimate-spike.md` for the accuracy evidence.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompressedSizeEstimate {
    /// Estimated level-6 bytes for files whose sampled ratio is < 0.35.
    pub compressible_bytes: u64,
    /// Estimated level-6 bytes for files whose sampled ratio is in [0.35, 0.8).
    pub medium_bytes: u64,
    /// Estimated level-6 bytes for files whose sampled ratio is >= 0.8.
    pub incompressible_bytes: u64,
}

/// Completion event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-preview-complete")]
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
    /// Estimated compressed size, present only for a compress-mode scan over a
    /// local source. `None` for copy/move scans and for remote (SMB/MTP)
    /// sources (sampling suppressed). The estimate rides the complete event
    /// only; while scanning the dialog shows a loading affordance.
    #[serde(default)]
    pub estimated_compressed_bytes: Option<CompressedSizeEstimate>,
}

/// Error event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-preview-error")]
pub struct ScanPreviewErrorEvent {
    pub preview_id: String,
    pub message: String,
    /// `true` when the walk didn't stop on its own: the watchdog gave up on a
    /// volume that had counted nothing for `SCAN_INACTIVITY_LIMIT`. The dialog
    /// says "not responding" and offers a retry for this one, and a plainer
    /// "couldn't size this" for every other message. A typed flag because the
    /// message is prose: classifying on its wording would break on the first
    /// copy edit.
    #[serde(default)]
    pub timed_out: bool,
}

/// Cancelled event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "scan-preview-cancelled")]
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
            progress_interval_ms: 200,
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

#[cfg(test)]
mod write_conflict_event_serde_tests {
    use super::*;

    fn sample_event(source_size: Option<u64>) -> WriteConflictEvent {
        WriteConflictEvent {
            operation_id: "op-1".to_string(),
            conflict_id: ConflictId(3),
            source_path: "/src/photos".to_string(),
            destination_path: "/dst/photos".to_string(),
            source_size,
            destination_size: Some(4_096),
            source_modified: Some(1_700_000_000),
            destination_modified: Some(1_700_000_001),
            destination_is_newer: true,
            size_difference: source_size.map(|s| 4_096_i64 - s as i64),
            source_is_directory: true,
            destination_is_directory: true,
        }
    }

    #[test]
    fn write_conflict_event_round_trips_with_known_source_size() {
        let event = sample_event(Some(1_024));
        let json = serde_json::to_string(&event).unwrap();
        // camelCase on the wire (matches the FE binding).
        assert!(json.contains("\"sourceSize\":1024"), "json was: {json}");
        let back: WriteConflictEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source_size, Some(1_024));
        assert_eq!(back.size_difference, Some(4_096 - 1_024));
        assert!(back.source_is_directory);
        assert!(back.destination_is_directory);
    }

    #[test]
    fn write_conflict_event_round_trips_with_unknown_source_size() {
        let event = sample_event(None);
        let json = serde_json::to_string(&event).unwrap();
        // `None` serializes as JSON null — the FE renders `(unknown)`.
        assert!(json.contains("\"sourceSize\":null"), "json was: {json}");
        let back: WriteConflictEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source_size, None);
        assert_eq!(back.size_difference, None);
    }
}
