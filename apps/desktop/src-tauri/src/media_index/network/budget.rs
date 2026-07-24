//! A byte-bounded admission gate for network prefetch (plan M2).
//!
//! Parallel network enrichment fetches image bytes ahead of the decode+inference stage so
//! the (wire-serialized) fetch of image k+1 overlaps the compute of image k. Left
//! unbounded, that prefetch could buffer gigabytes: a per-file cap is 256 MB
//! (`network/fetch.rs::MAX_FETCH_BYTES`) and each in-flight decode holds a ~36 MB bitmap,
//! so a COUNT-based queue on a RAW-heavy corpus blows past the memory watchdog's ceiling.
//! So admission is bounded by BYTES, not file count: a fetcher acquires an image's byte
//! size before reading it and releases it once the decode has consumed it, so the total
//! bytes held in flight never exceeds the budget (the one exception: a single item larger
//! than the whole budget is admitted alone, so an over-cap file can't deadlock the pass).
//!
//! The budget is a fraction of the shared memory ceiling, NOT a second ceiling — the
//! existing indexing memory watchdog still governs the pool as a whole; this only caps the
//! prefetch buffer so it can't be the thing that trips the watchdog.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;

/// The default prefetch byte budget for a parallel network pass: 256 MB. Enough to keep a
/// handful of normal photos (a few MB compressed) or a couple of RAWs in flight so fetch
/// overlaps compute, while capping the buffer far below the memory watchdog's ceiling on a
/// RAW-heavy corpus (a count-based queue could otherwise buffer gigabytes). A single file
/// larger than this is still admitted alone, so an over-cap RAW never deadlocks the pass.
pub(crate) const DEFAULT_PREFETCH_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// A counting gate over a fixed byte budget. `acquire` blocks until `bytes` fit (or the
/// pass is stopping); `release` returns them. Cloneable-by-reference via `&` across the
/// fetcher and compute workers.
pub(crate) struct ByteBudget {
    capacity: u64,
    /// Bytes currently admitted (held by in-flight fetch + decode). Guarded, with a condvar
    /// so a `release` wakes a blocked `acquire`.
    in_use: Mutex<u64>,
    available: Condvar,
}

impl ByteBudget {
    /// A budget of `capacity` bytes. A zero capacity is treated as 1 so the gate always
    /// admits (degenerate, but never deadlocks).
    pub(crate) fn new(capacity: u64) -> Self {
        Self {
            capacity: capacity.max(1),
            in_use: Mutex::new(0),
            available: Condvar::new(),
        }
    }

    /// Whether admitting `bytes` on top of `in_use` keeps the invariant: it fits within
    /// capacity, OR nothing is in flight (so an over-cap single item is admitted alone
    /// rather than deadlocking). Pure, so the admission rule is unit-testable.
    fn admits(capacity: u64, in_use: u64, bytes: u64) -> bool {
        in_use == 0 || in_use + bytes <= capacity
    }

    /// Acquire `bytes` of budget, blocking until they fit. Returns `true` on success, or
    /// `false` if `should_stop` went true while waiting (a stopping pass must not block
    /// here forever). Re-checks `should_stop` on a short timeout so a stop that fires with
    /// no concurrent `release` to wake the condvar is still observed.
    pub(crate) fn acquire(&self, bytes: u64, should_stop: &(dyn Fn() -> bool + Sync)) -> bool {
        let mut in_use = self.in_use.lock_ignore_poison();
        while !Self::admits(self.capacity, *in_use, bytes) {
            if should_stop() {
                return false;
            }
            let (guard, _timeout) = self
                .available
                .wait_timeout(in_use, Duration::from_millis(50))
                .expect("byte-budget condvar wait");
            in_use = guard;
        }
        if should_stop() {
            return false;
        }
        *in_use += bytes;
        true
    }

    /// Return `bytes` to the budget and wake a blocked `acquire`. Saturating, so a
    /// double-release (a bug) can't underflow.
    pub(crate) fn release(&self, bytes: u64) {
        let mut in_use = self.in_use.lock_ignore_poison();
        *in_use = in_use.saturating_sub(bytes);
        // A release can free room for a waiter blocked on a large request; wake all so the
        // right one proceeds (spurious wakeups just re-check the predicate).
        self.available.notify_all();
    }

