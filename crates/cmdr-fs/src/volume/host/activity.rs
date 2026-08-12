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
//! **The threshold stays with the caller.** This seam reports the raw signal —
//! how long since the user last touched this volume — because how long counts as
//! "busy" is a property of the work standing aside, not of the host. A transfer
//! that parks outright wants a short window; a scan that merely slows down wants
//! a long one. Each writes its own constant next to the reasoning for it.
//!
//! ❌ **Standing aside must never become starvation.** A backend that parks on
//! this signal needs its own floor — a minimum amount of progress between
//! yields — or continuous browsing stops the transfer entirely instead of
//! slowing it.

use std::time::Duration;

/// What the user is doing right now, as far as a backend needs to know.
///
/// Cmdr answers this from the app's foreground-activity tracker; a test or a
/// tool sees a permanently quiet machine ([`AlwaysIdle`]).
pub trait UserActivity: Send + Sync {
    /// Whether `volume_id` has been untouched by the user for at least
    /// `threshold`.
    ///
    /// `true` means "go ahead". Cheap enough to call between chunks of a
    /// transfer: one clock read and one small lookup. An unknown volume id reads
    /// as idle, because a volume the user has never navigated is one nobody is
    /// waiting on.
    fn volume_idle_for(&self, volume_id: &str, threshold: Duration) -> bool;
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
}

#[cfg(any(test, feature = "testing"))]
pub use scripted::BusyVolumes;

#[cfg(any(test, feature = "testing"))]
mod scripted {
    use std::collections::HashSet;
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
    #[derive(Default)]
    pub struct BusyVolumes {
        busy: Mutex<HashSet<String>>,
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

        /// The user stopped using `volume_id`, so anything parked on it resumes.
        pub fn goes_quiet(&self, volume_id: &str) {
            self.busy.lock_ignore_poison().remove(volume_id);
        }
    }

    impl UserActivity for BusyVolumes {
        fn volume_idle_for(&self, volume_id: &str, _threshold: Duration) -> bool {
            !self.busy.lock_ignore_poison().contains(volume_id)
        }
    }
}
