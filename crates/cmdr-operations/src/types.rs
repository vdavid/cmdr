//! The write-operation vocabulary: the enums, error types, status shapes, and
//! configuration every other module here speaks in.
//!
//! ❌ Nothing in this module may `use` a sibling. It is the floor of
//! `cmdr-operations`, and a single upward import welds eleven modules into
//! one cycle. `DETAILS.md` § "Why `types` imports nothing".
//!
//! The event payloads, the error family, the queue row's summary, `KnownSize`,
//! and what a stopped write left behind live one level down in
//! `types/events.rs`, `types/errors.rs`, `types/queue_row.rs`,
//! `types/known_size.rs`, and `types/leftovers.rs` (a leaf both events and
//! errors carry, so neither imports the other) and are re-exported here, so
//! every caller keeps its `types::WriteProgressEvent` /
//! `types::WriteOperationError` path.

use cmdr_operation_log::types::OpKind;
use serde::{Deserialize, Serialize};

mod errors;
mod events;
mod known_size;
mod leftovers;
mod queue_row;
pub use errors::*;
pub use events::*;
pub use known_size::KnownSize;
pub(crate) use leftovers::*;
pub use queue_row::*;

/// One original the operation kept under a new name because its old name was
/// occupied by the replacement. Shared by failure and cancellation payloads so
/// every terminal path can tell the user where their bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredOriginal {
    /// The name the entry had before the replacement took it.
    pub path: String,
    /// Where the original's bytes are now. Typed, so nothing has to parse a path
    /// back out of prose.
    pub kept_at: String,
}

impl RecoveredOriginal {
    pub(crate) fn new(path: &std::path::Path, kept_at: &std::path::Path) -> Self {
        Self {
            path: path.display().to_string(),
            kept_at: kept_at.display().to_string(),
        }
    }
}

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
/// inside a `.zip`, and copy/move INTO one), while `Compress` identifies a fresh
/// archive built from selected sources. Both are O(archive) temp+rename rewrites
/// that flow through `spawn_managed` with a real progress bar and the parent
/// drive's lane, NOT the instant path (a rewrite is not a metadata syscall).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum WriteOperationType {
    Copy,
    Move,
    Delete,
    Trash,
    Rename,
    CreateFolder,
    CreateFile,
    ArchiveEdit,
    Compress,
}

/// Whether cancelling an operation can also UNDO what it wrote, and when it
/// can't, why. Every construction site of an `OperationDescriptor` names one, so
/// a new spawn path can't skip the decision, and the frontend words the disabled
/// Rollback button from the variant (`file-operations/reversal-wording.ts`).
///
/// A property of the operation's STRATEGY, fixed for its whole life. Whether to
/// OFFER Rollback right now is the view's call on top of it, keyed on the phase.
/// DETAILS § "Rollback availability".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RollbackSupport {
    /// Cancelling can put things back: a local copy/move (`CopyTransaction` /
    /// `MoveTransaction`), a volume copy, or a compress into a NEW archive.
    Supported,
    /// A move between two volumes copies each file and then deletes its
    /// original, so a stop leaves every file whole on one side or the other,
    /// and there's no original left to return the landed ones to.
    CrossVolumeMove,
    /// A move within one non-local volume: the volume renames each item itself
    /// (or, where its renames copy, copies and deletes per item), and the
    /// driver stops without sending anything back.
    SameVolumeMove,
    /// An all-or-nothing temp+rename rewrite of a zip (an edit, a copy or move
    /// into one, a compress over an existing one): a cancel leaves the archive
    /// as it was, so there's nothing partial to undo.
    ArchiveRewrite,
    /// A move out of a zip: the archive drops the entries only after they all
    /// extracted, so a cancel leaves it intact and keeps the extracted copies.
    ArchiveMoveOut,
    /// A delete or trash: what's gone has nothing for a rollback to restore.
    NothingToPutBack,
    /// A rename or a new file/folder: no partial state, and no cancel path that
    /// could catch it mid-flight. Bulk rename stops before the next name.
    InstantChange,
    /// This operation IS the reversal of a finished one; rolling it back would
    /// re-apply what the person just undid.
    IsReversal,
    /// A retained failure row: the operation is over, and no live intent
    /// machine is left to reverse it.
    Settled,
}

