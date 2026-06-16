//! Cmdr-own-write ignore set.
//!
//! When Cmdr writes into `~/Downloads` (copy, move, mkdir, rename target,
//! paste), the watcher would otherwise fire a toast for our own action.
//! Call sites register their target paths via [`IgnoreSet::note_pending`]
//! just before the syscall, with a short TTL (5 s by default in the watcher).
//!
//! The watcher checks each incoming event against [`IgnoreSet::is_pending`]
//! and silently drops matches.
//!
//! ## Why a hashmap with TTL instead of a counter
//!
//! Simpler, no risk of leaking permanent entries if a write fails mid-flight,
//! and FS events arrive within a few hundred ms of the syscall, so 5 s is
//! plenty of headroom. Lazy expiry on every check, plus a 1000-entry FIFO
//! cap as a safety valve, keep the map small even if no events ever arrive
//! to trigger lazy expiry.
//!
//! ## Scoping
//!
//! `note_pending` silently no-ops for paths NOT under the resolved Downloads
//! root, so hook sites can call unconditionally. This rule is locked in by
//! the plan — don't move the filter to the call sites.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Default cap on the ignore set's in-memory entry count. The map normally
/// holds well under 10 entries; this cap is a safety valve in case events
/// never arrive in a session.
const DEFAULT_MAX_ENTRIES: usize = 1000;

#[derive(Debug)]
struct State {
    /// Deadline per path. `Instant` past now means expired.
    map: HashMap<PathBuf, Instant>,
    /// Insertion order, oldest at the front. Used for FIFO eviction at cap.
    /// Existing entries keep their position on a re-insert (their deadline
    /// is bumped in `map`, but they don't move in `order`).
    order: VecDeque<PathBuf>,
}

/// Set of paths Cmdr expects to write soon, with per-entry TTLs.
///
/// Cheap to clone via `Arc`; or share by reference. Internally a single
/// `std::sync::Mutex` guards the map+order. The mutex is `std::sync` (not
/// `tokio::sync`) on purpose: the critical section is tiny and called from
/// a sync `notify` callback, so we don't want to require a Tokio runtime.
#[derive(Debug)]
pub struct IgnoreSet {
    state: Mutex<State>,
    max_entries: usize,
    downloads_root: PathBuf,
}

impl IgnoreSet {
    /// Build an ignore set scoped to `downloads_root`. Paths outside this
    /// root will silently no-op on `note_pending`.
    pub fn new(downloads_root: PathBuf) -> Self {
        Self::with_capacity(downloads_root, DEFAULT_MAX_ENTRIES)
    }

