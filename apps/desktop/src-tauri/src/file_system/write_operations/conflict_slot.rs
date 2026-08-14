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
//!
//! Each state that holds a question also holds its [`ConflictId`], and an
//! answer has to name one. An operation raises its clashes one at a time, but
//! an answer's round trip (broadcast out, person, IPC back) can outlast the
//! clash it belongs to, so "the answer that just arrived" and "the question on
//! screen right now" are two different things. Matching the ids is what keeps a
//! late answer for a retired clash from deciding the one parked now, and it is
//! also what lets the slot report that honestly instead of confidently wrong.
//!
//! The armed state holds the QUESTION too, not just its id. `write-conflict` is
//! a broadcast, so it only reaches whoever was listening at the moment it went
//! out; anyone who arrives later (`cmdr://state`, an agent polling the
//! operation) would otherwise know an answer is owed and have no way to read
//! what it is owed about. Building the event is part of arming for the same
//! reason minting the id is: one transition under one lock, and no way to park
//! an operation on a question nobody can read back.

use crate::ignore_poison::IgnorePoison;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use super::human_wait::{HumanWaitClock, HumanWaitSource};
use super::types::{ConflictId, ConflictResolution, ConflictResolutionOutcome, WriteConflictEvent};

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
    /// A conflict is on screen. This sender unblocks the parked operation, and
    /// `prompt` is the question it went out with, readable by anyone who wasn't
    /// listening for the broadcast. Boxed: it's much the largest thing in this
    /// enum, and every other state would carry its size.
    Awaiting {
        id: ConflictId,
        tx: oneshot::Sender<ConflictResolutionResponse>,
        prompt: Box<WriteConflictEvent>,
    },
    /// The conflict was answered, and the operation carried on with that
    /// answer. Another answer naming it is a second opinion; one naming
    /// anything else is late for a question that's over.
    Answered { id: ConflictId },
}

/// The slot's contents: where the conflict stands, and how many this operation
/// has raised. Both live under one lock so minting an id and arming with it are
/// one transition — two ids for one question, or one id for two, would defeat
/// the point of having them.
struct Slot {
    state: SlotState,
    raised: u64,
}

/// Holds the sender a parked operation is listening on, and answers "did MY
/// answer land?" for every surface that tries.
pub struct ConflictSlot {
    inner: Mutex<Slot>,
    /// The operation's human-wait accounting. A clash on screen is the transfer
    /// waiting for somebody to decide, and the ETA must not charge it for that
    /// time. Kept in step with the slot's own state under the slot's lock — see
    /// [`sync_human_wait`](Self::sync_human_wait).
    human_wait: Arc<HumanWaitClock>,
}

impl ConflictSlot {
    pub fn new(human_wait: Arc<HumanWaitClock>) -> Self {
        Self {
            inner: Mutex::new(Slot {
                state: SlotState::Idle,
                raised: 0,
            }),
            human_wait,
        }
    }

    /// Mirrors "a clash is on screen" onto the human-wait clock. Called from
    /// every transition, DERIVED from the state that transition just left
    /// behind, so no path can arm the prompt and forget to start the clock (or
    /// answer it and leave the clock running, which would freeze the rate for
    /// the rest of the transfer).
    fn sync_human_wait(&self, slot: &Slot) {
        self.human_wait.set(
            HumanWaitSource::Conflict,
            matches!(slot.state, SlotState::Awaiting { .. }),
        );
    }

    /// Arms the slot with the sender the waiting operation listens on, and
    /// returns the `write-conflict` event to emit for it. Call this BEFORE
    /// emitting: a responder can only answer a conflict it has observed, and an
    /// answer arriving at an unarmed slot leaves the operation parked forever.
    ///
    /// `prompt` builds the event from the id this call mints, so the id on the
    /// wire, the id the slot is armed with, and the id
    /// [`answer`](Self::answer) requires back are one value by construction —
    /// and the question is stored where [`pending`](Self::pending) can read it
    /// out long after the broadcast is over.
    pub fn arm(
        &self,
        tx: oneshot::Sender<ConflictResolutionResponse>,
        prompt: impl FnOnce(ConflictId) -> WriteConflictEvent,
    ) -> WriteConflictEvent {
        let mut inner = self.inner.lock_ignore_poison();
        inner.raised = inner.raised.saturating_add(1);
        let id = ConflictId(inner.raised);
        let event = prompt(id);
        inner.state = SlotState::Awaiting {
            id,
            tx,
            prompt: Box::new(event.clone()),
        };
        self.sync_human_wait(&inner);
        event
    }

