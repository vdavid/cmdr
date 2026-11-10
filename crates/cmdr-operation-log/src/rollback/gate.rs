//! The entry point of the `rolling_back` state machine: gate an operation (and the
//! selection it was sent), set it `rolling_back` as late as possible, and hand the
//! plan to the injected spawn (`DETAILS.md` § "The `rolling_back` state machine +
//! startup reconcile"). The run itself is `execute_rollback` in the parent module.

use std::path::Path;

use rusqlite::Connection;

use super::super::store::{
    OperationRow, open_read_connection, read_item_row, read_operation, read_rollback_units_page, rollback_unit_of,
};
use super::super::tree::rollback_filter::{Narrowed, RollbackFilter, narrow};
use super::super::tree::selection::RollbackSelection;
use super::super::types::{EntryType, NotRollbackableReason, RollbackState};
use super::super::writer::OperationLogWriter;
use super::{
    InverseAction, RollbackRefusal, VolumeLookup, is_created_in_place, item_action, removal_target, untouched_state,
};

// ── The op-level gate (used by the managed rollback entry point + tested here) ─────────────

/// Check whether `op` may be rolled back right now: its stored `rollback_state`
/// and (for a connected-volume requirement) whether every volume it touches is
/// registered. Returns `Ok(())` to proceed, or the typed refusal. Does NOT mutate
/// anything — the caller sets `rolling_back` only on a successful spawn.
///
/// A rollback is never itself rolled back (no redo), whatever state it stored: an
/// inverse op's rows carry no dest, so a move's or rename's would hold every row
/// back, and undoing an undo is not a feature (`DETAILS.md` § "No rollback of a
/// rollback"). Checked first, so every entry (dialog, MCP, `undo_operations`) refuses
/// it the same way.
pub fn check_rollbackable(volumes: &dyn VolumeLookup, op: &OperationRow) -> Result<(), RollbackRefusal> {
    if op.rolls_back_op_id.is_some() {
        return Err(RollbackRefusal::IsARollback);
    }
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
        if volumes.get(volume_id).is_none() {
            return Err(RollbackRefusal::VolumeUnavailable {
                volume_id: volume_id.to_string(),
            });
        }
    }
    Ok(())
}

// ── Entry point: gate + set rolling_back + spawn (state machine) ──────────────

/// Everything the caller needs to actually run the inverse op after the gate
/// passed and `rolling_back` was set: the original op's row, the fresh id for its
/// inverse, which rows it takes, and where the reversal will act.
#[derive(Debug, Clone)]
pub struct InversePlan {
    /// The operation being reversed, as it read when the gate passed.
    pub original: OperationRow,
    /// The id the inverse operation journals under.
    pub inverse_op_id: String,
    /// The rows it takes, normalized by the gate; `None` takes every row. Hand it to
    /// [`execute_rollback_with_selection`](super::execute_rollback_with_selection), which normalizes it again.
    pub selection: Option<RollbackSelection>,
    /// Where the reversal will act.
    pub summary: InverseSummary,
}

/// Where a reversal will act, so its queue row can say so instead of sitting
/// nameless. Read off one journal row at dispatch, never the list.
#[derive(Debug, Clone, Default)]
pub struct InverseSummary {
    /// The folder the reversal takes items FROM (for a removal, the folder it
    /// cleans; for a created directory, that directory itself).
    pub from: Option<String>,
    /// Where a restore puts them back. `None` for a removal inverse, which has
    /// nowhere to put anything.
    pub to: Option<String>,
}

/// Summarize what reversing `op` will do, from one row it takes.
///
/// With a selection, that's a row the selection names (`Selection::sample_seq`: one
/// point read, never a stream, since this runs on the dispatch path). Otherwise the
/// newest `rollback_unit` row, the cheapest honest sample: for a copy it's the
/// created directory (the exact folder the undo cleans), and for anything else it's
/// a leaf whose parent folder is the place the user is watching.
fn summarize_inverse(conn: &Connection, op: &OperationRow, filter: Option<&RollbackFilter>) -> InverseSummary {
    let sampled = match filter.map(|filter| filter.sample_seq(conn)) {
        Some(Ok(Some(seq))) => read_item_row(conn, &op.op_id, seq)
            .ok()
            .flatten()
            .and_then(|row| rollback_unit_of(conn, &row).ok()),
        _ => None,
    };
    let unit = match sampled {
        Some(unit) => unit,
        None => match read_rollback_units_page(conn, &op.op_id, i64::MAX, 1) {
            Ok(mut page) if !page.is_empty() => page.swap_remove(0),
            _ => return InverseSummary::default(),
        },
    };
    let (_, acting_on) = removal_target(&unit);
    // A directory row IS the thing being removed; a file row names its folder.
    let from = if unit.entry_type == EntryType::Dir {
        Some(acting_on)
    } else {
        acting_on.parent().map(Path::to_path_buf)
    };
    let to = match item_action(op.kind, unit.entry_type, is_created_in_place(&unit)) {
        Some(InverseAction::RestoreMove) => unit.source_path.parent().map(Path::to_path_buf),
        _ => None,
    };
    InverseSummary {
        from: from.map(|p| p.to_string_lossy().into_owned()),
        to: to.map(|p| p.to_string_lossy().into_owned()),
    }
}

