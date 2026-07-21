//! The foreground-activity signal every background user yields to.
//!
//! A process-global "last foreground activity" timestamp, written by the hot
//! foreground filesystem IPC (directory listing — every navigation) via
//! [`note_foreground_activity`] / [`note_foreground_activity_on`]. Background work
//! reads it and backs off while the user is browsing. There are two SCOPES, and
//! picking the right one per consumer is the whole design:
//!
//! - **App-wide** ([`ForegroundActivity::idle_for`]): media enrichment
//!   (`media_index::scheduler`) uses it. Heavy on-device ML with no deadline, so
//!   foreground work anywhere is reason enough to wait.
//! - **Per volume** ([`ForegroundActivity::idle_for_volume`]): the network index
//!   scan (`indexing::scan_pace`) and cross-volume transfers (`SmbVolume`'s
//!   `Volume` foreground-yield methods) use it. Their contention is one share's SMB
//!   session, so browsing a LOCAL folder is no reason to slow a NAS scan.
//!
//! One call records both: [`note_foreground_activity_on`] stamps the volume's
//! timestamp AND the app-wide one, so an app-wide reader never misses activity.
//! A volume nobody has browsed has no entry and reads as idle (full speed) —
//! the right answer for a share the user hasn't touched this session.
//!
//! The decision is the pure [`is_idle`] over millis, unit-tested against a fake
//! clock; the global just supplies "now" from a monotonic base instant.

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

/// The foreground-activity tracker: the last time the user did foreground work,
/// app-wide and per volume, as millis since [`BASE`]. The app-wide read is
/// lock-free; the per-volume read takes an uncontended read lock over a map with
/// one entry per browsed volume (a handful, bounded by mounted volumes).
pub struct ForegroundActivity {
    last_activity_millis: AtomicU64,
    /// Last foreground activity per volume id, same clock as
    /// `last_activity_millis`. A missing key means "never browsed" ⇒ idle.
    per_volume: RwLock<HashMap<String, u64>>,
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

    /// Record foreground activity ON a specific volume. Stamps the volume AND the
    /// app-wide timestamp, so an app-wide reader never misses scoped activity.
    pub fn note_on(&self, volume_id: &str) {
        let now = millis_now();
        self.last_activity_millis.store(now, Ordering::Relaxed);
        let mut map = self.per_volume.write_ignore_poison();
        match map.get_mut(volume_id) {
            Some(slot) => *slot = now,
            None => {
                map.insert(volume_id.to_string(), now);
            }
        }
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
        let last = self.per_volume.read_ignore_poison().get(volume_id).copied()?;
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
