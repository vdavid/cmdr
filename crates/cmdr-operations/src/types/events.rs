//! Everything the backend BROADCASTS about a write operation: the payload of
//! every `write-*` / `scan-*` event, and the enums that exist only to ride one.
//!
//! The rule that decides what belongs here: a struct the app broadcasts as an
//! event payload lives in this file, and so does an enum whose only carrier is
//! one of them. A name two homes speak (an event AND a status query, say) is
//! vocabulary and stays in the parent `types`.
//!
//! These are plain `serde` + `specta` types. The wire event, with its name,
//! is the app's: `operations_host/wire.rs` wraps each one, and ❗ every doc
//! comment and `#[serde]` attribute here reaches `bindings.ts` verbatim.
//!
//! The builders are elsewhere on purpose: `WriteProgressEvent::new` /
//! `with_scan_meta` and `WriteErrorEvent::new` live in `event_sinks.rs` beside
//! the collector sinks.

use serde::{Deserialize, Serialize};

use cmdr_operation_log::rollback::SkipBreakdown;

use super::{
    ConflictId, OperationSnapshot, ProgressAxis, RecoveredOriginal, StagedLeftovers, TransferActivity,
    TrashRefusalKind, WriteOperationError, WriteOperationPhase, WriteOperationType,
};

// ============================================================================
// Progress and terminal events
// ============================================================================

/// Progress event payload for write operations.
///
/// `bytes_per_second`, `files_per_second`, and `eta_seconds` are populated by
/// `eta::EtaEstimator` from `enrich_progress_event`. They're optional because
/// the estimator returns `None` for both rates and ETA during the warm-up
/// window (first ~800 ms of a phase or before the second sample lands).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// The axis that decides the ETA, held steady by hysteresis
    /// (`eta.rs`). Every folder bar in the transfer tree fills along it. `None`
    /// before the estimator has seen the event.
    #[serde(default)]
    pub bottleneck: Option<ProgressAxis>,
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
    /// Which step of a multi-step operation this phase belongs to. A fresh
    /// compress onto a destination that can't take a stream zips locally first
    /// and then uploads, so it says 1 of 2 while compressing and 2 of 2 while
    /// transferring. `None` for single-step work, including a direct compress.
    #[serde(default)]
    pub step: Option<ProgressStep>,
}

/// Step `number` of `total` in a multi-step operation. Rides on
/// [`WriteProgressEvent::step`]; the backend decides it, the frontend only words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStep {
    pub number: u8,
    pub total: u8,
}

/// Completion event payload.
///
/// `files_processed` counts every source the operation considered (transferred + skipped),
/// matching the driver's `files_done`. `files_skipped` is the subset that was skipped via
/// conflict resolution (bulk pre-known-conflict skip, per-iter Skip from the resolver, or
/// closure-side Skip such as same-inode self-copy). For delete/trash, skipping isn't a
/// concept and the field is always 0. The FE uses both to compose user-facing summaries
/// like "Copy complete: 3 copied, 2 skipped" instead of the misleading "0 files".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteCompleteEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub bytes_processed: u64,
    /// What a cross-filesystem move found in the source that its copy phase
    /// never carried, and therefore left where it was. `None` (the ordinary
    /// case) means every source went, and the FE says nothing about it.
    #[serde(default)]
    pub appeared_during_move: Option<AppearedDuringMove>,
    /// Which of the TOP-LEVEL items the user picked landed nothing at all.
    /// `None` from an engine that doesn't track it, and the FE then words the
    /// summary from `files_skipped` alone. See [`TopLevelSkipped`].
    #[serde(default)]
    pub top_level_skipped: Option<TopLevelSkipped>,
    /// What a batch trash had to leave where it was. `None` (the ordinary case)
    /// means every item the operation touched went, and the FE says nothing
    /// about it. See [`TrashRefusedItems`].
    #[serde(default)]
    pub refused: Option<TrashRefusedItems>,
    /// What the destination couldn't store of the sources' metadata (creation
    /// dates on a server without them, Finder tags on a phone), counted only
    /// where a source item had something worth keeping. `None`, the ordinary
    /// case, means nothing a person would miss stayed behind, and the FE says
    /// nothing. See [`MetadataLeftBehind`].
    #[serde(default)]
    pub metadata_left_behind: Option<MetadataLeftBehind>,
    /// The closing flush couldn't confirm a copy's output is on the disk (an
    /// `fsync` answered with an error), so the files landed but may not
    /// survive a power loss or an unplugged drive. `None`, the ordinary case,
    /// means the flush confirmed everything, and the FE says nothing. A move
    /// never completes with this: it keeps its sources and stops with
    /// `MoveNotConfirmed` instead. See [`DurabilityUnconfirmed`].
    #[serde(default)]
    pub durability_unconfirmed: Option<DurabilityUnconfirmed>,
    /// The Locked items a move left in place: skipped on the person's answer, or
    /// locked by the system. `None`, the ordinary case, means it left none, and
    /// the FE says nothing. See [`LockedKept`].
    #[serde(default)]
    pub locked_kept: Option<LockedKept>,
}

