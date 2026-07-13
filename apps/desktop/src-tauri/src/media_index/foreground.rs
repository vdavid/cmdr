//! The app-wide foreground-activity signal that gates conservative network (SMB)
//! enrichment (plan Decision 6, M1.5).
//!
//! There is deliberately NO foreground/idle signal in `indexing/` (its only `Idle`
//! is `ActivityPhase::Idle`, an indexing work-state, not a user-foreground signal),
//! so this is new, minimal work: a single process-global "last foreground activity"
//! timestamp. High-traffic foreground filesystem IPC (directory listing — every
//! navigation) calls [`note_foreground_activity`]; the network enrichment pass only
//! proceeds once the app has been idle for a threshold, so a NAS is swept while the
//! user isn't actively browsing, never in competition with foreground work.
//!
//! The decision is the pure [`is_idle`] over millis, unit-tested against a fake
//! clock; the global just supplies "now" from a monotonic base instant.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A monotonic base instant so activity timestamps are small, wrap-free `u64` millis
/// (an `Instant` isn't storable in an atomic; millis-since-base is).
static BASE: LazyLock<Instant> = LazyLock::new(Instant::now);

fn millis_now() -> u64 {
    BASE.elapsed().as_millis() as u64
}

/// The app-wide foreground-activity tracker: the last time the user did foreground
/// work, as millis since [`BASE`]. Cheap, lock-free.
pub struct ForegroundActivity {
    last_activity_millis: AtomicU64,
}

impl ForegroundActivity {
    const fn new() -> Self {
        Self {
            last_activity_millis: AtomicU64::new(0),
        }
    }

    /// Record that foreground activity just happened (called from foreground IPC).
    pub fn note(&self) {
        self.last_activity_millis.store(millis_now(), Ordering::Relaxed);
    }

    /// Whether the app has been idle (no foreground activity) for at least
    /// `threshold`.
    pub fn idle_for(&self, threshold: Duration) -> bool {
        is_idle(
            millis_now(),
            self.last_activity_millis.load(Ordering::Relaxed),
            threshold,
        )
    }
}

/// The pure idle decision: idle iff at least `threshold` elapsed since the last
/// activity. Saturating so a clock quirk can't underflow into a false "busy".
pub fn is_idle(now_millis: u64, last_activity_millis: u64, threshold: Duration) -> bool {
    now_millis.saturating_sub(last_activity_millis) >= threshold.as_millis() as u64
}

/// The process-global tracker the live scheduler reads and foreground IPC writes.
static GLOBAL: ForegroundActivity = ForegroundActivity::new();

/// The process-global foreground-activity tracker.
pub fn global() -> &'static ForegroundActivity {
    &GLOBAL
}

/// Record foreground activity on the global tracker. Called from the hot foreground
/// filesystem IPC (directory listing) so network enrichment yields to real use.
pub fn note_foreground_activity() {
    GLOBAL.note();
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
}
