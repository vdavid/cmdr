//! Whether the user is busy on a volume, so bulk work can stand aside.
//!
//! A copy and the pane's directory listings usually share one connection, so a
//! running transfer competes with every navigation on that volume. A backend
//! that can pause between chunks asks here whether now is a moment to pause.
//!
//! **The scope is per volume, and that matters.** A transfer is work the user
//! asked for and is watching a progress bar for, so it stands aside only for
//! contention on the volume it's actually competing with. Browsing a local
//! folder has no business slowing a copy off a NAS.
//!
//! **Two halves, and both are needed.** A foreground operation with a beginning
//! and an end (a directory listing) holds a LEASE for its real duration, so a
//! share the user is waiting on reads as busy however long it takes. A TIMESTAMP
//! covers the gap AFTER one, so a burst of arrow-key presses reads as one
//! continuous action rather than one park per keystroke. Alone, the lease misses
//! the gaps and the timestamp decays mid-wait, which is why
//! [`volume_busy_for_user`] puts them together in ONE place for every consumer.
//!
//! **The threshold stays with the caller.** This seam reports the raw signals —
//! what is in flight, and how long since the user last touched this volume —
//! because how long counts as "busy" is a property of the work standing aside, not
//! of the host. A transfer that parks outright wants a short window; a scan that
//! merely slows down wants a long one. Each writes its own constant next to the
//! reasoning for it.
//!
//! ❌ **Standing aside must never become starvation.** A backend that parks on
//! this signal needs its own floor — a minimum amount of progress between
//! yields — or continuous browsing stops the transfer entirely instead of
//! slowing it.

use std::time::Duration;

/// What the user is doing right now, as far as a backend needs to know.
///
/// Cmdr answers this from the app's foreground-activity tracker; a test or a
/// tool sees a permanently quiet machine (`AlwaysIdle`).
pub trait UserActivity: Send + Sync {
    /// Whether `volume_id` has been untouched by the user for at least
    /// `threshold`.
    ///
    /// `true` means "go ahead". Cheap enough to call between chunks of a
    /// transfer: one clock read and one small lookup. An unknown volume id reads
    /// as idle, because a volume the user has never navigated is one nobody is
    /// waiting on.
    ///
    /// ❗ This is the DECAYING half on its own. Compose it with
    /// [`volume_foreground_leases`](Self::volume_foreground_leases) through
    /// [`volume_busy_for_user`] rather than reading it alone, or a foreground
    /// operation that outlives `threshold` stops counting while the user is still
    /// waiting on it.
    fn volume_idle_for(&self, volume_id: &str, threshold: Duration) -> bool;

    /// How many foreground operations are in flight on `volume_id` right now.
    ///
    /// The EXACT half: each one is a scoped operation the user is waiting on, held
    /// from its first byte to its last however long that takes. `0` is the honest
    /// answer for a host that tracks nothing, and the same one an untouched volume
    /// gives. Same cost rule as above: it sits between a transfer's chunks.
    fn volume_foreground_leases(&self, volume_id: &str) -> usize;
}

/// THE question background work asks: is the user waiting on this volume?
///
/// One rule in one place, because the two halves aren't interchangeable and a
/// consumer that picked either alone would be wrong in a case it never tests. The
/// lease covers an operation WHILE it runs; `quiet_window` covers the gap after it.
pub fn volume_busy_for_user(activity: &dyn UserActivity, volume_id: &str, quiet_window: Duration) -> bool {
    activity.volume_foreground_leases(volume_id) > 0 || !activity.volume_idle_for(volume_id, quiet_window)
}

/// Nobody is using anything: bulk work never stands aside.
///
/// The right answer for a bench (which wants the protocol's real throughput) and
/// for a test (which shouldn't depend on wall-clock timing to make progress).
pub(super) struct AlwaysIdle;

impl UserActivity for AlwaysIdle {
    fn volume_idle_for(&self, _volume_id: &str, _threshold: Duration) -> bool {
        true
    }

    fn volume_foreground_leases(&self, _volume_id: &str) -> usize {
        0
    }
}

#[cfg(any(test, feature = "testing"))]
pub use scripted::BusyVolumes;

#[cfg(any(test, feature = "testing"))]
mod scripted {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::UserActivity;
    use crate::ignore_poison::IgnorePoison;

