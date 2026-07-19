//! Per-subtree-anchor rescan throttle for the live reconciler's `MustScanSubDirs`
//! path.
//!
//! macOS coalesces a burst of changes under a directory into a single
//! `MustScanSubDirs` flag for that anchor, and the only honest response is to
//! re-walk the whole subtree. A hard-churning anchor (build output, package
//! caches, a busy log dir) raises that flag continuously, so an unthrottled
//! reconciler re-walks the same subtree back-to-back and burns CPU with almost
//! nothing new to show for it. This caps a given anchor to at most one re-walk
//! per [`RESCAN_THROTTLE_WINDOW`]: freshness stays bounded (a subtree is at most
//! one window stale) AND CPU stays bounded (one walk per window, not per event).
//!
//! ## Leading + trailing throttle, NOT debounce
//!
//! - **Leading edge:** an anchor with no record is eligible immediately, so a
//!   one-off `MustScanSubDirs` re-walks right away with no added latency.
//! - **Within the window:** further flags for the same anchor stay ineligible, so
//!   the caller skips the redundant walk.
//! - **Trailing edge (window end):** once the window elapses the anchor is
//!   eligible again, so a subtree under sustained change re-walks once per window
//!   forever. A debounce (wait for quiet) would starve a never-quiet anchor, so we
//!   don't debounce.
//!
//! Unlike the per-file [`super::throttle`], this carries no payload, no
//! significant-change bypass, and no Downloads exemption: a single growing file is
//! always handled by the per-file live path, never by a subtree re-walk, so there
//! is nothing here to bypass for. The whole surface is "may this anchor re-walk
//! now?" plus a completion timestamp.
//!
//! The engine is pure and clock-injected (`now: Instant` is always passed in) so
//! every rule below is deterministically unit-tested; it makes no filesystem,
//! logging, or clock calls.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Per-anchor rescan cadence cap (David's call; same 60s as the per-file throttle
/// window). A subtree anchor re-walks at most once per window under sustained
/// change, bounding both staleness and CPU.
pub(in crate::indexing) const RESCAN_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// Per-subtree-anchor rescan throttle. Records when each anchor last COMPLETED a
/// reconcile; an anchor is eligible to (re)start once the window has elapsed, or
/// immediately if never walked (the leading edge). Pure + clock-injected.
pub(in crate::indexing) struct RescanThrottle {
    window: Duration,
    /// When each anchor last completed a reconcile. Absent = never walked (eligible
    /// on the leading edge). Bounded by [`RescanThrottle::gc`].
    last_completed: HashMap<PathBuf, Instant>,
}

impl RescanThrottle {
    /// Production throttle with [`RESCAN_THROTTLE_WINDOW`].
    pub(in crate::indexing) fn new() -> Self {
        Self::with_window(RESCAN_THROTTLE_WINDOW)
    }

    /// Build a throttle with an explicit window. Tests use a short window to
    /// exercise the trailing edge without sleeping a real 60 s.
    pub(in crate::indexing) fn with_window(window: Duration) -> Self {
        Self {
            window,
            last_completed: HashMap::new(),
        }
    }

    /// Whether `path` may (re)start a reconcile now.
    ///
    /// Leading edge: an anchor with no completion record is eligible immediately,
    /// so a first `MustScanSubDirs` re-walks without added latency. Otherwise
    /// eligible iff a full window has elapsed since the last completion, which
    /// guarantees a sustained anchor still re-walks once per window (never
    /// starves). `now` is injected.
    pub(in crate::indexing) fn is_eligible(&self, path: &Path, now: Instant) -> bool {
        match self.last_completed.get(path) {
            None => true,
            Some(&completed_at) => now.duration_since(completed_at) >= self.window,
        }
    }

    /// Record that `path` finished a reconcile at `now`, so it throttles for the
    /// next `window`. Re-recording pushes the eligibility time forward, so repeated
    /// completions keep the anchor throttled at one walk per window.
    pub(in crate::indexing) fn record_completion(&mut self, path: &Path, now: Instant) {
        self.last_completed.insert(path.to_path_buf(), now);
    }

