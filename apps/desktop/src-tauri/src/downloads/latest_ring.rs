//! Bounded ring of recently-observed downloads.
//!
//! The watcher pushes every eligible download here; the "go to latest
//! download" action reads from the back. Capacity 10 by default, oldest
//! drops off the front. Re-pushing the same path moves it to the back so
//! the most-recent occurrence wins; we never want a stale entry to shadow
//! a re-downloaded file.
//!
//! The ring survives across hotkey presses and is cleared only on Cmdr
//! restart. Persisting it to disk would risk pointing the user at a file
//! that was deleted while Cmdr was closed; the watcher's scan-fallback
//! covers the cold-start path.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// Default capacity. Picked from the plan; small enough to be cheap, big
/// enough that "go to previous" extensions (deferred) wouldn't need a
/// data-structure swap.
const DEFAULT_CAPACITY: usize = 10;

#[derive(Debug)]
pub struct LatestRing {
    state: Mutex<VecDeque<(PathBuf, Instant)>>,
    capacity: usize,
}

impl LatestRing {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Test-friendly variant. Production code should use `new`.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            state: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    /// Record an observed download. If `path` is already in the ring, its
    /// existing entry is removed before the new one is pushed to the back;
    /// the most-recent occurrence wins and we never carry duplicates.
    pub fn push(&self, path: PathBuf, observed_at: Instant) {
        let mut s = self.state.lock().expect("LatestRing poisoned");
        if let Some(pos) = s.iter().position(|(p, _)| *p == path) {
            s.remove(pos);
        }
        s.push_back((path, observed_at));
        while s.len() > self.capacity {
            s.pop_front();
        }
    }

    /// The most-recently observed path, or `None` if the ring is empty.
    /// Returns an owned `PathBuf` so we don't have to hold the lock across
    /// the caller's use.
    pub fn latest(&self) -> Option<PathBuf> {
        let s = self.state.lock().expect("LatestRing poisoned");
        s.back().map(|(p, _)| p.clone())
    }

    /// Test-only observer of the ring's occupancy. Production reads only
    /// `push` + `latest`; this exists so unit tests can assert the capacity
    /// cap and dedup invariants.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let s = self.state.lock().expect("LatestRing poisoned");
        s.len()
    }
}

impl Default for LatestRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_ring_has_no_latest() {
        let ring = LatestRing::new();
        assert_eq!(ring.latest(), None);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn single_push_is_latest() {
        let ring = LatestRing::new();
        let p = pb("/Users/x/Downloads/foo.zip");
        ring.push(p.clone(), Instant::now());
        assert_eq!(ring.latest(), Some(p));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn overflow_drops_oldest() {
        let ring = LatestRing::new();
        // Default cap is 10.
        for i in 0..11 {
            ring.push(pb(&format!("/d/f{i}")), Instant::now());
        }
        assert_eq!(ring.len(), 10);
        assert_eq!(ring.latest(), Some(pb("/d/f10")));
        // The oldest (`/d/f0`) is gone, but `/d/f1` is still in.
        // Drain via repeated `latest` would require popping; instead just
        // assert the front-eviction shape by exact length and last entry.
    }

    #[test]
    fn re_push_moves_to_back() {
        let ring = LatestRing::new();
        let a = pb("/d/a");
        let b = pb("/d/b");
        let c = pb("/d/c");
        ring.push(a.clone(), Instant::now());
        ring.push(b.clone(), Instant::now());
        ring.push(c.clone(), Instant::now());
        ring.push(a.clone(), Instant::now());
        assert_eq!(ring.latest(), Some(a));
        assert_eq!(ring.len(), 3, "no duplicates after re-push");
    }

}
