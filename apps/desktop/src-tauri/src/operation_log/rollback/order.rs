//! The order a multi-operation undo must reverse in.
//!
//! Pure, and deliberately its own module: the rule is one line of code and a
//! paragraph of reasoning, and the reasoning is the part that must not be lost.

use crate::operation_log::store::OperationRow;

/// Order `ops` for a multi-operation undo: **newest first**.
///
/// **This ordering is data-safety-critical, not cosmetic.** A later batch can
/// rename a file INTO a name an earlier batch freed (batch one renamed
/// `a.txt` → `b.txt`; batch three then renamed `c.txt` → `a.txt`). Reversed
/// newest-first, batch three vacates `a.txt` before batch one needs it back, and
/// both restore. Reversed oldest-first, batch one finds `a.txt` occupied by a
/// DIFFERENT entry, and the pinned non-destructive restore correctly refuses to
/// overwrite it ([`super::SkipReason::RestoreTargetOccupied`]) — so the file is
/// never restored, and the only trace is a `partially_rolled_back` state the user
/// has to go looking for. Newest-first is what makes a clean undo possible at all.
///
/// Ties: the journal's clock is whole seconds, so a job's batches routinely share
/// one. `ops` arrives in the order the caller APPLIED the batches, so reversing it
/// before the sort gives tied batches the same newest-first meaning; the sort is
/// stable, so that fallback survives it. Never leave a tie to an arbitrary order —
/// the reused-name case is exactly where it would bite.
pub fn undo_order(ops: Vec<OperationRow>) -> Vec<OperationRow> {
    let mut ordered: Vec<OperationRow> = ops.into_iter().rev().collect();
    ordered.sort_by_key(|op| std::cmp::Reverse(op.started_at));
    ordered
}
