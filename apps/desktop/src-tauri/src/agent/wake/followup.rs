//! The turn a rejection earns: "you turned this down, what should I know?"
//!
//! The memory ring already recorded WHAT happened, with no model call (`../outcomes.rs`). This
//! is the other half: one turn, in the thread that produced the sweep, so the agent can turn a
//! raw log line into something it will actually act on next time. An approval gets no turn at
//! all: it is the agent being right, and there is nothing to ask about.
//!
//! ⚠️ **One turn per SWEEP, ❌ never per group.** "Reject all" over an eight-group sweep is
//! eight `Rejected` outcomes, and a turn each would be eight model calls all serialized behind
//! the same `ConversationLocks` guard. [`FollowUpQueue`] is what collapses them.
//!
//! ⚠️ **The gates are the wake loop's, not this module's.** `askCmdr.proactive` off, or any of
//! the three readiness gates closed, means no turn, and the ask is DROPPED rather than parked,
//! because a "why did you say no?" that arrives a week later, when the user finally sets an API
//! key, is worse than none.

use std::collections::BTreeMap;
use std::time::Duration;

use rusqlite::Connection;

use super::super::store::proposals::{count_ops, get_sweep, rejected_groups_since};
use super::super::types::{OpStatus, ProposalDecision, ProposalOutcomeKind, ProposalOutcomes};
use super::{WakeReadiness, WakeSettings};

const LOG_TARGET: &str = "agent::wake";

/// How long the loop waits after a rejection before asking, so a burst becomes one turn.
///
/// ⚠️ A TRAILING window: each further rejection in the same sweep pushes it out again. "Reject
/// all" walks the groups one IPC call at a time, and a leading window would fire on the first
/// one and ask about a fraction of what the user actually turned down.
pub(super) const COALESCE_WINDOW: Duration = Duration::from_secs(5);

/// The sweeps waiting for a follow-up turn: one entry each, however many of their groups the
/// user turned down.
#[derive(Default)]
pub(super) struct FollowUpQueue {
    /// Sweep id → (when the burst started, when it may run). `BTreeMap` so a tie between two
    /// due sweeps resolves the same way twice running.
    waiting: BTreeMap<i64, Waiting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Waiting {
    /// The first rejection of this burst, so the turn reports what the burst covered rather
    /// than every group the sweep has ever lost.
    since: u64,
    /// Unix seconds this may run at; pushed out by each further rejection.
    due: u64,
}

impl FollowUpQueue {
    /// Note one rejected group. The SWEEP is the key, which is the whole coalescing rule.
    pub(super) fn note(&mut self, set_id: i64, now: u64) {
        let due = now.saturating_add(COALESCE_WINDOW.as_secs());
        self.waiting
            .entry(set_id)
            .and_modify(|waiting| waiting.due = due)
            .or_insert(Waiting { since: now, due });
    }

    /// When the earliest waiting sweep may run, so the loop parks until then.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.waiting.values().map(|waiting| waiting.due).min()
    }

    /// Take the sweep whose window has closed, if any: its id and the instant its burst began.
    pub(super) fn take_due(&mut self, now: u64) -> Option<(i64, u64)> {
        let (set_id, waiting) = self
            .waiting
            .iter()
            .find(|(_, waiting)| waiting.due <= now)
            .map(|(set_id, waiting)| (*set_id, *waiting))?;
        self.waiting.remove(&set_id);
        Some((set_id, waiting.since))
    }

    /// Throw away every ask. What a closed gate does: the question is only worth asking while
    /// the answer is still fresh in the user's mind.
    pub(super) fn clear(&mut self) {
        self.waiting.clear();
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.waiting.len()
    }
}

/// Whether the agent may ask about a rejection at all.
///
/// Pure and shared by both ends of the window (the moment a rejection arrives, and the moment
/// its window closes), because a gate can shut in between and a turn that slipped through would
/// message the provider for somebody who has said no to exactly that.
pub(super) fn may_ask(settings: &WakeSettings, readiness: WakeReadiness) -> bool {
    settings.proactive && readiness.may_wake()
}

/// A follow-up that is going to happen: the thread to speak in, and what the user answered.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PreparedFollowUp {
    pub conversation_id: i64,
    pub outcomes: ProposalOutcomes,
}

