//! The foreground-activity signal every background user yields to.
//!
//! Two halves, per volume, and both are needed:
//!
//! - **A LEASE** ([`ForegroundActivity::lease_volume`]) held for the real duration
//!   of a foreground operation. A directory listing takes one at the top of its
//!   task and gives it back by drop, so a share the user is waiting on reads as
//!   busy for however long the listing actually takes.
//! - **A TIMESTAMP** of the last foreground activity, written by the hot
//!   foreground filesystem IPC via [`note_foreground_activity`] /
//!   [`note_foreground_activity_on`] and refreshed when a lease is released. It
//!   covers the GAP AFTER an operation, so a burst of arrow-key presses reads as
//!   one continuous action instead of one park per keystroke.
//!
//! Neither half alone is right: a timestamp decays while the operation it protects
//! is still running, and a lease says nothing about the moment between two of them.
//! The composed question ("is the user waiting on this volume?") is
//! `cmdr_fs::volume::host::activity::volume_busy_for_user`, which is where the two
//! are put together for every consumer.
//!
//! Background work reads this and backs off while the user is browsing. There are
//! two SCOPES, and picking the right one per consumer is the whole design:
//!
//! - **App-wide** ([`ForegroundActivity::idle_for`]): media enrichment
//!   (`media_index::scheduler`) uses it. Heavy on-device ML with no deadline, so
//!   foreground work anywhere is reason enough to wait. Timestamp only: a lease
//!   names a volume, and there is no app-wide claim to hold. Taking or releasing
//!   one stamps this timestamp, so a long listing keeps the app looking busy in
//!   the same decaying way a navigation does.
//! - **Per volume** ([`ForegroundActivity::idle_for_volume`] +
//!   [`ForegroundActivity::volume_lease_count`]): the network index scan
//!   (`indexing::network_scanner::scan_pace`) and cross-volume transfers
//!   (`SmbVolume`'s `Volume` foreground-yield methods) use both halves. Their
//!   contention is one share's SMB session, so browsing a LOCAL folder is no
//!   reason to slow a NAS scan.
//!
//! One call records both scopes: [`note_foreground_activity_on`] and every lease
//! stamp write the volume's timestamp AND the app-wide one, so an app-wide reader
//! never misses activity. A volume nobody has browsed has no entry and reads as
//! idle (full speed) — the right answer for a share the user hasn't touched this
//! session.
//!
//! The timestamp decision is the pure [`is_idle`] over millis, unit-tested against
//! a fake clock; the global just supplies "now" from a monotonic base instant.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

use crate::ignore_poison::RwLockIgnorePoison;

/// A monotonic base instant so activity timestamps are small, wrap-free `u64` millis
/// (an `Instant` isn't storable in an atomic; millis-since-base is).
static BASE: LazyLock<Instant> = LazyLock::new(Instant::now);

fn millis_now() -> u64 {
    BASE.elapsed().as_millis() as u64
}

/// What the foreground is doing on ONE volume: when it last did something, and
/// how many operations it has in flight right now.
struct VolumeActivity {
    /// Last foreground activity on this volume, millis since [`BASE`].
    last_millis: u64,
    /// Foreground operations in flight, one per live [`ForegroundLease`]. A count
    /// rather than a flag, so two panes listing one share both have to finish
    /// before it reads free (the same shape the transfer gauge uses).
    leases: usize,
}

/// The foreground-activity tracker: the last time the user did foreground work
/// (app-wide and per volume, as millis since [`BASE`]) plus the foreground
/// operations in flight per volume. The app-wide read is lock-free; the per-volume
/// read takes an uncontended read lock over a map with one entry per browsed
/// volume (a handful, bounded by mounted volumes).
pub struct ForegroundActivity {
    last_activity_millis: AtomicU64,
    /// Per volume id. A missing key means "never browsed" ⇒ idle, no leases.
    per_volume: RwLock<HashMap<String, VolumeActivity>>,
}

impl ForegroundActivity {
    fn new() -> Self {
        Self {
            last_activity_millis: AtomicU64::new(0),
            per_volume: RwLock::new(HashMap::new()),
        }
    }

    /// Record that foreground activity just happened, without attributing it to a
    /// volume (called from foreground IPC that has no volume id to hand).
    pub fn note(&self) {
        self.last_activity_millis.store(millis_now(), Ordering::Relaxed);
    }