    /// Like `new` but with a custom cap. Useful for tests that want to
    /// exercise the eviction path without inserting 1000 entries.
    pub fn with_capacity(downloads_root: PathBuf, max_entries: usize) -> Self {
        Self {
            state: Mutex::new(State {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
            max_entries,
            downloads_root,
        }
    }

    /// Register `path` as a pending Cmdr-own write that should suppress its
    /// matching FS event for `ttl`. No-ops if `path` isn't under the
    /// configured Downloads root (call sites can hand us anything).
    ///
    /// On a re-insert for an already-pending path, the deadline is bumped
    /// to `now + ttl` but the entry keeps its FIFO position.
    pub fn note_pending(&self, path: PathBuf, ttl: Duration) {
        if !path.starts_with(&self.downloads_root) {
            return;
        }
        let deadline = Instant::now() + ttl;
        let mut s = self.state.lock().expect("IgnoreSet poisoned");
        let inserting_new = !s.map.contains_key(&path);
        s.map.insert(path.clone(), deadline);
        if inserting_new {
            s.order.push_back(path);
            // Enforce the FIFO cap.
            while s.order.len() > self.max_entries {
                if let Some(oldest) = s.order.pop_front() {
                    s.map.remove(&oldest);
                }
            }
        }
    }

    /// Is `path` currently pending (registered and not yet expired)?
    ///
    /// Also performs lazy expiry: any entries already past their deadline
    /// are dropped during this check.
    pub fn is_pending(&self, path: &Path) -> bool {
        let now = Instant::now();
        let mut s = self.state.lock().expect("IgnoreSet poisoned");
        expire(&mut s, now);
        s.map.contains_key(path)
    }

    /// Current entry count. Performs lazy expiry first. Test-only: no
    /// production caller reads this; only the unit tests below.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let now = Instant::now();
        let mut s = self.state.lock().expect("IgnoreSet poisoned");
        expire(&mut s, now);
        s.map.len()
    }
}

fn expire(s: &mut State, now: Instant) {
    // Drop expired entries from `map`, then prune `order` to match. Borrow
    // checker forces this two-step shape: a single `retain` closure can't
    // mutate one field while reading another through the same `&mut State`.
    let map = &mut s.map;
    map.retain(|_, deadline| *deadline > now);
    s.order.retain(|p| map.contains_key(p));
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::thread;

    use tempfile::TempDir;

    fn make(root: &Path) -> IgnoreSet {
        IgnoreSet::new(root.to_path_buf())
    }

    #[test]
    fn note_then_check_immediately() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let p = root.path().join("foo.zip");
        set.note_pending(p.clone(), Duration::from_secs(1));
        assert!(set.is_pending(&p));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn expires_after_ttl() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let p = root.path().join("foo.zip");
        set.note_pending(p.clone(), Duration::from_millis(10));
        thread::sleep(Duration::from_millis(40));
        assert!(!set.is_pending(&p));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn outside_root_is_dropped() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let outside = PathBuf::from("/tmp/somewhere-else/foo.zip");
        set.note_pending(outside.clone(), Duration::from_secs(1));
        assert!(!set.is_pending(&outside));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn five_inside_paths_all_pending() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let paths: Vec<_> = (0..5).map(|i| root.path().join(format!("f{i}.zip"))).collect();
        for p in &paths {
            set.note_pending(p.clone(), Duration::from_secs(1));
        }
        for p in &paths {
            assert!(set.is_pending(p), "expected {p:?} pending");
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn reinsert_bumps_deadline() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let p = root.path().join("foo.zip");
        set.note_pending(p.clone(), Duration::from_millis(1));
        set.note_pending(p.clone(), Duration::from_secs(1));
        thread::sleep(Duration::from_millis(100));
        assert!(set.is_pending(&p));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn fifo_eviction_at_cap() {
        let root = TempDir::new().unwrap();
        let set = IgnoreSet::with_capacity(root.path().to_path_buf(), 3);
        let a = root.path().join("a");
        let b = root.path().join("b");
        let c = root.path().join("c");
        let d = root.path().join("d");
        set.note_pending(a.clone(), Duration::from_secs(1));
        set.note_pending(b.clone(), Duration::from_secs(1));
        set.note_pending(c.clone(), Duration::from_secs(1));
        set.note_pending(d.clone(), Duration::from_secs(1));
        assert!(!set.is_pending(&a), "oldest should be evicted");
        assert!(set.is_pending(&b));
        assert!(set.is_pending(&c));
        assert!(set.is_pending(&d));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn concurrent_inserts_dont_panic() {
        let root = TempDir::new().unwrap();
        let set = Arc::new(make(root.path()));
        let root_buf = root.path().to_path_buf();
        let s1 = Arc::clone(&set);
        let r1 = root_buf.clone();
        let s2 = Arc::clone(&set);
        let r2 = root_buf.clone();
        let h1 = thread::spawn(move || {
            for i in 0..50 {
                s1.note_pending(r1.join(format!("a{i}")), Duration::from_secs(1));
            }
        });
        let h2 = thread::spawn(move || {
            for i in 0..50 {
                s2.note_pending(r2.join(format!("b{i}")), Duration::from_secs(1));
            }
        });
        h1.join().unwrap();
        h2.join().unwrap();
        assert_eq!(set.len(), 100);
    }

    #[test]
    fn lazy_expiry_shrinks_len() {
        let root = TempDir::new().unwrap();
        let set = make(root.path());
        let p = root.path().join("foo.zip");
        set.note_pending(p, Duration::from_millis(5));
        assert_eq!(set.len(), 1);
        thread::sleep(Duration::from_millis(30));
        // `len` itself performs lazy expiry.
        assert_eq!(set.len(), 0);
    }

}
