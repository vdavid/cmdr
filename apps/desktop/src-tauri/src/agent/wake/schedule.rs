//! When the wake loop may next speak, and how long it parks in between.
//!
//! Pure: values in, values out, no clock and no state. `writer.rs` owns the `not_before` stamp
//! these functions compute and the timer they size; everything about WHEN is decided here so it
//! can be tested without an app handle or a thread.

use std::time::Duration;

use super::channel::WakeControl;

/// How long the loop waits with nothing scheduled. It would be correct to wait forever (every
/// arrival is a message), but a bounded park means a clock jump or a missed re-arm costs one
/// minute of latency rather than a loop that never wakes again.
pub(super) const IDLE_POLL: Duration = Duration::from_secs(60);

/// ⚠️ **How long a declined attempt waits before trying again, and why the loop needs one at
/// all.** A deadline that has passed stays passed. Without a backoff the park would compute to
/// zero, `recv_timeout` would return instantly, and the loop would spin a core flat for as long
/// as an overdue row sits there — the ordinary state for anybody without consent or an API key.
/// A gate or settings change clears it, so opening the gate is felt at once.
pub(super) const DECLINED_WAKE_BACKOFF: Duration = Duration::from_secs(5 * 60);

/// Whether the loop may attempt a wake at all right now.
///
/// A force is a developer explicitly asking for one, so it skips both clocks: the inbox's
/// deadlines and the `not_before` stamp that carries every backoff and the spacing.
pub(super) fn may_attempt(forced: bool, inbox_due: bool, not_before: u64, now: u64) -> bool {
    forced || (now >= not_before && inbox_due)
}

/// What a control message does to the `not_before` stamp.
pub(super) enum ControlWait {
    /// Drop the stamp: whatever the loop last decided may no longer hold, so act now.
    Clear,
}

/// How a control message moves the stamp. See [`ControlWait`].
pub(super) fn wait_after(control: &WakeControl) -> ControlWait {
    match control {
        WakeControl::SettingsChanged
        | WakeControl::ReadinessChanged
        | WakeControl::WakeFinished
        | WakeControl::ForceWake(_)
        | WakeControl::SweepRejected { .. } => ControlWait::Clear,
    }
}

/// Fold a waiting follow-up's coalescing window into the park the inbox asked for.
///
/// ⚠️ **An OVERDUE follow-up must not shorten the park**, which is the same spin trap
/// [`park_for`] guards for the inbox and it bites for a different reason. A window that has
/// closed and is still waiting means this pass declined to act on it, and the only reason it
/// can is that a background turn is already running. That turn takes minutes, and a
/// zero-length `recv_timeout` for its whole duration would spin a core flat. The
/// `WakeFinished` control message wakes the loop the moment it can be acted on.
pub(super) fn park_with_follow_up(wake: Duration, next_follow_up: Option<u64>, now: u64) -> Duration {
    match next_follow_up {
        Some(due) if due > now => wake.min(Duration::from_secs(due - now)),
        _ => wake,
    }
}

/// How long to park, given what is waiting and when a wake may next be attempted.
///
/// ⚠️ Capped at [`IDLE_POLL`] and floored by `not_before`. The floor is the load-bearing half: a
/// deadline in the past yields a zero-length park, and a zero-length `recv_timeout` returns
/// instantly, so without it an overdue row the loop declines to act on spins a core flat.
pub(super) fn park_for(next_deadline: Option<u64>, not_before: u64, now: u64) -> Duration {
    let Some(due) = next_deadline else {
        return IDLE_POLL;
    };
    Duration::from_secs(due.max(not_before).saturating_sub(now)).min(IDLE_POLL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ The spin guard. A deadline that has passed keeps having passed, so a park computed
    /// from it alone is zero-length, and a zero-length `recv_timeout` returns instantly. An
    /// overdue row the loop declines to act on is the ordinary state for anybody without
    /// consent or an API key — this is the difference between a parked thread and a hot core.
    #[test]
    fn an_overdue_row_the_loop_declined_parks_instead_of_spinning() {
        let overdue = Some(1_780_000_000);
        let now = 1_780_000_500;

        assert_eq!(park_for(overdue, 0, now), Duration::ZERO, "the deadline alone says now");
        assert_eq!(
            park_for(overdue, now + 30, now),
            Duration::from_secs(30),
            "the backoff is what keeps the thread asleep"
        );
        assert_eq!(
            park_for(overdue, now + DECLINED_WAKE_BACKOFF.as_secs(), now),
            IDLE_POLL,
            "a longer backoff still re-checks once a minute; `try_wake`'s own guard holds the rest"
        );
    }

    /// A deadline still ahead is honoured to the second, so the agent stays as attentive as the
    /// cadence setting asks.
    #[test]
    fn a_future_deadline_is_parked_for_exactly() {
        assert_eq!(park_for(Some(1_780_000_030), 0, 1_780_000_000), Duration::from_secs(30));
    }

    /// An empty inbox, or one holding only cold rows, waits out the idle poll rather than
    /// forever: a clock jump or a missed re-arm then costs a minute, not the rest of the run.
    #[test]
    fn nothing_waiting_falls_back_to_the_idle_poll() {
        assert_eq!(park_for(None, 0, 1_780_000_000), IDLE_POLL);
        assert_eq!(
            park_for(Some(1_780_009_999), 0, 1_780_000_000),
            IDLE_POLL,
            "and a distant deadline is capped, so the loop re-checks its own arithmetic"
        );
    }

    /// A rejection's coalescing window is the second clock the loop parks against, so a window
    /// closing soon has to shorten the wait: otherwise the ask sits until something unrelated
    /// wakes the loop.
    #[test]
    fn a_coalescing_window_closing_soon_shortens_the_park() {
        let now = 1_780_000_000;

        assert_eq!(
            park_with_follow_up(IDLE_POLL, Some(now + 5), now),
            Duration::from_secs(5)
        );
        assert_eq!(
            park_with_follow_up(Duration::from_secs(2), Some(now + 5), now),
            Duration::from_secs(2),
            "and the shorter of the two clocks wins"
        );
    }

    /// ⚠️ The follow-up half of the spin guard. A window that closed and is STILL waiting means
    /// a background turn is running, and that turn takes minutes: a zero-length park for its
    /// whole duration would spin a core flat. `WakeFinished` is what wakes the loop instead.
    #[test]
    fn an_overdue_follow_up_the_loop_declined_parks_instead_of_spinning() {
        let now = 1_780_000_000;

        assert_eq!(park_with_follow_up(IDLE_POLL, Some(now - 300), now), IDLE_POLL);
        assert_eq!(park_with_follow_up(IDLE_POLL, Some(now), now), IDLE_POLL);
        assert_eq!(park_with_follow_up(IDLE_POLL, None, now), IDLE_POLL);
    }

    /// The two clocks a wake waits on, and the one thing that skips them. A force replaces the
    /// timer rather than adding to it, which is the whole point of the developer hook.
    #[test]
    fn a_stamped_wait_holds_a_wake_back_and_a_force_walks_past_it() {
        let now = 1_780_000_000;

        assert!(!may_attempt(false, true, now + 60, now), "the stamp holds it");
        assert!(!may_attempt(false, false, 0, now), "and so does an inbox with nothing due");
        assert!(may_attempt(false, true, now, now), "the stamp expires to the second");
        assert!(may_attempt(true, false, now + 9_999, now), "a force skips both clocks");
    }
}