impl WriteCompleteEvent {
    /// The completion of an operation that skipped every one of its `skipped`
    /// top-level sources before touching anything: nothing was written, so
    /// `bytes_processed` is zero and `files_skipped` is the whole batch.
    pub fn all_skipped(operation_id: &str, operation_type: WriteOperationType, skipped: usize) -> Self {
        Self {
            operation_id: operation_id.to_string(),
            operation_type,
            files_processed: 0,
            files_skipped: skipped,
            bytes_processed: 0,
            appeared_during_move: None,
            top_level_skipped: None,
            refused: None,
            metadata_left_behind: None,
            durability_unconfirmed: None,
            locked_kept: None,
        }
    }
}

/// A copy that landed every file but couldn't prove them durable, for the
/// warning beside its completion. Typed, never a sentence: the FE words it,
/// and the log keeps the path and the errno.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DurabilityUnconfirmed {
    /// The destination's name as the volume list shows it, or `None` when the
    /// operation didn't capture its sides.
    pub volume_name: Option<String>,
}

/// The metadata a copy or move couldn't carry to its destination, per class,
/// for the one calm note on its completion
/// (`transfer/volume/DETAILS.md` § "Copies keep the source's metadata").
///
/// Not a failure and ❌ never folded into the skip counts: every byte landed.
/// Typed, never a sentence: the FE words it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MetadataLeftBehind {
    /// One entry per class that lost anything, in [`LostMetadataClass`]'s
    /// order. Never empty: an operation that lost nothing reports `None`.
    pub classes: Vec<LostMetadata>,
}

/// How many items lost one class of metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct LostMetadata {
    pub class: LostMetadataClass,
    /// Files and folders that had a value in this class and arrived without it.
    pub item_count: u32,
}

/// A class of metadata as a person sees it, which is coarser than the
/// engine's (`cmdr_fs::metadata::MetadataClasses`): only the ones a person can
/// notice are ever reported. atime, ACLs, and the quarantine never are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LostMetadataClass {
    /// A creation date earlier than the item's last edit, on a file or a folder.
    CreationDates,
    /// A file's permission bits, when they differ from what a fresh file gets.
    Permissions,
    /// Finder's Locked and Hidden, and the rarer user flags.
    LockedAndHidden,
    /// Finder tags and comments, custom icons, resource forks, and apps' own
    /// extended attributes.
    ExtendedAttributes,
    /// A folder's modification date.
    FolderDates,
}

/// The top-level items a batch trash could not take, when the rest of the batch
/// went through.
///
/// ❗ Not a skip, and ❌ never folded into `files_skipped`: a skip is something
/// the user or a policy CHOSE, while this is the OS turning an item down and the
/// item still sitting in the pane afterwards. A batch where EVERY item was
/// refused is a `WriteOperationError::TrashRefused` instead and never reaches a
/// completion event; this variant exists for the mixed ending, which would
/// otherwise read as a clean success and say nothing about what stayed.
///
/// Typed, never a sentence: the FE words this in ten locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashRefusedItems {
    /// How many top-level items stayed exactly where they were.
    pub item_count: u32,
    /// The reason that opens the most doors for the user, picked by the same
    /// `strongest_refusal` the all-refused error reports.
    pub reason: TrashRefusalKind,
}

