//! The rollback engine: reverse a journaled operation as a set of inverse
//! per-item actions, each rechecked against its recorded snapshot before it
//! touches anything.
//!
//! **Data-safety-critical.** A rollback must never destroy data — least of all
//! data the user created AFTER the operation. Two independent guards enforce that
//! (`DETAILS.md` § "The two data-safety guards"):
//!
//! - **Snapshot recheck.** Before reversing an item, verify it still matches the
//!   size/mtime the journal recorded ([`verify_snapshot`]). Any drift (a changed
//!   file) OR an unverifiable precondition (a field the backend can't prove — an
//!   absent mtime on MTP/SMB) ⇒ SKIP that item, never operate on it. The engine
//!   fails safe, never optimistic.
//! - **Pinned non-destructive restore.** A restore-move (move/trash/rename undo)
//!   NEVER overwrites: if the restore target is occupied by a DIFFERENT entry it
//!   skips that item ([`SkipReason::RestoreTargetOccupied`]). The one exception is
//!   a self-collision (restoring `dog.JPG` → `dog.jpg` on a case-insensitive
//!   volume, where the "occupant" IS the entry being restored, proven by node id
//!   or by exact names in a fresh listing, never by folding) — that's not a real
//!   collision, so it proceeds ([`is_self_collision`]).
//!
//! Undoing a copy also proves the copy's original is still there before it
//! deletes the copy ([`original_stays`]): never remove the only copy left.
//!
//! A skipped item leaves the operation `partially_rolled_back`; a fully reversed
//! one lands `rolled_back`; one that reversed nothing (all skipped, or canceled
//! before anything ran) returns to `rollbackable` so a retry can resume — every
//! per-item inverse is an idempotent recheck-then-act, so re-issuing is safe.
//!
//! **Planner here, executor injected.** This module decides: it pages the
//! journal, rechecks each item, works out the inverse act, and keeps the books.
//! It never touches a file itself — an injected [`RollbackRunner`] performs each
//! decided act, answers "should I stop?", parks while paused, and turns the
//! per-item progress into events. That keeps `operation_log` free of file-moving
//! code and lets the acting side use the cross-volume primitives that only
//! `cmdr-operations` can reach. See `rollback/runner.rs`.
//!
//! Reversal streams the original op's `rollback_unit` rows `seq DESC` through a
//! paged cursor (`store::read_rollback_units_page`), so a 1M-item op never
//! materializes its list. The `seq DESC` order removes copied files before the
//! `entry_type = dir` rows that held them. The inverse operation is itself
//! journaled with `rolls_back_op_id` set (so it appears in history and drives the
//! crash-reconcile), computing its own eligibility — a move/rename undo is
//! rollbackable again (redo), a delete-the-copies undo is not.

use rusqlite::Connection;

use super::store::{
    OperationItemRow, OperationLogStoreError, OperationRow, RollbackUnit, open_read_connection,
    read_rollback_progress_totals, read_rollback_rows_page, rollback_unit_of,
};
use super::tree::rollback_filter::{Narrowed, RollbackFilter, narrow};
use super::tree::selection::RollbackSelection;
use super::tree::{UnitRole, unit_role};
use super::types::{
    EntryType, ExecutionStatus, Initiator, NotRollbackableReason, OpKind, RollbackState, RowRole, SkipReason,
};
use super::writer::{OpenOperation, OperationLogWriter};

/// Rows streamed per page from the journal — bounded so a huge op never
/// materializes its full item list in memory.
const ROLLBACK_PAGE: u32 = 512;

/// Why a rollback request is refused at the operation level (before any item
/// runs). Typed across IPC/MCP — never a message string.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind",
    content = "detail"
)]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum RollbackRefusal {
    /// No operation with this id in the journal.
    UnknownOperation,
    /// The op is already being rolled back — the double-rollback guard.
    AlreadyRollingBack,
    /// The op was already fully reversed; there's nothing to undo.
    AlreadyRolledBack,
    /// The op is not rollbackable; carries the stored reason (delete, overwrote,
    /// archive-overwrite, zip-edit-unsupported, journal-incomplete).
    NotRollbackable(NotRollbackableReason),
    /// A volume the rollback needs isn't currently connected. Computed at rollback
    /// time from mount state, never stored; names the missing volume so the
    /// UI/agent can say "Volume 'Backup' is not connected".
    VolumeUnavailable { volume_id: String },
    /// The selection sent with the request can't run; carries why.
    Selection(SelectionRefusal),
}