    /// Bytes currently in flight (for tests / observability).
    #[cfg(test)]
    pub(crate) fn in_use(&self) -> u64 {
        *self.in_use.lock_ignore_poison()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    use super::*;

    fn never_stop() -> bool {
        false
    }

    #[test]
    fn admits_within_capacity_and_admits_an_oversize_item_only_when_empty() {
        // Fits under capacity.
        assert!(ByteBudget::admits(100, 0, 50));
        assert!(ByteBudget::admits(100, 50, 50));
        // Would overflow capacity with something already in flight ⇒ wait.
        assert!(!ByteBudget::admits(100, 60, 50));
        // An item bigger than the whole budget is admitted ONLY when nothing else is in
        // flight (so it can't deadlock), never alongside another.
        assert!(ByteBudget::admits(100, 0, 250));
        assert!(!ByteBudget::admits(100, 10, 250));
    }

    #[test]
    fn acquire_release_tracks_in_use() {
        let b = ByteBudget::new(1000);
        assert!(b.acquire(400, &never_stop));
        assert_eq!(b.in_use(), 400);
        assert!(b.acquire(600, &never_stop));
        assert_eq!(b.in_use(), 1000);
        b.release(400);
        assert_eq!(b.in_use(), 600);
    }

    #[test]
    fn acquire_returns_false_when_stopping_instead_of_blocking_forever() {
        let b = ByteBudget::new(100);
        assert!(b.acquire(100, &never_stop));
        // The budget is full; a further acquire would block. With `should_stop` true it
        // returns false promptly rather than hanging the pass.
        assert!(!b.acquire(50, &(|| true)));
        assert_eq!(b.in_use(), 100, "a refused acquire takes nothing");
    }

    #[test]
    fn concurrent_acquisitions_never_exceed_the_budget() {
        // The core invariant: with many threads acquiring/holding/releasing items that each
        // fit the budget, the total in flight NEVER exceeds capacity. A peak-tracking
        // wrapper around the mutex would be racy, so instead assert it from each acquirer's
        // own post-acquire snapshot (taken under the same lock discipline via `in_use()`).
        let capacity = 1000u64;
        let budget = Arc::new(ByteBudget::new(capacity));
        let peak = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::new();
        for i in 0..16 {
            let budget = budget.clone();
            let peak = peak.clone();
            handles.push(thread::spawn(move || {
                let bytes = 100 + (i % 5) as u64 * 90; // 100..=460, all <= capacity
                for _ in 0..50 {
                    assert!(budget.acquire(bytes, &never_stop));
                    let now = budget.in_use();
                    peak.fetch_max(now, Ordering::SeqCst);
                    assert!(now <= capacity, "in_use {now} exceeded capacity {capacity}");
                    // allowed-test-sleep: holding the bytes for a beat is what creates the
                    // contention this test needs; releasing instantly, nobody ever overlaps
                    thread::sleep(Duration::from_micros(50));
                    budget.release(bytes);
                }
            }));
        }
        for h in handles {
            h.join().expect("join");
        }
        assert_eq!(budget.in_use(), 0, "all bytes released");
        // Real contention happened (the budget actually filled up at some point).
        assert!(
            peak.load(Ordering::SeqCst) > capacity / 2,
            "expected real budget pressure"
        );
    }

    #[test]
    fn an_oversize_item_is_admitted_alone_then_normal_items_resume() {
        let budget = Arc::new(ByteBudget::new(100));
        // Hold a normal item, so the oversize one must wait for the budget to empty.
        assert!(budget.acquire(60, &never_stop));
        let b2 = budget.clone();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d2 = done.clone();
        let waiter = thread::spawn(move || {
            // 250 > capacity 100: admitted only once `in_use` hits 0.
            assert!(b2.acquire(250, &never_stop));
            d2.store(true, Ordering::SeqCst);
            b2.release(250);
        });
        // allowed-test-sleep: negative assertion. The waiter must still be blocked, so the window
        // is the evidence; there is no "it stayed blocked" event to wait on
        thread::sleep(Duration::from_millis(20));
        assert!(
            !done.load(Ordering::SeqCst),
            "oversize item must wait while another is in flight"
        );
        budget.release(60);
        waiter.join().expect("join");
        assert!(done.load(Ordering::SeqCst));
        assert_eq!(budget.in_use(), 0);
    }
}