    /// Whether a conflict is waiting for an answer right now, i.e. whether the
    /// operation is waiting on a person.
    pub fn is_awaiting(&self) -> bool {
        matches!(self.inner.lock_ignore_poison().state, SlotState::Awaiting { .. })
    }

    /// The question this operation is parked on, for a reader that wasn't around
    /// when it was broadcast. `None` once it's answered, abandoned, or was never
    /// raised — the same three cases [`answer`](Self::answer) refuses.
    pub fn pending(&self) -> Option<WriteConflictEvent> {
        match &self.inner.lock_ignore_poison().state {
            SlotState::Awaiting { prompt, .. } => Some((**prompt).clone()),
            SlotState::Idle | SlotState::Answered { .. } => None,
        }
    }

    /// Takes the pending conflict away, dropping its sender. The parked
    /// operation's receiver returns `Err`, which it reads as cancellation. What
    /// a cancel does; afterwards nothing is pending, so a late answer is
    /// truthfully told there's nothing to answer.
    pub fn abandon(&self) {
        let mut inner = self.inner.lock_ignore_poison();
        inner.state = SlotState::Idle;
        self.sync_human_wait(&inner);
    }

    /// Delivers `response` to the parked operation IF `conflict` is the clash it
    /// is parked on, and reports what that did. Only the first answer to one
    /// conflict reaches the operation, and an answer for any other conflict
    /// reaches nothing at all.
    pub fn answer(&self, conflict: ConflictId, response: ConflictResolutionResponse) -> ConflictResolutionOutcome {
        let mut inner = self.inner.lock_ignore_poison();
        let outcome = match std::mem::replace(&mut inner.state, SlotState::Idle) {
            SlotState::Awaiting { id, tx, .. } if id == conflict => {
                if tx.send(response).is_err() {
                    // The waiting task went away without disarming the slot, so
                    // this answer reached nothing and resolved nothing. Leaves
                    // the slot Idle: there's no conflict here any more.
                    log::warn!("A conflict answer arrived after its operation stopped listening; nothing to resolve");
                    ConflictResolutionOutcome::NoPendingConflict
                } else {
                    inner.state = SlotState::Answered { id };
                    ConflictResolutionOutcome::Resolved
                }
            }
            // A different question is on screen. Put it back untouched: this
            // answer was never about it, and the person it IS on screen for
            // hasn't clicked yet.
            SlotState::Awaiting { id, tx, prompt } => {
                inner.state = SlotState::Awaiting { id, tx, prompt };
                log::info!(
                    "A conflict answer for {conflict:?} arrived while the operation is parked on {id:?}; refusing it"
                );
                ConflictResolutionOutcome::StaleAnswer
            }
            SlotState::Answered { id } => {
                inner.state = SlotState::Answered { id };
                if id == conflict {
                    ConflictResolutionOutcome::AlreadyResolved
                } else {
                    log::info!("A conflict answer for {conflict:?} arrived after {id:?} was answered; refusing it");
                    ConflictResolutionOutcome::StaleAnswer
                }
            }
            SlotState::Idle => ConflictResolutionOutcome::NoPendingConflict,
        };
        // Whatever just happened, the clock follows the slot: an answered or
        // refused clash stops charging the person, a still-parked one keeps on.
        self.sync_human_wait(&inner);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::test_support::placeholder_conflict;
    use std::time::{Duration, Instant};

    /// Arms `slot` and hands back the clash's id. These tests are about the
    /// arbitration, so the question itself is a placeholder; the one test that
    /// cares reads it back through `pending`.
    fn park(slot: &ConflictSlot, tx: oneshot::Sender<ConflictResolutionResponse>) -> ConflictId {
        slot.arm(tx, placeholder_conflict).conflict_id
    }

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
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, rx) = oneshot::channel();
        let clash = park(&slot, tx);