/// The top-level items an operation landed nothing for, split the way the
/// user's SELECTION is.
///
/// ❗ Not derivable from `files_skipped`, and the summary breaks if you try.
/// `files_skipped` counts LEAVES, nested ones included, while the FE's phrase
/// ("Copied 2 files and 1 folder") is about the things the user picked. The FE
/// used to subtract the one from the other, which held only while every skip
/// was a top-level file: once a folder's child could be skipped, a Skip-All
/// over `readme.txt` + `docs/` reported "Copied 1 folder, skipped 3 files" and
/// lost the top-level file that did copy. So the engine reports both, and each
/// count is subtracted from its own kind.
///
/// A folder counts here only when NOTHING under it landed. One refused child
/// leaves its folder `Done`, because the folder did partly arrive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TopLevelSkipped {
    pub files: usize,
    pub folders: usize,
}

/// What a move's source sweep left in place because the move didn't carry it:
/// items that turned up in a source folder after the scan counted it (a
/// download finishing, a sync client landing a file), and originals someone
/// saved over after their copy started, whose new bytes never reached the
/// destination. The sweep leaves both (and whatever holds them) alone, and the
/// operation says so instead of reporting a clean move.
///
/// Typed, never a sentence: the FE words this in ten locales.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppearedDuringMove {
    /// How many items appeared and stayed behind. A whole unknown subtree
    /// counts once, since that's the item the user would recognize in the pane.
    pub item_count: u32,
    /// How many originals changed after their copy started and stayed behind.
    /// The destination holds the version the copy read.
    pub changed_count: u32,
    /// The name (not the path) of the source folder holding them, for the
    /// sentence: a folder source's own name, a file source's parent's. When
    /// several sources kept something, the first one's.
    pub folder_name: String,
    /// How many top-level sources kept something. `1` in the ordinary case; the
    /// FE reads a higher number as "and others" rather than naming them all.
    pub folder_count: u32,
}

/// Error event payload.
///
/// `error` is the typed `WriteOperationError` variant. The FE renders all
/// user-facing copy (title, explanation, suggestion) plus the category/retry
/// classification from this typed variant via `transfer-error-messages.ts`.
/// No rendered prose crosses IPC.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteErrorEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub error: WriteOperationError,
    /// How far the operation had got when it stopped, read from its live status
    /// before it unregisters. `None` when nothing was measured yet, or for a
    /// refusal that happened before any work started.
    pub progress_at_stop: Option<ProgressAtStop>,
}

/// What an operation had done at the moment it stopped, so the copy can say how
/// far it got rather than only that it stopped.
///
/// Typed, never a sentence: the FE words this in ten locales, and the counts are
/// formatted there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgressAtStop {
    pub files_done: u32,
    /// `0` while the scan hadn't finished counting.
    pub files_total: u32,
    pub bytes_done: u64,
    /// `0` while the scan hadn't finished counting.
    pub bytes_total: u64,
    /// A cross-filesystem move only: originals the source sweep had already
    /// removed, in top-level items.
    pub sources_removed: Option<u32>,
    /// A cross-filesystem move only: originals still standing in the source, in
    /// top-level items.
    pub sources_left: Option<u32>,
}

/// A queued operation found, when its turn came, that some of the top-level
/// sources it was started with are gone: moved or deleted while it waited.
///
/// The counts are top-level items, never a walk: one existence probe per source
/// is all the check costs. `next` says whether the operation is now parked on
/// the person (`continue_without_gone_sources` carries it on, a cancel ends it)
/// or has already finished because nothing was left. `queued_sources.rs`, and
/// `DETAILS.md` § "Sources that left while an operation queued".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteSourcesGoneEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// How many top-level sources the operation was started with.
    pub total_count: usize,
    /// How many of them are gone now.
    pub gone_count: usize,
    /// The first few gone sources' paths, in the caller's order, so a prompt can
    /// name them. Capped: `gone_count` is the honest total.
    pub gone_sample: Vec<String>,
    pub next: SourcesGoneNext,
}

/// A move found Locked items among what it's about to move, and asks before any
/// byte moves whether to unlock them for it.
///
/// Locked is Finder's Locked (`uchg`/`uappnd`) or an ACL denying `delete`: both
/// refuse the rename or the delete a move needs. The move parks until
/// `answer_locked_sources` says [`LockedSourcesAnswer`], or a cancel ends it.
/// Items the SYSTEM locked (`schg`) are never asked about: no answer could move
/// them, so they're skipped and counted in [`LockedKept`]. `locked_sources.rs`,
/// and `DETAILS.md` § "Moving Locked items".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteLockedSourcesEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// How many Locked items the move found, at any depth.
    pub locked_count: usize,
    /// The first few Locked items' paths, in the order the move found them, so a
    /// prompt can name them. Capped: `locked_count` is the honest total.
    pub locked_sample: Vec<String>,
}

