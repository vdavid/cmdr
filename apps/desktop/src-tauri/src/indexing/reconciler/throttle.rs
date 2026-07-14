//! Per-file throttle for LIVE index upserts (`reconciler.rs` live path only).
//!
//! On any running Mac, background apps rewrite the same files constantly (WALs,
//! logs, app-state SQLite DBs, plists), and Cmdr live-watches the whole boot
//! volume, so the reconciler+writer never go idle. This collapses the dominant
//! "same file rewritten rapidly" pattern into at most one index write per
//! [`THROTTLE_WINDOW`] per file, cutting redundant WAL churn (and taming Cmdr's
//! own DB/log self-write loop, which sits inside the watched tree — subsumed for
//! free, no special-casing).
//!
//! ## Leading + trailing throttle, NOT debounce
//!
//! - **Leading edge:** the first change to a key applies immediately (instant for
//!   normal edits).
//! - **Within the window:** further changes are suppressed, but the last-seen
//!   payload is remembered as `pending`.
//! - **Trailing edge (window end):** [`Throttle::sweep`] applies the last-seen
//!   payload — never re-stats (the sweep is a timer that must not block the live
//!   loop on a dead mount, and re-statting adds a phantom-apply-on-deleted-file
//!   case). Under sustained change this fires once per window forever (the
//!   throttle guarantee); a debounce would starve, so we don't debounce.
//!
//! ## Significant-change bypass
//!
//! A big jump applies immediately even mid-window (see [`is_significant`]): a file
//! genuinely growing surfaces promptly, while a tiny lock file flapping stays
//! throttled (the 512 KiB floor) and a 0-byte file flapping never bypasses (the
//! `new != 0` clause).
//!
//! ## Data-safety invariant
//!
//! A key with `pending == Some(_)` is NEVER evictable: dropping it would lose the
//! file's final size from the index forever (suppressed at t=30 s, then silent).
//! [`Throttle::sweep`] only cold-evicts `pending == None` keys, and a hard-cap
//! overflow flushes every pending payload before clearing (no silent data loss).
//!
//! The engine is pure and clock-injected (`now: Instant` passed in) so all of the
//! above is deterministically unit-tested below; it makes no filesystem or DB
//! calls. The Downloads exemption is a caller-supplied normalized prefix (resolved
//! once in `reconciler.rs`), keeping this module free of OS lookups.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The per-file throttle window: at most one index write per file per window
/// under sustained change (David's call, same value for every throttled key).
pub(super) const THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// Absolute floor for a "significant" size jump (512 KiB). Below `last_applied`
/// ~25 MiB this dominates the 2 % test, so tiny files can't bypass the throttle.
const SIGNIFICANT_FLOOR: u64 = 512 * 1024;

/// A key idle (no change, no pending) for this many windows is cold-evictable, so
/// the map stays bounded by the count of *actively*-churning keys.
const COLD_EVICTION_WINDOWS: u32 = 5;

/// Hard backstop on the map size. Reaching it is pathological (this many distinct
/// files churning at once); we flush every pending payload and reset rather than
/// grow unbounded or silently drop. Logged, never silent.
const HARD_CAP: usize = 100_000;

/// Whether a size change from `last_applied` to `new` is big enough to bypass the
/// throttle and apply immediately.
///
/// `(last_applied == 0 && new != 0)` — the empty→nonzero transition only. A 0-byte
/// lock/marker file touched repeatedly has `last_applied == 0 && new == 0`; flagging
/// that significant would re-open the exact mini-loop the floor targets, so it must
/// stay throttled.
///
/// Otherwise `|new - last_applied| >= max(2 % of last_applied, 512 KiB)`.
fn is_significant(new: u64, last_applied: u64) -> bool {
    if last_applied == 0 {
        return new != 0;
    }
    let diff = new.abs_diff(last_applied);
    let two_percent = last_applied / 50;
    diff >= two_percent.max(SIGNIFICANT_FLOOR)
}

/// One tracked key's throttle state.
struct Entry<P> {
    /// When this key's size was last written to the index.
    last_applied_at: Instant,
    /// The size last written (the baseline for [`is_significant`]).
    last_applied_size: u64,
    /// `Some` if a change was suppressed since the last apply and still needs a
    /// trailing flush; `None` if nothing is outstanding. Carries both the dirty
    /// bit and the payload to flush.
    pending: Option<Pending<P>>,
}

/// A suppressed change awaiting its trailing flush.
struct Pending<P> {
    size: u64,
    payload: P,
}

