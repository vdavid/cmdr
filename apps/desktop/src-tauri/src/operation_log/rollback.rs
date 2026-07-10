//! The rollback engine (M3): reverse a journaled operation as a set of inverse
//! per-item actions, each rechecked against its recorded snapshot before it
//! touches anything.
//!
//! **Data-safety-critical.** A rollback must never destroy data — least of all
//! data the user created AFTER the operation. Two independent guards enforce that
//! (D7):
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
//! Reversal streams the original op's `rollback_unit` rows `seq DESC` through a
//! paged cursor ([`store::read_rollback_units_page`]), so a 1M-item op never
//! materializes its list. The `seq DESC` order removes copied files before the
//! `entry_type = dir` rows that held them. The inverse operation is itself
//! journaled with `rolls_back_op_id` set (so it appears in history and drives the
//! crash-reconcile), computing its own eligibility — a move/rename undo is
//! rollbackable again (redo), a delete-the-copies undo is not.

use std::path::{Path, PathBuf};

use crate::file_system::VolumeManager;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError};

use super::capture::compute_eligibility;
use super::store::{
    OperationRow, RollbackUnit, fold_name, open_read_connection, ops_in_rolling_back, read_inverse_op, read_operation,
    read_operation_items, read_rollback_units_page,
};
use super::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, RowRole,
    SearchCoverage,
};
use super::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

/// Rows streamed per page from the journal — bounded so a huge op never
/// materializes its full item list in memory (D7).
const ROLLBACK_PAGE: u32 = 512;

/// Why a rollback request is refused at the operation level (before any item
/// runs). Typed across IPC/MCP — never a message string (`no-string-matching`).
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
    /// The op is already being rolled back — the double-rollback guard (Finding 7).
    AlreadyRollingBack,
    /// The op was already fully reversed; there's nothing to undo.
    AlreadyRolledBack,
    /// The op is not rollbackable; carries the stored reason (delete, overwrote,
    /// archive-overwrite, zip-edit-unsupported, journal-incomplete).
    NotRollbackable(NotRollbackableReason),
    /// A volume the rollback needs isn't currently connected. Computed at rollback
    /// time from mount state, never stored (D3); names the missing volume so the
    /// UI/agent can say "Volume 'Backup' is not connected".
    VolumeUnavailable { volume_id: String },
}

/// Why a single item was skipped rather than reversed. Feeds
/// `partially_rolled_back` and is recorded per item (D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A precondition field couldn't be verified (a recorded snapshot field whose
    /// live counterpart is absent, or no snapshot field at all) — fail safe, never
    /// operate on an unprovable file.
    UnverifiablePrecondition,
    /// The item changed since the op (size/mtime drift) — never touch a changed file.
    Drift,
    /// The restore target is occupied by a DIFFERENT entry — the pinned
    /// non-destructive policy skips rather than overwrite (D7).
    RestoreTargetOccupied,
    /// A directory the undo would remove isn't empty (a file was added since) — the
    /// create-folder / copied-dir recheck (D3).
    DirNotEmpty,
    /// The thing to reverse is already gone (trash emptied, item already restored):
    /// the desired end state already holds, so this is an idempotent no-op success.
    AlreadyGone,
    /// A backend error prevented reversing this item.
    Failed,
}

impl SkipReason {
    /// `AlreadyGone` means the end state we wanted already holds (idempotent
    /// re-issue), so it counts as reversed, not as a partial-blocking skip.
    fn counts_as_reversed(self) -> bool {
        matches!(self, SkipReason::AlreadyGone)
    }
}

/// The outcome of reversing one item.
enum ItemResult {
    /// Reversed (or already in the desired end state).
    Reversed,
    /// Skipped, with the typed reason.
    Skipped(SkipReason),
}

/// What a rollback DISPATCH returns to the FE/MCP: the inverse op's id. The
/// reversal itself is an async managed op, so the caller polls the ORIGINAL op's
/// `rollback_state` until it leaves `rolling_back` to observe the terminal result
/// (the M5 "dispatch then poll" contract).
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
/// item's entry type (D7's per-kind table).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotVerdict {
    /// Every recorded field verified equal against a present live value.
    Match,
    /// A recorded field's live counterpart differs — the file changed.
    Drift,
    /// A recorded field's live counterpart is absent, or nothing was recorded —
    /// unprovable, so fail safe.
    Unverifiable,
}

