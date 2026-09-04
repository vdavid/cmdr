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
//! **The seam reports facts; this module composes the decisions.** The trait
//! answers what is in flight and how long since the user last touched the volume;
//! [`volume_idle_for`], [`volume_busy_for_user`], and [`wait_until_volume_free`]
//! are free functions over it. A consumer that composed its own would be wrong in
//! a case it never tests.
//!
//! **The threshold stays with the caller.** How long counts as "busy" is a
//! property of the work standing aside, not of the host. A transfer that parks
//! outright wants a short window; a scan that merely slows down wants a long one.
//! Each writes its own constant next to the reasoning for it.
//!
//! **Waiting is an event, not a tick.** [`UserActivity::watch_volume`] hands out a
//! subscription that fires whenever either half moves, so a parked transfer sleeps
//! until the listing it's standing aside for actually ends instead of asking again
//! every few dozen milliseconds. Only the tail of the quiet window is a sleep, and
//! it's ONE sleep to a computed deadline.
//!
//! ❌ **Standing aside must never become starvation.** A backend that parks on
//! this signal needs its own floor — a minimum amount of progress between
//! yields — or continuous browsing stops the transfer entirely instead of
//! slowing it.

use std::future::pending;
use std::time::Duration;

use tokio::sync::watch;

/// What the user is doing right now, as far as a backend needs to know.
///
/// Cmdr answers this from the app's foreground-activity tracker; a test or a
/// tool sees a permanently quiet machine (`AlwaysIdle`).
pub trait UserActivity: Send + Sync {
    /// How long the user has left `volume_id` alone, or `None` when they've never
    /// touched it at all.
    ///
    /// Cheap enough to call between chunks of a transfer: one clock read and one
    /// small lookup. `None` reads as "idle however long you're asking about",
    /// because a volume the user has never navigated is one nobody is waiting on.
    ///
    /// ❗ This is the DECAYING half on its own. Compose it with
    /// [`volume_foreground_leases`](Self::volume_foreground_leases) through
    /// [`volume_busy_for_user`] rather than reading it alone, or a foreground
    /// operation that outlives the caller's window stops counting while the user
    /// is still waiting on it.
    fn volume_quiet_for(&self, volume_id: &str) -> Option<Duration>;

    /// How many foreground operations are in flight on `volume_id` right now.
    ///
    /// The EXACT half: each one is a scoped operation the user is waiting on, held
    /// from its first byte to its last however long that takes. `0` is the honest
    /// answer for a host that tracks nothing, and the same one an untouched volume
    /// gives. Same cost rule as above: it sits between a transfer's chunks.
    ///
    /// ❗ A host that hands out a lease must also refresh what
    /// [`volume_quiet_for`](Self::volume_quiet_for) reads, both when it takes one
    /// and when it gives one back. Taking is what keeps an app-wide reader in the
    /// loop; giving back is what starts the quiet window at the moment the
    /// operation ENDED, and it's what [`volume_busy_for_user`] leans on to read the
    /// two halves separately without ever reporting free mid-operation.
    fn volume_foreground_leases(&self, volume_id: &str) -> usize;

    /// A subscription that fires whenever either half of `volume_id`'s signal
    /// moves, so a waiter can sleep on the event instead of asking on a tick.
    ///
    /// ❗ Every write to either half has to bump it — a lease taken, a lease given
    /// back, a fresh timestamp. A host that moves a signal without bumping leaves
    /// [`wait_until_volume_free`] parked past the moment it should have resumed.
    fn watch_volume(&self, volume_id: &str) -> ActivityWatch;
}

/// A live subscription to one volume's activity, handed out by
/// [`UserActivity::watch_volume`].
///
/// It carries a VERSION, not a permit: taking one records where the volume's
/// signals stand, so a change landing between the moment a waiter takes the watch
/// and the moment it awaits is already recorded, and [`changed`](Self::changed)
/// returns at once. That's what makes [`wait_until_volume_free`] immune to a lost
/// wakeup, and why it takes the watch BEFORE it reads the signals.
pub struct ActivityWatch(Option<watch::Receiver<u64>>);

impl ActivityWatch {
    /// Watch a host's per-volume change counter.
    pub fn on(changes: watch::Receiver<u64>) -> Self {
        Self(Some(changes))
    }