/// The person's answer to a [`WriteLockedSourcesEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LockedSourcesAnswer {
    /// Unlock each Locked item right before the move takes it, and lock it again
    /// where it lands, so it ends up Locked at its new place.
    MoveAnyway,
    /// Leave the Locked items where they are, and move the rest.
    Skip,
}

/// The Locked items a move left where they were, for one calm line beside its
/// completion. `None` on [`WriteCompleteEvent::locked_kept`] means it left none.
/// Typed, never a sentence: the FE words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LockedKept {
    /// Locked items the person chose to skip.
    pub skipped: u32,
    /// Items the system locked (`schg`), which no answer can move.
    pub system_locked: u32,
}

/// What a queued operation does about sources that left while it waited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SourcesGoneNext {
    /// Some sources are still there. The operation waits for the person: carry
    /// on with the rest, or cancel.
    AwaitingAnswer,
    /// Every source is gone. The operation has completed with nothing to do;
    /// this event only says why.
    NothingLeft,
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
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
/// (`#lib/search/snapshot-purge.ts`). Why the outcome rides on this event rather
/// than a sibling one, and where each non-`Done` verdict comes from: `DETAILS.md`
/// § "Per-source outcomes".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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

/// The originals a cross-filesystem move still had in their old place when it
/// was stopped, once the whole copy was already at the destination.
///
/// **Independent of `outcome`, the same way `staged_leftovers` is.** No
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
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct CancelRollback {
    pub outcome: CancelRollbackOutcome,
    /// Items undone, counting the ones already in the desired end state.
    pub reversed: u32,
    /// One group per reason, with the complete count and one example file name.
    pub skips: Vec<SkipBreakdown>,
    /// Cmdr's own unfinished writes the cleanup couldn't take off the
    /// destination at once, one group per fate. Empty in the ordinary case,
    /// where it took them all.
    ///
    /// Independent of `outcome`: the ledger really can be reversed to the last
    /// entry while gigabytes of scratch sit at the destination, and `outcome`
    /// answers only for the ledger. It's the READOUT's job never to call that
    /// combination clean (`src/lib/file-operations/transfer/cancel-rollback-toast.ts`).
    pub staged_leftovers: Vec<StagedLeftovers>,
    /// The originals a landed cross-FS move was still clearing when it stopped.
    /// `None` everywhere else, which is every other operation Cmdr can cancel.
    ///
    /// Independent of `outcome` for the reason [`OriginalsStillInPlace`] gives,
    /// and the READOUT owes it a line even though `outcome` is `NotRolledBack`
    /// (`src/lib/file-operations/transfer/cancel-rollback-toast.ts`).
    pub originals_still_in_place: Option<OriginalsStillInPlace>,
    /// Originals that could not take their old names back and therefore kept a
    /// ` (recovered)` sibling name. Independent of `outcome`: a reversal can
    /// remove everything it wrote and still have to put an original beside a
    /// name another writer occupied.
    pub recovered: Vec<RecoveredOriginal>,
}

impl CancelRollback {
    /// No reversal ran: the operation stopped and kept what it had written.
    pub const fn none() -> Self {
        Self {
            outcome: CancelRollbackOutcome::NotRolledBack,
            reversed: 0,
            skips: Vec::new(),
            staged_leftovers: Vec::new(),
            originals_still_in_place: None,
            recovered: Vec::new(),
        }
    }

    /// Account for the originals a landed cross-FS move hadn't cleared yet, so
    /// the summary the user reads says where their files actually are.
    #[must_use]
    pub const fn with_originals_still_in_place(mut self, count: u32) -> Self {
        self.originals_still_in_place = Some(OriginalsStillInPlace { count });
        self
    }

    /// Name what the stop's cleanup couldn't take off the destination at once,
    /// so the summary the user reads accounts for it.
    #[must_use]
    pub fn with_staged_leftovers(mut self, leftovers: Vec<StagedLeftovers>) -> Self {
        self.staged_leftovers = leftovers;
        self
    }

    /// Name originals that had to keep another name, so the cancellation
    /// summary can point the user at them.
    #[must_use]
    pub fn with_recovered(mut self, recovered: Vec<RecoveredOriginal>) -> Self {
        self.recovered = recovered;
        self
    }
}