impl RollbackSupport {
    /// Whether a cancel with `rollback = true` can undo anything.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// Which of an operation's two progress counts decides how long it has left:
/// its bytes, or its file count. Decided once, in `eta.rs`, with hysteresis,
/// and carried on `write-progress` and the transfer tree's view alike, so every
/// surface fills its bars along the same axis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum ProgressAxis {
    Bytes,
    Files,
}

/// Phase of the operation (for progress reporting).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationPhase {
    /// Scanning source files to calculate total size
    Scanning,
    /// Copying files (for copy and cross-filesystem move)
    Copying,
    /// Reading uncompressed source bytes and writing compressed ZIP entries.
    Compressing,
    /// Closing and durably landing the ZIP after all source bytes were consumed.
    /// Both progress totals are zero because no measurable denominator remains.
    FinishingCompression,
    /// Uploading a completed archive to a remote destination. These bytes are
    /// compressed output bytes, not the source bytes reported by `Compressing`.
    Transferring,
    /// Closing and publishing a fully uploaded remote archive.
    /// Both progress totals are zero because no measurable denominator remains.
    FinishingTransfer,
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

/// One thin registry snapshot row (membership + lifecycle status, NOT 200 ms
/// progress). The queue window subscribes to `operations-changed` for the row
/// set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationSnapshot {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub status: LifecycleStatus,
    pub source: Option<String>,
    pub destination: Option<String>,
    /// See [`OperationDescriptor::rollback_support`](super::OperationDescriptor::rollback_support).
    pub rollback_support: RollbackSupport,
    /// See [`OperationDescriptor::reverses`](super::OperationDescriptor::reverses). Static for the op's whole life,
    /// which is why it rides the thin snapshot rather than the 200 ms progress
    /// tick.
    pub reverses: Option<OpKind>,
    /// Why the operation stopped, on a retained `Failed` row only; `None` on
    /// every live row. The typed variant, never rendered prose: the frontend's
    /// `transfer-error-messages.ts` owns the wording. DETAILS § "Retained
    /// failures".
    pub error: Option<WriteOperationError>,
    /// The queue row may offer Retry: a retained failure that kept a way to
    /// start again (`routing/retry.rs`), with no Retry for it already starting.
    /// `false` on every live row. Whether the ERROR is worth retrying is the
    /// frontend's call (`offersRetry`), the same one the error dialog makes.
    pub can_retry: bool,
    /// See [`OperationSummaryText::items`].
    pub items: Option<OperationItems>,
    /// What a copy or move does with a file that already exists at the
    /// destination, as it runs NOW: the confirmed policy until a clash answered
    /// with "… all" latches another (a broadcast follows that change). `None` for
    /// an operation with no policy, and on a retained failure.
    /// `conflict_policy_cell.rs`.
    pub conflict_policy: Option<ConflictResolution>,
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

/// What a copy does when the destination looks too small for it.
///
/// The pre-flight's figure is an upper bound: files already at the destination
/// can make a copy need less than it says (`free_space.rs`). So a shortfall is
/// the person's call, ❌ never a verdict. `Refuse` stops with `InsufficientSpace`
/// before anything is written, and the error dialog's "Copy anyway" starts the
/// same copy again with `Proceed`. A destination that really fills up mid-copy
/// still stops it, as `DestinationFull`.
// DEFAULT-OK: the zero value is `Refuse`, which asks the person rather than
// starting a copy the destination may not hold.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SpaceShortfall {
    /// Stop before writing anything, with `InsufficientSpace`.
    #[default]
    Refuse,
    /// Skip the check: the person chose to copy anyway.
    Proceed,
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

