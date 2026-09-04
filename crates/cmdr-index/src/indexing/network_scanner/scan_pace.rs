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
//! higher-priority claims on the connection) drops the budget to
//! [`YIELDING_LISTING_BUDGET`]; a quiet share gets [`FULL_LISTING_BUDGET`]. Both
//! claims arrive through the host policy seam (`indexing::host::policy`), asked once
//! per top-up and never per entry.
//!
//! "Browsing" covers a listing's REAL duration, not a window after it started: the
//! host holds a per-volume lease for as long as a foreground listing runs, so a
//! folder that takes ten seconds to come back holds this walk at one listing in
//! flight for those ten seconds. That is exactly the case the 10.7 s measurement
//! above describes, and it costs nothing structurally, because of the next
//! paragraph.
//!
//! **Forward progress is structural, not a floor.** The yielding budget is 1, never
//! 0, so there is no starvation case to defend against with a quota or a
//! consecutive-yield cap: a user who browses the share continuously for an hour
//! gets a scan that runs at one listing at a time for that hour and still finishes.
//! Nothing to reset, nothing to leak, nothing that can wedge. See
//! `indexing/DETAILS.md` § "Yielding to navigation".
//!
//! Scope is PER VOLUME ([`WorkClearance::volume_idle`], not `app_idle`): the
//! contention is one share's session, so browsing a local folder must not slow a
//! NAS scan.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::indexing::host::policy as host_policy;
use crate::indexing::host::policy::{HostPolicy, WorkClearance};

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

/// How long after a foreground operation ENDS a share still counts as "in use". The
/// window that covers the GAPS in real browsing (a person opens a folder every
/// second or so), so a session of clicking around is ONE throttled stretch rather
/// than a flapping budget; short enough that the scan is back at full speed a couple
/// of seconds after the user stops. The operations themselves are covered exactly,
/// by the host's leases, so this number no longer has to guess how long one takes.
pub(crate) const SCAN_FOREGROUND_IDLE_THRESHOLD: Duration = Duration::from_secs(2);

/// PURE: the in-flight listing budget for a share the host reports `clearance` on.
///
/// Both higher-priority claims yield the same way (the host's order: interactive >
/// transfers > indexing): the budget drops to one listing in flight, never zero, so
/// the scan slows but structurally keeps finishing.
///
/// Reads `volume_idle`, not `app_idle` — the contention here is one share's session.
/// The elapsed-versus-threshold rule that produced the flag stays host-side, so
/// there's exactly one place that owns "how long is quiet".
pub(crate) fn listing_budget(clearance: WorkClearance) -> usize {
    if clearance.volume_idle && !clearance.transfer_active {
        FULL_LISTING_BUDGET
    } else {
        YIELDING_LISTING_BUDGET
    }
}

/// The walk's handle on the pace decision: which volume to watch, how long it must
/// be quiet, and enough state to log a budget CHANGE once instead of per top-up.
pub struct ScanPacer {
    /// The volume whose foreground activity throttles this walk. `None` disables
    /// pacing entirely (tests that aren't about pacing, so they can't be perturbed
    /// by unrelated activity).
    volume_id: Option<String>,
    idle_threshold: Duration,
    /// The host asked at each top-up. Captured once at construction rather than
    /// resolved per question, so a walk paces against one policy for its lifetime.
    policy: Arc<dyn HostPolicy>,
    /// The budget we last logged, so a walk logs one line per transition rather
    /// than one per top-up (thousands per second).
    last_logged: AtomicUsize,
}

impl ScanPacer {
    /// Pace this walk against foreground activity on `volume_id`, asking whichever
    /// host the app installed (production).
    pub(crate) fn for_volume(volume_id: impl Into<String>) -> Self {
        Self::with_policy(volume_id, SCAN_FOREGROUND_IDLE_THRESHOLD, host_policy::current())
    }

    /// Pace against `volume_id` with an explicit idle threshold and an explicit host.
    /// Production goes through [`for_volume`](Self::for_volume); a test uses this to
    /// drive the real decision path against a policy it controls, instead of nudging
    /// process-global signals it can't reset.
    pub(crate) fn with_policy(
        volume_id: impl Into<String>,
        idle_threshold: Duration,
        policy: Arc<dyn HostPolicy>,
    ) -> Self {
        Self {
            volume_id: Some(volume_id.into()),
            idle_threshold,
            policy,
            last_logged: AtomicUsize::new(FULL_LISTING_BUDGET),
        }
    }

    /// Never throttle: always the full budget. For scans that aren't about pacing,
    /// so an unrelated navigation can't perturb them.
    #[cfg(any(test, feature = "testing"))]
    pub fn unpaced() -> Self {
        Self {
            volume_id: None,
            idle_threshold: SCAN_FOREGROUND_IDLE_THRESHOLD,
            policy: Arc::new(host_policy::AlwaysClear),
            last_logged: AtomicUsize::new(FULL_LISTING_BUDGET),
        }
    }

