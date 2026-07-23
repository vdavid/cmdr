//! Per-subtree-anchor rescan throttle for the live reconciler's `MustScanSubDirs`
//! path.
//!
//! macOS coalesces a burst of changes under a directory into a single
//! `MustScanSubDirs` flag for that anchor, and the only honest response is to
//! re-walk the whole subtree. A hard-churning anchor (build output, package
//! caches, a busy log dir) raises that flag continuously, so an unthrottled
//! reconciler re-walks the same subtree back-to-back and burns CPU with almost
//! nothing new to show for it. This caps a given anchor to at most one re-walk
//! per window: freshness stays bounded (a subtree is at most one window stale)
//! AND CPU stays bounded (one walk per window, not per event).
//!
//! ## The window is cost-proportional, per anchor
//!
//! Each anchor earns its own window from what its last walk COST:
//! `clamp([`WALK_COST_MULTIPLIER`] × walk_cost, [`RESCAN_THROTTLE_WINDOW`],
//! [`RESCAN_THROTTLE_MAX_WINDOW`])`, so an anchor spends at most ~1/30th of the
//! time re-walking itself. A sub-second anchor (nearly all of them) sits on the
//! floor and is as fresh as ever; a multi-second one backs off until it stops
//! eating the reconcile budget. The ceiling caps staleness: past half an hour, an
//! out-of-date subtree costs the user more than the CPU saves.
//!
//! **Cost is the WALK, not the reconcile's wall clock** (`ReconcileSummary::
//! walk_cost`, the duration minus the writer wait). Time parked on a saturated
//! writer queue belongs to the writer; charging it would let one transient global
//! saturation inflate every anchor's measured cost at once and back a whole volume
//! off for half an hour.
//!
//! ## Leading + trailing throttle, NOT debounce
//!
//! - **Leading edge:** an anchor with no record is eligible immediately, so a
//!   one-off `MustScanSubDirs` re-walks right away with no added latency.
//! - **Within the window:** further flags for the same anchor stay ineligible, so
//!   the caller skips the redundant walk.
//! - **Trailing edge (window end):** once the anchor's own window elapses it is
//!   eligible again, so a subtree under sustained change re-walks once per window
//!   forever. A debounce (wait for quiet) would starve a never-quiet anchor, so we
//!   don't debounce.
//!
//! Unlike the per-file [`super::throttle`], this carries no payload, no
//! significant-change bypass, and no Downloads exemption: a single growing file is
//! always handled by the per-file live path, never by a subtree re-walk, so there
//! is nothing here to bypass for. The whole surface is "may this anchor re-walk
//! now?" plus a completion (timestamp + cost).
//!
//! The engine is pure and clock-injected (`now: Instant` is always passed in) so
//! every rule below is deterministically unit-tested; it makes no filesystem,
//! logging, or clock calls.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The shortest window any anchor gets, whatever its walk cost (David's call;
/// same 60 s as the per-file throttle window). Nearly every anchor walks in well
/// under a second and lands here, exactly as fresh as a flat 60 s cap kept it.
pub(in crate::indexing) const RESCAN_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// The longest window any anchor gets. Past half an hour, a stale subtree costs
/// the user more than the CPU the back-off saves, so the scaling stops here.
pub(in crate::indexing) const RESCAN_THROTTLE_MAX_WINDOW: Duration = Duration::from_secs(30 * 60);

/// How many times its own walk cost an anchor rests before re-walking: an anchor
/// spends at most ~1/30th of the time re-walking itself, so no single subtree can
/// dominate the reconcile budget however expensive it is to list.
pub(in crate::indexing) const WALK_COST_MULTIPLIER: u32 = 30;

/// One anchor's last reconcile: when it finished, and the window it earned from
/// what that walk cost.
struct Completion {
    at: Instant,
    window: Duration,
}

/// Per-subtree-anchor rescan throttle. Records when each anchor last COMPLETED a
/// reconcile and how long that walk earns it; an anchor is eligible to (re)start
/// once ITS window has elapsed, or immediately if never walked (the leading edge).
/// Pure + clock-injected.
pub(in crate::indexing) struct RescanThrottle {
    floor: Duration,
    ceiling: Duration,
    /// Each anchor's last completion. Absent = never walked (eligible on the
    /// leading edge). Bounded by [`RescanThrottle::gc`].
    last_completed: HashMap<PathBuf, Completion>,
}

impl RescanThrottle {
    /// Production throttle, scaling between [`RESCAN_THROTTLE_WINDOW`] and
    /// [`RESCAN_THROTTLE_MAX_WINDOW`].
    pub(in crate::indexing) fn new() -> Self {
        Self::with_bounds(RESCAN_THROTTLE_WINDOW, RESCAN_THROTTLE_MAX_WINDOW)
    }