/// A drive or server came back carrying something an unfinished transfer left,
/// and Cmdr kept it rather than finish or remove it ([`KeptLeftover`] says
/// what, and where).
///
/// ❗ Belongs to NO operation, and carries no id: the transfer that left it
/// ended in an earlier session, or before the drive was unplugged. It's the
/// leftover sweep speaking (`crates/cmdr-operations/src/in_flight_sweep.rs`), which is
/// also why the drive's name rides along rather than being looked up — by the
/// time anything could look, the operation is long gone.
///
/// Nothing is asked of the person and nothing of theirs is at risk; this exists
/// so what was kept has a findable home. Fires once per thing the sweep keeps,
/// never per launch: the record retires the moment it's settled.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LeftoversKeptEvent {
    /// The drive's or server's display name, as the registry has it right now.
    pub volume_name: String,
    /// What was kept.
    pub kept: KeptLeftover,
}

/// What a [`LeftoversKeptEvent`] kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum KeptLeftover {
    /// A move's staging folder with files still in it, left exactly where it
    /// is: they can be the only copy, the move may already have removed the
    /// originals.
    StagingFolder {
        /// The folder's own name (`.cmdr-staging-<op>`), which is what the
        /// person would look for with hidden files shown.
        folder_name: String,
    },
    /// A landing whose temp isn't the size it was recorded at, so nothing
    /// proves it holds the file it was on its way to be: kept under a
    /// ` (recovered)` name beside where it belongs, ❌ never landed or removed.
    UnfinishedLanding {
        /// The name the file was on its way to (`notes.txt`).
        file_name: String,
        /// The name it's kept under now (`notes (recovered).txt`).
        kept_name: String,
    },
}

/// Typed `operations-changed` Tauri event carrying the thin registry snapshot
/// (membership + lifecycle status, NOT 200 ms progress). The struct name
/// kebab-cases to `operations-changed`. The queue window subscribes to it for
/// the row set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationsChanged {
    pub operations: Vec<OperationSnapshot>,
}

/// Cancelled event payload.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteSettledEvent {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Source volume id when known (MTP/SMB volume ops). `None` for local-FS
    /// operations.
    #[serde(default)]
    pub volume_id: Option<String>,
}

/// Conflict event payload (emitted when Stop mode encounters a conflict).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// their recursive total when known: the pre-flight scan's on a
    /// cross-volume transfer, the drive index's on a local one. `None`
    /// ("unknown") for a folder source with neither (the same-volume move fast
    /// path runs no pre-flight scan; the index may not cover the path), which
    /// the FE renders as `(unknown)`, mirroring `destination_size`.
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
    /// `true` when the destination entry holds the source's name in another
    /// Unicode spelling (`café` composed vs decomposed): it looks like the same
    /// name, but a byte-exact volume stores it as a different one.
    /// `destination_path` is that entry's own path, so an Overwrite replaces it
    /// and it keeps its stored spelling. The dialog says so, since nothing on
    /// screen can show the difference. `look_alike.rs::is_look_alike_clash`.
    #[serde(default)]
    pub destination_is_look_alike: bool,
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct WriteConflictResolvedEvent {
    pub operation_id: String,
    /// The clash that is over. Named, ❌ never implied: by the time this lands,
    /// the operation may already be parked on the NEXT one, and a surface that
    /// dropped "whatever it is showing" would throw away a live question.
    pub conflict_id: ConflictId,
}

/// Progress event during scanning phase (emitted in dry-run mode).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// The walk has met a file the cloud provider has evicted ("online-only"),
    /// so trashing this selection would download it just to fill a folder the
    /// person is about to empty. The delete confirmation reads it live and flips
    /// itself to the permanent delete. Always `false` for a scan whose sources
    /// aren't in a cloud drive: the tally is only armed there.
    /// See `delete/cloud_trash.rs`.
    #[serde(default)]
    pub online_only_found: bool,
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
    /// The finished walk's online-only verdict, the same flag
    /// `ScanPreviewProgressEvent::online_only_found` carries. `false` here is the
    /// definitive "nothing evicted in this tree", which is what releases a delete
    /// confirmation waiting to know whether it's a trash or a delete.
    #[serde(default)]
    pub online_only_found: bool,
}

/// Error event for scan preview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
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
            destination_is_look_alike: false,
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
