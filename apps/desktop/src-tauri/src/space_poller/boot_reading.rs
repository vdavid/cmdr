//! The boot volume's latest free-space reading, shared by the poll loop and the drive
//! index's on-demand "is the disk you live on low?" (`low_space::index_storage_low`).
//!
//! The poll loop records every boot-volume reading it takes, but it only polls the boot
//! volume while something shows it (a pane, or the low-disk-space warning). The index
//! asks regardless, so when the last reading is older than [`FRESH_FOR`] it takes one
//! itself. The lock is held across that fetch, so concurrent writers share one reading
//! instead of each paying for their own.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::file_system::volume::SpaceInfo;
use crate::ignore_poison::IgnorePoison;

/// How old a reading may be and still answer the index. A touch over the poller's 2 s
/// cadence, so while it polls the boot volume anyway its readings serve every ask, and a
/// writer asking once a second never pays for a fetch of its own.
pub(super) const FRESH_FOR: Duration = Duration::from_millis(2_500);

/// Whether a reading taken at `taken_at` still answers at `now`. A reading stamped after
/// `now` (recorded while this caller waited for the lock) is as fresh as it gets.
pub(super) fn is_fresh(taken_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(taken_at) < FRESH_FOR
}

/// The latest boot-volume reading and when it was taken.
pub(super) struct BootReading {
    latest: Mutex<Option<(Instant, SpaceInfo)>>,
}

impl BootReading {
    pub(super) const fn new() -> Self {
        Self {
            latest: Mutex::new(None),
        }
    }

    /// Records a reading someone else took (the poll loop).
    pub(super) fn record(&self, space: SpaceInfo, taken_at: Instant) {
        *self.latest.lock_ignore_poison() = Some((taken_at, space));
    }

    /// A reading no older than [`FRESH_FOR`] at `now`: the recorded one if it's fresh,
    /// else `fetch`'s, which is recorded for the next caller.
    ///
    /// Holds the lock across `fetch`, so callers arriving meanwhile wait for that one
    /// reading and then find it fresh: one fetch, however many writers ask at once.
    /// `None` when nothing fresh is recorded and `fetch` came back empty; the failed
    /// fetch isn't recorded, so the next caller tries again.
    pub(super) fn fresh_or(&self, now: Instant, fetch: impl FnOnce() -> Option<SpaceInfo>) -> Option<SpaceInfo> {
        let mut latest = self.latest.lock_ignore_poison();
        if let Some((taken_at, space)) = *latest
            && is_fresh(taken_at, now)
        {
            return Some(space);
        }
        let space = fetch()?;
        *latest = Some((now, space));
        Some(space)
    }
}

/// The one boot-volume reading the poll loop and the index share.
pub(super) static BOOT_READING: BootReading = BootReading::new();

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn reading(available: u64) -> SpaceInfo {
        SpaceInfo::bounded(100 * GIB, available)
    }

    #[test]
    fn a_reading_answers_until_it_is_older_than_the_window() {
        let at = Instant::now();
        assert!(is_fresh(at, at));
        assert!(is_fresh(at, at + FRESH_FOR - Duration::from_millis(1)));
        assert!(!is_fresh(at, at + FRESH_FOR));
    }

    #[test]
    fn a_reading_recorded_after_the_caller_looked_at_the_clock_is_fresh() {
        let now = Instant::now();
        assert!(is_fresh(now + Duration::from_secs(1), now));
    }

    #[test]
    fn the_pollers_reading_spares_the_index_a_fetch_of_its_own() {
        let cache = BootReading::new();
        let now = Instant::now();
        cache.record(reading(10 * GIB), now);
        let got = cache.fresh_or(now + Duration::from_secs(1), || panic!("the poller's reading is fresh"));
        assert_eq!(got, Some(reading(10 * GIB)));
    }

    #[test]
    fn a_stale_reading_is_replaced_by_a_fresh_one_which_then_serves() {
        let cache = BootReading::new();
        let start = Instant::now();
        cache.record(reading(10 * GIB), start);
        let later = start + FRESH_FOR;
        assert_eq!(cache.fresh_or(later, || Some(reading(3 * GIB))), Some(reading(3 * GIB)));
        assert_eq!(
            cache.fresh_or(later + Duration::from_secs(1), || panic!("recorded at `later`")),
            Some(reading(3 * GIB))
        );
    }

    #[test]
    fn a_failed_fetch_answers_nothing_and_is_not_remembered() {
        let cache = BootReading::new();
        let now = Instant::now();
        assert_eq!(cache.fresh_or(now, || None), None);
        assert_eq!(cache.fresh_or(now, || Some(reading(GIB))), Some(reading(GIB)));
    }

    #[test]
    fn concurrent_writers_share_one_fetch() {
        let cache = Arc::new(BootReading::new());
        let fetches = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let start = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (cache, fetches, start) = (Arc::clone(&cache), Arc::clone(&fetches), Arc::clone(&start));
                std::thread::spawn(move || {
                    start.wait();
                    cache.fresh_or(now, || {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        Some(reading(GIB))
                    })
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().expect("the thread finishes"), Some(reading(GIB)));
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1, "no thundering statfs");
    }
}
