//! How long an operation has spent waiting on a PERSON rather than on itself.
//!
//! A transfer parked on a conflict prompt, or paused by the user, moves no
//! bytes — and that is not slowness, it is somebody thinking. The rate
//! estimator (`super::eta`) subtracts this clock's reading from the wall time
//! between two progress samples, so the seconds a prompt sat open never land in
//! the throughput window. Without it, the first sample after a five-minute
//! answer divides one file's worth of bytes by five minutes and the dialog
//! reports "0.4 files/s, 409h 39m left" over a transfer that is running fine.
//!
//! ❌ Only a wait on a PERSON belongs here. A slow SMB read, a busy device, a
//! retry backoff are all the operation being slow, and the ETA has to say so.
//!
//! ## Why a clock rather than two predicates
//!
//! `PauseGate::is_paused()` and `ConflictSlot::is_awaiting()` answer "right
//! now", and the estimator needs "how much, since the last sample" — a wait
//! that opens and closes between two progress events is invisible to a
//! predicate. Both of those owners drive this clock from inside the same
//! critical section that flips their own state, so the two can't drift.
//!
//! The two sources are tracked separately and reported as a UNION: the main
//! window pauses an operation before prompting about its clash, so the two
//! intervals routinely overlap, and summing them would subtract that time
//! twice and leave real working seconds excluded afterwards.

use crate::ignore_poison::IgnorePoison;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Who is holding the operation up. One open interval each; the clock reports
/// their union.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanWaitSource {
    /// The user pressed Pause.
    Pause,
    /// A conflict prompt is on screen, unanswered.
    Conflict,
}

impl HumanWaitSource {
    const fn index(self) -> usize {
        match self {
            Self::Pause => 0,
            Self::Conflict => 1,
        }
    }
}

#[derive(Debug)]
struct ClockState {
    /// Which sources are waiting right now, indexed by [`HumanWaitSource::index`].
    waiting: [bool; 2],
    /// When the current union interval opened, or `None` while nobody waits.
    since: Option<Instant>,
    /// Every closed interval, added up.
    total: Duration,
}

/// One operation's human-wait accounting. Monotonic: the total only grows.
#[derive(Debug)]
pub struct HumanWaitClock {
    inner: Mutex<ClockState>,
}

impl HumanWaitClock {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ClockState {
                waiting: [false; 2],
                since: None,
                total: Duration::ZERO,
            }),
        }
    }

    /// A clock several owners share. `PauseGate` and `ConflictSlot` each hold
    /// one of these, handed to them by the operation state that owns the clock.
    pub fn shared() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new())
    }

    /// Opens or closes `source`'s interval. Idempotent: setting what is already
    /// set changes nothing, so a caller may simply mirror its own state.
    pub fn set(&self, source: HumanWaitSource, waiting: bool) {
        self.set_at(source, waiting, Instant::now());
    }

    /// [`set`](Self::set) against an injected clock, for tests that drive a
    /// synthetic timeline.
    pub fn set_at(&self, source: HumanWaitSource, waiting: bool, now: Instant) {
        let mut inner = self.inner.lock_ignore_poison();
        inner.waiting[source.index()] = waiting;
        let any = inner.waiting.iter().any(|w| *w);
        match (any, inner.since) {
            // The union opened.
            (true, None) => inner.since = Some(now),
            // The union closed: bank the interval.
            (false, Some(started)) => {
                inner.total += now.saturating_duration_since(started);
                inner.since = None;
            }
            // Still waiting on someone, or still waiting on nobody.
            (true, Some(_)) | (false, None) => {}
        }
    }

    /// Total time this operation has spent waiting on a person, counting an
    /// interval that is still open up to `now`.
    pub fn total_at(&self, now: Instant) -> Duration {
        let inner = self.inner.lock_ignore_poison();
        match inner.since {
            Some(started) => inner.total + now.saturating_duration_since(started),
            None => inner.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_fresh_clock_has_waited_on_nobody() {
        let start = Instant::now();
        assert_eq!(HumanWaitClock::new().total_at(at(start, 5_000)), Duration::ZERO);
    }

    #[test]
    fn one_closed_interval_is_banked() {
        let start = Instant::now();
        let clock = HumanWaitClock::new();
        clock.set_at(HumanWaitSource::Conflict, true, at(start, 1_000));
        clock.set_at(HumanWaitSource::Conflict, false, at(start, 4_000));
        assert_eq!(clock.total_at(at(start, 10_000)), Duration::from_secs(3));
    }

    #[test]
    fn an_open_interval_counts_up_to_now() {
        let start = Instant::now();
        let clock = HumanWaitClock::new();
        clock.set_at(HumanWaitSource::Pause, true, at(start, 1_000));
        assert_eq!(clock.total_at(at(start, 3_500)), Duration::from_millis(2_500));
        // And it keeps growing until somebody closes it.
        assert_eq!(clock.total_at(at(start, 5_000)), Duration::from_secs(4));
    }

    #[test]
    fn overlapping_sources_count_once() {
        // What the main window's conflict host does: pause the operation, then
        // prompt about the clash, then resume. Summing the two intervals would
        // charge the same seconds twice and leave real working time excluded
        // afterwards.
        let start = Instant::now();
        let clock = HumanWaitClock::new();
        clock.set_at(HumanWaitSource::Pause, true, at(start, 1_000));
        clock.set_at(HumanWaitSource::Conflict, true, at(start, 1_200));
        clock.set_at(HumanWaitSource::Conflict, false, at(start, 6_000));
        clock.set_at(HumanWaitSource::Pause, false, at(start, 6_200));

        assert_eq!(
            clock.total_at(at(start, 10_000)),
            Duration::from_millis(5_200),
            "the union runs from the first source opening to the last one closing",
        );
    }

    #[test]
    fn repeating_a_state_changes_nothing() {
        let start = Instant::now();
        let clock = HumanWaitClock::new();
        clock.set_at(HumanWaitSource::Pause, false, at(start, 500));
        clock.set_at(HumanWaitSource::Pause, true, at(start, 1_000));
        clock.set_at(HumanWaitSource::Pause, true, at(start, 3_000));
        clock.set_at(HumanWaitSource::Pause, false, at(start, 4_000));
        clock.set_at(HumanWaitSource::Pause, false, at(start, 9_000));
        assert_eq!(clock.total_at(at(start, 10_000)), Duration::from_secs(3));
    }
}