    /// A host whose signals can never move (`AlwaysIdle`), so there's nothing to
    /// wake for.
    pub fn never() -> Self {
        Self(None)
    }

    /// Resolve once the volume's signals have moved since this watch was taken.
    /// Never resolves for a host that can't move them.
    pub async fn changed(&mut self) {
        if let Some(receiver) = &mut self.0 {
            if receiver.changed().await.is_ok() {
                return;
            }
            // The sender outlives every waiter in practice (a tracker keeps its
            // per-volume entry for the life of the process), but a closed channel
            // answers instantly and forever, which would turn this into a spin.
            // Degrade to "nothing will ever change here" instead.
            self.0 = None;
        }
        pending().await
    }
}

/// Whether the user has left `volume_id` alone for at least `threshold`.
///
/// ❗ The decaying half alone. Reach for [`volume_busy_for_user`] unless you
/// specifically want the timestamp.
pub fn volume_idle_for(activity: &dyn UserActivity, volume_id: &str, threshold: Duration) -> bool {
    activity
        .volume_quiet_for(volume_id)
        .is_none_or(|quiet| quiet >= threshold)
}

/// THE question background work asks: is the user waiting on this volume?
///
/// One rule in one place, because the two halves aren't interchangeable and a
/// consumer that picked either alone would be wrong in a case it never tests. The
/// lease covers an operation WHILE it runs; `quiet_window` covers the gap after it.
///
/// The two halves are read one after the other, so state can move between them,
/// and the ORDER is what makes that harmless: a lease taken in the gap leaves a
/// fresh timestamp for the second read to find, and a lease released in the gap
/// has already short-circuited to "busy". There is no ordering that reports free
/// while the user is waiting. That leans on an implementor refreshing its
/// timestamp whenever it hands out a lease, which is the contract
/// [`volume_foreground_leases`](UserActivity::volume_foreground_leases) states.
pub fn volume_busy_for_user(activity: &dyn UserActivity, volume_id: &str, quiet_window: Duration) -> bool {
    activity.volume_foreground_leases(volume_id) > 0 || !volume_idle_for(activity, volume_id, quiet_window)
}

/// Park until nobody is waiting on `volume_id` any more: every foreground
/// operation has ended AND the volume has been quiet for `quiet_window` since.
/// Returns immediately when that's already true.
///
/// Two different waits, because the two halves are different kinds of fact. A
/// lease has an owner, so its end is an EVENT to sleep on. A timestamp going stale
/// has nobody to announce it, so the leftover window is ONE sleep to a computed
/// deadline. ❌ Neither is a tick loop: a poll that re-asks every few dozen
/// milliseconds is the thing this replaced, and it costs a wakeup per tick per
/// parked transfer while making the resume LATER, not sooner.
///
/// A wake means "something moved", never "you're free" — the loop re-reads both
/// halves every time, so a spurious wake, a fresh navigation landing mid-window,
/// or a second listing starting all just re-park with a new deadline.
///
/// Cancellation belongs to the caller: this only ever resolves when the volume is
/// free, so a caller that must also unblock on a cancel or a cap races it in a
/// `select!` (`write_operations/transfer/checkpoint_stream.rs` does both).
pub async fn wait_until_volume_free(activity: &dyn UserActivity, volume_id: &str, quiet_window: Duration) {
    loop {
        // Take the watch BEFORE reading the signals. Anything that moves from here
        // on is recorded on it, so the wait below can't sleep through a release
        // that lands in the gap between the read and the await.
        let mut moved = activity.watch_volume(volume_id);

        if activity.volume_foreground_leases(volume_id) > 0 {
            // An operation is in flight; its end is the only thing worth waking for.
            moved.changed().await;
            continue;
        }
        let Some(remaining) = time_until_quiet(activity, volume_id, quiet_window) else {
            return;
        };
        // Nothing in flight, so the volume goes free on its own at a known instant.
        // Sleep straight to it, but stay awake to a lease taken in the meantime.
        tokio::select! {
            () = tokio::time::sleep(remaining) => {}
            () = moved.changed() => {}
        }
    }
}

