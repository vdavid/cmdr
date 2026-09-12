//! Knowing when a volume's index has let go of the volume.
//!
//! A [`VolumeHold`] is one start's stake in a volume, from the moment its
//! reservation wins to the moment the manager it built is gone. The registry can't
//! answer "is anything still working on this volume?" on its own: a teardown that
//! meets `Initializing` frees the key while the start behind it is still standing
//! its manager up, and a drain or a claimed teardown runs behind a transient phase
//! that says nothing about when it ends. The hold answers in every one of those
//! windows, and its drop is the one moment the answer changes.

use std::collections::HashMap;
use std::sync::{Condvar, LazyLock, Mutex, PoisonError};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::indexing::volume::VolumeId;

/// How many holds each volume has outstanding, and the signal the last one
/// leaving sends. A volume nothing holds has no entry.
///
/// A LEAF lock, like the read-handle tables: the reservation takes it while it
/// holds `INDEX_REGISTRY`, and nothing takes the registry while holding it.
struct Holds {
    counts: Mutex<HashMap<VolumeId, usize>>,
    let_go: Condvar,
}

static HOLDS: LazyLock<Holds> = LazyLock::new(|| Holds {
    counts: Mutex::new(HashMap::new()),
    let_go: Condvar::new(),
});

/// One start's stake in a volume: alive from its reservation until the manager it
/// built has shut down and gone.
///
/// ⚠️ **Taken inside the critical section that reserves the slot**
/// (`reservation.rs`). Taken after it, a stop could free the slot, find nothing
/// holding the volume, and answer "released" while the start goes on to stand a
/// manager up.
///
/// It moves INTO the `IndexManager` the start builds and drops with it, which is
/// after `shutdown` on every path: a drain, a claimed teardown, a start whose slot
/// was taken away, a start that failed. ❌ Don't release it by hand anywhere else;
/// riding the manager's own drop is what keeps a new teardown path from forgetting
/// it.
pub(crate) struct VolumeHold {
    volume_id: VolumeId,
}

impl VolumeHold {
    /// Stake a claim on `volume_id`. Only the reservation calls this, under the
    /// registry lock.
    pub(super) fn take(volume_id: &str) -> Self {
        *HOLDS
            .counts
            .lock_ignore_poison()
            .entry(volume_id.to_string())
            .or_insert(0) += 1;
        Self {
            volume_id: volume_id.to_string(),
        }
    }

    /// A hold for a manager a test builds without a registry slot.
    #[cfg(test)]
    pub(crate) fn for_test(volume_id: &str) -> Self {
        Self::take(volume_id)
    }
}

impl Drop for VolumeHold {
    fn drop(&mut self) {
        let mut counts = HOLDS.counts.lock_ignore_poison();
        let Some(count) = counts.get_mut(&self.volume_id) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.remove(&self.volume_id);
            // One condvar serves every volume, so this wakes waiters on other
            // volumes too; each re-checks its own before it answers.
            HOLDS.let_go.notify_all();
        }
    }
}

/// Whether any start or manager holds `volume_id` right now.
pub(super) fn is_held(volume_id: &str) -> bool {
    HOLDS.counts.lock_ignore_poison().contains_key(volume_id)
}

/// Wait up to `wait` for nothing to hold `volume_id`, answering whether the volume
/// was let go.
///
/// Woken by the drop of a volume's last hold, ❌ never by polling. A zero `wait`
/// answers at once from what the table holds right now.
pub(super) fn wait_until_released(volume_id: &str, wait: Duration) -> bool {
    let counts = HOLDS.counts.lock_ignore_poison();
    // Recovering is right for the same reason `lock_ignore_poison` is: this is a
    // count table, and no critical section here can panic part way through.
    let (counts, _) = HOLDS
        .let_go
        .wait_timeout_while(counts, wait, |counts| counts.contains_key(volume_id))
        .unwrap_or_else(PoisonError::into_inner);
    !counts.contains_key(volume_id)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    #[test]
    fn a_volume_nothing_holds_is_let_go_at_once() {
        assert!(!is_held("release-test-never-held"));
        assert!(wait_until_released("release-test-never-held", Duration::ZERO));
    }

    #[test]
    fn every_hold_has_to_let_go_before_the_volume_does() {
        let volume_id = "release-test-two-holds";
        let first = VolumeHold::take(volume_id);
        let second = VolumeHold::take(volume_id);

        drop(first);
        assert!(is_held(volume_id), "a second start still holds the volume");
        assert!(
            !wait_until_released(volume_id, Duration::ZERO),
            "and a wait that ran out says so"
        );
        drop(second);
        assert!(wait_until_released(volume_id, Duration::ZERO), "the last hold let go");
    }

    /// The wait is a subscription: the drop wakes it. A waiter that only noticed at
    /// its deadline would still answer "released", so the time it took is the
    /// assertion.
    #[test]
    fn a_waiter_wakes_as_the_last_hold_lets_go() {
        let volume_id = "release-test-wake";
        let wait = Duration::from_secs(5);
        let hold = VolumeHold::take(volume_id);
        let (waiting, waiter_started) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            waiting.send(()).expect("the test is listening");
            let started = Instant::now();
            let released = wait_until_released(volume_id, wait);
            (released, started.elapsed())
        });
        waiter_started.recv().expect("the waiter starts");

        drop(hold);
        let (released, took) = waiter.join().expect("the waiter doesn't panic");

        assert!(released, "the volume was let go inside the wait");
        assert!(
            took < wait,
            "the drop has to wake the waiter, not leave it to its deadline (took {took:?})"
        );
    }
}
