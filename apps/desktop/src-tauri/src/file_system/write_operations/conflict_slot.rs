//! The one place a Stop-mode conflict answer is arbitrated.
//!
//! An operation that parks on a Stop-mode conflict arms this slot with the
//! sender it's listening on, then emits `write-conflict`. That event reaches
//! every webview, so several surfaces can render the prompt and each of them
//! can be answered. Exactly one answer can reach the operation, and the loser
//! has to be told: a surface that believes it answered leaves its prompt up,
//! and the person who clicked sees nothing happen.
//!
//! So the slot is a three-state machine, not an `Option<Sender>`: the state a
//! second answer lands in is what distinguishes "someone beat you to it" from
//! "this operation isn't asking anything". A bool alongside the sender would
//! carry the same information, and could desync from it; here the take and the
//! bookkeeping are one transition under one lock.

use crate::ignore_poison::IgnorePoison;
use std::sync::Mutex;
use tokio::sync::oneshot;

use super::types::{ConflictResolution, ConflictResolutionOutcome};

/// Response to a conflict resolution request.
#[derive(Debug, Clone)]
pub struct ConflictResolutionResponse {
    pub resolution: ConflictResolution,
    pub apply_to_all: bool,
}

/// Where this operation's conflict stands.
enum SlotState {
    /// Nothing is being asked: no conflict has been raised yet, or a cancel
    /// took the pending one away.
    Idle,
    /// A conflict is on screen. This sender unblocks the parked operation.
    Awaiting(oneshot::Sender<ConflictResolutionResponse>),
    /// The conflict was answered, and the operation carried on with that
    /// answer. Anything arriving now is a second opinion.
    Answered,
}

/// Holds the sender a parked operation is listening on, and answers "did MY
/// answer land?" for every surface that tries.
pub struct ConflictSlot {
    state: Mutex<SlotState>,
}

impl ConflictSlot {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SlotState::Idle),
        }
    }

    /// Arms the slot with the sender the waiting operation listens on. Call
    /// this BEFORE emitting `write-conflict`: a responder can only answer a
    /// conflict it has observed, and an answer arriving at an unarmed slot
    /// leaves the operation parked forever.
    pub fn arm(&self, tx: oneshot::Sender<ConflictResolutionResponse>) {
        *self.state.lock_ignore_poison() = SlotState::Awaiting(tx);
    }

    /// Whether a conflict is waiting for an answer right now, i.e. whether the
    /// operation is waiting on a person.
    pub fn is_awaiting(&self) -> bool {
        matches!(&*self.state.lock_ignore_poison(), SlotState::Awaiting(_))
    }

    /// Takes the pending conflict away, dropping its sender. The parked
    /// operation's receiver returns `Err`, which it reads as cancellation. What
    /// a cancel does; afterwards nothing is pending, so a late answer is
    /// truthfully told there's nothing to answer.
    pub fn abandon(&self) {
        *self.state.lock_ignore_poison() = SlotState::Idle;
    }

    /// Delivers `response` to the parked operation, and reports what that did.
    /// Only the first answer to one conflict reaches the operation.
    pub fn answer(&self, response: ConflictResolutionResponse) -> ConflictResolutionOutcome {
        let mut state = self.state.lock_ignore_poison();
        match std::mem::replace(&mut *state, SlotState::Idle) {
            SlotState::Awaiting(tx) => {
                if tx.send(response).is_err() {
                    // The waiting task went away without disarming the slot, so
                    // this answer reached nothing and resolved nothing. Leaves
                    // the slot Idle: there's no conflict here any more.
                    log::warn!("A conflict answer arrived after its operation stopped listening; nothing to resolve");
                    return ConflictResolutionOutcome::NoPendingConflict;
                }
                *state = SlotState::Answered;
                ConflictResolutionOutcome::Resolved
            }
            SlotState::Answered => {
                *state = SlotState::Answered;
                ConflictResolutionOutcome::AlreadyResolved
            }
            SlotState::Idle => ConflictResolutionOutcome::NoPendingConflict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overwrite() -> ConflictResolutionResponse {
        ConflictResolutionResponse {
            resolution: ConflictResolution::Overwrite,
            apply_to_all: false,
        }
    }

    fn skip() -> ConflictResolutionResponse {
        ConflictResolutionResponse {
            resolution: ConflictResolution::Skip,
            apply_to_all: true,
        }
    }

    #[tokio::test]
    async fn the_first_answer_reaches_the_waiter_and_is_reported_as_resolved() {
        let slot = ConflictSlot::new();
        let (tx, rx) = oneshot::channel();
        slot.arm(tx);

        assert_eq!(slot.answer(overwrite()), ConflictResolutionOutcome::Resolved);

        let delivered = rx.await.expect("the waiter gets the answer");
        assert_eq!(delivered.resolution, ConflictResolution::Overwrite);
        assert!(!delivered.apply_to_all);
        assert!(!slot.is_awaiting(), "an answered conflict is no longer waiting");
    }

    #[tokio::test]
    async fn a_second_answer_is_reported_as_already_resolved_and_delivers_nothing() {
        let slot = ConflictSlot::new();
        let (tx, rx) = oneshot::channel();
        slot.arm(tx);

        assert_eq!(slot.answer(overwrite()), ConflictResolutionOutcome::Resolved);
        assert_eq!(slot.answer(skip()), ConflictResolutionOutcome::AlreadyResolved);

        // The operation acted on the FIRST answer; the second changed nothing.
        let delivered = rx.await.expect("the waiter gets the answer");
        assert_eq!(delivered.resolution, ConflictResolution::Overwrite);
    }

    #[test]
    fn an_unarmed_slot_reports_no_pending_conflict() {
        let slot = ConflictSlot::new();
        assert_eq!(slot.answer(skip()), ConflictResolutionOutcome::NoPendingConflict);
    }

    #[test]
    fn an_abandoned_conflict_reports_no_pending_conflict() {
        // What a cancel leaves behind: the sender is dropped (the waiter reads
        // that as cancellation), so there's nothing left to answer.
        let slot = ConflictSlot::new();
        let (tx, mut rx) = oneshot::channel();
        slot.arm(tx);
        slot.abandon();

        assert!(rx.try_recv().is_err(), "abandoning drops the sender");
        assert_eq!(slot.answer(skip()), ConflictResolutionOutcome::NoPendingConflict);
    }

    #[test]
    fn an_answer_nobody_is_listening_for_reports_no_pending_conflict() {
        // The waiting task went away without disarming the slot (it was dropped
        // mid-flight). The answer reaches nothing, so it resolved nothing.
        let slot = ConflictSlot::new();
        let (tx, rx) = oneshot::channel();
        slot.arm(tx);
        drop(rx);

        assert_eq!(slot.answer(skip()), ConflictResolutionOutcome::NoPendingConflict);
        assert!(!slot.is_awaiting());
    }
}