/// Recheck a live entry against the recorded snapshot (D7). Every snapshot field
/// that was recorded (`Some`) must have a present, equal live value; a recorded
/// field whose live counterpart is absent is Unverifiable (fail safe). At least
/// one field must have been recorded and verified, else there's nothing to prove
/// identity on ⇒ Unverifiable. So a copy leaf that recorded only size (volume
/// transfers don't carry mtime) still verifies on size, while an item whose only
/// recorded field (mtime) is absent live (MTP/SMB) is Unverifiable and skipped.
fn verify_snapshot(snap_size: Option<i64>, snap_mtime: Option<i64>, live: &FileEntry) -> SnapshotVerdict {
    let mut verified_any = false;
    if let Some(sm) = snap_mtime {
        match live.modified_at {
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
        match live.size {
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
/// restoring — a case-only or identity rename, not a real collision (Finding 8)?
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
/// nothing reversed — a deliberate stop, not a completed attempt (D7). A run that
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

// ── The op-level gate (used by the M3c entry point + tested here) ─────────────

/// Check whether `op` may be rolled back right now: its stored `rollback_state`
/// and (for a connected-volume requirement) whether every volume it touches is
/// registered. Returns `Ok(())` to proceed, or the typed refusal. Does NOT mutate
/// anything — the caller sets `rolling_back` only on a successful spawn (D7).
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

    // Every volume the op touches must be connected NOW (cross-volume gate, D3).
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

// ── The executor ─────────────────────────────────────────────────────────────

/// Reverse `original` as its inverse operation, streaming `rollback_unit` rows
/// `seq DESC` and rechecking each against its snapshot. Journals the inverse op
/// (with `rolls_back_op_id = original.op_id`) directly through `writer`, marks the
/// original's items `rolled_back`/`skipped`, and resolves the original's
/// `rollback_state`. Returns the tally.
///
/// `cancel` is polled between items (a rollback is cancelable like any op): a
/// canceled run keeps what it reversed and records the rest as untouched.
///
/// This is the awaitable core the managed entry point (M3c) spawns; it takes only
/// the `VolumeManager` and the `writer` (which yields the read connection via its
/// db path), so it's driven directly in tests without a live manager/runtime.
pub async fn execute_rollback(
    vm: &VolumeManager,
    writer: &OperationLogWriter,
    original: &OperationRow,
    inverse_op_id: &str,
    initiator: Initiator,
    is_canceled: &(dyn Fn() -> bool + Sync),
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
            finalize_inverse(writer, inverse_op_id, inv_kind, ExecutionStatus::Failed, 0);
            let _ = writer.set_rollback_state(&original.op_id, RollbackState::Rollbackable, None);
            return RollbackReport {
                reversed: 0,
                skipped: 0,
                canceled: false,
                final_state: RollbackState::Rollbackable,
            };
        }
    };

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
            if is_canceled() {
                canceled = true;
                break 'pages;
            }
            if unit.entry_type == EntryType::Dir {
                deferred_dirs.push(unit);
                continue;
            }
            let result = reverse_item(vm, original.kind, &unit).await;
            acc.record(&unit, result);
        }
        // Flush this page's side-effects durably (bounded memory: never buffer the
        // whole op's inverse rows; and a crash mid-stream leaves the inverse op's
        // recorded outcomes for the reconcile to read).
        acc.flush(writer, inverse_op_id, &original.op_id);
    }

    // Phase two: the buffered directory rows, deepest path first (so a child dir is
    // removed before its parent). Skipped entirely if the run was canceled.
    if !canceled {
        deferred_dirs.sort_by_key(|u| std::cmp::Reverse(u.source_path.components().count()));
        for unit in &deferred_dirs {
            let result = reverse_item(vm, original.kind, unit).await;
            acc.record(unit, result);
        }
    }
    acc.flush(writer, inverse_op_id, &original.op_id);

    let inv_status = if canceled {
        ExecutionStatus::Canceled
    } else {
        ExecutionStatus::Done
    };
    finalize_inverse(writer, inverse_op_id, inv_kind, inv_status, acc.reversed);

    let final_state = resolve_final_state(acc.reversed, acc.skipped, canceled);
    if let Err(e) = writer.set_rollback_state(&original.op_id, final_state, None) {
        log::warn!(target: "operation_log", "rollback: resolve original state failed: {e}");
    }

    RollbackReport {
        reversed: acc.reversed,
        skipped: acc.skipped,
        canceled,
        final_state,
    }
}

/// Accumulates a rollback run's tally + the two journal side-effects (the inverse
/// op's item rows, and the original op's per-item outcome updates), so the driver
/// loop appends without wrestling closure borrows.
#[derive(Default)]
struct RunAcc {
    reversed: u64,
    skipped: u64,
    inverse_items: Vec<JournalItem>,
    original_outcomes: Vec<(i64, ItemOutcome)>,
    next_inverse_seq: i64,
}

