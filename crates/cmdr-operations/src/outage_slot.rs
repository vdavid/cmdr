//! Which volume an operation is waiting on to come back, if any, and how patient
//! it may be.
//!
//! A leaf the operation state owns, so the one activity classifier
//! (`WriteOperationState::activity`, and the transfer probe it consults) can say
//! "waiting for the NAS to come back" without reaching into the transfer layer
//! that runs the wait (`transfer/volume_outage.rs`).
//!
//! Several tasks of one operation can wait at once (the concurrent window streams
//! up to eight files, and a dropped share fails them all within a second), so the
//! slot counts its waiters: the activity names the volume while ANY of them
//! waits, and the ETA's working-time clock (`human_wait.rs`) is held open from
//! the first one in to the last one out.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::human_wait::{HumanWaitClock, HumanWaitSource};
use cmdr_fs::ignore_poison::IgnorePoison;

/// How long a transfer waits for a volume that went away mid-copy before it
/// gives up and fails the way it always did.
///
/// Fifteen minutes: long enough to ride out a router restart, a NAS waking its
/// disks, a Synology reboot or DSM update, or a laptop roaming between access points (the outages users report,
/// `#398`), and short enough that a person who walked away comes back to an
/// honest failure rather than a dialog that waits forever. Nobody has to wait it
/// out: Cancel works throughout.
pub(crate) const OUTAGE_WAIT_BUDGET: Duration = Duration::from_secs(15 * 60);

/// How long to wait between two reconnect attempts, in order; the last value
/// repeats until the budget runs out. The first attempt goes out at once.
///
/// Growing, so a server that's rebooting isn't dialed every second, and capped
/// at 30 s, so a volume that comes back is noticed within half a minute even
/// when nothing else (a pane, the backend's own watcher loop) is dialing it.
const RECONNECT_CADENCE: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
    Duration::from_secs(30),
];

/// How long one reconnect attempt may take before the wait abandons it and
/// counts it as "still down", to try again on the next tick.
///
/// A backend's attempt is expected to return promptly (connected, refused, or
/// unreachable), but one that never returns would otherwise hold the whole wait
/// until its budget runs out, long after the volume came back (`#406`: an SMB
/// handshake that a server accepted and never answered). 15 s covers a TCP
/// connect timeout plus a handshake on a slow link, and stays well under the
/// cadence's 30 s step. Dropping the attempt is safe: every backend's reconnect
/// is single-flight and rebuilds from scratch on the next call.
///
/// It's the safety net, not the usual end of an attempt: SMB's own handshake
/// deadline is shorter (10 s, `cmdr-smb`'s `HANDSHAKE_RESPONSE_TIMEOUT`, which a
/// test there pins under this 15 s), so a silent server comes back as a typed
/// error from the backend first. Lowering this below 10 s turns that around.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);

/// How often a wait looks up between attempts: for a reconnect someone else made
/// (a pane's own backoff, the watcher's loop) and for a pause.
const POLL_EVERY: Duration = Duration::from_secs(1);

/// The patience of one operation's outage waits. Production always runs
/// [`OutagePolicy::default`]; it lives on the state so a test can shrink it
/// without a process-wide override racing other tests.
#[derive(Debug, Clone, Copy)]
pub struct OutagePolicy {
    /// See [`OUTAGE_WAIT_BUDGET`].
    pub budget: Duration,
    /// See [`RECONNECT_CADENCE`].
    pub cadence: &'static [Duration],
    /// See [`POLL_EVERY`].
    pub poll_every: Duration,
    /// See [`ATTEMPT_TIMEOUT`].
    pub attempt_timeout: Duration,
}

impl Default for OutagePolicy {
    fn default() -> Self {
        Self {
            budget: OUTAGE_WAIT_BUDGET,
            cadence: &RECONNECT_CADENCE,
            poll_every: POLL_EVERY,
            attempt_timeout: ATTEMPT_TIMEOUT,
        }
    }
}

impl OutagePolicy {
    /// The wait before reconnect attempt `n + 1`, given `n` attempts have failed.
    pub fn delay_after(&self, failed_attempts: usize) -> Duration {
        let last = self.cadence.last().copied().unwrap_or(POLL_EVERY);
        self.cadence
            .get(failed_attempts.saturating_sub(1))
            .copied()
            .unwrap_or(last)
    }
}