/// Why a rollback refuses the selection it was sent. The dialog keeps its button off
/// at "0 of M" and refuses toggles past the cap itself, so each of these is a backstop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum SelectionRefusal {
    /// It takes no unit that still awaits reversal ("0 of M").
    NothingSelected,
    /// The op's tree summary isn't built yet, and a selection is decided off it.
    SummaryNotBuilt,
    /// It holds more hand toggles than a selection may.
    TooManyToggles,
}

impl SkipReason {
    /// `AlreadyGone` means the end state we wanted already holds (idempotent
    /// re-issue), so it counts as reversed, not as a partial-blocking skip.
    pub fn counts_as_reversed(self) -> bool {
        matches!(self, SkipReason::AlreadyGone)
    }
}

/// The outcome of reversing one item. Shared with the in-flight reversals
/// (`cmdr_operations::reversal`) so both report the same verdicts.
pub enum ItemResult {
    /// Reversed (or already in the desired end state).
    Reversed,
    /// Skipped, with the typed reason.
    Skipped(SkipReason),
}

/// What a rollback DISPATCH returns to the FE/MCP: the inverse op's id. The
/// reversal itself is an async managed op, so the caller polls the ORIGINAL op's
/// `rollback_state` until it leaves `rolling_back` to observe the terminal result
/// (the MCP tools' "dispatch then poll" contract).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct RollbackDispatch {
    pub inverse_op_id: String,
}

/// The result of a rollback run over the whole op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackReport {
    /// Items reversed (or already gone — idempotent no-ops).
    pub reversed: u64,
    /// Items skipped (drift, unverifiable, occupied target, non-empty dir, error).
    pub skipped: u64,
    /// Items still awaiting reversal that the run's selection left out, never
    /// attempted. Any keeps the op `partially_rolled_back`.
    pub left_out: u64,
    /// Which reason left which file alone: one group per [`SkipReason`], with the
    /// complete count and one example file name. The counts sum to `skipped`, so a
    /// report can name a file instead of a reason class without understating the rest.
    pub skips: Vec<SkipBreakdown>,
    /// The run stopped early because it was canceled.
    pub canceled: bool,
    /// The state the original op resolves to.
    pub final_state: RollbackState,
}

// ── Pure decision helpers (unit-tested in isolation) ─────────────────────────

/// The inverse operation's `kind`, for its own journal row + eligibility. A
/// copy/create/compress undo is a delete (not rollbackable again); a move/trash
/// undo is a move (rollbackable again — redo); a rename undo is a rename.
pub fn inverse_kind(kind: OpKind) -> OpKind {
    match kind {
        OpKind::Copy | OpKind::CreateFolder | OpKind::CreateFile | OpKind::ArchiveEdit => OpKind::Delete,
        OpKind::Move | OpKind::Trash => OpKind::Move,
        OpKind::Rename => OpKind::Rename,
        // Writing the prior tags back is itself a tag write, so a tag undo undoes too.
        OpKind::Tag => OpKind::Tag,
        // Delete is gated op-level (never rollbackable); its inverse is unreachable.
        OpKind::Delete => OpKind::Delete,
    }
}

/// The shape of the inverse for one item, derived purely from the op kind and the
/// item's entry type (`DETAILS.md` § "Per-kind inverse table").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InverseAction {
    /// Remove the created file at its dest, only if it still matches the snapshot
    /// (create_file, compress archive).
    RemoveFileIfUnchanged,
    /// Remove a copied file at its dest, only if it still matches the snapshot AND
    /// its original is still where it was copied from, unchanged in size: removing
    /// a copy is permanent, so it must never take the only copy left.
    RemoveCopyIfOriginalStays,
    /// Remove the created/copied directory at its dest, only if still empty (copy's
    /// created dirs, create_folder).
    RemoveDirIfEmpty,
    /// Move the item back from where it ended up (dest) to its original (source):
    /// move, trash restore, rename-back.
    RestoreMove,
    /// Write the file's prior Finder-tags value back, only while it still carries
    /// the value the write left (a tag write).
    RestoreTags,
}