/// How much of `quiet_window` is still to run before `volume_id` counts as quiet,
/// or `None` when it already does.
fn time_until_quiet(activity: &dyn UserActivity, volume_id: &str, quiet_window: Duration) -> Option<Duration> {
    let quiet_for = activity.volume_quiet_for(volume_id)?;
    quiet_window.checked_sub(quiet_for).filter(|left| !left.is_zero())
}

/// Nobody is using anything: bulk work never stands aside.
///
/// The right answer for a bench (which wants the protocol's real throughput) and
/// for a test (which shouldn't depend on wall-clock timing to make progress).
pub(super) struct AlwaysIdle;

impl UserActivity for AlwaysIdle {
    fn volume_quiet_for(&self, _volume_id: &str) -> Option<Duration> {
        None
    }

    fn volume_foreground_leases(&self, _volume_id: &str) -> usize {
        0
    }

    fn watch_volume(&self, _volume_id: &str) -> ActivityWatch {
        ActivityWatch::never()
    }
}

#[cfg(any(test, feature = "testing"))]
pub use scripted::BusyVolumes;

#[cfg(any(test, feature = "testing"))]
mod scripted {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use tokio::sync::watch;

    use super::{ActivityWatch, UserActivity};
    use crate::ignore_poison::IgnorePoison;

    /// One volume's scripted state, plus the change counter a waiter subscribes to.
    struct Scripted {
        /// The user touched this volume and hasn't stopped, as far as the test is
        /// concerned.
        busy: bool,
        leases: usize,
        changes: watch::Sender<u64>,
    }

    impl Default for Scripted {
        fn default() -> Self {
            Self {
                busy: false,
                leases: 0,
                changes: watch::Sender::new(0),
            }
        }
    }

    /// A [`UserActivity`] driven by a test rather than by a
    /// clock: the volumes marked busy are busy, everything else is idle.
    ///
    /// Waiting out a real threshold is what makes yield tests slow and flaky, so
    /// there are no real durations here at all: a busy volume reports zero quiet
    /// time (busy against any window a caller picks) and a quiet one reports
    /// "never touched". A test that needs the transfer to resume calls
    /// [`goes_quiet`](Self::goes_quiet), and the waiter wakes on the event rather
    /// than on a clock.
    ///
    /// The two halves of the signal are driven separately, so a test can say
    /// "a listing is in flight" ([`holds_a_lease`](Self::holds_a_lease)) without
    /// also saying "and the timestamp is fresh", and see which one a consumer
    /// actually reads.
    #[derive(Default)]
    pub struct BusyVolumes {
        volumes: Mutex<HashMap<String, Scripted>>,
    }

    impl BusyVolumes {
        /// A quiet machine.
        pub fn new() -> Self {
            Self::default()
        }

        /// Marks `volume_id` busy, as if the user just navigated it.
        pub fn is_busy(self, volume_id: &str) -> Self {
            self.becomes_busy(volume_id);
            self
        }

        /// The user starts using `volume_id` while the test is running.
        pub fn becomes_busy(&self, volume_id: &str) {
            self.change(volume_id, |scripted| scripted.busy = true);
        }

        /// A foreground operation started on `volume_id` and hasn't finished.
        pub fn holds_a_lease(self, volume_id: &str) -> Self {
            self.takes_a_lease(volume_id);
            self
        }

        /// A foreground operation starts on `volume_id` while the test is running.
        pub fn takes_a_lease(&self, volume_id: &str) {
            self.change(volume_id, |scripted| scripted.leases += 1);
        }

        /// One foreground operation on `volume_id` finished.
        pub fn releases_a_lease(&self, volume_id: &str) {
            self.change(volume_id, |scripted| {
                scripted.leases = scripted.leases.saturating_sub(1);
            });
        }

        /// The user stopped using `volume_id`, so anything parked on it resumes.
        pub fn goes_quiet(&self, volume_id: &str) {
            self.change(volume_id, |scripted| scripted.busy = false);
        }

        /// Move a volume's state and tell everyone watching it, the way a real
        /// tracker moves both under one lock.
        fn change(&self, volume_id: &str, edit: impl FnOnce(&mut Scripted)) {
            let mut volumes = self.volumes.lock_ignore_poison();
            let scripted = volumes.entry(volume_id.to_string()).or_default();
            edit(scripted);
            scripted.changes.send_modify(|version| *version += 1);
        }