    /// Bound the map: drop records for anchors NOT in `live_anchors` whose window
    /// has fully elapsed. Such a record is inert (its anchor is neither pending a
    /// walk nor throttling one), so dropping it only means the anchor would be
    /// eligible on its leading edge again, which it already is. Keep any anchor
    /// still in `live_anchors` (it may re-walk again) and any record still within
    /// its window (dropping it would let a churning anchor re-walk early, defeating
    /// the throttle). Mirrors the per-file throttle's cold-eviction discipline.
    pub(in crate::indexing) fn gc(&mut self, live_anchors: &HashSet<PathBuf>, now: Instant) {
        let window = self.window;
        self.last_completed
            .retain(|path, &mut completed_at| live_anchors.contains(path) || now.duration_since(completed_at) < window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(100);

    fn throttle() -> RescanThrottle {
        RescanThrottle::with_window(WINDOW)
    }

    #[test]
    fn absent_anchor_is_eligible_immediately() {
        let t = throttle();
        let t0 = Instant::now();
        assert!(
            t.is_eligible(Path::new("/a"), t0),
            "never-walked anchor re-walks on the leading edge"
        );
    }

    #[test]
    fn just_after_completion_is_not_eligible_within_window() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/a"), t0);
        assert!(!t.is_eligible(Path::new("/a"), t0), "same instant is within the window");
        assert!(
            !t.is_eligible(Path::new("/a"), t0 + WINDOW - Duration::from_millis(1)),
            "just before the window elapses, still throttled"
        );
    }

    #[test]
    fn eligible_again_after_window_elapses() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/a"), t0);
        assert!(
            t.is_eligible(Path::new("/a"), t0 + WINDOW),
            "exactly one window later is eligible (never starves)"
        );
        assert!(
            t.is_eligible(Path::new("/a"), t0 + WINDOW * 3),
            "well past the window is eligible"
        );
    }

    #[test]
    fn completion_of_one_anchor_does_not_affect_another() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/a"), t0);
        assert!(!t.is_eligible(Path::new("/a"), t0), "/a is throttled");
        assert!(t.is_eligible(Path::new("/b"), t0), "/b is untouched, still eligible");
    }

    #[test]
    fn re_recording_pushes_eligibility_forward() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/a"), t0);
        // A second completion just before the first window would have elapsed
        // re-arms the throttle from the newer instant.
        let t1 = t0 + WINDOW - Duration::from_millis(1);
        t.record_completion(Path::new("/a"), t1);
        assert!(
            !t.is_eligible(Path::new("/a"), t0 + WINDOW),
            "old window elapsing doesn't free the anchor; the newer completion still throttles"
        );
        assert!(
            t.is_eligible(Path::new("/a"), t1 + WINDOW),
            "eligible one window after the newer completion"
        );
    }

    #[test]
    fn gc_drops_dead_elapsed_record_but_keeps_live_and_within_window() {
        let mut t = throttle();
        let t0 = Instant::now();
        // /dead: completed, window elapsed, not live → droppable.
        t.record_completion(Path::new("/dead"), t0);
        // /live: completed, window elapsed, but still a live anchor → kept.
        t.record_completion(Path::new("/live"), t0);
        // /fresh: completed recently, within window, not live → kept.
        let t_fresh = t0 + WINDOW * 2;
        t.record_completion(Path::new("/fresh"), t_fresh);

        let mut live = HashSet::new();
        live.insert(PathBuf::from("/live"));

        // Run gc a full window after /fresh's completion, so /dead and /live are
        // both window-elapsed while /fresh is exactly at its window edge (kept).
        let now = t_fresh + WINDOW - Duration::from_millis(1);
        t.gc(&live, now);

        assert!(
            !t.last_completed.contains_key(Path::new("/dead")),
            "elapsed, non-live record is dropped"
        );
        assert!(
            t.last_completed.contains_key(Path::new("/live")),
            "live anchor's record is kept"
        );
        assert!(
            t.last_completed.contains_key(Path::new("/fresh")),
            "within-window record is kept even if not live"
        );
    }
}