/// [`rollback_operation_with_selection`] taking every row: the default "undo the
/// whole thing", and "Roll back the rest" of a partial op.
pub fn rollback_operation<F>(
    volumes: &dyn VolumeLookup,
    writer: &OperationLogWriter,
    op_id: &str,
    spawn: F,
) -> Result<InversePlan, RollbackRefusal>
where
    F: FnOnce(&InversePlan) -> Result<(), RollbackRefusal>,
{
    rollback_operation_with_selection(volumes, writer, op_id, None, spawn)
}

/// The entry point of the `rolling_back` state machine (`DETAILS.md` § "The
/// `rolling_back` state machine + startup reconcile"): read the op, gate it
/// (unknown / already rolling back / not rollbackable / a volume disconnected / a
/// selection it can't run), then — as late as possible — set it `rolling_back` and
/// hand the plan to `spawn`, which launches the inverse operation. If `spawn` fails
/// synchronously (a volume dropped between the gate and the spawn, so the inverse
/// never starts), reset `rolling_back → rollbackable` in the SAME call before
/// returning the error, so the op isn't wedged behind the `AlreadyRollingBack`
/// guard and an immediate retry is accepted. The double-rollback guard is
/// automatic: a second call reads the op as `rolling_back` and refuses, and the
/// flip is a compare-and-set, so a second call racing the first is refused too.
///
/// `selection` narrows the run to the rows it takes (`None`: every row). The gate
/// normalizes it (❌ never trusted from the wire) and refuses it typed
/// ([`SelectionRefusal`](super::SelectionRefusal)) before anything is set; a selection that takes every row
/// is the same as `None`.
///
/// `spawn` is injected so the manager wiring (which lives in `cmdr-operations`,
/// where the `OperationManager` is reachable) supplies the real managed-op spawn,
/// while tests drive the gate/reset logic directly.
pub fn rollback_operation_with_selection<F>(
    volumes: &dyn VolumeLookup,
    writer: &OperationLogWriter,
    op_id: &str,
    selection: Option<&RollbackSelection>,
    spawn: F,
) -> Result<InversePlan, RollbackRefusal>
where
    F: FnOnce(&InversePlan) -> Result<(), RollbackRefusal>,
{
    let conn = open_read_connection(writer.db_path()).map_err(|_| RollbackRefusal::UnknownOperation)?;
    let op = read_operation(&conn, op_id)
        .map_err(|_| RollbackRefusal::UnknownOperation)?
        .ok_or(RollbackRefusal::UnknownOperation)?;

    check_rollbackable(volumes, &op)?;

    let filter = match selection.map(|wire| narrow(&conn, op_id, wire)) {
        None | Some(Ok(Narrowed::Everything)) => None,
        Some(Ok(Narrowed::Some(filter))) => Some(*filter),
        Some(Ok(Narrowed::Refused(refusal))) => return Err(RollbackRefusal::Selection(refusal)),
        Some(Err(e)) => {
            // The journal couldn't answer for this op: the same "couldn't establish
            // it" an unreadable op row gets.
            log::warn!(target: "operation_log", "rollback: the selection didn't resolve: {e}");
            return Err(RollbackRefusal::UnknownOperation);
        }
    };
    let selection = filter.as_ref().map(|filter| filter.normalized().clone());
    let summary = summarize_inverse(&conn, &op, filter.as_ref());
    drop(filter);
    drop(conn);

    let plan = InversePlan {
        original: op,
        inverse_op_id: super::super::new_operation_id(),
        selection,
        summary,
    };
    // Set `rolling_back` as late as possible — right before the spawn — to shrink
    // the window in which a crash leaves it set with no inverse row (the reconcile
    // resolves that anyway, straight back to rollbackable). A compare-and-set against
    // the state the gate read: of two dispatches that both passed the gate (a
    // narrowed dialog press and an agent's whole-op call), only one flips it, and
    // the other is refused like any later dispatch, so it can't reverse the rows
    // the person left out.
    match writer.claim_rollback(op_id, plan.original.rollback_state) {
        Ok(true) => {}
        Ok(false) => return Err(RollbackRefusal::AlreadyRollingBack),
        Err(e) => {
            log::warn!(target: "operation_log", "rollback: set rolling_back failed: {e}");
            return Err(RollbackRefusal::UnknownOperation);
        }
    }
    match spawn(&plan) {
        Ok(()) => Ok(plan),
        Err(refusal) => {
            // The inverse never started — put back the state the op held (a partial
            // op stays partial) so a retry isn't refused, BEFORE returning the typed error.
            if let Err(e) = writer.set_rollback_state(op_id, untouched_state(plan.original.rollback_state), None) {
                log::warn!(target: "operation_log", "rollback: reset after failed spawn failed: {e}");
            }
            Err(refusal)
        }
    }
}