    /// Build a throttle with explicit window bounds. Tests use short (or zero)
    /// bounds to exercise the trailing edge without sleeping a real minute. A
    /// ceiling below the floor would make the clamp meaningless, so it's lifted to
    /// the floor (equal bounds = a flat window).
    pub(in crate::indexing) fn with_bounds(floor: Duration, ceiling: Duration) -> Self {
        Self {
            floor,
            ceiling: ceiling.max(floor),
            last_completed: HashMap::new(),
        }
    }

    /// The window a walk costing `walk_cost` earns: [`WALK_COST_MULTIPLIER`] times
    /// the cost, clamped to the bounds. Saturates at the ceiling rather than
    /// overflowing on an absurd cost.
    fn window_for(&self, walk_cost: Duration) -> Duration {
        walk_cost
            .checked_mul(WALK_COST_MULTIPLIER)
            .unwrap_or(self.ceiling)
            .clamp(self.floor, self.ceiling)
    }

    /// Whether `path` may (re)start a reconcile now.
    ///
    /// Leading edge: an anchor with no completion record is eligible immediately,
    /// so a first `MustScanSubDirs` re-walks without added latency. Otherwise
    /// eligible iff the anchor's own window has elapsed since its last completion,
    /// which guarantees even an expensive anchor still re-walks once per window
    /// (never starves). `now` is injected.
    pub(in crate::indexing) fn is_eligible(&self, path: &Path, now: Instant) -> bool {
        match self.last_completed.get(path) {
            None => true,
            Some(completion) => now.duration_since(completion.at) >= completion.window,
        }
    }

    /// Record that `path` finished a reconcile at `now` after a walk costing
    /// `walk_cost`, so it throttles for the window that cost earns. Re-recording
    /// pushes the eligibility time forward AND re-measures the window, so an anchor
    /// that gets cheaper (a cache someone emptied) speeds back up on its next walk.
    pub(in crate::indexing) fn record_completion(&mut self, path: &Path, now: Instant, walk_cost: Duration) {
        let window = self.window_for(walk_cost);
        self.last_completed
            .insert(path.to_path_buf(), Completion { at: now, window });
    }

