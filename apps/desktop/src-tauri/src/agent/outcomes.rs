//! What the user did with a proposal, and how the agent hears about it.
//!
//! An approval or a rejection the agent never hears about is a lesson it can't learn, and there
//! are two channels here because neither one alone is enough:
//!
//! - **The user's timeline** is a typed [`ConversationEvent`]. ⚠️ Those never enter the LLM
//!   transcript by design (`../store/events.rs`), so an outcome recorded only there teaches the
//!   agent nothing.
//! - **The agent's lesson** is a line in the memory ring (`../memory/outcomes.rs`), written on
//!   the ALWAYS-path with no model call. That is what covers approvals, which get no follow-up
//!   turn at all — without it, rejections would produce every lesson and approvals none, and
//!   the agent would over-correct toward proposing nothing.
//!
//! A rejection additionally asks for a follow-up turn, coalesced per SWEEP by the wake loop
//! (`../wake/followup.rs`). ⚠️ **Per sweep, ❌ never per group**: "reject all" over an
//! eight-group sweep is eight rejections, and a turn each would be eight model calls serialized
//! behind one conversation lock.
//!
//! ⚠️ **A dismissal is not a rejection.** Closing a rename review with Escape records the group
//! as `rejected` because the lifecycle needs an answer, but the user expressed no opinion about
//! the proposal's content. Teaching the agent from it, and spending a model call to ask "why
//! did you say no?" in whatever thread the user has open, is the one bug in this area that
//! would ship unnoticed. [`RejectSource`] is what keeps the two apart.

use rusqlite::Connection;

use super::memory::MemoryStore;
use super::store::proposals::{ProposalGroup, count_ops, get_group, get_sweep};
use super::store::{AgentStoreError, ConversationEvent, append_event};
use super::types::{OpStatus, ProposalDecision, ProposalOutcomeKind};
use super::wake::{WakeControl, send_control};

const LOG_TARGET: &str = "agent::outcomes";

/// Why a group left `pending` without being approved.
///
/// ⚠️ The two are the same store transition and a completely different signal. Only
/// [`RejectSource::Review`] is a judgment about the proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectSource {
    /// The user said no in the review. A judgment: worth remembering, worth asking about.
    Review,
    /// A review dialog closed without an answer (`cancel_bulk_rename_proposal`). The group
    /// needs an answer and gets one, but there is nothing here to learn from, and the sweep's
    /// conversation is the user's ACTIVE RAIL THREAD, so a follow-up turn would land in the
    /// middle of whatever they were doing.
    DialogDismissed,
}

/// Record a rejection on both channels and ask for the follow-up turn.
///
/// The group is read BEFORE the transition by the caller, because that is the group the user
/// was looking at when they said no.
pub fn record_rejection(
    conn: &Connection,
    memory: Option<&MemoryStore>,
    source: RejectSource,
    group: &ProposalGroup,
    ops: u32,
    now: i64,
) {
    // ⚠️ The whole point of the parameter. A dismissal is the same store transition and no
    // signal at all: nothing to remember, nothing on the timeline, and above all no turn.
    if source == RejectSource::DialogDismissed {
        return;
    }
    let decision = ProposalDecision {
        verb: group.verb,
        what: group.display_name.clone(),
        ops,
        outcome: ProposalOutcomeKind::Rejected,
    };
    record(conn, memory, group.set_id, decision, now, true);
}

/// Record what an approved group ACTUALLY did, once its operation has settled.
///
/// ⚠️ **Not at `approve`.** That call is only the claim, and a claimed group can go on to skip
/// every file behind a fingerprint mismatch. An outcome recorded there would tell the agent the
/// user wanted something they never got.
///
/// No follow-up turn: an approval is the agent being right, and there is nothing to ask about.
/// The memory line is the whole lesson, which is exactly why it has to exist.
pub fn record_completion(conn: &Connection, memory: Option<&MemoryStore>, group_id: i64, now: i64) {
    let group = match get_group(conn, group_id) {
        Ok(Some(group)) => group,
        Ok(None) => return,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the agent did not hear how group {group_id} ended: {e}");
            return;
        }
    };
    let decision = match completed_decision(conn, &group) {
        Ok(decision) => decision,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the agent did not hear how group {group_id} ended: {e}");
            return;
        }
    };
    record(conn, memory, group.set_id, decision, now, false);
}

/// The per-op tallies the engine wrote back, plus the accepted set they came out of.
///
/// `ops` is what the user ACCEPTED (every row minus the ones they deselected at review), so a
/// cancelled group reads honestly: the three tallies simply don't add up to it, which is the
/// truth about a run that stopped partway.
fn completed_decision(conn: &Connection, group: &ProposalGroup) -> Result<ProposalDecision, AgentStoreError> {
    let total = count_ops(conn, group.id, None)?;
    let excluded = count_ops(conn, group.id, Some(OpStatus::Excluded))?;
    Ok(ProposalDecision {
        verb: group.verb,
        what: group.display_name.clone(),
        ops: total.saturating_sub(excluded) as u32,
        outcome: ProposalOutcomeKind::Ran {
            done: count_ops(conn, group.id, Some(OpStatus::Done))? as u32,
            skipped: count_ops(conn, group.id, Some(OpStatus::Skipped))? as u32,
            failed: count_ops(conn, group.id, Some(OpStatus::Failed))? as u32,
        },
    })
}

/// Both channels, and the follow-up ask. Every failure here is logged and swallowed: the user's
/// answer is already durable in `main.db`, and a lost lesson costs a re-proposal, never
/// correctness.
fn record(
    conn: &Connection,
    memory: Option<&MemoryStore>,
    set_id: i64,
    decision: ProposalDecision,
    now: i64,
    follow_up: bool,
) {
    if let Some(memory) = memory {
        memory.record_outcome(&format!("{} {}", day_of(now), decision.render()));
    }
    // Nullable, and NULLed when a thread is deleted, so a sweep whose thread is gone still
    // teaches the agent — it just has no timeline to say so on.
    match get_sweep(conn, set_id) {
        Ok(Some(sweep)) => {
            if let Some(conversation_id) = sweep.conversation_id
                && let Err(e) = append_event(
                    conn,
                    conversation_id,
                    &ConversationEvent::ProposalDecided { decision },
                    now,
                )
            {
                log::warn!(target: LOG_TARGET, "a decision left no line in its thread: {e}");
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!(target: LOG_TARGET, "a decision left no line in its thread: {e}"),
    }
    if follow_up {
        // ⚠️ A CONTROL message, so it can never be dropped for the rollup bound. The wake loop
        // coalesces per sweep and owns every gate; nothing here decides whether a turn runs.
        send_control(WakeControl::SweepRejected { set_id });
    }
}

/// The local day a decision happened on, `YYYY-MM-DD`, for the memory line. The same shape the
/// cost meter stamps, so two records of the same afternoon agree about which day it was.
fn day_of(now: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(now, 0)
        .single()
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH)
        .with_timezone(&crate::agent::chat::session::local_offset())
        .format("%Y-%m-%d")
        .to_string()
}