    /// Stamp `volume_id` (and the app) as active NOW, and adjust its in-flight
    /// lease count under the same write lock.
    ///
    /// ❗ Both halves move together on purpose. A reader that saw the count drop
    /// before the timestamp that has to carry the debounce would find the volume
    /// briefly free the instant a listing ended, which is exactly the flap the
    /// debounce exists to prevent.
    fn stamp(&self, volume_id: &str, adjust_leases: impl FnOnce(&mut usize)) {
        let now = millis_now();
        self.last_activity_millis.store(now, Ordering::Relaxed);
        let mut map = self.per_volume.write_ignore_poison();
        let slot = map.entry(volume_id.to_string()).or_insert(VolumeActivity {
            last_millis: now,
            leases: 0,
        });
        slot.last_millis = now;
        adjust_leases(&mut slot.leases);
    }

    /// Record foreground activity ON a specific volume. Stamps the volume AND the
    /// app-wide timestamp, so an app-wide reader never misses scoped activity.
    pub fn note_on(&self, volume_id: &str) {
        self.stamp(volume_id, |_| {});
    }

    /// Claim that the user is waiting on `volume_id` for as long as the returned
    /// guard lives, and stamp the volume as active now.
    ///
    /// This is the EXACT half of the signal: a directory listing takes one for its
    /// whole duration, so a share the user is waiting on stays busy however long
    /// the listing runs, where a timestamp would have decayed underneath it.
    pub fn lease_volume(&self, volume_id: &str) -> ForegroundLease<'_> {
        self.stamp(volume_id, |leases| *leases += 1);
        ForegroundLease {
            activity: self,
            volume_id: volume_id.to_string(),
        }
    }

    /// Give a lease back and restamp the volume, so the post-operation debounce
    /// starts from the moment the operation actually ended. Only [`ForegroundLease`]
    /// calls this.
    fn release_lease(&self, volume_id: &str) {
        self.stamp(volume_id, |leases| *leases = leases.saturating_sub(1));
    }

    /// How many foreground operations are in flight on `volume_id` right now. A
    /// volume nobody has browsed holds none.
    pub fn volume_lease_count(&self, volume_id: &str) -> usize {
        self.per_volume
            .read_ignore_poison()
            .get(volume_id)
            .map_or(0, |slot| slot.leases)
    }

    /// Whether the app has been idle (no foreground activity anywhere) for at
    /// least `threshold`.
    pub fn idle_for(&self, threshold: Duration) -> bool {
        is_idle(
            millis_now(),
            self.last_activity_millis.load(Ordering::Relaxed),
            threshold,
        )
    }

    /// `(now, last foreground activity on `volume_id`)` in the same millis clock,
    /// or `None` when nobody has browsed this volume.
    ///
    /// ❌ Don't collapse the missing entry to a `0` timestamp: `0` is a real point
    /// on this clock (millis since [`BASE`], set lazily on first use), so "never
    /// browsed" would read as "browsed at startup" and make every background user
    /// stand aside for the app's first `threshold`. Callers that want a decision
    /// rather than raw millis take the `None` arm as "idle".
    pub fn volume_activity_millis(&self, volume_id: &str) -> Option<(u64, u64)> {
        let last = self.per_volume.read_ignore_poison().get(volume_id)?.last_millis;
        Some((millis_now(), last))
    }

    /// Whether `volume_id` has been idle (no foreground activity on THIS volume)
    /// for at least `threshold`. A volume nobody has browsed reads as idle.
    pub fn idle_for_volume(&self, volume_id: &str, threshold: Duration) -> bool {
        match self.volume_activity_millis(volume_id) {
            Some((now, last)) => is_idle(now, last, threshold),
            None => true, // never browsed ⇒ nothing to stand aside for
        }
    }
}

/// A live claim that the user is waiting on one volume RIGHT NOW.
///
/// Release is by DROP and only by drop, which is the whole design: the lease comes
/// back on the error path, on a panic, and when the task holding it is dropped
/// (runtime shutdown included), with nobody having to remember anything.
/// ❌ Don't grow a manual `release()` — a second way out is a way to leak one, and
/// a leaked lease pins a share busy for the rest of the session.
///
/// Dropping also restamps the volume, so the debounce window starts when the
/// operation ended rather than when it began.
#[must_use = "the volume is busy only while the lease is alive; binding it to `_` drops it immediately"]
pub struct ForegroundLease<'a> {
    activity: &'a ForegroundActivity,
    volume_id: String,
}