/// Whether this row names a directory the operation CREATED rather than one it
/// moved. `record_created_dirs` writes the created path as BOTH source and dest,
/// so the two being equal is the marker — there is no separate column for it.
///
/// It decides the inverse for a MOVE's created directories (a cross-FS move
/// creates destination folders exactly like a copy does). Reading those as a
/// move's usual restore renames a directory onto itself: a no-op the engine
/// counts as reversed, leaving the moved folder's empty skeleton at the
/// destination. A move of an existing directory records source ≠ dest, so it
/// keeps its restore.
fn is_created_in_place(unit: &RollbackUnit) -> bool {
    unit.dest_volume_id.as_deref() == Some(unit.source_volume_id.as_str())
        && unit.dest_path.as_deref() == Some(unit.source_path.as_path())
}

/// The inverse for one item, the ONE decision both the reversal and the dispatch
/// summary read. A directory the operation created in place is taken away again
/// whatever the op kind; everything else follows the kind's table.
fn item_action(kind: OpKind, entry_type: EntryType, created_in_place: bool) -> Option<InverseAction> {
    if entry_type == EntryType::Dir && created_in_place {
        return Some(InverseAction::RemoveDirIfEmpty);
    }
    inverse_action(kind, entry_type)
}

/// Whether a rollback has any inverse for this row at all: [`item_action`]'s answer,
/// for readers that only need yes or no (the tree's row roles, `crate::tree`).
pub(crate) fn has_inverse(kind: OpKind, entry_type: EntryType, created_in_place: bool) -> bool {
    item_action(kind, entry_type, created_in_place).is_some()
}

fn inverse_action(kind: OpKind, entry_type: EntryType) -> Option<InverseAction> {
    match kind {
        OpKind::Copy => Some(match entry_type {
            EntryType::File => InverseAction::RemoveCopyIfOriginalStays,
            EntryType::Dir => InverseAction::RemoveDirIfEmpty,
        }),
        OpKind::CreateFile | OpKind::ArchiveEdit => Some(InverseAction::RemoveFileIfUnchanged),
        OpKind::CreateFolder => Some(InverseAction::RemoveDirIfEmpty),
        OpKind::Move | OpKind::Trash | OpKind::Rename => Some(InverseAction::RestoreMove),
        OpKind::Tag => Some(InverseAction::RestoreTags),
        // Delete is never rollbackable (gated before we reach items).
        OpKind::Delete => None,
    }
}

/// The verdict of rechecking an item against its recorded snapshot.
///
/// Shared with the in-flight reversals in `cmdr_operations::reversal`, so a
/// cancel-and-roll-back and a Roll back from history agree on what "this file
/// changed" means. ❌ Don't fork a second definition of that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotVerdict {
    /// Every recorded field verified equal against a present live value.
    Match,
    /// A recorded field's live counterpart differs — the file changed.
    Drift,
    /// A recorded field's live counterpart is absent, or nothing was recorded —
    /// unprovable, so fail safe.
    Unverifiable,
}

/// Recheck a live entry against the recorded snapshot. Every snapshot field
/// that was recorded (`Some`) must have a present, equal live value; a recorded
/// field whose live counterpart is absent is Unverifiable (fail safe). At least
/// one field must have been recorded and verified, else there's nothing to prove
/// identity on ⇒ Unverifiable. So a copy leaf that recorded only size (a volume
/// leaf whose landed mtime wasn't read back) still verifies on size, while an item whose only
/// recorded field (mtime) is absent live (MTP/SMB) is Unverifiable and skipped.
///
/// Takes the two live fields rather than a `FileEntry`: it reads exactly these
/// two of that type's 28 fields, and the local in-flight reversals hold a
/// `std::fs::Metadata` they'd otherwise have to inflate into a serde/specta
/// struct just to ask this question.
pub fn verify_snapshot(
    snap_size: Option<i64>,
    snap_mtime: Option<i64>,
    live_size: Option<u64>,
    live_mtime: Option<u64>,
) -> SnapshotVerdict {
    let mut verified_any = false;
    if let Some(sm) = snap_mtime {
        match live_mtime {
            None => return SnapshotVerdict::Unverifiable,
            Some(lm) => {
                if lm as i64 != sm {
                    return SnapshotVerdict::Drift;
                }
                verified_any = true;
            }
        }
    }
    if let Some(ss) = snap_size {
        match live_size {
            None => return SnapshotVerdict::Unverifiable,
            Some(ls) => {
                if ls as i64 != ss {
                    return SnapshotVerdict::Drift;
                }
                verified_any = true;
            }
        }
    }
    if verified_any {
        SnapshotVerdict::Match
    } else {
        SnapshotVerdict::Unverifiable
    }
}

