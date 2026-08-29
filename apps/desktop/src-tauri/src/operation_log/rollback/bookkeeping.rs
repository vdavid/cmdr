//! The reversal's books: the running tally, and the two journal side-effects it
//! writes (the inverse operation's item rows, and the original's per-item
//! outcomes).
//!
//! Split from the loop so `rollback.rs` reads as the decision it makes and the
//! order it makes them in. Nothing here decides anything.

use std::path::Path;

use super::super::capture::compute_eligibility;
use super::super::store::RollbackUnit;
use super::super::types::{ExecutionStatus, ItemOutcome, OpKind, RowRole, SearchCoverage};
use super::super::writer::{FinalizeOperation, ItemOutcomeUpdate, JournalItem, OperationLogWriter};
use super::skips::SkipTally;
use super::{ItemResult, removal_target};

/// Accumulates a rollback run's tally + the two journal side-effects (the inverse
/// op's item rows, and the original op's per-item outcome updates), so the driver
/// loop appends without wrestling closure borrows.
#[derive(Default)]
pub(super) struct RunAcc {
    pub(super) reversed: u64,
    pub(super) skipped: u64,
    inverse_items: Vec<JournalItem>,
    original_outcomes: Vec<ItemOutcomeUpdate>,
    next_inverse_seq: i64,
    /// The per-reason breakdown of what was left alone. Grows by at most one entry per
    /// [`SkipReason`](super::super::types::SkipReason), so it stays bounded across a
    /// 1M-item stream.
    pub(super) skips: SkipTally,
}

impl RunAcc {
    pub(super) fn record(&mut self, unit: &RollbackUnit, result: ItemResult) {
        // A skip that counts as reversed (`AlreadyGone` — the end state already holds)
        // is NOT a skip, so it reports no reason: the column explains items the undo
        // left behind, and an idempotent no-op left nothing behind.
        let (original_outcome, skip_reason) = match &result {
            ItemResult::Reversed => (ItemOutcome::RolledBack, None),
            ItemResult::Skipped(r) if r.counts_as_reversed() => (ItemOutcome::RolledBack, None),
            ItemResult::Skipped(reason) => (ItemOutcome::Skipped, Some(*reason)),
        };
        if original_outcome == ItemOutcome::RolledBack {
            self.reversed += 1;
        } else {
            self.skipped += 1;
        }
        if let Some(reason) = skip_reason {
            // Group by reason at the location the undo found the item — the name the
            // file carries now, which is what the user sees in the pane.
            self.skips.record(reason, &removal_target(unit).1);
        }
        self.original_outcomes.push(ItemOutcomeUpdate {
            seq: unit.seq,
            outcome: original_outcome,
            skip_reason,
        });
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
    pub(super) fn flush(&mut self, writer: &OperationLogWriter, inverse_op_id: &str, original_op_id: &str) {
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
pub(super) fn finalize_inverse(
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
        ended_at: super::super::now_secs(),
        item_count: None,
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