impl Drop for ForegroundLease<'_> {
    fn drop(&mut self) {
        self.activity.release_lease(&self.volume_id);
    }
}

/// The pure idle decision: idle iff at least `threshold` elapsed since the last
/// activity. Saturating so a clock quirk can't underflow into a false "busy".
pub fn is_idle(now_millis: u64, last_activity_millis: u64, threshold: Duration) -> bool {
    now_millis.saturating_sub(last_activity_millis) >= threshold.as_millis() as u64
}

/// The process-global tracker background work reads and foreground IPC writes.
/// `LazyLock` (not a plain `static`) because the per-volume map isn't
/// const-constructible.
static GLOBAL: LazyLock<ForegroundActivity> = LazyLock::new(ForegroundActivity::new);

/// The process-global foreground-activity tracker.
pub fn global() -> &'static ForegroundActivity {
    &GLOBAL
}

/// Record foreground activity on the global tracker, unattributed. Called from the
/// hot foreground filesystem IPC that has no volume id to hand.
pub fn note_foreground_activity() {
    GLOBAL.note();
}

/// Record foreground activity ON a volume (the hot listing IPC, which knows which
/// volume the user navigated). Feeds both the app-wide and the per-volume readers,
/// so the network index scan and SMB transfers back off for THIS share while media
/// enrichment backs off for any activity at all.
pub fn note_foreground_activity_on(volume_id: &str) {
    GLOBAL.note_on(volume_id);
}