/// What the caller should do with a change (see [`Throttle::on_change`]).
pub(super) enum ThrottleOutcome<P> {
    /// Apply now — the caller writes the upsert. Carries the payload back so the
    /// caller can move it into the write with no extra bookkeeping.
    Apply(P),
    /// Suppress — the caller writes nothing; the payload is held for the trailing
    /// flush, which [`Throttle::sweep`] surfaces when the window elapses.
    Suppress,
}

/// A per-key leading+trailing throttle over live index upserts. Generic over the
/// payload `P` it carries so the engine stays pure (unit-tested with `P = ()`);
/// the live path instantiates `P = PendingUpsert` (`reconciler.rs`).
///
/// Visible to the whole `indexing` module because it appears in the
/// `pub(in crate::indexing)` `process_fs_event` signature (its `event_loop.rs`
/// callers pass `None`); the methods stay `pub(super)` (reconciler-only).
pub(in crate::indexing) struct Throttle<P> {
    window: Duration,
    /// Normalized paths under this prefix are never throttled (always applied).
    /// Resolved once by the caller; `None` if the OS reports no Downloads dir.
    exempt_prefix: Option<String>,
    entries: HashMap<String, Entry<P>>,
    /// Whether the hard-cap warning has already been logged, so a sustained
    /// overflow logs once rather than every sweep.
    hard_cap_warned: bool,
}

impl<P> Throttle<P> {
    /// Build a throttle with the production [`THROTTLE_WINDOW`] and a Downloads
    /// exemption prefix (already normalized by the caller, `None` if absent).
    pub(super) fn new(exempt_prefix: Option<String>) -> Self {
        Self::with_window(THROTTLE_WINDOW, exempt_prefix)
    }

    /// Build a throttle with an explicit window. Tests use a short window to
    /// exercise the trailing flush without sleeping a real 60 s.
    pub(super) fn with_window(window: Duration, exempt_prefix: Option<String>) -> Self {
        Self {
            window,
            exempt_prefix,
            entries: HashMap::new(),
            hard_cap_warned: false,
        }
    }

