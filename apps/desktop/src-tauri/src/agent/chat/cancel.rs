//! The one registry of in-flight turns, keyed by conversation: who can be stopped, and since
//! which row the running turn has been writing.
//!
//! ⚠️ **It lives here, below `commands/`, because a WAKE has to register too.** A wake is a
//! multi-second background turn spending the user's money, which `docs/design-principles.md`
//! requires be cancelable; its runner sits in `agent/wake/` and may not import upward. Both
//! halves registering in one place is also what lets `ask_cmdr_cancel` stop either one without
//! knowing which it is.
//!
//! **Several registrations per thread, on purpose.** A send registers BEFORE it reaches the
//! thread's single-flight lock (so an immediate Stop finds it), which means a send queued behind
//! a running turn is registered at the same time as that turn. So each registration is its own
//! entry: a Stop trips every one of the thread's turns, the running and the queued alike, and a
//! turn that ends removes only its own.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{LazyLock, Mutex};

use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;
use crate::settings::AskCmdrSwitch;

const LOG_TARGET: &str = "agent::chat";

/// Which turn an event belongs to: the generation of the turn's registration, distinct for every
/// turn for the life of the process. Every event a turn emits carries it (`stream::AskCmdrTurn`),
/// and the send answers it, so the rail can tell a late terminal of a turn it already ended from
/// the one it is showing now. `i64` because it crosses to TypeScript as a `number`.
pub type TurnRun = i64;

/// One thread's live registrations, each tagged with the generation that owns it.
type Registrations = Vec<(TurnRun, CancellationToken)>;

/// Every live registration, by conversation.
static CANCELS: LazyLock<Mutex<HashMap<i64, Registrations>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Hands out a distinct generation per registration, so dropping one never removes another.
static NEXT_GENERATION: AtomicI64 = AtomicI64::new(0);

/// The running turn's seq high-water mark, by conversation: the thread's last `seq` when the
/// turn took the single-flight lock. Rows after it belong to that turn.
static IN_FLIGHT: LazyLock<Mutex<HashMap<i64, i64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// One turn's place in the registry. The token is what the turn watches; dropping the
/// registration removes exactly this entry, so a turn that returns early or panics can't leave
/// a token behind for a later turn on the same thread to be stopped by.
pub struct CancelRegistration {
    conversation_id: i64,
    generation: TurnRun,
    token: CancellationToken,
}

impl CancelRegistration {
    /// The token the turn hands to the runtime.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// The turn this registration belongs to, which every event it emits carries.
    pub fn run(&self) -> TurnRun {
        self.generation
    }
}

impl Drop for CancelRegistration {
    fn drop(&mut self) {
        let mut cancels = CANCELS.lock_ignore_poison();
        if let Some(entries) = cancels.get_mut(&self.conversation_id) {
            entries.retain(|(generation, _)| *generation != self.generation);
            if entries.is_empty() {
                cancels.remove(&self.conversation_id);
            }
        }
    }
}

/// Register a fresh cancel token for one turn of a conversation.
///
/// Called BEFORE the turn's thread is spawned, so a stop arriving immediately still lands on a
/// registered token rather than on an empty map.
pub fn register_cancel(conversation_id: i64) -> CancelRegistration {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    let token = CancellationToken::new();
    CANCELS
        .lock_ignore_poison()
        .entry(conversation_id)
        .or_default()
        .push((generation, token.clone()));
    CancelRegistration {
        conversation_id,
        generation,
        token,
    }
}

/// What one Stop found in a thread's registry: how many terminals the rail should wait for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelOutcome {
    /// Registrations this Stop tripped that no earlier Stop had: each owes a terminal nobody
    /// was waiting for before.
    pub tripped: usize,
    /// Every registration still alive, tripped now or earlier: each still owes its terminal.
    /// Zero means no turn runs or waits in the thread, so no terminal is coming at all.
    pub live: usize,
}

/// Trip every registered turn for a thread: the running one and any queued behind it.
/// Idempotent: an unknown id (already finished, or never started) trips nothing. A clean stop at
/// the next tool boundary or stream chunk, ❌ never a hard abort.
pub fn cancel_turn(conversation_id: i64) -> CancelOutcome {
    let cancels = CANCELS.lock_ignore_poison();
    let Some(entries) = cancels.get(&conversation_id) else {
        return CancelOutcome { tripped: 0, live: 0 };
    };
    let mut tripped = 0;
    for (_, token) in entries {
        if !token.is_cancelled() {
            token.cancel();
            tripped += 1;
        }
    }
    CancelOutcome {
        tripped,
        live: entries.len(),
    }
}

/// Trip every in-flight turn, rail and wake alike: cloud AI was just turned off. Returns how
/// many were tripped. Entries stay registered; each turn's registration removes itself as it
/// winds down.
pub fn cancel_all() -> usize {
    let cancels = CANCELS.lock_ignore_poison();
    let mut tripped = 0;
    for (_, token) in cancels.values().flatten() {
        token.cancel();
        tripped += 1;
    }
    tripped
}

/// Ask Cmdr's switch moved: unless it's on, trip every turn, rail and wake alike. Off, the rail
/// shows its gate where the composer was, so a turn left running would have no Stop to press.
/// Each tripped turn ends with its own `Stopped`, which is what tells the rail.
pub fn stop_all_unless_on(switch: AskCmdrSwitch) {
    if switch == AskCmdrSwitch::On {
        return;
    }
    let tripped = cancel_all();
    if tripped > 0 {
        log::info!(target: LOG_TARGET, "Ask Cmdr turned off: stopped {tripped} turn(s)");
    }
}