        assert_eq!(slot.answer(clash, overwrite()), ConflictResolutionOutcome::Resolved);

        let delivered = rx.await.expect("the waiter gets the answer");
        assert_eq!(delivered.resolution, ConflictResolution::Overwrite);
        assert!(!delivered.apply_to_all);
        assert!(!slot.is_awaiting(), "an answered conflict is no longer waiting");
    }

    #[tokio::test]
    async fn a_second_answer_is_reported_as_already_resolved_and_delivers_nothing() {
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, rx) = oneshot::channel();
        let clash = park(&slot, tx);

        assert_eq!(slot.answer(clash, overwrite()), ConflictResolutionOutcome::Resolved);
        assert_eq!(slot.answer(clash, skip()), ConflictResolutionOutcome::AlreadyResolved);

        // The operation acted on the FIRST answer; the second changed nothing.
        let delivered = rx.await.expect("the waiter gets the answer");
        assert_eq!(delivered.resolution, ConflictResolution::Overwrite);
    }

    #[tokio::test]
    async fn the_seconds_a_clash_sits_on_screen_are_charged_to_the_person() {
        // The rate window subtracts this clock, so the transfer isn't blamed for
        // the minutes a prompt spent waiting for a decision. Both ends matter: a
        // clock left running would freeze the rate for the rest of the copy.
        let clock = HumanWaitClock::shared();
        let slot = ConflictSlot::new(Arc::clone(&clock));
        let start = Instant::now();
        assert_eq!(clock.total_at(start), Duration::ZERO);

        let (tx, rx) = oneshot::channel();
        let clash = park(&slot, tx);
        let armed_at = Instant::now();
        assert!(
            clock.total_at(armed_at + Duration::from_secs(2)) >= Duration::from_secs(2),
            "an unanswered clash is time spent waiting on a person",
        );

        assert_eq!(slot.answer(clash, overwrite()), ConflictResolutionOutcome::Resolved);
        rx.await.expect("the waiter gets the answer");
        let banked = clock.total_at(Instant::now());
        assert_eq!(
            banked,
            clock.total_at(Instant::now() + Duration::from_secs(5)),
            "the answer closes the clock, so the copying that follows counts as the transfer's own",
        );
    }

    #[test]
    fn abandoning_a_clash_stops_charging_the_person() {
        // What a cancel leaves behind. Nobody is being asked anything any more,
        // so the clock has to close even though no answer ever arrived.
        let clock = HumanWaitClock::shared();
        let slot = ConflictSlot::new(Arc::clone(&clock));
        let (tx, _rx) = oneshot::channel();
        park(&slot, tx);
        slot.abandon();

        let banked = clock.total_at(Instant::now());
        assert_eq!(banked, clock.total_at(Instant::now() + Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn the_question_is_readable_for_as_long_as_it_is_unanswered() {
        // What a late reader (`cmdr://state`, an agent polling the operation)
        // depends on: the broadcast is long over, and the question is still
        // there to be read — until it's answered, when there is no question.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, rx) = oneshot::channel();
        let event = slot.arm(tx, placeholder_conflict);

        let seen = slot.pending().expect("a parked operation can say what it is asking");
        assert_eq!(seen.conflict_id, event.conflict_id);
        assert_eq!(seen.destination_path, event.destination_path);

        assert_eq!(
            slot.answer(event.conflict_id, overwrite()),
            ConflictResolutionOutcome::Resolved
        );
        rx.await.expect("the waiter gets the answer");
        assert!(slot.pending().is_none(), "an answered clash is no longer a question");
    }

    #[test]
    fn an_abandoned_question_is_no_longer_readable() {
        // A cancel takes the question away, so nothing may still advertise it as
        // answerable: an agent that answered it would be told there's nothing
        // pending, and the prompt it read came from here.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, _rx) = oneshot::channel();
        slot.arm(tx, placeholder_conflict);
        slot.abandon();

        assert!(slot.pending().is_none());
    }

    #[test]
    fn an_unarmed_slot_reports_no_pending_conflict() {
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        assert_eq!(
            slot.answer(ConflictId(1), skip()),
            ConflictResolutionOutcome::NoPendingConflict
        );
    }

    #[test]
    fn an_abandoned_conflict_reports_no_pending_conflict() {
        // What a cancel leaves behind: the sender is dropped (the waiter reads
        // that as cancellation), so there's nothing left to answer.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, mut rx) = oneshot::channel();
        let clash = park(&slot, tx);
        slot.abandon();

        assert!(rx.try_recv().is_err(), "abandoning drops the sender");
        assert_eq!(slot.answer(clash, skip()), ConflictResolutionOutcome::NoPendingConflict);
    }

    #[tokio::test]
    async fn an_answer_for_a_retired_conflict_never_lands_on_the_one_parked_now() {
        // The wedge this guards: an answer for clash A, sent while A was on
        // screen and arriving after the operation has already parked on B. B is
        // a different question, so A's answer must not resolve it — and B's
        // sender must survive, because the person looking at B hasn't clicked.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx_a, rx_a) = oneshot::channel();
        let a = park(&slot, tx_a);
        assert_eq!(slot.answer(a, overwrite()), ConflictResolutionOutcome::Resolved);
        rx_a.await.expect("A's own answer reaches A");

        let (tx_b, mut rx_b) = oneshot::channel();
        let b = park(&slot, tx_b);
        assert_ne!(a, b, "each clash is raised under its own id");

        // A's late answer, arriving now.
        assert_eq!(
            slot.answer(a, skip()),
            ConflictResolutionOutcome::StaleAnswer,
            "an answer for a retired clash is refused, not reported as resolving the live one"
        );
        assert!(rx_b.try_recv().is_err(), "B is still waiting for an answer of its own");
        assert!(slot.is_awaiting(), "the refusal leaves B on screen, unanswered");

        // B's own answer still works, and reaches B.
        assert_eq!(slot.answer(b, skip()), ConflictResolutionOutcome::Resolved);
        let delivered = rx_b.await.expect("B gets the answer that named B");
        assert_eq!(delivered.resolution, ConflictResolution::Skip);
    }

    #[tokio::test]
    async fn an_answer_for_a_clash_the_operation_left_behind_is_refused() {
        // Same shape without a second question on screen: the operation answered
        // A and carried on. A second answer naming A is a second opinion
        // (`AlreadyResolved`), but one naming a clash that was never the last
        // question is late — the slot says so rather than acting on it.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx_a, rx_a) = oneshot::channel();
        let a = park(&slot, tx_a);
        assert_eq!(slot.answer(a, overwrite()), ConflictResolutionOutcome::Resolved);
        rx_a.await.expect("A's answer reaches A");

        let (tx_b, rx_b) = oneshot::channel();
        let b = park(&slot, tx_b);
        assert_eq!(slot.answer(b, skip()), ConflictResolutionOutcome::Resolved);
        rx_b.await.expect("B's answer reaches B");

        assert_eq!(slot.answer(a, overwrite()), ConflictResolutionOutcome::StaleAnswer);
        assert_eq!(slot.answer(b, overwrite()), ConflictResolutionOutcome::AlreadyResolved);
    }

    #[test]
    fn an_answer_nobody_is_listening_for_reports_no_pending_conflict() {
        // The waiting task went away without disarming the slot (it was dropped
        // mid-flight). The answer reaches nothing, so it resolved nothing.
        let slot = ConflictSlot::new(HumanWaitClock::shared());
        let (tx, rx) = oneshot::channel();
        let clash = park(&slot, tx);
        drop(rx);

        assert_eq!(slot.answer(clash, skip()), ConflictResolutionOutcome::NoPendingConflict);
        assert!(!slot.is_awaiting());
    }
}