/// What a run did with the rows it streamed: each one reversed, skipped, or left out
/// by its selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
struct RunTally {
    reversed: u64,
    skipped: u64,
    left_out: u64,
}

/// Resolve the state the original op lands in from the run tally. `before` is the
/// state the op held when this run started.
///
/// A run that STOPPED (`stopped`: a cancel, or a journal page it couldn't read, so
/// it never saw the rest of the op) with nothing reversed leaves
/// the op as it found it: a clean retry, or still partial when an earlier run
/// already reversed some of it. A run that stopped after reversing something is
/// partial. A run that attempted every item resolves by outcome: no skips ⇒
/// `RolledBack` (including a vacuously-empty op); any skip (drift, unverifiable,
/// occupied target) ⇒ `PartiallyRolledBack`, even if nothing could be reversed, and
/// "Finish" re-attempts those items with a fresh recheck. The one exception is an
/// empty pass over a partial op: it found nothing to retry, which proves nothing
/// came back, so the op stays partial.
///
/// A row the run's selection LEFT OUT still awaits reversal, so like a skip it keeps
/// the op partial: "Roll back the rest" picks it up. Kani proves `RolledBack` never
/// comes with a skip or a left-out row (`rollback/proofs.rs`).
fn resolve_final_state(tally: RunTally, stopped: bool, before: RollbackState) -> RollbackState {
    let RunTally {
        reversed,
        skipped,
        left_out,
    } = tally;
    let resuming = before == RollbackState::PartiallyRolledBack;
    if stopped {
        if reversed == 0 {
            untouched_state(before)
        } else {
            RollbackState::PartiallyRolledBack
        }
    } else if skipped > 0 || left_out > 0 || (resuming && reversed == 0) {
        RollbackState::PartiallyRolledBack
    } else {
        RollbackState::RolledBack
    }
}

/// The state an op returns to when a rollback run changed nothing: still partial if
/// an earlier run reversed some of it, else a clean retry.
fn untouched_state(before: RollbackState) -> RollbackState {
    if before == RollbackState::PartiallyRolledBack {
        RollbackState::PartiallyRolledBack
    } else {
        RollbackState::Rollbackable
    }
}

// ── The planner: the item loop ───────────────────────────────────────────────

/// Reverse `original` as its inverse operation, streaming `rollback_unit` rows
/// `seq DESC` and rechecking each against its snapshot. Journals the inverse op
/// (with `rolls_back_op_id = original.op_id`) directly through `writer`, marks the
/// original's items `rolled_back`/`skipped`, and resolves the original's
/// `rollback_state`. Returns the tally.
///
/// `runner` performs each decided act and carries the loop's three live answers:
/// stop, pause, and progress. It's polled BETWEEN items (a rollback is stoppable
/// and pausable like any op), and a run that stops keeps what it reversed and
/// leaves the rest untouched for a retry.
///
/// This is the awaitable core the managed rollback entry point spawns; it takes only
/// the volumes, the `writer` (which yields the read connection via its
/// db path), and the runner, so it's driven directly in tests without a live
/// manager/runtime.
pub async fn execute_rollback(
    volumes: &dyn VolumeLookup,
    writer: &OperationLogWriter,
    original: &OperationRow,
    inverse_op_id: &str,
    initiator: Initiator,
    runner: &dyn RollbackRunner,
) -> RollbackReport {
    execute_rollback_with_selection(volumes, writer, original, inverse_op_id, initiator, None, runner).await
}

