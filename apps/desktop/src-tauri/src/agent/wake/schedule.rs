//! When the wake loop may next speak, and how long it parks in between.
//!
//! Pure: values in, values out, no clock and no state. `writer.rs` owns the `not_before` stamp
//! these functions compute and the timer they size; everything about WHEN is decided here so it
//! can be tested without an app handle or a thread.

use std::time::Duration;

use super::channel::{WakeCompletion, WakeControl};

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

/// ⚠️ **The least time between two wakes, and the reason it is not the cadence slider.**
///
/// The slider (`WAKE_DELAY_STOPS`, five seconds at its attentive end) says how QUICKLY the agent
/// reacts to something interesting. This says how OFTEN it may speak at all. Conflating the two
/// is what let a five-second setting mean 43 model calls in seven minutes on 2026-09-03, which
/// burned 374,127 tokens and exhausted the user's provider quota.
///
/// Fifteen minutes: an uninvited colleague interrupting four times an hour is already at the edge
/// of welcome, and it caps a miscalibration at 96 wakes a day rather than the roughly 8,800 that
/// day's rate extrapolates to. A force skips it, and so does the follow-up a rejected sweep
/// earns: that one answers something the user just did.
pub(super) const MIN_WAKE_SPACING: Duration = Duration::from_secs(15 * 60);

/// How long the loop stays quiet after the provider refused in a way it will refuse again.
///
/// A rejected key and an exhausted quota are settled facts about the rest of the day, not
/// transient failures worth retrying at the ordinary pace. After the 2026-09-03 quota died at
/// 09:06 local, the app sent 261 further requests over roughly six hours and every one came back
/// 403. Six hours turns that into four attempts, and a settings or readiness change (which is
/// what fixing the key sends) lifts it at once rather than making the user wait it out.
pub(super) const PROVIDER_REFUSAL_BACKOFF: Duration = Duration::from_secs(6 * 60 * 60);

/// How long the loop stays quiet the first time a wake finds the provider overloaded through all
/// of its turn's retries; each busy wake in a row doubles it, up to [`PROVIDER_BUSY_BACKOFF_MAX`].
///
/// The middle tier between the two above, decided 2026-11-04 (#921): an overload is transient and
/// the key is fine, so six hours would punish the user for Google's busy night, but the ordinary
/// fifteen-minute spacing walks a wake straight back into the same overload.
pub(super) const PROVIDER_BUSY_BACKOFF: Duration = Duration::from_secs(30 * 60);

/// The longest the busy tier holds the loop, however many busy wakes in a row.
pub(super) const PROVIDER_BUSY_BACKOFF_MAX: Duration = Duration::from_secs(2 * 60 * 60);

/// The loop's whole "may it speak yet" state: the `not_before` stamp, and how many wakes in a row
/// found the provider busy (what the busy tier doubles by). `writer.rs` holds it; only
/// [`pacing_after`] moves it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Pacing {
    /// Unix seconds before which no wake is attempted.
    pub(super) not_before: u64,
    /// Busy wakes in a row, reset by any wake that got past the overload and by a settings or
    /// readiness change (which can mean another provider).
    pub(super) busy_streak: u32,
}

/// What a control message does to the `not_before` stamp.
enum ControlWait {
    /// Drop the stamp: whatever the loop last decided may no longer hold, so act now.
    Clear,
    /// Leave the stamp alone. The message says nothing about whether the loop may speak.
    Keep,
    /// Stamp this far ahead, from now.
    For(Duration),
}

/// The pacing a control message leaves behind, given the one the loop holds now.
///
/// The whole of the loop's "may it speak yet" policy, in one pure place: a gate moving clears
/// the wait, a wake that spoke imposes one, a busy provider a doubling one, and a provider that
/// refused a long one.
pub(super) fn pacing_after(control: &WakeControl, pacing: Pacing, now: u64) -> Pacing {
    let not_before = match wait_after(control, pacing.busy_streak) {
        ControlWait::Clear => 0,
        ControlWait::Keep => pacing.not_before,
        ControlWait::For(wait) => now.saturating_add(wait.as_secs()),
    };
    let busy_streak = match control {
        WakeControl::WakeFinished(WakeCompletion::ProviderBusy) => pacing.busy_streak.saturating_add(1),
        WakeControl::WakeFinished(WakeCompletion::Wake | WakeCompletion::ProviderRefused)
        | WakeControl::SettingsChanged
        | WakeControl::ReadinessChanged => 0,
        // A follow-up that wasn't busy says nothing about the next wake's provider (it may have
        // ended on anything), and a force or a rejection is the user, not the provider.
        WakeControl::WakeFinished(WakeCompletion::FollowUp)
        | WakeControl::ForceWake(_)
        | WakeControl::SweepRejected { .. } => pacing.busy_streak,
    };
    Pacing {
        not_before,
        busy_streak,
    }
}

