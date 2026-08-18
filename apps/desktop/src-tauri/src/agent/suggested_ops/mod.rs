//! Suggested ops: the service over the proposal spine.
//!
//! The store (`agent/store/proposals/`) owns rows, the lifecycle machine, and the claim
//! transaction. This layer owns everything above them that a row can't: turning a selector
//! into a frozen op list against the drive index, and reporting the one metric that says
//! whether the feature is worth having (acceptance rate, agent-spec D46).
//!
//! ## The guiding principle
//!
//! We do not trust the agent. Its suggestions can be formally valid and factually
//! hallucinated, and we can never know which — so the job is not to make the agent safe, it is
//! to lay everything out for the user to decide. Once the user approves, it is exactly as if
//! the user started the action, because they did.
//!
//! Two consequences this module keeps:
//!
//! - ❌ **No agent-specific safety behaviour on the execution path.** Approved ops are queued
//!   ops. An irreversible group is disclosed, never refused.
//! - ✅ **The effort goes into disclosure**: the deterministic facts (size, dates, the frozen
//!   snapshot) that let a user check a suggestion against something the agent could not
//!   invent.
//!
//! Depth: `DETAILS.md`.

mod analytics;
pub mod selector;

#[cfg(test)]
mod tests;

pub use selector::{DriveIndex, IndexedFile, OpSelector, SelectorIndex, SelectorRefusal};

use rusqlite::Connection;

use super::store::AgentStoreError;
use super::store::proposals::{
    ClaimOutcome, GroupIntent, NewGroup, NewOp, NewSweep, OpSnapshot, RejectOutcome, claim_group_for_execution,
    count_ops, create_group, create_sweep, get_group,
};

/// A sweep as created: its id and the ids of the groups inside it, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedSweep {
    pub set_id: i64,
    pub group_ids: Vec<i64>,
}

/// Create a sweep and its groups, and report each group as proposed.
///
/// The op lists arrive already concrete: a caller working from a selector resolves it with
/// [`resolve_selector_ops`] first, so the freeze happens before anything is written and a
/// group's rows are the only account of what it proposes.
pub fn propose(
    conn: &Connection,
    sweep: &NewSweep,
    groups: &[NewGroup],
    now: i64,
) -> Result<ProposedSweep, AgentStoreError> {
    let set_id = create_sweep(conn, sweep, now)?;
    let mut group_ids = Vec::with_capacity(groups.len());
    for group in groups {
        group_ids.push(create_group(conn, set_id, group, now)?);
        analytics::group_proposed(group.intent.verb(), group.intent.op_count());
    }
    Ok(ProposedSweep { set_id, group_ids })
}

/// Claim a group for execution on the user's say-so, and report an approval.
///
/// A thin wrapper on the store's claim: the transaction, and every refusal it can produce,
/// belongs there. This adds only the metric, and only when a claim actually went through — a
/// refused claim is not an approval.
pub fn approve(conn: &Connection, group_id: i64, now: i64) -> Result<ClaimOutcome, AgentStoreError> {
    let outcome = claim_group_for_execution(conn, group_id, now)?;
    if let ClaimOutcome::Claimed(claimed) = &outcome {
        analytics::group_approved(claimed.group.verb, claimed.binding.op_count);
    }
    Ok(outcome)
}

/// Reject a group on the user's say-so, and report the rejection.
///
/// The verb and count are read BEFORE the transition, because that's the group the user was
/// looking at when they said no.
pub fn reject(conn: &Connection, group_id: i64, now: i64) -> Result<RejectOutcome, AgentStoreError> {
    let before = get_group(conn, group_id)?;
    let outcome = super::store::proposals::reject_group(conn, group_id, now)?;
    if let (RejectOutcome::Rejected, Some(group)) = (&outcome, before) {
        let op_count = count_ops(conn, group_id, Some(crate::agent::types::OpStatus::Pending))?;
        analytics::group_rejected(group.verb, op_count);
    }
    Ok(outcome)
}

/// Resolve a selector into the ops a group will freeze, and the text that names the pattern.
///
/// This is where "every installer in Downloads you've already opened" becomes a concrete list.
/// It happens ONCE, before the group is written, against the drive index. ❌ Nothing
/// re-resolves a selector afterwards: the group's rows are what the user reviews and what the
/// executor runs, and re-resolving would break the equality between the two.
pub fn resolve_selector_ops(index: &dyn SelectorIndex, selector: &OpSelector) -> Result<Vec<NewOp>, SelectorRefusal> {
    let files = index.resolve(selector)?;
    Ok(files
        .into_iter()
        .map(|file| NewOp {
            source_path: file.path,
            snapshot: Some(OpSnapshot {
                size: file.size,
                mtime: file.modified_at,
                inode: file.inode,
            }),
        })
        .collect())
}

/// Wrap an intent built from a resolved selector into a group, carrying the pattern as the
/// group's display text and the selector itself as provenance.
///
/// The selector rides along as JSON so the review dialog can render its predicates in the
/// user's own language, and so "why is this file here?" has an answer. ❌ It is stored to be
/// SHOWN, never to be re-run.
pub fn selector_group(
    selector: &OpSelector,
    intent: GroupIntent,
    rationale: Option<String>,
) -> Result<NewGroup, serde_json::Error> {
    Ok(NewGroup {
        intent,
        source_volume_id: selector.root.volume_id.clone(),
        display_name: selector.pattern_text(),
        rationale,
        selector: Some(selector.to_json()?),
    })
}