impl RunAcc {
    fn record(&mut self, unit: &RollbackUnit, result: ItemResult) {
        let original_outcome = match &result {
            ItemResult::Reversed => ItemOutcome::RolledBack,
            ItemResult::Skipped(r) if r.counts_as_reversed() => ItemOutcome::RolledBack,
            ItemResult::Skipped(_) => ItemOutcome::Skipped,
        };
        if original_outcome == ItemOutcome::RolledBack {
            self.reversed += 1;
        } else {
            self.skipped += 1;
        }
        self.original_outcomes.push((unit.seq, original_outcome));
        // Journal what the inverse op did to this item: reversed ⇒ Done, skipped ⇒
        // Skipped, so reconcile can read "did anything durably reverse" off the
        // inverse op's rows.
        self.inverse_items
            .push(inverse_item_row(self.next_inverse_seq, unit, &result));
        self.next_inverse_seq += 1;
    }

    /// Persist and clear the batched side-effects: the inverse op's item rows and
    /// the original op's per-item outcome updates. Called per page so a huge
    /// rollback never buffers more than one page in memory, and a crash mid-stream
    /// leaves durable progress for the reconcile. The running tallies + `seq`
    /// counter persist across flushes.
    fn flush(&mut self, writer: &OperationLogWriter, inverse_op_id: &str, original_op_id: &str) {
        if !self.inverse_items.is_empty()
            && let Err(e) = writer.record_items(inverse_op_id, std::mem::take(&mut self.inverse_items))
        {
            log::warn!(target: "operation_log", "rollback: record inverse items failed: {e}");
        }
        if !self.original_outcomes.is_empty()
            && let Err(e) = writer.set_item_outcomes(original_op_id, std::mem::take(&mut self.original_outcomes))
        {
            log::warn!(target: "operation_log", "rollback: set original item outcomes failed: {e}");
        }
    }
}

/// Finalize the inverse op's journal row, computing its own eligibility (a
/// delete-the-copies undo is not rollbackable; a move/rename undo is — redo).
fn finalize_inverse(
    writer: &OperationLogWriter,
    inverse_op_id: &str,
    inv_kind: OpKind,
    execution_status: ExecutionStatus,
    reversed: u64,
) {
    // The inverse never overwrites (pinned Skip), so `any_overwrote = false`.
    let (state, reason) = compute_eligibility(inv_kind, false, None, false);
    if let Err(e) = writer.finalize_operation(FinalizeOperation {
        op_id: inverse_op_id.to_string(),
        execution_status,
        rollback_state: state,
        not_rollbackable_reason: reason,
        archive_subkind: None,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        ended_at: super::now_secs(),
        items_done: reversed,
        bytes_total: 0,
        dev_summary: None,
    }) {
        log::warn!(target: "operation_log", "rollback: finalize inverse op failed: {e}");
    }
}

/// Build the inverse op's journal row for one reversed/skipped item. The row's
/// source is the location the inverse op acted on — the dest of the original item
/// (the removed copy, or the location a restore-move brought back FROM), falling
/// back to source when no dest was recorded (create_file/folder record source ==
/// dest). Its outcome reflects reversed vs skipped.
fn inverse_item_row(seq: i64, unit: &RollbackUnit, result: &ItemResult) -> JournalItem {
    let outcome = match result {
        ItemResult::Reversed => ItemOutcome::Done,
        ItemResult::Skipped(r) if r.counts_as_reversed() => ItemOutcome::Done,
        ItemResult::Skipped(_) => ItemOutcome::Skipped,
    };
    let (act_vol, act_path) = removal_target(unit);
    let (dir, name) = split(&act_path);
    JournalItem {
        seq,
        entry_type: unit.entry_type,
        row_role: RowRole::RollbackUnit,
        source_volume_id: act_vol,
        source_dir: dir,
        source_name: name,
        dest_volume_id: None,
        dest_dir: None,
        dest_name: None,
        size: unit.size,
        mtime: unit.mtime,
        outcome,
        overwrote: false,
    }
}

fn split(path: &Path) -> (String, String) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    (dir, name)
}

