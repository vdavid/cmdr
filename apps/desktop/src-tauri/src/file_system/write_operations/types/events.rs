//! Everything the backend BROADCASTS about a write operation: the payload of
//! every `write-*` / `scan-*` event, and the enums that exist only to ride one.
//!
//! The rule that decides what belongs here: a struct carrying
//! `#[tauri_specta(event_name = ...)]` lives in this file, and so does an enum
//! whose only carrier is one of them. A name two homes speak (an event AND a
//! status query, say) is vocabulary and stays in the parent `types`.
//!
//! The builders are elsewhere on purpose: `WriteProgressEvent::new` /
//! `with_scan_meta` and `WriteErrorEvent::new` live in `event_sinks.rs` beside
//! the sinks that emit them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use crate::operation_log::rollback::SkipBreakdown;

use super::{ConflictId, TransferActivity, WriteOperationError, WriteOperationPhase, WriteOperationType};

// ============================================================================
// Progress and terminal events
// ============================================================================

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

/// How much of what a cancelled operation had written got undone.
///
/// Three states, because two can't tell "no reversal ran" from "the reversal ran
/// and left things behind" — which it does whenever it meets a file something
/// else changed since.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    clippy::enum_variant_names,
    reason = "The shared postfix is the point: these are the same words `RollbackState` uses for a history reversal, so one vocabulary describes both."
)]
pub enum CancelRollbackOutcome {
    /// No reversal ran, or it stopped before reaching a single item: everything
    /// the operation wrote is still where it landed.
    NotRolledBack,
    /// Everything the operation still claimed is undone.
    RolledBack,
    /// The reversal ran but left items behind — see [`CancelRollback::skips`].
    PartiallyRolledBack,
}

/// The staged `.cmdr-tmp-*` writes a cancel's sweep asked the destination to
/// remove and didn't get.
///
/// **Deliberately not a [`SkipBreakdown`].** A skip is a LEDGER item the
/// reversal walked to and chose to leave alone, named by the file the user is
/// looking at; these are Cmdr's own scratch files for writes that never
/// finished, they carry no user-facing name, and nothing chose to keep them —
/// the destination refused (`transfer/volume/cleanup.rs`). Folding them into
/// `skips` would also fold them into `reversed + skipped`, which is what the
/// reversal's progress bar drains over, so the bar would report progress across
/// items no reversal ever walks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StagedLeftovers {
    /// Every one the sweep couldn't remove, not a sample.
    pub count: u32,
    /// The on-disk leaf name of the FIRST one, so a report can name it when it's
    /// the only one. That name is the temp's own (`photo.jpg.cmdr-tmp-<uuid>`),
    /// which is what the user would be looking for at the destination.
    pub example_name: String,
}

impl StagedLeftovers {
    /// Fold the paths a sweep couldn't remove into the shape a report reads.
    /// `None` when it removed everything, which is the ordinary ending.
    pub fn of(unremoved: &[PathBuf]) -> Option<Self> {
        let first = unremoved.first()?;
        Some(Self {
            count: unremoved.len() as u32,
            example_name: first
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
    }
}

/// The originals a cross-filesystem move still had in their old place when it
/// was stopped, once the whole copy was already at the destination.
///
/// **Independent of `outcome`, the same way [`StagedLeftovers`] is.** No
/// reversal ran here and none can: the bytes are across a filesystem boundary,
/// so carrying them home would be a second full transfer the user never asked
/// for. `NotRolledBack` says that truthfully and says all it can. What it can't
/// say is that the move ALREADY LANDED — every source's copy is at the
/// destination and durable, some originals are gone for good, and the ones
/// counted here are duplicates of files that now live somewhere else. Without
/// this the readout stays silent on all of it, which is the one reading a user
/// who pressed Rollback must not be left with.
///
/// Set only by `transfer/move_op/cross_fs.rs`'s phase-4 source sweep, which is
/// the only place that state exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OriginalsStillInPlace {
    /// Top-level items — the ones the user picked, files and folders alike, and
    /// the same unit the sweep's own progress counts in. Never zero: the sweep
    /// reads the intent at the top of each item, so a stop always leaves at
    /// least the item it was about to take.
    pub count: u32,
}

/// What the reversal after a cancel managed to undo, and what it left alone.
///
/// Reuses [`SkipBreakdown`] so a cancelled transfer and a Roll back from history
/// report their leftovers in one vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelRollback {
    pub outcome: CancelRollbackOutcome,
    /// Items undone, counting the ones already in the desired end state.
    pub reversed: u32,
    /// One group per reason, with the complete count and one example file name.
    pub skips: Vec<SkipBreakdown>,
    /// The staged partials the abandoned writes left that the sweep couldn't
    /// take away. `None` in the ordinary case, where it took them all.
    ///
    /// Independent of `outcome`: the ledger really can be reversed to the last
    /// entry while gigabytes of scratch sit at the destination, and `outcome`
    /// answers only for the ledger. It's the READOUT's job never to call that
    /// combination clean (`src/lib/file-operations/transfer/cancel-rollback-toast.ts`).
    pub staged_leftovers: Option<StagedLeftovers>,
    /// The originals a landed cross-FS move was still clearing when it stopped.
    /// `None` everywhere else, which is every other operation Cmdr can cancel.
    ///
    /// Independent of `outcome` for the reason [`OriginalsStillInPlace`] gives,
    /// and the READOUT owes it a line even though `outcome` is `NotRolledBack`
    /// (`src/lib/file-operations/transfer/cancel-rollback-toast.ts`).
    pub originals_still_in_place: Option<OriginalsStillInPlace>,
}

impl CancelRollback {
    /// No reversal ran: the operation stopped and kept what it had written.
    pub const fn none() -> Self {
        Self {
            outcome: CancelRollbackOutcome::NotRolledBack,
            reversed: 0,
            skips: Vec::new(),
            staged_leftovers: None,
            originals_still_in_place: None,
        }
    }

    /// Account for the originals a landed cross-FS move hadn't cleared yet, so
    /// the summary the user reads says where their files actually are.
    #[must_use]
    pub const fn with_originals_still_in_place(mut self, count: u32) -> Self {
        self.originals_still_in_place = Some(OriginalsStillInPlace { count });
        self
    }

    /// Name the staged partials the cancel's sweep couldn't remove, so the
    /// summary the user reads accounts for them.
    #[must_use]
    pub fn with_staged_leftovers(mut self, unremoved: &[PathBuf]) -> Self {
        self.staged_leftovers = StagedLeftovers::of(unremoved);
        self
    }
}

/// Cancelled event payload.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "write-cancelled")]
pub struct WriteCancelledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    /// What the reversal undid, if one ran at all.
    pub rollback: CancelRollback,
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