/// What the backend did with an answer to "these items are Locked, move them
/// anyway?". Produced by `locked_sources::answer_locked_sources`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LockedSourcesAnswerOutcome {
    /// The move was waiting on this question and carries on with the answer.
    Answered,
    /// The operation is live but isn't asking: somebody answered first, or a
    /// cancel took the question away.
    NotAsking,
    /// Nothing is registered under this operation id.
    UnknownOperation,
}

/// What the backend did with "carry on without the sources that left".
/// Produced by `queued_sources::continue_without_gone_sources`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SourcesGoneAnswerOutcome {
    /// The operation was waiting on this question and carries on with the
    /// sources that are still there.
    Continued,
    /// The operation is live but isn't asking: somebody answered first, or a
    /// cancel took the question away.
    NotAsking,
    /// Nothing is registered under this operation id.
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
/// at all). `crates/cmdr-operations/DETAILS.md` § "Naming a wait".
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
    /// A volume the transfer runs on went away (a share dropped off the
    /// network) and the operation is waiting for it to come back, reconnecting
    /// on its own, for a bounded time (`transfer/volume_outage.rs`).
    /// [`TransferActivity::reconnecting_volume`] names which one.
    Reconnecting,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
    /// Nothing has landed yet and every file in flight is still being opened on
    /// the source: the request is out, and no byte of any file has come back.
    ///
    /// Before the first byte there's no ETA to protect, so the UI says this
    /// straight away rather than waiting out the stall notice's threshold.
    pub opening_source: bool,
    /// How fast the SOURCE's connection is receiving, in bytes per second, while
    /// the byte counter above stands still: a response on its way that hasn't
    /// finished arriving. `None` while bytes land (the ordinary rate says it
    /// all), while paused or asking a person, and whenever the source has no
    /// connection to count or nothing is arriving.
    ///
    /// Two caveats, both load-bearing. It's CONNECTION-wide, so a listing on the
    /// same share adds to it. And it's never progress: those bytes are unverified
    /// until their response completes, so ❌ never add them to a byte bar.
    pub source_inbound_bytes_per_second: Option<u64>,
    /// The display name of the volume the operation is waiting on to come
    /// back, while [`waiting_on`](Self::waiting_on) is
    /// [`Reconnecting`](TransferWaitReason::Reconnecting), and `None` otherwise.
    /// Carried here rather than looked up by the view: the queue window knows
    /// an operation's paths, not its volumes.
    pub reconnecting_volume: Option<String>,
}

// ============================================================================
// Retrying a failed operation
// ============================================================================

/// What the queue window's Retry on a failed row did (`retry_failed_operation`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum RetryOutcome {
    /// The transfer started again under this new id, and the failed row is
    /// gone.
    Started { operation_id: String },
    /// Nothing to retry: no retained failure has this id, it carries no way to
    /// start again (an operation kind without Retry), or a Retry for it is
    /// already starting.
    NothingToRetry,
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
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationStatus {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub phase: WriteOperationPhase,
    /// The manager's own lifecycle status. `None` once the operation has left the
    /// registry and only its status-cache row survives.
    ///
    /// ❌ Never re-derive one from the state registry's `contains` or any other
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

// ============================================================================
// Result types
// ============================================================================

/// Result of starting a write operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteOperationStartResult {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
}

// ============================================================================
// Configuration
// ============================================================================

/// How many of an operation's top-level items are files and how many are
/// folders, as the person who started it saw them in the pane. The queue row
/// says "2 folders and 1 file" from it. Only the starter knows this without a
/// `stat` per source (and a network source makes that a round trip each), so
/// it rides the start config, and a caller that didn't say leaves it `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemKinds {
    pub files: u32,
    pub folders: u32,
}