    /// Whether `path` (a normalized live path) is exempt from throttling because
    /// it lives under the user's Downloads (active downloads want a live size).
    pub(super) fn is_exempt(&self, path: &str) -> bool {
        match &self.exempt_prefix {
            Some(prefix) => {
                path == prefix
                    || path
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            None => false,
        }
    }

    /// Record a change to `key` and decide whether to apply it now.
    ///
    /// Leading edge (unknown key) and window-elapsed / significant changes apply
    /// (resetting the window); everything else is suppressed, storing `payload`
    /// for the trailing flush. `payload` is only retained on `Suppress`; on
    /// `Apply` it's handed straight back.
    pub(super) fn on_change(&mut self, key: &str, new_size: u64, payload: P, now: Instant) -> ThrottleOutcome<P> {
        match self.entries.get_mut(key) {
            None => {
                self.entries.insert(
                    key.to_string(),
                    Entry {
                        last_applied_at: now,
                        last_applied_size: new_size,
                        pending: None,
                    },
                );
                ThrottleOutcome::Apply(payload)
            }
            Some(entry) => {
                let window_elapsed = now.duration_since(entry.last_applied_at) >= self.window;
                if window_elapsed || is_significant(new_size, entry.last_applied_size) {
                    entry.last_applied_at = now;
                    entry.last_applied_size = new_size;
                    entry.pending = None;
                    ThrottleOutcome::Apply(payload)
                } else {
                    entry.pending = Some(Pending {
                        size: new_size,
                        payload,
                    });
                    ThrottleOutcome::Suppress
                }
            }
        }
    }

    /// Flush every key whose window has elapsed and still has a pending payload,
    /// then evict cold keys. Returns `(key, payload)` for each trailing flush so
    /// the caller can write the last-seen size.
    ///
    /// Ordering guarantees the data-safety invariant: pending keys are flushed
    /// (and only then have `pending == None`) *before* cold eviction, and cold
    /// eviction retains any key that is still pending. A hard-cap overflow flushes
    /// all remaining pending payloads before clearing, so no suppressed size is
    /// ever lost.
    pub(super) fn sweep(&mut self, now: Instant) -> Vec<(String, P)> {
        let mut flushes: Vec<(String, P)> = Vec::new();
        let window = self.window;

        for (key, entry) in self.entries.iter_mut() {
            let due = entry.pending.is_some() && now.duration_since(entry.last_applied_at) >= window;
            if due {
                let pending = entry.pending.take().expect("pending checked Some above");
                entry.last_applied_at = now;
                entry.last_applied_size = pending.size;
                flushes.push((key.clone(), pending.payload));
            }
        }

        // Cold-evict quiet, non-pending keys. Never a pending key (data-safety).
        let cold_cutoff = window.saturating_mul(COLD_EVICTION_WINDOWS);
        self.entries
            .retain(|_, entry| entry.pending.is_some() || now.duration_since(entry.last_applied_at) < cold_cutoff);

        // Hard-cap backstop: flush every pending payload, then clear. Losing the
        // `last_applied` baselines only costs a one-time extra apply per key next
        // time they change (harmless); losing a pending payload would corrupt the
        // index, so we never do that silently.
        if self.entries.len() > HARD_CAP {
            if !self.hard_cap_warned {
                log::warn!(
                    target: "indexing::throttle",
                    // allowed-pluralize-noun: HARD_CAP is the const 100_000, never 1.
                    "Live-write throttle map exceeded {HARD_CAP} keys; flushing all pending and resetting baselines"
                );
                self.hard_cap_warned = true;
            }
            for (key, entry) in self.entries.drain() {
                if let Some(pending) = entry.pending {
                    flushes.push((key, pending.payload));
                }
            }
        } else {
            self.hard_cap_warned = false;
        }

        flushes
    }

    #[cfg(test)]
    fn pending_size(&self, key: &str) -> Option<u64> {
        self.entries.get(key).and_then(|e| e.pending.as_ref().map(|p| p.size))
    }

    #[cfg(test)]
    fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test throttle carrying a unit payload, with a 60 s window and no
    /// Downloads exemption. Clock is injected per call, so no real time passes.
    fn throttle() -> Throttle<()> {
        Throttle::with_window(THROTTLE_WINDOW, None)
    }

    fn is_apply(outcome: ThrottleOutcome<()>) -> bool {
        matches!(outcome, ThrottleOutcome::Apply(()))
    }

    #[test]
    fn leading_edge_applies_immediately() {
        let mut t = throttle();
        let t0 = Instant::now();
        assert!(is_apply(t.on_change("/a", 100, (), t0)), "first change applies");
        assert_eq!(t.pending_size("/a"), None, "nothing pending after a leading apply");
    }

    #[test]
    fn within_window_is_suppressed_and_sets_pending() {
        let mut t = throttle();
        let t0 = Instant::now();
        assert!(is_apply(t.on_change("/a", 100, (), t0)));
        // A tiny change 1 s later: within the window, sub-floor, so suppressed.
        let outcome = t.on_change("/a", 100 + 1024, (), t0 + Duration::from_secs(1));
        assert!(matches!(outcome, ThrottleOutcome::Suppress));
        assert_eq!(t.pending_size("/a"), Some(100 + 1024), "suppressed size remembered");
    }

    #[test]
    fn trailing_flush_applies_last_seen_size_and_clears_pending() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.on_change("/a", 100, (), t0);
        // Three sub-floor changes within the window; only the last survives.
        t.on_change("/a", 200, (), t0 + Duration::from_secs(1));
        t.on_change("/a", 300, (), t0 + Duration::from_secs(2));
        t.on_change("/a", 400, (), t0 + Duration::from_secs(3));
        assert_eq!(t.pending_size("/a"), Some(400));

        // Before the window elapses: nothing flushes.
        assert!(t.sweep(t0 + Duration::from_secs(30)).is_empty());
        assert_eq!(t.pending_size("/a"), Some(400));

        // After the window: the LAST-seen size flushes and pending clears.
        let flushes = t.sweep(t0 + THROTTLE_WINDOW);
        assert_eq!(flushes.len(), 1);
        assert_eq!(flushes[0].0, "/a");
        assert_eq!(t.pending_size("/a"), None, "pending cleared after flush");
    }

    #[test]
    fn sustained_change_fires_once_per_window_not_debounce() {
        let mut t = throttle();
        let t0 = Instant::now();
        let mut applies = 0;
        // One sub-floor change every second for 3 windows, plus a sweep each
        // second. A debounce (wait-for-quiet) would never fire under this load.
        for sec in 0..180u64 {
            let now = t0 + Duration::from_secs(sec);
            // Small +1 KiB steps: never individually significant.
            if is_apply(t.on_change("/a", 100 + sec * 1024, (), now)) {
                applies += 1;
            }
            applies += t.sweep(now).len();
        }
        // Leading apply at t=0, then a trailing flush at ~60 s and ~120 s: three.
        assert_eq!(
            applies, 3,
            "throttle fires ~once per window, not never (debounce) nor every event"
        );
    }