    /// A [`UserActivity`] driven by a test rather than by a
    /// clock: the volumes in the set are busy, everything else is idle.
    ///
    /// Waiting out a real threshold is what makes yield tests slow and flaky, so
    /// `threshold` is ignored entirely. A test that needs the transfer to resume
    /// calls [`goes_quiet`](Self::goes_quiet).
    ///
    /// The two halves of the signal are driven separately, so a test can say
    /// "a listing is in flight" ([`holds_a_lease`](Self::holds_a_lease)) without
    /// also saying "and the timestamp is fresh", and see which one a consumer
    /// actually reads.
    #[derive(Default)]
    pub struct BusyVolumes {
        busy: Mutex<HashSet<String>>,
        leases: Mutex<HashMap<String, usize>>,
    }

    impl BusyVolumes {
        /// A quiet machine.
        pub fn new() -> Self {
            Self::default()
        }

        /// Marks `volume_id` busy, as if the user just navigated it.
        pub fn is_busy(self, volume_id: &str) -> Self {
            self.busy.lock_ignore_poison().insert(volume_id.to_string());
            self
        }

        /// A foreground operation started on `volume_id` and hasn't finished.
        pub fn holds_a_lease(self, volume_id: &str) -> Self {
            *self
                .leases
                .lock_ignore_poison()
                .entry(volume_id.to_string())
                .or_default() += 1;
            self
        }

        /// One foreground operation on `volume_id` finished.
        pub fn releases_a_lease(&self, volume_id: &str) {
            if let Some(count) = self.leases.lock_ignore_poison().get_mut(volume_id) {
                *count = count.saturating_sub(1);
            }
        }

        /// The user stopped using `volume_id`, so anything parked on it resumes.
        pub fn goes_quiet(&self, volume_id: &str) {
            self.busy.lock_ignore_poison().remove(volume_id);
        }
    }

    impl UserActivity for BusyVolumes {
        fn volume_idle_for(&self, volume_id: &str, _threshold: Duration) -> bool {
            !self.busy.lock_ignore_poison().contains(volume_id)
        }

        fn volume_foreground_leases(&self, volume_id: &str) -> usize {
            self.leases.lock_ignore_poison().get(volume_id).copied().unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{AlwaysIdle, BusyVolumes, UserActivity, volume_busy_for_user};

    /// A window nobody in production would use, to prove the composition never
    /// depends on wall-clock timing.
    const ANY_WINDOW: Duration = Duration::from_secs(30);

    /// The lease half on its own: an operation in flight makes the volume busy
    /// even though the timestamp says the user hasn't touched it. This is the
    /// case a timestamp-only signal gets wrong, and the reason the lease exists.
    #[test]
    fn an_operation_in_flight_is_busy_even_with_a_quiet_timestamp() {
        let listing_running = BusyVolumes::new().holds_a_lease("smb://naspi/media");
        assert!(listing_running.volume_idle_for("smb://naspi/media", ANY_WINDOW));
        assert!(volume_busy_for_user(&listing_running, "smb://naspi/media", ANY_WINDOW));
    }

    /// The timestamp half on its own: nothing is in flight, but the user was here
    /// a moment ago, so the quiet window still holds the volume busy. This is the
    /// case a lease-only signal gets wrong.
    #[test]
    fn a_fresh_timestamp_is_busy_even_with_nothing_in_flight() {
        let just_navigated = BusyVolumes::new().is_busy("smb://naspi/media");
        assert_eq!(just_navigated.volume_foreground_leases("smb://naspi/media"), 0);
        assert!(volume_busy_for_user(&just_navigated, "smb://naspi/media", ANY_WINDOW));
    }

    /// Both halves clear ⇒ free. Releasing the last lease on an otherwise-quiet
    /// volume is what lets background work resume with no re-arming.
    #[test]
    fn a_volume_is_free_once_both_halves_are_clear() {
        let activity = BusyVolumes::new().holds_a_lease("smb://naspi/media");
        activity.releases_a_lease("smb://naspi/media");
        assert!(!volume_busy_for_user(&activity, "smb://naspi/media", ANY_WINDOW));
    }

    /// The scope guarantee survives the composition: a listing on one share must
    /// not park a copy running on another.
    #[test]
    fn a_lease_on_one_volume_leaves_every_other_volume_free() {
        let activity = BusyVolumes::new().holds_a_lease("smb://naspi/media");
        assert!(!volume_busy_for_user(&activity, "smb://naspi/backups", ANY_WINDOW));
    }

    /// A host with no signals reports free, so a bench or a CLI tool runs at the
    /// protocol's real speed rather than standing aside for a user who isn't there.
    #[test]
    fn a_host_that_tracks_nothing_is_never_busy() {
        assert!(!volume_busy_for_user(&AlwaysIdle, "smb://naspi/media", ANY_WINDOW));
    }
}