/// Configuration for write operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// Explicit leaf name for a single copy or move; the destination still names its parent.
    #[serde(default)]
    pub destination_name: Option<String>,
    /// What a copy does when the destination looks too small. See [`SpaceShortfall`].
    #[serde(default)]
    pub space_shortfall: SpaceShortfall,
    /// The top-level sources split into files and folders, for the queue row.
    /// See [`ItemKinds`].
    #[serde(default)]
    pub source_kinds: Option<ItemKinds>,
}

impl Default for WriteOperationConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: default_progress_interval(),
            conflict_resolution: ConflictResolution::Stop,
            dry_run: false,
            preview_id: None,
            max_conflicts_to_show: default_max_conflicts_to_show(),
            pre_known_conflicts: Vec::new(),
            destination_name: None,
            space_shortfall: SpaceShortfall::Refuse,
            source_kinds: None,
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

/// Which dialog a scan preview walks for, which decides what the walk keeps
/// beyond its totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ScanPreviewPurpose {
    /// A copy or a move: the walk also builds the transfer's live tree
    /// (`transfer_tree/`), so the operation that consumes the preview has it
    /// from its first moment.
    Transfer,
    /// A compress: the local walk samples a compressed-size estimate. The tree
    /// comes from the compress plan instead.
    Compress,
    /// A delete or a trash: totals only. ❌ Never a tree, which only transfers
    /// show.
    Delete,
}

/// Result of starting a scan preview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct ScanPreviewStartResult {
    pub preview_id: String,
}

/// Why a scan preview wouldn't start. Nothing is walked and no preview exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum ScanPreviewRefusal {
    /// No volume answers for the source's (non-local) volume id: a phone that was
    /// unplugged, or one listed but not connected, typically under a
    /// search-results pane still showing its files. Walking the path on the Mac
    /// instead is what this exists to stop: it can only fail, and the dialog
    /// would then offer a Retry that never works.
    SourceNotConnected { volume_id: String },
}

/// Cached scan-preview totals, returned by `check_scan_preview_status` when the
/// scan has already completed. Lets the FE recover from a race where events
/// fired between IPC dispatch and listener registration (M2a's watcher-backed
/// oracle can finish a scan in ~5 ms, so the FE sometimes registers its
/// listeners too late and never sees the progress/complete events).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// Explicit leaf name for a single copy or move; the destination still names its parent.
    #[serde(default)]
    pub destination_name: Option<String>,
    /// Deflate level (1..=9) for zip writes this op produces (compress, or
    /// copy/move INTO an archive); `None` = the crate default (level 6). The
    /// frontend reads the `behavior.archiveCompressionLevel` setting at dispatch
    /// and passes it here; non-archive copies ignore it. The mutator clamps to
    /// 1..=9 (an out-of-range level hard-errors the edit, not clamps).
    #[serde(default)]
    pub compression_level: Option<i64>,
    /// What a copy does when the destination looks too small. See [`SpaceShortfall`].
    #[serde(default)]
    pub space_shortfall: SpaceShortfall,
    /// The top-level sources split into files and folders, for the queue row.
    /// See [`ItemKinds`].
    #[serde(default)]
    pub source_kinds: Option<ItemKinds>,
}

impl Default for VolumeCopyConfig {
    fn default() -> Self {
        Self {
            progress_interval_ms: DEFAULT_PROGRESS_INTERVAL_MS,
            conflict_resolution: ConflictResolution::Stop,
            max_conflicts_to_show: 100,
            preview_id: None,
            pre_known_conflicts: Vec::new(),
            destination_name: None,
            compression_level: None,
            space_shortfall: SpaceShortfall::Refuse,
            source_kinds: None,
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
            destination_name: config.destination_name.clone(),
            // `WriteOperationConfig` is the legacy local-only path (no archive
            // routing rides it), so the level has no source here.
            compression_level: None,
            space_shortfall: config.space_shortfall,
            source_kinds: config.source_kinds,
        }
    }
}
