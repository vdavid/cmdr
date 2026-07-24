//! How hard a `Volume`-trait index scan is allowed to hit a share right now.
//!
//! A NAS scan and the pane's own directory listings share ONE SMB session (every
//! `SmbVolume` clone multiplexes frames over the same connection), so a walk
//! running 64 listings in flight buries a navigation behind its backlog: a
//! 40-entry folder took 10.7 s mid-scan on a real QNAP, and was instant the moment
//! the scan finished (2026-07-19, `/Volumes/naspi`, ~2M entries).
//!
//! So the walk asks this module, at every top-up, how many listings it may keep in
//! flight. Browsing the share OR a running user-initiated transfer on it (both
//! higher-priority claims on the connection — `crate::priority`) drops the budget
//! to [`YIELDING_LISTING_BUDGET`]; a quiet share gets [`FULL_LISTING_BUDGET`].
//!
//! **Forward progress is structural, not a floor.** The yielding budget is 1, never
//! 0, so there is no starvation case to defend against with a quota or a
//! consecutive-yield cap: a user who browses the share continuously for an hour
//! gets a scan that runs at one listing at a time for that hour and still finishes.
//! Nothing to reset, nothing to leak, nothing that can wedge. See
//! `indexing/DETAILS.md` § "Yielding to navigation".
//!
//! Scope is PER VOLUME (`priority::foreground`'s scoped signal): the contention
//! is one share's session, so browsing a local folder must not slow a NAS scan.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::priority::foreground;

/// Listings in flight while the share is quiet. Directory listing is latency-bound
/// (each dir is open+query+close round trips over an otherwise-idle link), so
/// keeping many in flight is a near-linear speedup until the server's SMB credits
/// saturate. 64 captures essentially all of that win while staying gentle on a NAS
/// that's also serving other load; past it a fresh scan on a real raidz1-HDD QNAP
/// became bound by the single SQLite writer, not by listing parallelism (disks
/// ~15% busy, ARC served most metadata). See DETAILS § "Bounded-concurrency walk".
pub(crate) const FULL_LISTING_BUDGET: usize = 64;

/// Listings in flight while the user is actively browsing THIS share. One, so a
/// navigation queues behind at most one background listing instead of a 64-deep
/// backlog.
///
/// ❌ Never lower this to 0. It's what makes forward progress structural: a
/// gate that can reach zero turns "indexing is in the way" into "indexing never
/// finishes", and then needs a quota or a yield cap to climb back out. One
/// listing at a time is slow, never stopped.
pub(crate) const YIELDING_LISTING_BUDGET: usize = 1;

/// How long after a navigation a share still counts as "in use". Long enough to
/// span the gaps in real browsing (a person opens a folder every second or so), so
/// a session of clicking around is ONE throttled stretch rather than a flapping
/// budget; short enough that the scan is back at full speed a couple of seconds
/// after the user stops.
pub(crate) const SCAN_FOREGROUND_IDLE_THRESHOLD: Duration = Duration::from_secs(2);

/// PURE: the in-flight listing budget for a share whose last foreground activity
/// was at `last_foreground_millis`, as of `now_millis`, and on which a
/// user-initiated transfer is (`transfer_active`) or isn't running.
///
/// Both higher-priority claims yield the same way (`crate::priority`'s order:
/// interactive > transfers > indexing): the budget drops to one listing in
/// flight, never zero, so the scan slows but structurally keeps finishing.
///
/// Injected clock + threshold so the whole decision is unit-testable without
/// sleeping. Delegates the elapsed-vs-threshold comparison to
/// [`foreground::is_idle`] so there's one saturating-subtraction rule in the app.
pub(crate) fn listing_budget(
    now_millis: u64,
    last_foreground_millis: u64,
    threshold: Duration,
    transfer_active: bool,
) -> usize {
    if !transfer_active && foreground::is_idle(now_millis, last_foreground_millis, threshold) {
        FULL_LISTING_BUDGET
    } else {
        YIELDING_LISTING_BUDGET
    }
}