/// Gather what one sweep lost and where to say so. `None` when there is nothing to ask about.
///
/// Two ordinary reasons for `None`, neither of them a failure: the sweep's thread is gone
/// (`conversation_id` is nullable and is NULLed when a thread is deleted, so the decision
/// record outlives it), and nothing in the window is actually rejected any more.
pub(super) fn prepare(conn: &Connection, set_id: i64, since: i64) -> Option<PreparedFollowUp> {
    let conversation_id = match get_sweep(conn, set_id) {
        Ok(Some(sweep)) => sweep.conversation_id?,
        Ok(None) => return None,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "a rejected sweep could not be read back: {e}");
            return None;
        }
    };
    let groups = match rejected_groups_since(conn, set_id, since) {
        Ok(groups) => groups,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "a rejected sweep's groups could not be read back: {e}");
            return None;
        }
    };
    let decisions: Vec<ProposalDecision> = groups
        .into_iter()
        .map(|group| ProposalDecision {
            // The rows never move out of `pending` on a rejection, so this is still the set the
            // user was looking at when they said no.
            ops: count_ops(conn, group.id, Some(OpStatus::Pending)).unwrap_or(0) as u32,
            verb: group.verb,
            what: group.display_name,
            outcome: ProposalOutcomeKind::Rejected,
        })
        .collect();
    if decisions.is_empty() {
        return None;
    }
    Some(PreparedFollowUp {
        conversation_id,
        outcomes: ProposalOutcomes { decisions },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ **The coalescing guard.** "Reject all" over an eight-group sweep sends eight
    /// rejections; eight follow-up turns would be eight model calls, every one of them queued
    /// behind the same conversation lock, for one decision the user experienced as one click.
    #[test]
    fn a_whole_sweep_rejected_at_once_asks_one_question() {
        let mut queue = FollowUpQueue::default();

        for _ in 0..8 {
            queue.note(42, 1_780_000_000);
        }

        assert_eq!(queue.len(), 1, "eight rejected groups, one sweep, one turn");
        assert_eq!(
            queue.take_due(1_780_000_000 + COALESCE_WINDOW.as_secs()),
            Some((42, 1_780_000_000))
        );
        assert_eq!(queue.take_due(1_780_009_999), None, "and nothing is left to ask twice");
    }

    /// Two sweeps are two decisions, so they stay two questions. Collapsing by anything coarser
    /// would drop one of them on the floor.
    #[test]
    fn two_sweeps_stay_two_questions() {
        let mut queue = FollowUpQueue::default();
        queue.note(1, 1_780_000_000);
        queue.note(2, 1_780_000_000);

        assert_eq!(queue.len(), 2);
    }

    /// The window is TRAILING: rejecting groups one at a time must not fire on the first and
    /// ask about a fraction of what the user turned down.
    #[test]
    fn each_further_rejection_pushes_the_window_out() {
        let mut queue = FollowUpQueue::default();
        queue.note(7, 1_780_000_000);
        queue.note(7, 1_780_000_003);

        assert_eq!(
            queue.take_due(1_780_000_005),
            None,
            "the first window closed, but the burst had not"
        );
        assert_eq!(
            queue.take_due(1_780_000_008),
            Some((7, 1_780_000_000)),
            "and the burst is reported from where it STARTED"
        );
    }

    /// ⚠️ **A rejection with `askCmdr.proactive` off runs no turn.** The setting is the user's
    /// "no thanks" to an agent that starts conversations, and a follow-up is exactly that: it
    /// happens to be prompted by their own click, which is not the same as being invited.
    #[test]
    fn a_rejection_asks_nothing_when_the_agent_may_not_speak() {
        let on = WakeSettings::default();
        let off = WakeSettings {
            proactive: false,
            ..WakeSettings::default()
        };

        assert!(may_ask(&on, WakeReadiness::Ready), "the shipped default asks");
        assert!(!may_ask(&off, WakeReadiness::Ready), "opting out means silence");
        for gap in [
            WakeReadiness::NeedsConsent,
            WakeReadiness::NeedsFullDiskAccess,
            WakeReadiness::NeedsApiKey,
        ] {
            assert!(
                !may_ask(&on, gap),
                "a closed gate is still closed for a follow-up: {gap:?}"
            );
        }
    }

    /// Nothing runs before its window closes, or the loop would spend a model call on the first
    /// click of a burst.
    #[test]
    fn nothing_is_asked_before_its_window_closes() {
        let mut queue = FollowUpQueue::default();
        queue.note(3, 1_780_000_000);

        assert_eq!(queue.take_due(1_780_000_004), None);
        assert_eq!(queue.next_due(), Some(1_780_000_000 + COALESCE_WINDOW.as_secs()));
    }
}