/// [`execute_rollback`] over only the rows `selection` takes (`None` takes every row,
/// exactly as [`execute_rollback`]). The engine normalizes the selection again here
/// (❌ never trusts what it was handed) and decides each RAW row before resolving its
/// paths, so a row left out costs one cached lookup and is never verified, acted on,
/// or recorded. Every row it takes goes through the same guards as an unfiltered
/// run's. Left-out rows keep the op `partially_rolled_back`.
///
/// A selection the run can't resolve (the summary went missing, a read failed)
/// reverses nothing and leaves the op as it found it.
#[allow(
    clippy::too_many_arguments,
    reason = "execute_rollback's arguments plus the selection it narrows to"
)]
pub async fn execute_rollback_with_selection(
    volumes: &dyn VolumeLookup,
    writer: &OperationLogWriter,
    original: &OperationRow,
    inverse_op_id: &str,
    initiator: Initiator,
    selection: Option<&RollbackSelection>,
    runner: &dyn RollbackRunner,
) -> RollbackReport {
    let inv_kind = inverse_kind(original.kind);

    // Open the inverse op's journal row (Running). It carries the same volumes as
    // the original and links back via `rolls_back_op_id` (drives crash-reconcile).
    if let Err(e) = writer.open_operation(OpenOperation {
        op_id: inverse_op_id.into(),
        kind: inv_kind,
        initiator,
        source_volume_id: original.source_volume_id.clone(),
        dest_volume_id: original.dest_volume_id.clone(),
        item_count: inverse_planned_item_count(original),
        started_at: super::now_secs(),
        rolls_back_op_id: Some(original.op_id.as_str().into()),
        execution_status: ExecutionStatus::Running,
    }) {
        log::warn!(target: "operation_log", "rollback: open inverse op failed: {e}");
    }

    let mut acc = RunAcc::default();
    let mut left_out: u64 = 0;
    let mut canceled = false;
    // A page the journal couldn't hand over (busy past its timeout, an undecodable
    // row, an I/O error). The run never saw what lies past it, so it ends like a
    // stop: keep what it reversed, leave the rest for a retry, never claim the op is
    // fully undone.
    let mut read_failed = false;

    let conn = match open_read_connection(writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback: read connection failed: {e}");
            return abandon_run(writer, original, inverse_op_id, inv_kind);
        }
    };
    let mut filter: Option<RollbackFilter> = match selection.map(|wire| narrow(&conn, &original.op_id, wire)) {
        None | Some(Ok(Narrowed::Everything)) => None,
        Some(Ok(Narrowed::Some(filter))) => Some(*filter),
        Some(Ok(Narrowed::Refused(refusal))) => {
            log::warn!(target: "operation_log", "rollback: the selection can't run: {refusal:?}");
            return abandon_run(writer, original, inverse_op_id, inv_kind);
        }
        Some(Err(e)) => {
            log::warn!(target: "operation_log", "rollback: the selection didn't resolve: {e}");
            return abandon_run(writer, original, inverse_op_id, inv_kind);
        }
    };

    // The bar's totals before the first act, so a reversal has no scanning phase and
    // its bar means something from the first frame: one aggregate off the journal, or
    // a selection's counts off the tree summary. Both count exactly the rows the loop
    // credits (`credits_progress`).
    let totals = match &filter {
        Some(filter) => filter.progress_totals(),
        None => read_rollback_progress_totals(&conn, &original.op_id, original.kind).unwrap_or_default(),
    };
    let mut stand = ProgressStand::over(totals);

    // Reverse in two phases, matching `CopyTransaction::rollback`: first every
    // FILE (streamed `seq DESC`, so the 1M-row list is never materialized), then
    // the DIRECTORY rows deepest-first — a dir can only be removed once its
    // contents are gone, and pure `seq DESC` puts deep dirs (highest seq) before
    // the files they hold. Dirs are a small fraction of an op (interning shares
    // them), so buffering just the dir rows stays bounded.
    let mut deferred_dirs: Vec<RollbackUnit> = Vec::new();
    let mut before = i64::MAX;
    'pages: loop {
        // Also asked per page, not only per item: a narrow selection over a big op
        // streams page after page it keeps nothing of (2M rows take about 3 s), and a
        // stop in there must land on the click.
        if runner.should_stop() {
            canceled = true;
            break;
        }
        let rows = match read_rollback_rows_page(&conn, &original.op_id, before, ROLLBACK_PAGE) {
            Ok(rows) => rows,
            Err(e) => {
                log::warn!(target: "operation_log", "rollback: page read failed: {e}");
                read_failed = true;
                break;
            }
        };
        let Some(last) = rows.last() else { break };
        // Page on the raw rows, never the kept ones: a page the selection keeps none of
        // would otherwise read again forever.
        before = last.seq;
        let page = match admit_page(&conn, filter.as_mut(), &rows) {
            Ok(admitted) => {
                left_out += admitted.left_out;
                admitted.units
            }
            Err(e) => {
                log::warn!(target: "operation_log", "rollback: page read failed: {e}");
                read_failed = true;
                break;
            }
        };

        for unit in page {
            if runner.should_stop() {
                canceled = true;
                break 'pages;
            }
            if unit.entry_type == EntryType::Dir {
                deferred_dirs.push(unit);
                continue;
            }
            // E2E-only pacing, answered by the runner. In production it answers
            // `None` and nothing else happens. Under E2E it opens a known window per
            // item, which is what lets a spec watch a reversal run and press Cancel or
            // Pause inside it without staging thousands of files. It sits ABOVE the
            // two gates below so a click landing inside the window is honored for
            // THIS item rather than the next one. Only the FILE loop is paced: the
            // deferred-dir phase below is mostly empty leftovers, so slowing it would
            // buy a spec dead time rather than a window.
            if let Some(pace) = runner.pace_between_items() {
                crate::rt::time::sleep(pace).await;
            }
            // Park here, BEFORE the item is verified — never between "verified
            // unchanged" and the act, where a ten-minute pause would leave a stale
            // verification authorizing a destructive one. A stop wins over a pause,
            // so re-ask the moment the park lets go.
            runner.wait_while_paused().await;
            if runner.should_stop() {
                canceled = true;
                break 'pages;
            }
            let credits = credits_progress(original.kind, &unit);
            if credits {
                stand.announce(runner, Some(&unit));
            }
            let result = reverse_item(volumes, runner, original.kind, &unit).await;
            // A stop observed INSIDE an item (a cancel during a cross-volume stream)
            // ends the run instead of recording a skip: nothing was left behind for a
            // reason the skip column exists to explain, and an unrecorded item is one
            // an idempotent retry simply re-attempts.
            if matches!(result, ItemResult::Skipped(_)) && runner.should_stop() {
                canceled = true;
                break 'pages;
            }
            if credits {
                stand.credit(&unit);
            }
            acc.record(&unit, result);
        }
        // Flush this page's side-effects durably (bounded memory: never buffer the
        // whole op's inverse rows; and a crash mid-stream leaves the inverse op's
        // recorded outcomes for the reconcile to read).
        acc.flush(writer, inverse_op_id, &original.op_id);
    }

    // Phase two: the buffered directory rows, deepest path first (so a child dir is
    // removed before its parent). Skipped entirely if the run was canceled or a page
    // failed to read (the unread files may still sit inside these dirs). A trashed or
    // moved folder is a unit here too (a big trash can put 100,000 of them in this
    // sweep), so the sweep parks on a pause BEFORE each item is verified, as the file
    // loop does, polls the stop, and credits each folder unit on the bar.
    if !canceled && !read_failed {
        deferred_dirs.sort_by_key(|u| std::cmp::Reverse(u.source_path.components().count()));
        for unit in &deferred_dirs {
            if runner.should_stop() {
                canceled = true;
                break;
            }
            runner.wait_while_paused().await;
            if runner.should_stop() {
                canceled = true;
                break;
            }
            let credits = credits_progress(original.kind, unit);
            if credits {
                stand.announce(runner, Some(unit));
            }
            let result = reverse_item(volumes, runner, original.kind, unit).await;
            if credits {
                stand.credit(unit);
            }
            acc.record(unit, result);
        }
    }
    acc.flush(writer, inverse_op_id, &original.op_id);
    // The frame that lands on the total (or wherever a stop left it), so a bar that
    // was throttled mid-item still ends where the run ended.
    stand.announce(runner, None);

    let inv_status = if read_failed {
        ExecutionStatus::Failed
    } else if canceled {
        ExecutionStatus::Canceled
    } else {
        ExecutionStatus::Done
    };
    finalize_inverse(writer, inverse_op_id, inv_kind, inv_status, acc.totals(original));

    let stopped = canceled || read_failed;
    let tally = RunTally {
        reversed: acc.reversed,
        skipped: acc.skipped,
        left_out,
    };
    let final_state = resolve_final_state(tally, stopped, original.rollback_state);
    if let Err(e) = writer.set_rollback_state(&original.op_id, final_state, None) {
        log::warn!(target: "operation_log", "rollback: resolve original state failed: {e}");
    }

    RollbackReport {
        reversed: acc.reversed,
        skipped: acc.skipped,
        left_out,
        skips: acc.skips.into_breakdowns(),
        canceled,
        final_state,
    }
}