    /// Bound the map: drop records for anchors NOT in `live_anchors` whose OWN
    /// window has fully elapsed. Such a record is inert (its anchor is neither
    /// pending a walk nor throttling one), so dropping it only means the anchor
    /// would be eligible on its leading edge again, which it already is. Keep any
    /// anchor still in `live_anchors` (it may re-walk again) and any record still
    /// within its window: measuring against one global window instead would evict
    /// an expensive anchor as soon as the floor elapsed, and it would then re-walk
    /// on its leading edge, defeating the back-off entirely. Mirrors the per-file
    /// throttle's cold-eviction discipline.
    pub(in crate::indexing) fn gc(&mut self, live_anchors: &HashSet<PathBuf>, now: Instant) {
        self.last_completed.retain(|path, completion| {
            live_anchors.contains(path) || now.duration_since(completion.at) < completion.window
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(100);

    fn throttle() -> RescanThrottle {
        RescanThrottle::with_bounds(WINDOW, WINDOW)
    }

    /// A throttle with the production bounds, for the cost-scaling rules.
    fn production_throttle() -> RescanThrottle {
        RescanThrottle::new()
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
        t.record_completion(Path::new("/a"), t0, Duration::ZERO);
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
        t.record_completion(Path::new("/a"), t0, Duration::ZERO);
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
        t.record_completion(Path::new("/a"), t0, Duration::ZERO);
        assert!(!t.is_eligible(Path::new("/a"), t0), "/a is throttled");
        assert!(t.is_eligible(Path::new("/b"), t0), "/b is untouched, still eligible");
    }

    #[test]
    fn re_recording_pushes_eligibility_forward() {
        let mut t = throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/a"), t0, Duration::ZERO);
        // A second completion just before the first window would have elapsed
        // re-arms the throttle from the newer instant.
        let t1 = t0 + WINDOW - Duration::from_millis(1);
        t.record_completion(Path::new("/a"), t1, Duration::ZERO);
        assert!(
            !t.is_eligible(Path::new("/a"), t0 + WINDOW),
            "old window elapsing doesn't free the anchor; the newer completion still throttles"
        );
        assert!(
            t.is_eligible(Path::new("/a"), t1 + WINDOW),
            "eligible one window after the newer completion"
        );
    }

    /// A cheap walk stays exactly as fresh as it is today: `30 × 100 ms` is far
    /// under the floor, so the anchor re-walks once a minute, unchanged.
    #[test]
    fn a_cheap_walk_keeps_the_floor_window() {
        let mut t = production_throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/cheap"), t0, Duration::from_millis(100));
        assert!(
            !t.is_eligible(Path::new("/cheap"), t0 + Duration::from_secs(59)),
            "still inside the 60 s floor"
        );
        assert!(
            t.is_eligible(Path::new("/cheap"), t0 + RESCAN_THROTTLE_WINDOW),
            "a cheap anchor is as fresh as ever: the floor, not a longer window"
        );
    }

    /// An expensive walk backs off proportionally: a 10 s walk earns a 5 min
    /// window, so it stops burning a permanent ~17% duty cycle on one anchor.
    #[test]
    fn an_expensive_walk_backs_off_proportionally() {
        let mut t = production_throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/webkit-cache"), t0, Duration::from_secs(10));
        assert!(
            !t.is_eligible(Path::new("/webkit-cache"), t0 + RESCAN_THROTTLE_WINDOW),
            "the old flat 60 s window no longer frees an expensive anchor"
        );
        assert!(
            !t.is_eligible(Path::new("/webkit-cache"), t0 + Duration::from_secs(299)),
            "still inside its 5 min window"
        );
        assert!(
            t.is_eligible(Path::new("/webkit-cache"), t0 + Duration::from_secs(300)),
            "eligible at 30 × 10 s, and never starves"
        );
    }

    /// A pathologically expensive walk clamps at the ceiling instead of scaling
    /// to 2.5 h: a subtree that stale is worse than the CPU it saves.
    #[test]
    fn a_very_expensive_walk_clamps_at_the_ceiling() {
        let mut t = production_throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/huge"), t0, Duration::from_secs(300));
        assert!(
            !t.is_eligible(
                Path::new("/huge"),
                t0 + RESCAN_THROTTLE_MAX_WINDOW - Duration::from_secs(1)
            ),
            "throttled right up to the ceiling"
        );
        assert!(
            t.is_eligible(Path::new("/huge"), t0 + RESCAN_THROTTLE_MAX_WINDOW),
            "clamped at 30 min, not 30 × 300 s"
        );
    }

    /// Cost is the WALK, so an anchor whose walk was cheap keeps the floor even
    /// when the reconcile ran long parked on a saturated writer queue.
    #[test]
    fn a_walk_that_was_mostly_writer_wait_keeps_the_floor() {
        let mut t = production_throttle();
        let t0 = Instant::now();
        // A 20 s reconcile with 19 s of writer wait: a 1 s walk, so the floor.
        t.record_completion(Path::new("/waited"), t0, Duration::from_secs(1));
        assert!(
            t.is_eligible(Path::new("/waited"), t0 + RESCAN_THROTTLE_WINDOW),
            "a 1 s walk earns the floor; charging it the full 20 s would throttle it for 10 min"
        );
    }

    /// `gc` measures each record against ITS OWN window. Using one global window
    /// would evict a long-throttled anchor the moment the floor elapsed, and the
    /// anchor would then re-walk on its leading edge, defeating the back-off.
    #[test]
    fn gc_respects_the_per_anchor_window() {
        let mut t = production_throttle();
        let t0 = Instant::now();
        t.record_completion(Path::new("/expensive"), t0, Duration::from_secs(10));
        t.record_completion(Path::new("/cheap"), t0, Duration::from_millis(100));

        // Two minutes on: past the 60 s floor, well inside /expensive's 5 min window.
        t.gc(&HashSet::new(), t0 + Duration::from_secs(120));

        assert!(
            t.last_completed.contains_key(Path::new("/expensive")),
            "a still-throttling record is kept, whatever its window"
        );
        assert!(
            !t.last_completed.contains_key(Path::new("/cheap")),
            "an elapsed, non-live record is still dropped"
        );
    }

    #[test]
    fn gc_drops_dead_elapsed_record_but_keeps_live_and_within_window() {
        let mut t = throttle();
        let t0 = Instant::now();
        // /dead: completed, window elapsed, not live → droppable.
        t.record_completion(Path::new("/dead"), t0, Duration::ZERO);
        // /live: completed, window elapsed, but still a live anchor → kept.
        t.record_completion(Path::new("/live"), t0, Duration::ZERO);
        // /fresh: completed recently, within window, not live → kept.
        let t_fresh = t0 + WINDOW * 2;
        t.record_completion(Path::new("/fresh"), t_fresh, Duration::ZERO);

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