/// Reverse one item: derive the inverse action, recheck against the snapshot, and
/// (only if it verifies AND the target is clear) act — always through the `Volume`
/// trait, so local and remote reverse uniformly.
async fn reverse_item(vm: &VolumeManager, kind: OpKind, unit: &RollbackUnit) -> ItemResult {
    let Some(action) = inverse_action(kind, unit.entry_type) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    match action {
        InverseAction::RemoveFileIfUnchanged => remove_file_if_unchanged(vm, unit).await,
        InverseAction::RemoveDirIfEmpty => remove_dir_if_empty(vm, unit).await,
        InverseAction::RestoreMove => restore_move(vm, unit).await,
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

async fn remove_file_if_unchanged(vm: &VolumeManager, unit: &RollbackUnit) -> ItemResult {
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
    match verify_snapshot(unit.size, unit.mtime, &live) {
        SnapshotVerdict::Match => match volume.delete(&path).await {
            Ok(()) => ItemResult::Reversed,
            Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
            Err(_) => ItemResult::Skipped(SkipReason::Failed),
        },
        SnapshotVerdict::Drift => ItemResult::Skipped(SkipReason::Drift),
        SnapshotVerdict::Unverifiable => ItemResult::Skipped(SkipReason::UnverifiablePrecondition),
    }
}

async fn remove_dir_if_empty(vm: &VolumeManager, unit: &RollbackUnit) -> ItemResult {
    let (vol_id, path) = removal_target(unit);
    let Some(volume) = vm.get(&vol_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    if !volume.exists(&path).await {
        // Already removed ⇒ idempotent no-op.
        return ItemResult::Skipped(SkipReason::AlreadyGone);
    }
    // Only remove a directory the undo created if it's still empty — a file added
    // since must not be swept away (D3). A `seq DESC` stream removes the dir's own
    // (unchanged) contents first, so a genuinely-restored tree is empty here.
    match volume.list_directory(&path, None).await {
        Ok(entries) if entries.is_empty() => match volume.delete(&path).await {
            Ok(()) => ItemResult::Reversed,
            Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
            Err(_) => ItemResult::Skipped(SkipReason::Failed),
        },
        Ok(_) => ItemResult::Skipped(SkipReason::DirNotEmpty),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}

async fn restore_move(vm: &VolumeManager, unit: &RollbackUnit) -> ItemResult {
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
        match verify_snapshot(unit.size, unit.mtime, &from_entry) {
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

    // Same volume ⇒ a plain rename (also the same-FS move / rename-back / same-FS
    // trash-restore path). Cross-volume ⇒ stream the bytes across then delete the
    // source side (per-leaf; cross-volume dirs never reach here — they're recorded
    // per file, and cross-volume can't be a self-collision so the target is clear).
    let acted = if same_volume {
        from_volume.rename(from_path, to_path, force).await
    } else {
        cross_volume_restore(
            from_volume.as_ref(),
            from_path,
            to_volume.as_ref(),
            to_path,
            &from_entry,
        )
        .await
    };
    match acted {
        Ok(()) => ItemResult::Reversed,
        Err(VolumeError::AlreadyExists(_)) => ItemResult::Skipped(SkipReason::RestoreTargetOccupied),
        Err(VolumeError::NotFound(_)) => ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}

/// Move a single file across volumes: stream its bytes to the target, then delete
/// the source side. Cross-volume restores are always per-file (directories are
/// recorded per leaf), so a directory here is a journal-shape bug — refuse safe.
async fn cross_volume_restore(
    from_volume: &dyn Volume,
    from_path: &Path,
    to_volume: &dyn Volume,
    to_path: &Path,
    from_entry: &FileEntry,
) -> Result<(), VolumeError> {
    if from_entry.is_directory {
        return Err(VolumeError::NotSupported);
    }
    let size = from_entry.size.unwrap_or(0);
    let stream = from_volume.open_read_stream(from_path).await?;
    let noop = |_written: u64, _total: u64| std::ops::ControlFlow::Continue(());
    to_volume.write_from_stream(to_path, size, stream, &noop).await?;
    from_volume.delete(from_path).await
}

// ── Entry point: gate + set rolling_back + spawn (state machine) ──────────────

/// Everything the caller needs to actually run the inverse op after the gate
/// passed and `rolling_back` was set: the original op's row and the fresh id for
/// its inverse.
#[derive(Debug, Clone)]
pub struct InversePlan {
    pub original: OperationRow,
    pub inverse_op_id: String,
}

/// The entry point (D7 state machine): read the op, gate it (unknown / already
/// rolling back / not rollbackable / a volume disconnected), then — as late as
/// possible — set it `rolling_back` and hand the plan to `spawn`, which launches
/// the inverse operation. If `spawn` fails synchronously (a volume dropped between
/// the gate and the spawn, so the inverse never starts), reset `rolling_back →
/// rollbackable` in the SAME call before returning the error, so the op isn't
/// wedged behind the `AlreadyRollingBack` guard and an immediate retry is accepted
/// (Finding 3). The double-rollback guard is automatic: a second call reads the op
/// as `rolling_back` and refuses.
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
    drop(conn);

    check_rollbackable(vm, &op)?;

    let plan = InversePlan {
        original: op,
        inverse_op_id: uuid::Uuid::new_v4().to_string(),
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
            // isn't refused, BEFORE returning the typed error (Finding 3).
            if let Err(e) = writer.set_rollback_state(op_id, RollbackState::Rollbackable, None) {
                log::warn!(target: "operation_log", "rollback: reset after failed spawn failed: {e}");
            }
            Err(refusal)
        }
    }
}

// ── Startup reconcile: resolve ops left mid-rollback by a crash ───────────────

/// On open, resolve every operation left `rolling_back` by a crash mid-rollback
/// (Finding 7 + 3), deterministically:
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

#[cfg(test)]
mod tests;