/// The busy tier's wait after `streak` busy wakes in a row before this one: 30 minutes, then 60,
/// then two hours from there on.
fn busy_backoff(streak: u32) -> Duration {
    PROVIDER_BUSY_BACKOFF
        .saturating_mul(1 << streak.min(8))
        .min(PROVIDER_BUSY_BACKOFF_MAX)
}

fn wait_after(control: &WakeControl, busy_streak: u32) -> ControlWait {
    match control {
        WakeControl::WakeFinished(WakeCompletion::Wake) => ControlWait::For(MIN_WAKE_SPACING),
        WakeControl::WakeFinished(WakeCompletion::ProviderRefused) => ControlWait::For(PROVIDER_REFUSAL_BACKOFF),
        WakeControl::WakeFinished(WakeCompletion::ProviderBusy) => ControlWait::For(busy_backoff(busy_streak)),
        // A follow-up answers something the user just did, so it neither earns a spacing nor
        // lifts the one a wake left: clearing here would let a rejection the user clicked
        // through pull an unrelated wake in ahead of its turn.
        WakeControl::WakeFinished(WakeCompletion::FollowUp) => ControlWait::Keep,
        // ❌ Never `WakeFinished(_)` with a wildcard: a completion added later would silently
        // inherit the ordinary spacing, which is the whole thing this file exists to prevent.
        WakeControl::SettingsChanged
        | WakeControl::ReadinessChanged
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
    use crate::agent::wake::channel::WakeCompletion;

    /// The policy the tests pin, in seconds: a wake may speak once a quarter of an hour.
    const SPACING_SECS: u64 = 15 * 60;
    /// And a provider that refused is left alone for six hours.
    const REFUSAL_SECS: u64 = 6 * 60 * 60;
    /// A busy provider: half an hour, doubling to two hours.
    const BUSY_SECS: u64 = 30 * 60;
    const BUSY_MAX_SECS: u64 = 2 * 60 * 60;

    /// The stamp a control leaves, from a stamp with no busy streak behind it.
    fn stamp_of(control: &WakeControl, not_before: u64, now: u64) -> u64 {
        pacing_after(
            control,
            Pacing {
                not_before,
                busy_streak: 0,
            },
            now,
        )
        .not_before
    }

    /// ⚠️ **The busy tier sits between the two others.** An overloaded provider is transient
    /// (the key is fine), so six hours would punish the user for a busy night, and the ordinary
    /// spacing walks back into the overload. Half an hour, doubling while it stays busy, never
    /// past two hours.
    #[test]
    fn a_busy_provider_backs_off_half_an_hour_doubling_to_two_hours() {
        let now = 1_780_000_000;
        let busy = WakeControl::WakeFinished(WakeCompletion::ProviderBusy);
        let mut pacing = Pacing::default();
        let mut waits = Vec::new();
        for _ in 0..5 {
            pacing = pacing_after(&busy, pacing, now);
            waits.push(pacing.not_before - now);
        }

        assert_eq!(
            waits,
            vec![BUSY_SECS, 2 * BUSY_SECS, BUSY_MAX_SECS, BUSY_MAX_SECS, BUSY_MAX_SECS]
        );
        const { assert!(BUSY_SECS > SPACING_SECS && BUSY_MAX_SECS < REFUSAL_SECS) };
    }

    /// A wake that got through ends the streak, so the next overload starts again at half an
    /// hour; so does a settings or readiness change, which can mean another provider. A force or
    /// a follow-up that wasn't busy leaves it where it was.
    #[test]
    fn the_busy_streak_resets_once_the_provider_answers_or_the_slot_changes() {
        let now = 1_780_000_000;
        let busy = WakeControl::WakeFinished(WakeCompletion::ProviderBusy);
        let streaked = pacing_after(&busy, pacing_after(&busy, Pacing::default(), now), now);
        assert_eq!(streaked.busy_streak, 2);

        for control in [
            WakeControl::WakeFinished(WakeCompletion::Wake),
            WakeControl::WakeFinished(WakeCompletion::ProviderRefused),
            WakeControl::SettingsChanged,
            WakeControl::ReadinessChanged,
        ] {
            let after = pacing_after(&control, streaked, now);
            assert_eq!(after.busy_streak, 0, "{control:?} ends the streak");
            assert_eq!(
                pacing_after(&busy, after, now).not_before - now,
                BUSY_SECS,
                "after {control:?} the next busy wake waits half an hour again"
            );
        }
        assert_eq!(
            pacing_after(&WakeControl::WakeFinished(WakeCompletion::FollowUp), streaked, now).busy_streak,
            2
        );
    }

    /// Fixing things is felt at once here too: a settings or readiness change lifts a busy wait.
    #[test]
    fn a_settings_or_readiness_change_lifts_a_busy_backoff() {
        let now = 1_780_000_000;
        let held = stamp_of(&WakeControl::WakeFinished(WakeCompletion::ProviderBusy), 0, now);
        assert_eq!(held, now + BUSY_SECS);
        for control in [WakeControl::ReadinessChanged, WakeControl::SettingsChanged] {
            assert_eq!(stamp_of(&control, held, now), 0, "{control:?} lifts it");
        }
    }

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

    /// ⚠️ **The seatbelt.** Two wakes cannot run closer together than [`MIN_WAKE_SPACING`],
    /// however overdue the inbox is and however short the cadence slider is set. A wake that
    /// RAN used to clear the stamp outright, so the next deadline brought the agent straight
    /// back: with the five-second hot cadence that meant back-to-back model calls, which is how
    /// 43 of them happened in seven minutes on 2026-09-03.
    #[test]
    fn two_wakes_cannot_run_closer_together_than_the_spacing() {
        let now = 1_780_000_000;

        let stamp = stamp_of(&WakeControl::WakeFinished(WakeCompletion::Wake), 0, now);

        assert_eq!(stamp, now + SPACING_SECS);
        assert!(
            !may_attempt(false, true, stamp, now + SPACING_SECS - 1),
            "an inbox due a second early still waits"
        );
        assert!(
            may_attempt(false, true, stamp, now + SPACING_SECS),
            "and the spacing expires to the second"
        );
    }

    /// A settings or readiness change is a reason the last decision no longer holds, so it is
    /// felt at once: somebody who just turned the agent on, granted disk access, or set a key
    /// must not wait out a spacing earned before any of that.
    #[test]
    fn a_settings_or_readiness_change_clears_the_wait_immediately() {
        let now = 1_780_000_000;
        let held = now + SPACING_SECS;

        for control in [WakeControl::SettingsChanged, WakeControl::ReadinessChanged] {
            assert_eq!(stamp_of(&control, held, now), 0, "{control:?} has to be felt now");
        }
        assert!(may_attempt(false, true, 0, now));
    }

    /// ⚠️ **A follow-up neither imposes a spacing nor lifts one.** It answers something the user
    /// just did, so it is not the agent speaking uninvited; but clearing the stamp would let a
    /// rejection the user clicked through pull an unrelated wake in ahead of its spacing.
    #[test]
    fn a_follow_up_leaves_the_wakes_spacing_exactly_as_it_found_it() {
        let now = 1_780_000_000;
        let held = now + 600;

        let stamp = stamp_of(&WakeControl::WakeFinished(WakeCompletion::FollowUp), held, now);

        assert_eq!(stamp, held);
        assert_eq!(
            stamp_of(&WakeControl::WakeFinished(WakeCompletion::FollowUp), 0, now),
            0,
            "and it invents no wait where there was none"
        );
    }

    /// ⚠️ **A dead key costs a handful of attempts a day, not hundreds.** After the 2026-09-03
    /// quota died at 09:06 local, the app kept firing: 261 further requests over roughly six
    /// hours, every one rejected. Six hours of quiet turns that into four.
    #[test]
    fn a_provider_that_refused_holds_the_agent_off_for_hours() {
        let now = 1_780_000_000;

        let stamp = stamp_of(&WakeControl::WakeFinished(WakeCompletion::ProviderRefused), 0, now);

        assert_eq!(stamp, now + REFUSAL_SECS);
        assert!(
            !may_attempt(false, true, stamp, now + REFUSAL_SECS - 1),
            "and the inbox filling all day does not shorten it"
        );
    }

    /// ⚠️ **Fixing the key is felt at once.** A readiness change is what setting a working key
    /// sends, and waiting out six hours after the user has already fixed the thing would read as
    /// the agent having given up on them.
    #[test]
    fn a_settings_or_readiness_change_lifts_a_dead_keys_backoff() {
        let now = 1_780_000_000;
        let held = stamp_of(&WakeControl::WakeFinished(WakeCompletion::ProviderRefused), 0, now);

        for control in [WakeControl::ReadinessChanged, WakeControl::SettingsChanged] {
            assert_eq!(stamp_of(&control, held, now), 0, "{control:?} lifts it");
        }
    }

    /// The two clocks a wake waits on, and the one thing that skips them. A force replaces the
    /// timer rather than adding to it, which is the whole point of the developer hook.
    #[test]
    fn a_stamped_wait_holds_a_wake_back_and_a_force_walks_past_it() {
        let now = 1_780_000_000;

        assert!(!may_attempt(false, true, now + 60, now), "the stamp holds it");
        assert!(
            !may_attempt(false, false, 0, now),
            "and so does an inbox with nothing due"
        );
        assert!(may_attempt(false, true, now, now), "the stamp expires to the second");
        assert!(may_attempt(true, false, now + 9_999, now), "a force skips both clocks");
    }
}