    /// How many listings the walk may have in flight right now.
    ///
    /// This is the walk's ONE policy question, asked at each listing top-up. It must
    /// stay at that cadence: the walk visits millions of entries and asks this once
    /// per dispatched listing, which is what keeps the seam off the hot path.
    pub(crate) fn listing_budget(&self) -> usize {
        let Some(volume_id) = self.volume_id.as_deref() else {
            return FULL_LISTING_BUDGET;
        };
        let budget = listing_budget(self.policy.clearance(volume_id, self.idle_threshold));
        self.log_transition(volume_id, budget);
        budget
    }

    /// One log line per budget CHANGE. A scan that yields and resumes repeatedly
    /// while the user browses would otherwise emit thousands of identical lines.
    fn log_transition(&self, volume_id: &str, budget: usize) {
        if self.last_logged.swap(budget, Ordering::Relaxed) == budget {
            return;
        }
        let in_flight = cmdr_fs::pluralize::pluralize(budget as u64, "listing");
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
    use crate::indexing::host::policy::FakeHostPolicy;

    /// Build a clearance the way the host would report it.
    fn clearance(volume_idle: bool, transfer_active: bool) -> WorkClearance {
        WorkClearance {
            app_idle: volume_idle,
            volume_idle,
            transfer_active,
        }
    }

    /// The core decision: a share the user is on gets the yielding budget, and a
    /// quiet one gets full speed.
    #[test]
    fn a_browsed_volume_gets_the_yielding_budget() {
        assert_eq!(
            listing_budget(clearance(false, false)),
            YIELDING_LISTING_BUDGET,
            "navigating this share throttles its scan"
        );
        assert_eq!(
            listing_budget(clearance(true, false)),
            FULL_LISTING_BUDGET,
            "a quiet share runs at full speed"
        );
    }

    /// Transfers trump indexing: a running transfer yields the throttled budget even
    /// on a foreground-quiet share, and the two claims don't mask each other.
    #[test]
    fn a_running_transfer_gets_the_yielding_budget_regardless_of_foreground() {
        assert_eq!(
            listing_budget(clearance(true, true)),
            YIELDING_LISTING_BUDGET,
            "a transfer alone must throttle the scan"
        );
        assert_eq!(
            listing_budget(clearance(false, true)),
            YIELDING_LISTING_BUDGET,
            "both claims together are still one yielding budget"
        );
        assert_eq!(listing_budget(clearance(true, false)), FULL_LISTING_BUDGET);
    }

    /// The scope contract, at the decision: only the PER-VOLUME idle flag may reach
    /// the budget. Reading `app_idle` here would let browsing a local folder throttle
    /// a NAS scan that isn't competing with it.
    #[test]
    fn app_wide_activity_alone_does_not_throttle_a_quiet_share() {
        let busy_app_quiet_share = WorkClearance {
            app_idle: false,
            volume_idle: true,
            transfer_active: false,
        };
        assert_eq!(listing_budget(busy_app_quiet_share), FULL_LISTING_BUDGET);
    }

    /// THE anti-starvation guarantee, as a property: no reachable input yields a
    /// budget of 0. Forward progress is structural — there is no floor, quota, or
    /// yield cap to get wrong, because the walk is never fully stopped.
    #[test]
    fn the_budget_is_never_zero_for_any_input() {
        for app_idle in [false, true] {
            for volume_idle in [false, true] {
                for transfer_active in [false, true] {
                    let c = WorkClearance {
                        app_idle,
                        volume_idle,
                        transfer_active,
                    };
                    assert!(listing_budget(c) >= 1, "budget must never reach 0 for {c:?}");
                }
            }
        }
    }

    /// A share nobody has browsed runs at full speed from the very first listing: a
    /// first scan must not start out throttled.
    #[test]
    fn a_never_browsed_volume_starts_at_full_speed() {
        let pacer = ScanPacer::with_policy(
            "test://scan_pace/never_browsed",
            Duration::from_secs(30),
            FakeHostPolicy::shared(),
        );
        assert_eq!(pacer.listing_budget(), FULL_LISTING_BUDGET);
    }

    /// An unpaced walk ignores the host entirely, so tests and callers with no volume
    /// identity always get the full budget.
    #[test]
    fn an_unpaced_walk_always_gets_the_full_budget() {
        assert_eq!(ScanPacer::unpaced().listing_budget(), FULL_LISTING_BUDGET);
    }

    /// End to end through the seam: the pacer reads whatever the host says, and keeps
    /// reading it, so a share that goes quiet is back at full speed with no re-arm.
    #[test]
    fn the_pacer_tracks_the_host_as_it_changes() {
        let host = FakeHostPolicy::shared();
        let pacer = ScanPacer::with_policy("test://scan_pace/tracked", Duration::from_secs(30), host.clone());

        assert_eq!(pacer.listing_budget(), FULL_LISTING_BUDGET, "nothing competing yet");

        host.note_foreground_activity();
        assert_eq!(pacer.listing_budget(), YIELDING_LISTING_BUDGET, "browsing right now");

        host.note_foreground_quiet();
        assert_eq!(
            pacer.listing_budget(),
            FULL_LISTING_BUDGET,
            "quiet again ⇒ full speed, no debounce and no manual re-arm"
        );

        host.note_transfer_started();
        assert_eq!(pacer.listing_budget(), YIELDING_LISTING_BUDGET, "a transfer claims it");

        host.note_transfer_finished();
        assert_eq!(pacer.listing_budget(), FULL_LISTING_BUDGET);
    }
}