/// The walk's handle on the pace decision: which volume to watch, how long it must
/// be quiet, and enough state to log a budget CHANGE once instead of per top-up.
pub(crate) struct ScanPacer {
    /// The volume whose foreground activity throttles this walk. `None` disables
    /// pacing entirely (tests that aren't about pacing, so they can't be perturbed
    /// by unrelated activity).
    volume_id: Option<String>,
    idle_threshold: Duration,
    /// The budget we last logged, so a walk logs one line per transition rather
    /// than one per top-up (thousands per second).
    last_logged: AtomicUsize,
}

impl ScanPacer {
    /// Pace this walk against foreground activity on `volume_id` (production).
    pub(crate) fn for_volume(volume_id: impl Into<String>) -> Self {
        Self {
            volume_id: Some(volume_id.into()),
            idle_threshold: SCAN_FOREGROUND_IDLE_THRESHOLD,
            last_logged: AtomicUsize::new(FULL_LISTING_BUDGET),
        }
    }

    /// Pace against `volume_id` with an explicit idle threshold, so tests exercise
    /// the real decision path without waiting out the production window.
    #[cfg(test)]
    pub(crate) fn with_threshold(volume_id: impl Into<String>, idle_threshold: Duration) -> Self {
        Self {
            volume_id: Some(volume_id.into()),
            idle_threshold,
            last_logged: AtomicUsize::new(FULL_LISTING_BUDGET),
        }
    }

    /// Never throttle: always the full budget. For scans that aren't about pacing,
    /// so an unrelated navigation can't perturb them.
    #[cfg(test)]
    pub(crate) fn unpaced() -> Self {
        Self {
            volume_id: None,
            idle_threshold: SCAN_FOREGROUND_IDLE_THRESHOLD,
            last_logged: AtomicUsize::new(FULL_LISTING_BUDGET),
        }
    }

    /// How many listings the walk may have in flight right now. Cheap enough to
    /// call at every top-up (two uncontended read locks over tiny maps).
    pub(crate) fn listing_budget(&self) -> usize {
        let Some(volume_id) = self.volume_id.as_deref() else {
            return FULL_LISTING_BUDGET;
        };
        let transfer_active = crate::priority::transfers::transfer_active(volume_id);
        // A volume nobody has browsed has no timestamp at all, which reads as
        // foreground-idle; otherwise the pure decision below owns the call.
        let budget = match foreground::global().volume_activity_millis(volume_id) {
            Some((now, last)) => listing_budget(now, last, self.idle_threshold, transfer_active),
            None => listing_budget(0, 0, Duration::ZERO, transfer_active),
        };
        self.log_transition(volume_id, budget);
        budget
    }