/// End a run that never streamed a row: the inverse op is `failed`, and the original
/// goes back to how it was, since nothing was reversed.
fn abandon_run(
    writer: &OperationLogWriter,
    original: &OperationRow,
    inverse_op_id: &str,
    inv_kind: OpKind,
) -> RollbackReport {
    finalize_inverse(
        writer,
        inverse_op_id,
        inv_kind,
        ExecutionStatus::Failed,
        InverseTotals::default(),
    );
    let final_state = untouched_state(original.rollback_state);
    let _ = writer.set_rollback_state(&original.op_id, final_state, None);
    RollbackReport {
        reversed: 0,
        skipped: 0,
        left_out: 0,
        skips: Vec::new(),
        canceled: false,
        final_state,
    }
}

/// One page's rows as the run takes them: the units it will act on, each path
/// resolved, and how many the selection left out. Resolves the whole page before any
/// item runs, so a page with a row it can't read fails as a unit, as it always has.
struct AdmittedPage {
    units: Vec<RollbackUnit>,
    left_out: u64,
}

fn admit_page(
    conn: &Connection,
    filter: Option<&mut RollbackFilter>,
    rows: &[OperationItemRow],
) -> Result<AdmittedPage, OperationLogStoreError> {
    let mut admitted = AdmittedPage {
        units: Vec::with_capacity(rows.len()),
        left_out: 0,
    };
    let mut filter = filter.map(|filter| filter.on(conn));
    for row in rows {
        if let Some(filter) = filter.as_mut()
            && !filter.admits(row)?
        {
            admitted.left_out += 1;
            continue;
        }
        admitted.units.push(rollback_unit_of(conn, row)?);
    }
    Ok(admitted)
}