        /// Read one volume's state, defaulting to "never touched".
        fn read<T>(&self, volume_id: &str, of: impl FnOnce(&Scripted) -> T, when_unknown: T) -> T {
            self.volumes
                .lock_ignore_poison()
                .get(volume_id)
                .map_or(when_unknown, of)
        }
    }

    impl UserActivity for BusyVolumes {
        fn volume_quiet_for(&self, volume_id: &str) -> Option<Duration> {
            self.read(volume_id, |scripted| scripted.busy.then_some(Duration::ZERO), None)
        }

        fn volume_foreground_leases(&self, volume_id: &str) -> usize {
            self.read(volume_id, |scripted| scripted.leases, 0)
        }

        fn watch_volume(&self, volume_id: &str) -> ActivityWatch {
            // Subscribing creates the entry when it's missing, so a watch taken
            // before the first `takes_a_lease` still sees it.
            let mut volumes = self.volumes.lock_ignore_poison();
            ActivityWatch::on(volumes.entry(volume_id.to_string()).or_default().changes.subscribe())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::sync::watch;

    use super::{
        ActivityWatch, AlwaysIdle, BusyVolumes, UserActivity, volume_busy_for_user, volume_idle_for,
        wait_until_volume_free,
    };

    /// A window nobody in production would use, to prove the composition never
    /// depends on wall-clock timing.
    const ANY_WINDOW: Duration = Duration::from_secs(30);

    /// The lease half on its own: an operation in flight makes the volume busy
    /// even though the timestamp says the user hasn't touched it. This is the
    /// case a timestamp-only signal gets wrong, and the reason the lease exists.
    #[test]
    fn an_operation_in_flight_is_busy_even_with_a_quiet_timestamp() {
        let listing_running = BusyVolumes::new().holds_a_lease("smb://naspi/media");
        assert!(volume_idle_for(&listing_running, "smb://naspi/media", ANY_WINDOW));
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

    /// A free volume doesn't park at all, and a host that tracks nothing never
    /// waits — `AlwaysIdle`'s watch can never fire, so a wait that reached it
    /// would hang a bench forever.
    #[tokio::test(start_paused = true)]
    async fn a_free_volume_never_parks() {
        wait_until_volume_free(&AlwaysIdle, "smb://naspi/media", ANY_WINDOW).await;
        wait_until_volume_free(&BusyVolumes::new(), "smb://naspi/media", ANY_WINDOW).await;
    }

    /// THE reason this is a wait and not a poll: the park ends on the EVENT of the
    /// last lease coming back, with no tick in between. Under a paused clock no
    /// timer can fire at all, so a tick loop would hang here.
    #[tokio::test(start_paused = true)]
    async fn the_park_ends_the_moment_the_last_lease_comes_back() {
        let volume_id = "smb://naspi/media";
        let activity = Arc::new(BusyVolumes::new().holds_a_lease(volume_id));

        let releasing = Arc::clone(&activity);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            releasing.releases_a_lease(volume_id);
        });

        wait_until_volume_free(&*activity, volume_id, ANY_WINDOW).await;
        assert!(!volume_busy_for_user(&*activity, volume_id, ANY_WINDOW));
    }

    /// A second listing starting while the first ends must re-park rather than let
    /// the transfer through: the wake means "something moved", never "you're free".
    #[tokio::test(start_paused = true)]
    async fn a_second_operation_starting_re_parks_the_wait() {
        let volume_id = "smb://naspi/media";
        let activity = Arc::new(BusyVolumes::new().holds_a_lease(volume_id));

        let scripted = Arc::clone(&activity);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            // Overlapping listings: the second starts before the first ends, so
            // the volume is never actually free in between.
            scripted.takes_a_lease(volume_id);
            scripted.releases_a_lease(volume_id);
            tokio::task::yield_now().await;
            scripted.releases_a_lease(volume_id);
        });