    /// One log line per budget CHANGE. A scan that yields and resumes repeatedly
    /// while the user browses would otherwise emit thousands of identical lines.
    fn log_transition(&self, volume_id: &str, budget: usize) {
        if self.last_logged.swap(budget, Ordering::Relaxed) == budget {
            return;
        }
        let in_flight = crate::pluralize::pluralize(budget as u64, "listing");
        if budget == FULL_LISTING_BUDGET {
            log::debug!("scan_pace: '{volume_id}' is quiet again, scan back to {in_flight} in flight");
        } else {
            log::debug!(
                "scan_pace: '{volume_id}' is in use (browsing or a transfer), throttling the scan to {in_flight} in flight"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core decision: a share the user just touched gets the yielding budget,
    /// and it climbs back to full exactly at the threshold.
    #[test]
    fn a_recently_browsed_volume_gets_the_yielding_budget() {
        let threshold = Duration::from_secs(2);
        let last = 10_000;
        assert_eq!(
            listing_budget(10_000, last, threshold, false),
            YIELDING_LISTING_BUDGET,
            "navigating right now throttles the scan"
        );
        assert_eq!(
            listing_budget(11_999, last, threshold, false),
            YIELDING_LISTING_BUDGET,
            "still inside the quiet window"
        );
        assert_eq!(
            listing_budget(12_000, last, threshold, false),
            FULL_LISTING_BUDGET,
            "the window elapsed ⇒ full speed"
        );
    }

    /// Transfers trump indexing: a running transfer yields the throttled budget
    /// even on a foreground-quiet share, and the two claims don't mask each other.
    #[test]
    fn a_running_transfer_gets_the_yielding_budget_regardless_of_foreground() {
        let threshold = Duration::from_secs(2);
        // Foreground long quiet, transfer running ⇒ yield.
        assert_eq!(
            listing_budget(100_000, 0, threshold, true),
            YIELDING_LISTING_BUDGET,
            "a transfer alone must throttle the scan"
        );
        // Both busy ⇒ still the same single yielding budget.
        assert_eq!(listing_budget(10_000, 10_000, threshold, true), YIELDING_LISTING_BUDGET);
        // Transfer done, foreground quiet ⇒ full speed again.
        assert_eq!(listing_budget(100_000, 0, threshold, false), FULL_LISTING_BUDGET);
    }

    /// A share nobody has browsed runs at full speed from the very first listing —
    /// a first scan must not start out throttled. "Never browsed" is an ABSENT
    /// entry, not a zero timestamp (`ForegroundActivity::idle_for_volume`); the
    /// pure decision below never sees it, which is why this asserts at the pacer.
    #[test]
    fn a_never_browsed_volume_starts_at_full_speed() {
        let pacer = ScanPacer::with_threshold("test://scan_pace/never_browsed", Duration::from_secs(30));
        assert_eq!(pacer.listing_budget(), FULL_LISTING_BUDGET);
    }

    /// THE anti-starvation guarantee, as a property: no reachable input yields a
    /// budget of 0. Forward progress is structural — there is no floor, quota, or
    /// yield cap to get wrong, because the walk is never fully stopped.
    #[test]
    fn the_budget_is_never_zero_for_any_input() {
        let threshold = Duration::from_secs(2);
        for now in [0_u64, 1, 999, 1_000, 100_000, u64::MAX] {
            for last in [0_u64, 1, 999, 100_000, u64::MAX] {
                for transfer_active in [false, true] {
                    assert!(
                        listing_budget(now, last, threshold, transfer_active) >= 1,
                        "budget must never reach 0 (now={now}, last={last}, transfer={transfer_active})"
                    );
                }
            }
        }
    }

    /// An unpaced walk ignores foreground activity entirely, so tests and callers
    /// with no volume identity always get the full budget.
    #[test]
    fn an_unpaced_walk_always_gets_the_full_budget() {
        assert_eq!(ScanPacer::unpaced().listing_budget(), FULL_LISTING_BUDGET);
    }

    /// End to end over the real global tracker: noting activity on this volume
    /// throttles it, and an untouched volume stays at full speed. The volume ids
    /// are unique to this test, so the process-global tracker can't cross-talk
    /// with other tests running in parallel.
    #[test]
    fn the_pacer_reads_the_scoped_global_signal() {
        let browsed = "test://scan_pace/browsed";
        let quiet = "test://scan_pace/quiet";
        let pacer = ScanPacer::with_threshold(browsed, Duration::from_secs(30));
        let quiet_pacer = ScanPacer::with_threshold(quiet, Duration::from_secs(30));

        assert_eq!(pacer.listing_budget(), FULL_LISTING_BUDGET, "nothing noted yet");
        foreground::note_foreground_activity_on(browsed);
        assert_eq!(
            pacer.listing_budget(),
            YIELDING_LISTING_BUDGET,
            "browsing this share throttles its scan"
        );
        assert_eq!(
            quiet_pacer.listing_budget(),
            FULL_LISTING_BUDGET,
            "browsing one share must not throttle another's scan"
        );
    }

    /// Resume: once the user stops, the scan is back at full speed as soon as the
    /// quiet window elapses — no extra debounce, no manual re-arm.
    #[tokio::test]
    async fn the_scan_returns_to_full_speed_once_the_share_goes_quiet() {
        let volume_id = "test://scan_pace/goes_quiet";
        let pacer = ScanPacer::with_threshold(volume_id, Duration::from_millis(50));

        foreground::note_foreground_activity_on(volume_id);
        assert_eq!(pacer.listing_budget(), YIELDING_LISTING_BUDGET, "browsing right now");

        // allowed-test-sleep: outliving the 50 ms quiet window is the subject. The pacer reads the
        // real clock against the last foreground timestamp, so only elapsed wall time reopens it
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(
            pacer.listing_budget(),
            FULL_LISTING_BUDGET,
            "the quiet window elapsed ⇒ full speed again"
        );
    }
}
