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
//!   a case-only self-collision (restoring `dog.JPG` → `dog.jpg` on a
//!   case-insensitive volume, where the "occupant" IS the same inode) — that's not
//!   a real collision, so it proceeds ([`is_self_collision`]).
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
//! `write_operations` can reach. See `rollback/runner.rs`.
//!
//! Reversal streams the original op's `rollback_unit` rows `seq DESC` through a
//! paged cursor (`store::read_rollback_units_page`), so a 1M-item op never
//! materializes its list. The `seq DESC` order removes copied files before the
//! `entry_type = dir` rows that held them. The inverse operation is itself
//! journaled with `rolls_back_op_id` set (so it appears in history and drives the
//! crash-reconcile), computing its own eligibility — a move/rename undo is
//! rollbackable again (redo), a delete-the-copies undo is not.

use std::path::{Path, PathBuf};

use crate::file_system::VolumeManager;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::VolumeError;

use super::store::{
    OperationRow, RollbackUnit, fold_name, open_read_connection, ops_in_rolling_back, read_inverse_op, read_operation,
    read_operation_items, read_rollback_file_totals, read_rollback_units_page,
};
use super::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, SkipReason,
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
}

impl SkipReason {
    /// `AlreadyGone` means the end state we wanted already holds (idempotent
    /// re-issue), so it counts as reversed, not as a partial-blocking skip.
    pub(crate) fn counts_as_reversed(self) -> bool {
        matches!(self, SkipReason::AlreadyGone)
    }
}

/// The outcome of reversing one item. Shared with the in-flight reversals
/// (`file_system::write_operations::reversal`) so both report the same verdicts.
pub(crate) enum ItemResult {
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
        // Delete is gated op-level (never rollbackable); its inverse is unreachable.
        OpKind::Delete => OpKind::Delete,
    }
}

/// The shape of the inverse for one item, derived purely from the op kind and the
/// item's entry type (`DETAILS.md` § "Per-kind inverse table").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InverseAction {
    /// Remove the created/copied file at its dest, only if it still matches the
    /// snapshot (copy file, create_file, compress archive).
    RemoveFileIfUnchanged,
    /// Remove the created/copied directory at its dest, only if still empty (copy's
    /// created dirs, create_folder).
    RemoveDirIfEmpty,
    /// Move the item back from where it ended up (dest) to its original (source):
    /// move, trash restore, rename-back.
    RestoreMove,
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

fn inverse_action(kind: OpKind, entry_type: EntryType) -> Option<InverseAction> {
    match kind {
        OpKind::Copy => Some(match entry_type {
            EntryType::File => InverseAction::RemoveFileIfUnchanged,
            EntryType::Dir => InverseAction::RemoveDirIfEmpty,
        }),
        OpKind::CreateFile | OpKind::ArchiveEdit => Some(InverseAction::RemoveFileIfUnchanged),
        OpKind::CreateFolder => Some(InverseAction::RemoveDirIfEmpty),
        OpKind::Move | OpKind::Trash | OpKind::Rename => Some(InverseAction::RestoreMove),
        // Delete is never rollbackable (gated before we reach items).
        OpKind::Delete => None,
    }
}