/// Hold `volume_id` busy for as long as the returned guard lives.
///
/// Taken by the spawned directory-listing task (`file_system::listing::streaming`),
/// which is what makes "is the user waiting on this share?" a fact for the real
/// duration of a listing instead of an estimate that expires mid-wait.
pub fn lease_foreground_on(volume_id: &str) -> ForegroundLease<'static> {
    GLOBAL.lease_volume(volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_only_after_the_threshold_elapses() {
        let threshold = Duration::from_secs(5);
        // Fake clock: last activity at 1_000 ms.
        let last = 1_000;
        // 3 s later (4_000 ms): only 3 s idle < 5 s ⇒ NOT idle.
        assert!(!is_idle(4_000, last, threshold), "3s idle is below the 5s threshold");
        // Exactly 5 s later (6_000 ms): idle.
        assert!(is_idle(6_000, last, threshold), "5s idle meets the threshold");
        // 10 s later: idle.
        assert!(is_idle(11_000, last, threshold));
    }

    #[test]
    fn a_now_before_last_activity_reads_as_not_idle_never_panics() {
        // Saturating subtraction: a now earlier than last activity yields 0 elapsed.
        assert!(!is_idle(500, 1_000, Duration::from_secs(1)));
    }

    #[test]
    fn note_then_immediately_check_is_not_idle() {
        let tracker = ForegroundActivity::new();
        tracker.note();
        assert!(
            !tracker.idle_for(Duration::from_secs(1)),
            "just noted activity ⇒ not idle for a 1s window"
        );
    }

    /// The scope contract the scan and SMB transfers depend on: activity on one
    /// volume must NOT make another volume look busy. Without this, browsing a
    /// local folder would throttle a NAS scan that isn't competing with it.
    #[test]
    fn activity_on_one_volume_leaves_other_volumes_idle() {
        let tracker = ForegroundActivity::new();
        let window = Duration::from_secs(1);
        tracker.note_on("smb://naspi/media");
        assert!(
            !tracker.idle_for_volume("smb://naspi/media", window),
            "the browsed volume is busy"
        );
        assert!(
            tracker.idle_for_volume("root", window),
            "a volume nobody browsed stays idle"
        );
    }

    /// A never-browsed volume has no entry at all; it must read idle rather than
    /// panic or default to busy (a busy default would stall every first scan).
    #[test]
    fn an_unknown_volume_reads_as_idle() {
        let tracker = ForegroundActivity::new();
        assert!(tracker.idle_for_volume("never-seen", Duration::from_millis(1)));
    }

    /// A scoped note also feeds the app-wide reader, so media enrichment (which
    /// only reads app-wide) can't miss navigation that was attributed to a volume.
    #[test]
    fn a_scoped_note_also_marks_the_app_busy() {
        let tracker = ForegroundActivity::new();
        tracker.note_on("smb://naspi/media");
        assert!(!tracker.idle_for(Duration::from_secs(1)));
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;

    /// THE reason the lease exists: a listing that takes longer than the threshold
    /// must keep the volume busy for its whole duration. A timestamp alone decays
    /// while the user is still staring at a spinner.
    #[test]
    fn a_held_lease_keeps_the_volume_busy_however_long_the_threshold_has_passed() {
        let tracker = ForegroundActivity::new();
        let volume = "smb://naspi/media";
        let _listing = tracker.lease_volume(volume);
        assert_eq!(tracker.volume_lease_count(volume), 1);
        assert!(
            tracker.idle_for_volume(volume, Duration::ZERO),
            "the timestamp alone has already decayed"
        );
    }

    /// The other half: once the listing ends the volume is free again.
    #[test]
    fn the_lease_comes_back_on_drop() {
        let tracker = ForegroundActivity::new();
        let volume = "smb://naspi/media";
        {
            let _listing = tracker.lease_volume(volume);
        }
        assert_eq!(tracker.volume_lease_count(volume), 0);
    }

    /// Releasing RESTAMPS the volume, so the post-listing debounce measures from
    /// when the listing ended. Stamping only at the start would leave a share that
    /// took ten seconds to list instantly free the moment it finished, and a burst
    /// of navigations would park a transfer once per keystroke instead of once.
    #[test]
    fn releasing_a_lease_restamps_the_volume_so_the_debounce_starts_then() {
        let tracker = ForegroundActivity::new();
        let volume = "smb://naspi/restamp";
        let listing = tracker.lease_volume(volume);
        let (_, taken_at) = tracker
            .volume_activity_millis(volume)
            .expect("taking a lease stamps the volume");

        // The millis clock has to actually tick, or "restamped" and "not
        // restamped" are the same number and the assertion below is vacuous.
        crate::test_support::wait_until(
            Duration::from_secs(2),
            "the millis clock to advance past the moment the lease was taken",
            || {
                tracker
                    .volume_activity_millis(volume)
                    .is_some_and(|(now, _)| now > taken_at)
            },
        );

        drop(listing);
        let (_, released_at) = tracker
            .volume_activity_millis(volume)
            .expect("the volume keeps its entry");
        assert!(
            released_at > taken_at,
            "the debounce has to start when the listing ended, not when it began"
        );
    }

    /// Two panes listing one share: the volume stays busy until the LAST listing
    /// ends, exactly like the transfer gauge's count.
    #[test]
    fn two_listings_on_one_volume_both_have_to_finish() {
        let tracker = ForegroundActivity::new();
        let volume = "smb://naspi/media";
        let first = tracker.lease_volume(volume);
        let second = tracker.lease_volume(volume);
        assert_eq!(tracker.volume_lease_count(volume), 2);
        drop(first);
        assert_eq!(tracker.volume_lease_count(volume), 1, "one listing is still running");
        drop(second);
        assert_eq!(tracker.volume_lease_count(volume), 0);
    }

    /// The scope contract again, for the exact half: a listing on one share must
    /// not make another share look busy.
    #[test]
    fn a_lease_on_one_volume_leaves_every_other_volume_free() {
        let tracker = ForegroundActivity::new();
        let _listing = tracker.lease_volume("smb://naspi/media");
        assert_eq!(tracker.volume_lease_count("root"), 0);
        assert!(tracker.idle_for_volume("root", Duration::from_millis(1)));
    }

    /// RAII is the whole point: a listing task that panics must give the lease
    /// back, or the share stays pinned busy for the rest of the session.
    #[test]
    fn a_lease_dropped_by_a_panic_still_comes_back() {
        let tracker = ForegroundActivity::new();
        let volume = "smb://naspi/panics";
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _listing = tracker.lease_volume(volume);
            panic!("the listing task blew up");
        }));
        assert!(unwound.is_err(), "the panic has to actually happen");
        assert_eq!(tracker.volume_lease_count(volume), 0);
    }

    /// A volume nobody has listed holds no lease, rather than panicking on a
    /// missing entry.
    #[test]
    fn an_unknown_volume_holds_no_lease() {
        assert_eq!(ForegroundActivity::new().volume_lease_count("never-seen"), 0);
    }

    /// Taking a lease is foreground activity, so an app-wide reader (media
    /// enrichment) sees it the same way it sees a navigation.
    #[test]
    fn taking_a_lease_also_marks_the_app_busy() {
        let tracker = ForegroundActivity::new();
        let _listing = tracker.lease_volume("smb://naspi/media");
        assert!(!tracker.idle_for(Duration::from_secs(30)));
    }
}