/// Whether reversing `unit` moves the bar: every [`UnitRole::Selectable`] row, file
/// or folder unit. A folder a copy or move made around its files is a leftover the
/// sweep removes, not something a person is waiting on. The totals count the same
/// rows (`store::read_rollback_progress_totals`, and a selection's
/// `units_selectable` off the tree summary).
fn credits_progress(kind: OpKind, unit: &RollbackUnit) -> bool {
    let role = unit_role(
        kind,
        RowRole::RollbackUnit,
        unit.entry_type,
        is_created_in_place(unit),
        unit.outcome,
        unit.rollback_skip_reason,
    );
    role == UnitRole::Selectable
}

mod bookkeeping;
mod gate;
mod inverse;
mod order;
mod reconcile;
mod runner;
mod self_collision;
mod skips;
mod tags;
use bookkeeping::{InverseTotals, RunAcc, finalize_inverse, inverse_planned_item_count};
pub use gate::{
    InversePlan, InverseSummary, check_rollbackable, rollback_operation, rollback_operation_with_selection,
};
use inverse::{removal_target, reverse_item, vanished_item};
pub use order::undo_order;
pub use reconcile::reconcile_rolling_back_on_open;
use runner::ProgressStand;
pub use runner::{ActRefusal, InverseAct, RollbackProgress, RollbackRunner, VolumeLookup};
use self_collision::is_self_collision;
#[cfg(test)]
use self_collision::{SelfCollision, SpellingTwin, classify_self_collision, listing_confirms_twin};
pub use skips::{SkipBreakdown, SkipTally};
use tags::restore_tags;

// The end-to-end cells drive the write engine's real executor, so they live
// beside it: `crates/cmdr-operations/src/rollback/planner_tests/`.
#[cfg(test)]
mod decision_tests;
#[cfg(kani)]
mod proofs;