/// The verdict of rechecking an item against its recorded snapshot.
///
/// Shared with the in-flight reversals in `file_system::write_operations`, so a
/// cancel-and-roll-back and a Roll back from history agree on what "this file
/// changed" means. ❌ Don't fork a second definition of that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotVerdict {
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
/// identity on ⇒ Unverifiable. So a copy leaf that recorded only size (volume
/// transfers don't carry mtime) still verifies on size, while an item whose only
/// recorded field (mtime) is absent live (MTP/SMB) is Unverifiable and skipped.
///
/// Takes the two live fields rather than a `FileEntry`: it reads exactly these
/// two of that type's 28 fields, and the local in-flight reversals hold a
/// `std::fs::Metadata` they'd otherwise have to inflate into a serde/specta
/// struct just to ask this question.
pub(crate) fn verify_snapshot(
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

/// Lower-case each path component (Unicode + NFC via [`fold_name`]) for a
/// case-insensitive path comparison — the trait-level fallback of the self-
/// collision guard on backends without inodes (MTP).
fn fold_path(path: &Path) -> PathBuf {
    path.components()
        .map(|c| match c {
            std::path::Component::Normal(name) => PathBuf::from(fold_name(&name.to_string_lossy())),
            other => PathBuf::from(other.as_os_str()),
        })
        .collect()
}

/// Is the entry occupying the restore target actually the SAME entry we're
/// restoring — a case-only or identity rename, not a real collision?
///
/// Where real inodes exist (`LocalPosixVolume`), same inode ⇒ same entry (an inode
/// match already implies the same device). On the trait level (MTP/SMB have no
/// inode) the fallback compares case-normalized paths, but ONLY within one volume:
/// a target that differs from the source only by case is the self-collision (the
/// case-insensitive volume folded `dog.jpg` and `dog.JPG` onto one entry). A
/// cross-volume restore to the same relative path is NEVER self (the occupant is a
/// genuinely different file on a different device), so `same_volume` gates the
/// path-fold fallback — without it, a move-back to `/a.txt` on another volume would
/// wrongly overwrite a new `/a.txt` the user created there.
fn is_self_collision(same_volume: bool, from: &Path, to: &Path, from_entry: &FileEntry, occupant: &FileEntry) -> bool {
    if let (Some(a), Some(b)) = (from_entry.inode, occupant.inode) {
        return a == b;
    }
    same_volume && fold_path(from) == fold_path(to)
}

/// Resolve the state the original op lands in from the run tally.
///
/// `Rollbackable` (a clean retry) is reserved for a run that was CANCELED with
/// nothing reversed — a deliberate stop, not a completed attempt. A run that
/// actually attempted the items resolves by outcome: no skips ⇒ `RolledBack`
/// (including a vacuously-empty op); any skip (drift, unverifiable, occupied
/// target) ⇒ `PartiallyRolledBack`, even if nothing could be reversed — the honest
/// "we couldn't fully undo this", since those skips won't clear on a retry.
fn resolve_final_state(reversed: u64, skipped: u64, canceled: bool) -> RollbackState {
    if canceled {
        // A deliberate stop: clean retry if nothing ran, else a partial that
        // reversed what it managed before the stop.
        if reversed == 0 {
            RollbackState::Rollbackable
        } else {
            RollbackState::PartiallyRolledBack
        }
    } else if skipped == 0 {
        RollbackState::RolledBack
    } else {
        RollbackState::PartiallyRolledBack
    }
}

// ── The op-level gate (used by the managed rollback entry point + tested here) ─────────────

/// Check whether `op` may be rolled back right now: its stored `rollback_state`
/// and (for a connected-volume requirement) whether every volume it touches is
/// registered. Returns `Ok(())` to proceed, or the typed refusal. Does NOT mutate
/// anything — the caller sets `rolling_back` only on a successful spawn.
pub fn check_rollbackable(vm: &VolumeManager, op: &OperationRow) -> Result<(), RollbackRefusal> {
    match op.rollback_state {
        RollbackState::RollingBack => return Err(RollbackRefusal::AlreadyRollingBack),
        RollbackState::RolledBack => return Err(RollbackRefusal::AlreadyRolledBack),
        RollbackState::NotRollbackable => {
            let reason = op
                .not_rollbackable_reason
                .unwrap_or(NotRollbackableReason::PermanentDelete);
            return Err(RollbackRefusal::NotRollbackable(reason));
        }
        // Rollbackable or PartiallyRolledBack (a resumed rollback): proceed.
        RollbackState::Rollbackable | RollbackState::PartiallyRolledBack => {}
    }

    // Every volume the op touches must be connected NOW (the cross-volume gate).
    for volume_id in [op.source_volume_id.as_deref(), op.dest_volume_id.as_deref()]
        .into_iter()
        .flatten()
    {
        if vm.get(volume_id).is_none() {
            return Err(RollbackRefusal::VolumeUnavailable {
                volume_id: volume_id.to_string(),
            });
        }
    }
    Ok(())
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
/// the `VolumeManager`, the `writer` (which yields the read connection via its
/// db path), and the runner, so it's driven directly in tests without a live
/// manager/runtime.
pub async fn execute_rollback(
    vm: &VolumeManager,
    writer: &OperationLogWriter,
    original: &OperationRow,
    inverse_op_id: &str,
    initiator: Initiator,
    runner: &dyn RollbackRunner,
) -> RollbackReport {
    let inv_kind = inverse_kind(original.kind);

    // Open the inverse op's journal row (Running). It carries the same volumes as
    // the original and links back via `rolls_back_op_id` (drives crash-reconcile).
    if let Err(e) = writer.open_operation(OpenOperation {
        op_id: inverse_op_id.to_string(),
        kind: inv_kind,
        initiator,
        source_volume_id: original.source_volume_id.clone(),
        dest_volume_id: original.dest_volume_id.clone(),
        item_count: original.items_done,
        started_at: super::now_secs(),
        rolls_back_op_id: Some(original.op_id.clone()),
        execution_status: ExecutionStatus::Running,
    }) {
        log::warn!(target: "operation_log", "rollback: open inverse op failed: {e}");
    }

    let mut acc = RunAcc::default();
    let mut canceled = false;

    let conn = match open_read_connection(writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback: read connection failed: {e}");
            // Can't stream — resolve back to rollbackable (nothing reversed).
            finalize_inverse(
                writer,
                inverse_op_id,
                inv_kind,
                ExecutionStatus::Failed,
                InverseTotals::default(),
            );
            let _ = writer.set_rollback_state(&original.op_id, RollbackState::Rollbackable, None);
            return RollbackReport {
                reversed: 0,
                skipped: 0,
                skips: Vec::new(),
                canceled: false,
                final_state: RollbackState::Rollbackable,
            };
        }
    };

    // The bar's totals, straight off the journal: one indexed count before the
    // first act, so a reversal has no scanning phase and its bar means something
    // from the first frame. Only FILE rows count — the deferred dir phase below
    // removes leftovers rather than moving anything a person is waiting on.
    let mut stand = ProgressStand::over(read_rollback_file_totals(&conn, &original.op_id).unwrap_or_default());

    // Reverse in two phases, matching `CopyTransaction::rollback`: first every
    // FILE (streamed `seq DESC`, so the 1M-row list is never materialized), then
    // the created DIRECTORY rows deepest-first — a dir can only be removed once its
    // contents are gone, and pure `seq DESC` puts deep dirs (highest seq) before
    // the files they hold. Dirs are a small fraction of an op (interning shares
    // them), so buffering just the dir rows stays bounded.
    let mut deferred_dirs: Vec<RollbackUnit> = Vec::new();
    let mut before = i64::MAX;
    'pages: loop {
        let page = match read_rollback_units_page(&conn, &original.op_id, before, ROLLBACK_PAGE) {
            Ok(p) => p,
            Err(e) => {
                log::warn!(target: "operation_log", "rollback: page read failed: {e}");
                break;
            }
        };
        if page.is_empty() {
            break;
        }
        before = page.last().map(|u| u.seq).unwrap_or(before);

        for unit in page {
            if runner.should_stop() {
                canceled = true;
                break 'pages;
            }
            if unit.entry_type == EntryType::Dir {
                deferred_dirs.push(unit);
                continue;
            }
            // E2E-only pacing. In production both the env var and the IPC override are
            // unset, so this is one atomic load plus one `LazyLock` deref and nothing
            // else happens. Under E2E it opens a known window per item, which is what
            // lets a spec watch a reversal run and press Cancel or Pause inside it
            // without staging thousands of files. It sits ABOVE the two gates below so
            // a click landing inside the window is honored for THIS item rather than
            // the next one. Only the FILE loop is paced: the deferred-dir phase below
            // removes empty leftovers, so slowing it would buy a spec dead time rather
            // than a window.
            if let Some(ms) = crate::test_mode::effective_rollback_throttle_ms() {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
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
            stand.announce(runner, Some(&unit));
            let result = reverse_item(vm, runner, original.kind, &unit).await;
            // A stop observed INSIDE an item (a cancel during a cross-volume stream)
            // ends the run instead of recording a skip: nothing was left behind for a
            // reason the skip column exists to explain, and an unrecorded item is one
            // an idempotent retry simply re-attempts.
            if matches!(result, ItemResult::Skipped(_)) && runner.should_stop() {
                canceled = true;
                break 'pages;
            }
            stand.credit(&unit);
            acc.record(&unit, result);
        }
        // Flush this page's side-effects durably (bounded memory: never buffer the
        // whole op's inverse rows; and a crash mid-stream leaves the inverse op's
        // recorded outcomes for the reconcile to read).
        acc.flush(writer, inverse_op_id, &original.op_id);
    }
    // The frame that lands on the total (or wherever a stop left it), so a bar that
    // was throttled mid-item still ends where the run ended.
    stand.announce(runner, None);

    // Phase two: the buffered directory rows, deepest path first (so a child dir is
    // removed before its parent). Skipped entirely if the run was canceled — and it
    // polls the stop itself, so a reversal of a directory-heavy op stops on the
    // click rather than at the end of the sweep.
    if !canceled {
        deferred_dirs.sort_by_key(|u| std::cmp::Reverse(u.source_path.components().count()));
        for unit in &deferred_dirs {
            if runner.should_stop() {
                canceled = true;
                break;
            }
            let result = reverse_item(vm, runner, original.kind, unit).await;
            acc.record(unit, result);
        }
    }
    acc.flush(writer, inverse_op_id, &original.op_id);

    let inv_status = if canceled {
        ExecutionStatus::Canceled
    } else {
        ExecutionStatus::Done
    };
    finalize_inverse(writer, inverse_op_id, inv_kind, inv_status, acc.totals());

    let final_state = resolve_final_state(acc.reversed, acc.skipped, canceled);
    if let Err(e) = writer.set_rollback_state(&original.op_id, final_state, None) {
        log::warn!(target: "operation_log", "rollback: resolve original state failed: {e}");
    }

    RollbackReport {
        reversed: acc.reversed,
        skipped: acc.skipped,
        skips: acc.skips.into_breakdowns(),
        canceled,
        final_state,
    }
}

/// Reverse one item: derive the inverse action, recheck against the snapshot, and
/// (only if it verifies AND the target is clear) hand the decided act to the
/// runner. Every read here goes through the `Volume` trait, so local and remote
/// verify uniformly.
async fn reverse_item(
    vm: &VolumeManager,
    runner: &dyn RollbackRunner,
    kind: OpKind,
    unit: &RollbackUnit,
) -> ItemResult {
    let action = if unit.entry_type == EntryType::Dir && is_created_in_place(unit) {
        // A directory the operation created, whatever the op kind (see
        // `is_created_in_place`): the inverse is to take it away again, never to
        // rename it onto itself.
        InverseAction::RemoveDirIfEmpty
    } else {
        match inverse_action(kind, unit.entry_type) {
            Some(action) => action,
            None => return ItemResult::Skipped(SkipReason::Failed),
        }
    };
    match action {
        InverseAction::RemoveFileIfUnchanged => remove_file_if_unchanged(vm, runner, unit).await,
        InverseAction::RemoveDirIfEmpty => remove_dir_if_empty(vm, runner, unit).await,
        InverseAction::RestoreMove => restore_move(vm, runner, unit).await,
    }
}

/// The (volume, path) a removal inverse targets: the item's dest (the copy /
/// created path), falling back to source when no dest was recorded.
fn removal_target(unit: &RollbackUnit) -> (String, PathBuf) {
    match (&unit.dest_volume_id, &unit.dest_path) {
        (Some(vol), Some(path)) => (vol.clone(), path.clone()),
        _ => (unit.source_volume_id.clone(), unit.source_path.clone()),
    }
}

async fn remove_file_if_unchanged(vm: &VolumeManager, runner: &dyn RollbackRunner, unit: &RollbackUnit) -> ItemResult {
    let (vol_id, path) = removal_target(unit);
    let Some(volume) = vm.get(&vol_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    let live = match volume.get_metadata(&path).await {
        Ok(entry) => entry,
        // Already gone ⇒ the desired end state already holds (idempotent).
        Err(VolumeError::NotFound(_)) => return ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => return ItemResult::Skipped(SkipReason::Failed),
    };
    match verify_snapshot(unit.size, unit.mtime, live.size, live.modified_at) {
        SnapshotVerdict::Match => match runner
            .perform(InverseAct::RemoveFile {
                volume: &volume,
                path: &path,
            })
            .await
        {
            Ok(()) => ItemResult::Reversed,
            Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
            Err(_) => ItemResult::Skipped(SkipReason::Failed),
        },
        SnapshotVerdict::Drift => ItemResult::Skipped(SkipReason::Drift),
        SnapshotVerdict::Unverifiable => ItemResult::Skipped(SkipReason::UnverifiablePrecondition),
    }
}

async fn remove_dir_if_empty(vm: &VolumeManager, runner: &dyn RollbackRunner, unit: &RollbackUnit) -> ItemResult {
    let (vol_id, path) = removal_target(unit);
    let Some(volume) = vm.get(&vol_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    if !volume.exists(&path).await {
        // Already removed ⇒ idempotent no-op.
        return ItemResult::Skipped(SkipReason::AlreadyGone);
    }
    // Only remove a directory the undo created if it's still empty — a file added
    // since must not be swept away. A `seq DESC` stream removes the dir's own
    // (unchanged) contents first, so a genuinely-restored tree is empty here.
    match volume.list_directory(&path, None).await {
        Ok(entries) if entries.is_empty() => match runner
            .perform(InverseAct::RemoveDir {
                volume: &volume,
                path: &path,
            })
            .await
        {
            Ok(()) => ItemResult::Reversed,
            Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
            Err(_) => ItemResult::Skipped(SkipReason::Failed),
        },
        Ok(_) => ItemResult::Skipped(SkipReason::DirNotEmpty),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}

async fn restore_move(vm: &VolumeManager, runner: &dyn RollbackRunner, unit: &RollbackUnit) -> ItemResult {
    if unit.outcome != ItemOutcome::Done {
        return ItemResult::Skipped(SkipReason::Failed);
    }
    // Restore moves the item back FROM where it landed (dest) TO its original
    // (source). Both must be present in the row (move/trash/rename all record a
    // dest); a row without one is a journal shape bug — skip safe.
    let (Some(from_vol_id), Some(from_path)) = (&unit.dest_volume_id, &unit.dest_path) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    let to_vol_id = &unit.source_volume_id;
    let to_path = &unit.source_path;

    let Some(from_volume) = vm.get(from_vol_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    let Some(to_volume) = vm.get(to_vol_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };

    // The thing to move back must still be where the op left it.
    let from_entry = match from_volume.get_metadata(from_path).await {
        Ok(e) => e,
        // Gone (trash emptied, item moved within trash, already restored) ⇒ skip.
        Err(VolumeError::NotFound(_)) => return ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => return ItemResult::Skipped(SkipReason::Failed),
    };
    // For a file, verify it hasn't changed since the op (dirs: existence only —
    // a subtree isn't cheaply verifiable, so existence + a clear target is the
    // contract). Drift / unverifiable ⇒ skip.
    if unit.entry_type == EntryType::File {
        match verify_snapshot(unit.size, unit.mtime, from_entry.size, from_entry.modified_at) {
            SnapshotVerdict::Match => {}
            SnapshotVerdict::Drift => return ItemResult::Skipped(SkipReason::Drift),
            SnapshotVerdict::Unverifiable => return ItemResult::Skipped(SkipReason::UnverifiablePrecondition),
        }
    }

    // Pinned non-destructive policy: never overwrite the restore target. If it's
    // occupied by a DIFFERENT entry, skip. A case-only self-collision (the target
    // IS the same inode/path-fold as what we're restoring) is not a real collision,
    // so restoring over it is safe — `force` lets the same-entry rename land where
    // a case-insensitive volume reports the target "exists".
    let same_volume = from_vol_id == to_vol_id;
    let mut force = false;
    if let Ok(occupant) = to_volume.get_metadata(to_path).await {
        if is_self_collision(same_volume, from_path, to_path, &from_entry, &occupant) {
            force = true;
        } else {
            return ItemResult::Skipped(SkipReason::RestoreTargetOccupied);
        }
    }

    // A cross-volume restore is always per-leaf (directories are recorded per file,
    // and cross-volume can't be a self-collision, so the target is clear); a
    // directory row reaching the streaming path would be a journal-shape bug, so
    // refuse it here rather than asking the executor to guess.
    if !same_volume && from_entry.is_directory {
        return ItemResult::Skipped(SkipReason::Failed);
    }
    match runner
        .perform(InverseAct::Restore {
            from: &from_volume,
            from_path,
            to: &to_volume,
            to_path,
            same_volume,
            force,
        })
        .await
    {
        Ok(()) => ItemResult::Reversed,
        Err(VolumeError::AlreadyExists(_)) => ItemResult::Skipped(SkipReason::RestoreTargetOccupied),
        Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}

// ── Entry point: gate + set rolling_back + spawn (state machine) ──────────────

/// Everything the caller needs to actually run the inverse op after the gate
/// passed and `rolling_back` was set: the original op's row, the fresh id for its
/// inverse, and where the reversal will act.
#[derive(Debug, Clone)]
pub struct InversePlan {
    pub original: OperationRow,
    pub inverse_op_id: String,
    pub summary: InverseSummary,
}

/// Where a reversal will act, so its queue row can say so instead of sitting
/// nameless. Read off the newest journal row at dispatch — one row, never the
/// list.
#[derive(Debug, Clone, Default)]
pub struct InverseSummary {
    /// The folder the reversal takes items FROM (for a removal, the folder it
    /// cleans; for a created directory, that directory itself).
    pub from: Option<String>,
    /// Where a restore puts them back. `None` for a removal inverse, which has
    /// nowhere to put anything.
    pub to: Option<String>,
}

/// Summarize what reversing `op` will do, from its newest `rollback_unit` row.
///
/// The newest row is the cheapest honest sample: for a copy it's the created
/// directory (the exact folder the undo cleans), and for anything else it's a
/// leaf whose parent folder is the place the user is watching.
fn summarize_inverse(conn: &rusqlite::Connection, op: &OperationRow) -> InverseSummary {
    let Ok(page) = read_rollback_units_page(conn, &op.op_id, i64::MAX, 1) else {
        return InverseSummary::default();
    };
    let Some(unit) = page.first() else {
        return InverseSummary::default();
    };
    let (_, acting_on) = removal_target(unit);
    // A directory row IS the thing being removed; a file row names its folder.
    let from = if unit.entry_type == EntryType::Dir {
        Some(acting_on)
    } else {
        acting_on.parent().map(Path::to_path_buf)
    };
    let to = match inverse_action(op.kind, unit.entry_type) {
        Some(InverseAction::RestoreMove) => unit.source_path.parent().map(Path::to_path_buf),
        _ => None,
    };
    InverseSummary {
        from: from.map(|p| p.to_string_lossy().into_owned()),
        to: to.map(|p| p.to_string_lossy().into_owned()),
    }
}

/// The entry point of the `rolling_back` state machine (`DETAILS.md` § "The
/// `rolling_back` state machine + startup reconcile"): read the op, gate it
/// (unknown / already rolling back / not rollbackable / a volume disconnected),
/// then — as late as possible — set it `rolling_back` and hand the plan to
/// `spawn`, which launches the inverse operation. If `spawn` fails synchronously
/// (a volume dropped between the gate and the spawn, so the inverse never
/// starts), reset `rolling_back → rollbackable` in the SAME call before
/// returning the error, so the op isn't wedged behind the `AlreadyRollingBack`
/// guard and an immediate retry is accepted. The double-rollback guard is
/// automatic: a second call reads the op as `rolling_back` and refuses.
///
/// `spawn` is injected so the manager wiring (which lives in `write_operations`,
/// where the `OperationManager` is reachable) supplies the real managed-op spawn,
/// while tests drive the gate/reset logic directly.
pub fn rollback_operation<F>(
    vm: &VolumeManager,
    writer: &OperationLogWriter,
    op_id: &str,
    spawn: F,
) -> Result<InversePlan, RollbackRefusal>
where
    F: FnOnce(&InversePlan) -> Result<(), RollbackRefusal>,
{
    let conn = open_read_connection(writer.db_path()).map_err(|_| RollbackRefusal::UnknownOperation)?;
    let op = read_operation(&conn, op_id)
        .map_err(|_| RollbackRefusal::UnknownOperation)?
        .ok_or(RollbackRefusal::UnknownOperation)?;

    check_rollbackable(vm, &op)?;

    let summary = summarize_inverse(&conn, &op);
    drop(conn);

    let plan = InversePlan {
        original: op,
        inverse_op_id: super::new_operation_id(),
        summary,
    };
    // Set `rolling_back` as late as possible — right before the spawn — to shrink
    // the window in which a crash leaves it set with no inverse row (the reconcile
    // resolves that anyway, straight back to rollbackable).
    if let Err(e) = writer.set_rollback_state(op_id, RollbackState::RollingBack, None) {
        log::warn!(target: "operation_log", "rollback: set rolling_back failed: {e}");
    }
    match spawn(&plan) {
        Ok(()) => Ok(plan),
        Err(refusal) => {
            // The inverse never started — undo the `rolling_back` mark so a retry
            // isn't refused, BEFORE returning the typed error.
            if let Err(e) = writer.set_rollback_state(op_id, RollbackState::Rollbackable, None) {
                log::warn!(target: "operation_log", "rollback: reset after failed spawn failed: {e}");
            }
            Err(refusal)
        }
    }
}

// ── Startup reconcile: resolve ops left mid-rollback by a crash ───────────────

/// On open, resolve every operation left `rolling_back` by a crash mid-rollback
/// deterministically:
///
/// - An **inverse op row exists** (crashed mid-stream, so it's unfinalized):
///   reconcile from its recorded per-item outcomes — `partially_rolled_back` if it
///   durably reversed anything (an item with outcome `done`), else back to
///   `rollbackable`.
/// - **No inverse op row** (crashed after setting `rolling_back` but before/at the
///   spawn — the Finding-3 window): straight back to `rollbackable`; nothing ran.
///
/// Either way a re-issued rollback resumes safely — every per-item inverse is an
/// idempotent recheck-then-act, so already-reversed items no-op. Called once at
/// [`start`](super::start), beside the migration-ladder open path.
pub fn reconcile_rolling_back_on_open(writer: &OperationLogWriter) {
    let conn = match open_read_connection(writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback reconcile: read connection failed: {e}");
            return;
        }
    };
    let stuck = match ops_in_rolling_back(&conn) {
        Ok(ops) => ops,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback reconcile: query failed: {e}");
            return;
        }
    };
    for op in stuck {
        let resolved = reconcile_one(&conn, &op.op_id);
        if let Err(e) = writer.set_rollback_state(&op.op_id, resolved, None) {
            log::warn!(target: "operation_log", "rollback reconcile: set state for {} failed: {e}", op.op_id);
        } else {
            log::info!(target: "operation_log", "rollback reconcile: {} left rolling_back ⇒ {resolved:?}", op.op_id);
        }
    }
}

/// The reconcile verdict for one stuck op (see [`reconcile_rolling_back_on_open`]).
fn reconcile_one(conn: &rusqlite::Connection, op_id: &str) -> RollbackState {
    match read_inverse_op(conn, op_id) {
        Ok(Some(inverse)) => {
            // Read a bounded prefix of the inverse's items: any `done` means
            // something was durably reversed ⇒ partial; none ⇒ back to rollbackable.
            match read_operation_items(conn, &inverse.op_id, 10_000) {
                Ok(items) if items.iter().any(|i| i.outcome == ItemOutcome::Done) => RollbackState::PartiallyRolledBack,
                _ => RollbackState::Rollbackable,
            }
        }
        // No inverse op ever opened ⇒ nothing ran ⇒ cleanly rollbackable again.
        _ => RollbackState::Rollbackable,
    }
}

mod bookkeeping;
mod order;
mod runner;
mod skips;
use bookkeeping::{InverseTotals, RunAcc, finalize_inverse};
pub use order::undo_order;
use runner::ProgressStand;
pub use runner::{InverseAct, RollbackProgress, RollbackRunner};
pub use skips::SkipBreakdown;
pub(crate) use skips::SkipTally;

#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod undo_tests;
