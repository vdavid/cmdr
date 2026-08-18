//! What execution writes back: per-op outcomes, and the group's own end.

use rusqlite::{Connection, params};

use super::super::AgentStoreError;
use crate::agent::types::{OpStatus, ProposalStatus};

/// What marking a group completed did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteOutcome {
    /// The group moved `approved` -> `completed`.
    Completed,
    /// It was not `approved` any more. Nothing changed.
    NotApproved,
    /// No group with that id.
    Unknown,
}

/// Record where one op ended up.
///
/// **Last write wins, deliberately.** A cross-filesystem move speaks twice for one source:
/// `Done` when staging finishes, then `Done` again once the source-delete phase removes it,
/// or `Skipped` when the rename phase left it standing. Staging succeeding says nothing about
/// where the item ended up, so the LAST word is the verdict and this overwrites rather than
/// refusing a second write.
///
/// It refuses to touch a row the user deselected: an `excluded` op was never in the accepted
/// set, so nothing that ran can be about it.
pub fn record_op_outcome(conn: &Connection, op_id: i64, outcome: OpStatus) -> Result<bool, AgentStoreError> {
    let updated = conn
        .prepare_cached("UPDATE proposal_ops SET status = ?2 WHERE id = ?1 AND status != ?3")?
        .execute(params![op_id, outcome.as_token(), OpStatus::Excluded.as_token()])?;
    Ok(updated > 0)
}

/// Mark a group completed once its operation is over.
///
/// Conditional on `approved`, the same shape as the claim and the rejection, so this can
/// never resurrect a group the recovery sweep already froze or overwrite an answer the user
/// gave.
///
/// **Completed means execution FINISHED, not that every op succeeded.** It is written when
/// the operation settles, whatever the outcome — success, cancel, failure, even a panic —
/// because the distinction this status carries is "this group is no longer in flight" versus
/// "we lost track of it". The per-op statuses say what actually happened to each source; a
/// cancelled group keeps `pending` rows for the ops nothing ever reached.
///
/// Without this write, a group that ran to its end before a quit comes back `interrupted` on
/// the next launch and asks the user to re-approve work that already happened.
pub fn mark_group_completed(conn: &Connection, group_id: i64) -> Result<CompleteOutcome, AgentStoreError> {
    let updated = conn
        .prepare_cached("UPDATE proposals SET status = ?3 WHERE id = ?1 AND status = ?2")?
        .execute(params![
            group_id,
            ProposalStatus::Approved.as_token(),
            ProposalStatus::Completed.as_token()
        ])?;
    if updated > 0 {
        return Ok(CompleteOutcome::Completed);
    }
    let exists: bool = conn
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM proposals WHERE id = ?1)")?
        .query_row(params![group_id], |row| row.get(0))?;
    Ok(if exists {
        CompleteOutcome::NotApproved
    } else {
        CompleteOutcome::Unknown
    })
}