        wait_until_volume_free(&*activity, volume_id, ANY_WINDOW).await;
        assert_eq!(activity.volume_foreground_leases(volume_id), 0);
    }

    /// A host that releases its lease at the WORST possible moment: the instant
    /// the wait reads the count, which is after the wait has taken its watch and
    /// before it awaits.
    ///
    /// A wait ordered read-then-subscribe drops that release on the floor and
    /// parks forever. Taking the watch first records it, so the await returns at
    /// once. This is a deterministic lost-wakeup probe: no threads, no timing.
    struct ReleasesWhileBeingRead {
        leases: AtomicUsize,
        changes: watch::Sender<u64>,
    }

    impl UserActivity for ReleasesWhileBeingRead {
        fn volume_quiet_for(&self, _volume_id: &str) -> Option<Duration> {
            None
        }

        fn volume_foreground_leases(&self, _volume_id: &str) -> usize {
            let held = self.leases.swap(0, Ordering::SeqCst);
            if held > 0 {
                self.changes.send_modify(|version| *version += 1);
            }
            held
        }

        fn watch_volume(&self, _volume_id: &str) -> ActivityWatch {
            ActivityWatch::on(self.changes.subscribe())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_landing_between_the_read_and_the_await_is_not_lost() {
        let activity = ReleasesWhileBeingRead {
            leases: AtomicUsize::new(1),
            changes: watch::Sender::new(0),
        };
        // The paused clock makes a hang instant: nothing else can advance time, so
        // the timeout fires immediately if the wakeup was dropped.
        tokio::time::timeout(
            Duration::from_secs(60),
            wait_until_volume_free(&activity, "smb://naspi/media", ANY_WINDOW),
        )
        .await
        .expect("a lease released between the read and the await must still wake the park");
    }

    /// A host whose quiet time runs off tokio's clock, counting how often the wait
    /// asks for it.
    struct QuietSince {
        since: tokio::time::Instant,
        reads: AtomicUsize,
        changes: watch::Sender<u64>,
    }

    impl UserActivity for QuietSince {
        fn volume_quiet_for(&self, _volume_id: &str) -> Option<Duration> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Some(self.since.elapsed())
        }

        fn volume_foreground_leases(&self, _volume_id: &str) -> usize {
            0
        }

        fn watch_volume(&self, _volume_id: &str) -> ActivityWatch {
            ActivityWatch::on(self.changes.subscribe())
        }
    }

    /// The debounce tail is ONE sleep to a computed deadline. Nothing announces a
    /// timestamp going stale, so this is the one part that has to be a timer — and
    /// the read count is what proves it isn't a tick loop wearing a timer's
    /// clothes: work out the deadline once, wake at it, confirm once.
    #[tokio::test(start_paused = true)]
    async fn the_quiet_window_is_waited_out_in_one_sleep() {
        let window = Duration::from_millis(500);
        let already_quiet_for = Duration::from_millis(200);
        let since = tokio::time::Instant::now();
        tokio::time::advance(already_quiet_for).await;
        let activity = QuietSince {
            since,
            reads: AtomicUsize::new(0),
            changes: watch::Sender::new(0),
        };

        let started = tokio::time::Instant::now();
        wait_until_volume_free(&activity, "smb://naspi/media", window).await;

        assert_eq!(
            started.elapsed(),
            window - already_quiet_for,
            "the wait sleeps exactly the leftover window, not a rounded-up pile of ticks"
        );
        assert_eq!(
            activity.reads.load(Ordering::SeqCst),
            2,
            "one read to work out the deadline and one to confirm at it; more means a poll loop"
        );
    }

    /// The tail is a deadline, not a fixed nap: navigation landing mid-window
    /// pushes it out, and the wait wakes on that rather than sleeping through to a
    /// stale instant.
    #[tokio::test(start_paused = true)]
    async fn navigation_during_the_quiet_window_pushes_the_deadline_out() {
        let volume_id = "smb://naspi/media";
        let activity = Arc::new(BusyVolumes::new().is_busy(volume_id));

        let scripted = Arc::clone(&activity);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            // Still browsing: the volume must not come free on the original
            // deadline just because a timer expired.
            scripted.becomes_busy(volume_id);
            tokio::task::yield_now().await;
            scripted.goes_quiet(volume_id);
        });

        wait_until_volume_free(&*activity, volume_id, ANY_WINDOW).await;
        assert!(!volume_busy_for_user(&*activity, volume_id, ANY_WINDOW));
    }
}