/// Marks a conversation's turn as running since `since_seq` until dropped. Taken by the
/// single-flight wrapper once it holds the thread's lock, so at most one mark per thread exists.
pub(crate) struct InFlightMark(i64);

impl Drop for InFlightMark {
    fn drop(&mut self) {
        IN_FLIGHT.lock_ignore_poison().remove(&self.0);
    }
}

/// Record that a turn now runs in `conversation_id`, writing rows after `since_seq`.
pub(crate) fn mark_in_flight(conversation_id: i64, since_seq: i64) -> InFlightMark {
    IN_FLIGHT.lock_ignore_poison().insert(conversation_id, since_seq);
    InFlightMark(conversation_id)
}

/// The running turn's seq high-water mark, or `None` when no turn runs in the thread. A reload
/// reads it to tell the running turn's result-less calls from a crashed turn's.
pub fn turn_in_flight_since_seq(conversation_id: i64) -> Option<i64> {
    IN_FLIGHT.lock_ignore_poison().get(&conversation_id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is a process-global, so the ids here are deliberately far from anything the
    /// other tests use.
    #[test]
    fn a_registered_turn_can_be_stopped_and_an_unknown_one_is_a_no_op() {
        let registration = register_cancel(-9_001);
        let token = registration.token();
        assert!(!token.is_cancelled());

        cancel_turn(-9_002); // nothing running under that id
        assert!(
            !token.is_cancelled(),
            "a stop for another thread must not touch this one"
        );

        assert_eq!(cancel_turn(-9_001), CancelOutcome { tripped: 1, live: 1 });
        assert!(token.is_cancelled());
    }

    /// ⚠️ The rail waits for one terminal per turn a Stop reaches. A second Stop of a turn still
    /// winding down owes no new terminal, but the turn is live: its first one is still coming. And
    /// a Stop that reaches nothing must say so, or the rail waits for a terminal that never comes
    /// and takes the thread's NEXT turn for it.
    #[test]
    fn a_second_stop_trips_nothing_new_but_still_sees_the_live_turn() {
        let running = register_cancel(-9_011);

        assert_eq!(cancel_turn(-9_011), CancelOutcome { tripped: 1, live: 1 });
        assert_eq!(cancel_turn(-9_011), CancelOutcome { tripped: 0, live: 1 });

        drop(running);
        assert_eq!(cancel_turn(-9_011), CancelOutcome { tripped: 0, live: 0 });
    }

    /// Turning cloud AI off stops every running turn at once, rail and wake alike, whatever
    /// thread they belong to.
    #[test]
    fn cancel_all_trips_every_registered_turn() {
        let first = register_cancel(-9_101);
        let second = register_cancel(-9_102);

        assert!(cancel_all() >= 2, "both turns were registered");
        assert!(first.token().is_cancelled());
        assert!(second.token().is_cancelled());
    }

    /// ⚠️ A send registers before it waits for the thread's lock, so a queued send is registered
    /// while the running turn still is. It used to overwrite the running turn's token, which made
    /// that turn unstoppable.
    #[test]
    fn a_queued_send_never_replaces_the_running_turns_token() {
        let running = register_cancel(-9_201);
        let _queued = register_cancel(-9_201);

        cancel_turn(-9_201);

        assert!(running.token().is_cancelled());
    }

    /// A Stop pressed while a send is queued stops the queued send as well as the running turn:
    /// the user asked the thread to stop, not one of its turns.
    #[test]
    fn a_stop_during_queued_trips_both_turns() {
        let running = register_cancel(-9_301);
        let queued = register_cancel(-9_301);

        assert_eq!(cancel_turn(-9_301), CancelOutcome { tripped: 2, live: 2 });

        assert!(running.token().is_cancelled());
        assert!(queued.token().is_cancelled());
    }

    /// The running turn ending must take only its own entry: the queued turn that starts next has
    /// to stay stoppable.
    #[test]
    fn ending_one_turn_leaves_the_other_registered() {
        let running = register_cancel(-9_401);
        let queued = register_cancel(-9_401);

        drop(running);
        cancel_turn(-9_401);

        assert!(queued.token().is_cancelled());
    }

    /// ⚠️ A turn that returned early must not leave its token behind: the next turn on the same
    /// thread would then be cancellable by a stop meant for the previous one.
    #[test]
    fn dropping_the_registration_clears_the_entry() {
        let registration = register_cancel(-9_003);
        let token = registration.token();
        drop(registration);

        cancel_turn(-9_003);
        assert!(
            !token.is_cancelled(),
            "the entry was gone, so there was nothing to trip"
        );
    }

    /// ⚠️ With Ask Cmdr off, the rail shows its gate in place of the composer, so a turn still
    /// running would have no Stop left to press and would keep spending.
    #[test]
    fn turning_ask_cmdr_off_stops_every_running_turn() {
        let running = register_cancel(-9_601);

        stop_all_unless_on(AskCmdrSwitch::On);
        assert!(!running.token().is_cancelled(), "a switch that stayed on stops nothing");

        stop_all_unless_on(AskCmdrSwitch::Off);
        assert!(running.token().is_cancelled());

        let managed = register_cancel(-9_602);
        stop_all_unless_on(AskCmdrSwitch::ManagedOff);
        assert!(
            managed.token().is_cancelled(),
            "a policy pinning it off stops turns too"
        );
    }

    #[test]
    fn the_in_flight_mark_reads_back_until_the_turn_ends() {
        assert_eq!(turn_in_flight_since_seq(-9_501), None);

        let mark = mark_in_flight(-9_501, 41);
        assert_eq!(turn_in_flight_since_seq(-9_501), Some(41));

        drop(mark);
        assert_eq!(turn_in_flight_since_seq(-9_501), None);
    }
}