/// How long a stopped operation's cleanup gives its destination
/// (`transfer/leftover_cleanup.rs`). ❗ None of it delays the terminal event: the
/// inline deletes are capped by [`inline_budget`](Self::inline_budget), and the
/// rest happens in the background after the operation has ended.
#[derive(Debug, Clone, Copy)]
pub struct CleanupPolicy {
    /// The most the inline deletes may take in all, on a destination that
    /// reads live but may be dying under them. What's left when it runs out
    /// goes to the background pass.
    pub inline_budget: Duration,
    /// How long the background pass waits for a destination that's out, after
    /// its one reconnect attempt, before it hands what's left to the ledger's
    /// reconnect sweep.
    pub wait: Duration,
    /// How long the background pass waits before it asks a destination that
    /// REFUSED a delete again, doubling for each later ask: long enough for a
    /// dropped write's handle close, or a dropped session's teardown, to reach
    /// the server.
    pub retry_after: Duration,
    /// How long the background pass waits before it asks a temp's name again
    /// on a destination where a cut-off upload may land after its delete
    /// (`Volume::cut_off_write_may_land_late`): long enough for the server to
    /// finish reading the dropped request.
    pub late_landing_recheck: Duration,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            inline_budget: Duration::from_secs(5),
            wait: Duration::from_secs(20),
            retry_after: Duration::from_secs(3),
            late_landing_recheck: Duration::from_secs(2),
        }
    }
}

// DEFAULT-OK: no waiters and a first id of 0 is exactly an operation that has
// waited for nothing yet; it claims nothing about any volume.
#[derive(Debug, Default)]
struct Waiting {
    next_id: u64,
    /// One entry per waiter, in the order they began: `(id, volume name)`.
    waiters: Vec<(u64, String)>,
}

/// The operation's view of its outage waits. See the module docs.
#[derive(Debug)]
pub struct OutageSlot {
    waiting: Mutex<Waiting>,
    clock: Arc<HumanWaitClock>,
}

impl OutageSlot {
    pub(crate) fn new(clock: Arc<HumanWaitClock>) -> Self {
        Self {
            waiting: Mutex::new(Waiting::default()),
            clock,
        }
    }

    /// A waiter starts waiting for `volume_name`. It stops when the guard drops,
    /// whichever way the wait ended.
    pub(crate) fn begin(self: &Arc<Self>, volume_name: &str) -> OutageSlotGuard {
        let mut waiting = self.waiting.lock_ignore_poison();
        let id = waiting.next_id;
        waiting.next_id += 1;
        waiting.waiters.push((id, volume_name.to_owned()));
        if waiting.waiters.len() == 1 {
            self.clock.set(HumanWaitSource::VolumeOutage, true);
        }
        OutageSlotGuard {
            slot: Arc::clone(self),
            id,
        }
    }

    /// The volume the operation is waiting on to come back, or `None` while
    /// nothing is. With several waiters, the one that has waited longest.
    pub fn waiting_for(&self) -> Option<String> {
        self.waiting
            .lock_ignore_poison()
            .waiters
            .first()
            .map(|(_, name)| name.clone())
    }

    fn end(&self, id: u64) {
        let mut waiting = self.waiting.lock_ignore_poison();
        waiting.waiters.retain(|(waiter, _)| *waiter != id);
        if waiting.waiters.is_empty() {
            self.clock.set(HumanWaitSource::VolumeOutage, false);
        }
    }
}

/// One waiter's place in the [`OutageSlot`]; dropping it ends the wait.
pub(crate) struct OutageSlotGuard {
    slot: Arc<OutageSlot>,
    id: u64,
}

impl Drop for OutageSlotGuard {
    fn drop(&mut self) {
        self.slot.end(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::Instant;

    fn slot() -> (Arc<OutageSlot>, Arc<HumanWaitClock>) {
        let clock = HumanWaitClock::shared();
        (Arc::new(OutageSlot::new(Arc::clone(&clock))), clock)
    }

    #[test]
    fn an_idle_slot_waits_for_nothing() {
        let (slot, _) = slot();
        assert_eq!(slot.waiting_for(), None);
    }

    #[test]
    fn the_slot_names_the_volume_while_anyone_waits() {
        let (slot, _) = slot();
        let first = slot.begin("Naspolya");
        let second = slot.begin("Naspolya");
        assert_eq!(slot.waiting_for().as_deref(), Some("Naspolya"));
        drop(first);
        assert_eq!(
            slot.waiting_for().as_deref(),
            Some("Naspolya"),
            "one task still waits, so the operation still does"
        );
        drop(second);
        assert_eq!(slot.waiting_for(), None);
    }

    /// The ETA must not divide the next file's bytes by the minutes the share
    /// was gone, so the outage runs on the same working-time clock a pause does,
    /// once however many tasks share it.
    #[test]
    fn the_outage_is_charged_to_the_clock_once() {
        let (slot, clock) = slot();
        let start = Instant::now();
        let first = slot.begin("Naspolya");
        let second = slot.begin("Naspolya");
        drop(first);
        drop(second);
        let waited = clock.total_at(Instant::now());
        assert!(
            waited <= start.elapsed(),
            "two waiters must not count the same seconds twice"
        );
    }

    #[test]
    fn the_cadence_grows_and_then_holds() {
        let policy = OutagePolicy::default();
        assert_eq!(policy.delay_after(1), Duration::from_secs(2));
        assert_eq!(policy.delay_after(2), Duration::from_secs(4));
        assert_eq!(policy.delay_after(5), Duration::from_secs(30));
        assert_eq!(policy.delay_after(50), Duration::from_secs(30), "the last step repeats");
    }
}