    #[test]
    fn significant_jump_bypasses_mid_window() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.on_change("/a", 1_000_000, (), t0);
        // +2 MiB one second later: over both the 2 % and the 512 KiB floor.
        let outcome = t.on_change("/a", 1_000_000 + 2 * 1024 * 1024, (), t0 + Duration::from_secs(1));
        assert!(
            is_apply(outcome),
            "a big jump applies immediately, resetting the window"
        );
        assert_eq!(t.pending_size("/a"), None);
    }

    #[test]
    fn sub_floor_flap_stays_throttled() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.on_change("/lock", 4096, (), t0);
        // A 200-byte file flapping by a few bytes: under the 512 KiB floor.
        for sec in 1..30u64 {
            let outcome = t.on_change("/lock", 4096 + (sec % 3) * 100, (), t0 + Duration::from_secs(sec));
            assert!(
                matches!(outcome, ThrottleOutcome::Suppress),
                "sub-floor change stays throttled"
            );
        }
    }

    #[test]
    fn zero_to_zero_is_not_significant_but_zero_to_nonzero_is() {
        // A 0-byte lock file flapping (last == 0, new == 0) must NOT bypass.
        assert!(!is_significant(0, 0), "(0, 0) is not significant");
        // The empty→nonzero transition IS significant.
        assert!(is_significant(1, 0), "(nonzero, 0) is significant");
        assert!(is_significant(10 * 1024 * 1024, 0), "empty→large is significant");
    }

    #[test]
    fn zero_byte_lock_file_flap_is_never_significant_and_stays_throttled() {
        let mut t = throttle();
        let t0 = Instant::now();
        // Leading apply establishes last_applied_size = 0.
        assert!(is_apply(t.on_change("/lock", 0, (), t0)));
        // Repeated 0→0 touches: never significant, always suppressed.
        for sec in 1..10u64 {
            let outcome = t.on_change("/lock", 0, (), t0 + Duration::from_secs(sec));
            assert!(matches!(outcome, ThrottleOutcome::Suppress), "0→0 flap stays throttled");
        }
    }

    #[test]
    fn pending_key_is_never_cold_evicted() {
        let mut t = throttle();
        let t0 = Instant::now();
        // Suppress a change so the key is pending, then let it go silent well past
        // the cold-eviction horizon WITHOUT its window elapsing relative to the
        // sweep... to isolate eviction, re-arm pending each check.
        t.on_change("/a", 100, (), t0);
        t.on_change("/a", 100 + 1024, (), t0 + Duration::from_secs(1));
        assert_eq!(t.pending_size("/a"), Some(100 + 1024), "pending set");

        // Sweep far in the future: the trailing flush fires (window elapsed), but
        // the key must not vanish before its size B is written.
        let flushes = t.sweep(t0 + THROTTLE_WINDOW * (COLD_EVICTION_WINDOWS + 2));
        assert_eq!(flushes.len(), 1, "pending was flushed, not silently evicted");
        assert_eq!(flushes[0].0, "/a");
    }

    #[test]
    fn cold_non_pending_key_is_evicted() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.on_change("/a", 100, (), t0); // leading apply, pending == None
        assert!(t.contains("/a"));
        // No further activity: past the cold horizon it's dropped to bound memory.
        t.sweep(t0 + THROTTLE_WINDOW * (COLD_EVICTION_WINDOWS + 1));
        assert!(!t.contains("/a"), "quiet non-pending key is cold-evicted");
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn downloads_prefix_is_exempt() {
        let t: Throttle<()> = Throttle::with_window(THROTTLE_WINDOW, Some("/Users/me/Downloads".to_string()));
        assert!(t.is_exempt("/Users/me/Downloads"));
        assert!(t.is_exempt("/Users/me/Downloads/big.iso"));
        assert!(
            !t.is_exempt("/Users/me/Downloads-sibling/x"),
            "sibling prefix is not under Downloads"
        );
        assert!(!t.is_exempt("/Users/me/Documents/x"));
        let none: Throttle<()> = Throttle::with_window(THROTTLE_WINDOW, None);
        assert!(
            !none.is_exempt("/Users/me/Downloads/big.iso"),
            "no Downloads dir → nothing exempt"
        );
    }
}
