//! How many DISTINCT rescan anchors are arriving, which is the one thing the
//! per-subtree throttle cannot bound.
//!
//! [`super::throttle`] caps how often a GIVEN anchor re-walks. It contributes
//! nothing to bounding how many distinct anchors show up, because it is
//! eligible-on-first-sight by design (a leading edge, deliberately not a
//! debounce), so a machine producing one-shot anchors — a compiler's fingerprint
//! dirs, an updater's staging dirs — pays one subtree walk per anchor at whatever
//! rate it produces them. The cost scales with the user's workload rather than
//! with anything Cmdr controls.
//!
//! This counts distinct anchor ARRIVALS per volume over a rolling window and
//! answers one question: is this volume producing more anchors than walking them
//! one at a time can be worth? A yes routes the anchor to the visible once-a-day
//! sweep ([`super::route`]), the same place a root-scale anchor goes.
//!
//! Pure and clock-injected like its neighbours, so every rule here is
//! deterministically unit-tested with no clock, no logger, and no filesystem.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

/// How long a window of arrivals is measured over. Matches [`super::churn`]'s
/// window so the two read the same stretch of time: when a churn line names a
/// window, this is the state the router was in during it.
pub(in crate::indexing) const CARDINALITY_WINDOW: Duration = super::churn::CHURN_WINDOW;

/// Distinct deep anchors in one window that count as more than the drain should
/// try to walk one at a time.
///
/// ⚠️ **This number is a GUESS, and it is waiting on a measurement.** No
/// distribution of per-window anchor cardinality has been collected;
/// [`super::churn`]'s INFO line is what will collect it (an ordinary week on a
/// quiet machine, then a `docs/notes/` note), and the threshold should be re-set
/// from that data. It is one constant on purpose, so re-setting it is a one-line
/// change.
///
/// What it is positioned against, which is the only anchor-cardinality data this
/// repo actually holds (David's machine, 2026-07-19..23, a sampled log from a
/// machine running six cargo builds — a heavy case, not a typical one):
/// - 5,876 distinct anchors across a sampled day, which is on the order of 60 per
///   15-minute window if they were spread evenly.
/// - 1,595 one-shot anchors in the single worst window.
///
/// 256 sits several times above that heavy machine's average window and an order
/// of magnitude below its worst, so an ordinary machine should never reach it
/// while a genuine storm crosses it in its first minutes. Corroborating, not
/// deriving: [`super::churn`] stops tracking per-anchor tallies at 64, so a window
/// past this has long since lost the ability to name a culprit folder at all.
pub(in crate::indexing) const HIGH_CARDINALITY_ANCHORS: usize = 256;

/// One volume's distinct anchor arrivals over a rolling window.
pub(in crate::indexing) struct AnchorArrivals {
    window: Duration,
    threshold: usize,
    started: Instant,
    distinct: HashSet<PathBuf>,
    crossed_this_window: bool,
    crossed_last_window: bool,
}

impl AnchorArrivals {
    /// A window with the production bounds, starting at `now`.
    fn new(now: Instant) -> Self {
        Self::with_bounds(CARDINALITY_WINDOW, HIGH_CARDINALITY_ANCHORS, now)
    }

    /// Explicit bounds, so the tests read as policy instead of waiting 15 minutes.
    fn with_bounds(window: Duration, threshold: usize, now: Instant) -> Self {
        Self {
            window,
            threshold,
            started: now,
            distinct: HashSet::new(),
            crossed_this_window: false,
            crossed_last_window: false,
        }
    }

    /// Note one anchor arriving, and answer whether the volume is now in the
    /// high-cardinality state.
    fn note_arrival(&mut self, anchor: &Path, now: Instant) -> bool {
        self.roll_if_due(now);
        // Stop at the threshold: the set exists to answer "have we seen this many
        // DISTINCT anchors yet", and once it has, another path can't change the
        // answer. That caps the memory at `threshold` paths per volume per window,
        // in exactly the case (a storm of unique one-shot anchors) where an
        // unbounded set would be worst.
        if self.distinct.len() < self.threshold {
            self.distinct.insert(anchor.to_path_buf());
        }
        self.crossed_this_window |= self.distinct.len() >= self.threshold;
        // The PREVIOUS window counts too. A window starts from zero distinct
        // anchors, so a machine still churning would read low for the minutes it
        // takes to re-cross, and every boundary would hand the drain a fresh burst
        // of walks. Carrying one window means the verdict stays put for as long as
        // anchors keep arriving, and lifts one to two windows after they stop.
        self.crossed_this_window || self.crossed_last_window
    }

    /// How many paths the window is holding, for the memory bound's test.
    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.distinct.len()
    }

    /// Start a fresh window if `now` has left the current one behind. Lazy, driven
    /// by arrivals: a volume with no arrivals has nothing to decide, so it needs no
    /// tick of its own.
    fn roll_if_due(&mut self, now: Instant) {
        if now.saturating_duration_since(self.started) < self.window {
            return;
        }
        // More than one window of silence means the windows in between saw no
        // arrivals at all, so there is nothing to carry from them.
        let one_window_ago = now.saturating_duration_since(self.started) < self.window * 2;
        self.crossed_last_window = one_window_ago && self.crossed_this_window;
        self.crossed_this_window = false;
        self.distinct.clear();
        self.distinct.shrink_to_fit();
        self.started = now;
    }
}

/// Per-volume arrival windows. Module-global for the same reason
/// [`super::route`]'s sweep ledger is: a reconciler is recreated on every scan
/// cycle, so a per-instance window would forget the storm each time.
///
/// Per VOLUME, not process-wide like [`super::churn`]: the verdict routes one
/// volume to its own whole-volume sweep, so a churning boot disk must not send a
/// quiet external drive off to re-walk itself.
static ARRIVALS: LazyLock<Mutex<HashMap<String, AnchorArrivals>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Note one anchor arriving on `volume_id`, and answer whether the volume is now
/// in the high-cardinality state.
pub(super) fn note_arrival(volume_id: &str, anchor: &Path, now: Instant) -> bool {
    ARRIVALS
        .lock_ignore_poison()
        .entry(volume_id.to_string())
        .or_insert_with(|| AnchorArrivals::new(now))
        .note_arrival(anchor, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(15 * 60);
    const THRESHOLD: usize = 8;

    fn arrivals(now: Instant) -> AnchorArrivals {
        AnchorArrivals::with_bounds(WINDOW, THRESHOLD, now)
    }

    fn anchor(i: usize) -> PathBuf {
        PathBuf::from(format!("/Users/me/projects/thing-{i}/target"))
    }

    /// An ordinary machine: a handful of anchors a window, nothing that needs
    /// bounding. This is the no-behavior-change case, and it is the one that must
    /// never break.
    #[test]
    fn a_quiet_window_never_reads_as_high_cardinality() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..THRESHOLD - 1 {
            assert!(
                !window.note_arrival(&anchor(i), t0 + Duration::from_secs(i as u64)),
                "{} distinct anchors is an ordinary machine",
                i + 1
            );
        }
    }

    /// Distinctness is the signal, never volume. One hard-churning subtree raising
    /// the same anchor forever is exactly what the per-subtree throttle already
    /// bounds, and it must not drag the whole volume onto the sweep path.
    #[test]
    fn one_anchor_repeating_forever_is_never_high_cardinality() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..10_000 {
            assert!(
                !window.note_arrival(Path::new("/Users/me/one/busy/target"), t0 + Duration::from_millis(i)),
                "the throttle owns this case; cardinality is 1 however loud it gets"
            );
        }
    }

    /// The bound that makes this safe to ship. A storm is exactly the case where
    /// a set of every path seen would grow without limit, and it is the case this
    /// runs in. Once the answer is known, keeping more paths cannot change it.
    #[test]
    fn the_arrival_set_never_grows_past_the_threshold() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..50_000 {
            window.note_arrival(&anchor(i), t0);
        }
        assert!(
            window.tracked() <= THRESHOLD,
            "a storm must not be able to grow the set: {} paths tracked",
            window.tracked()
        );
    }

    /// The state is a WINDOW, not a latch: once the storm stops, the volume goes
    /// back to walking its anchors one at a time. Without this a single bad
    /// afternoon would route the machine to a daily sweep for the rest of the
    /// session.
    #[test]
    fn a_volume_that_goes_quiet_stops_routing() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..THRESHOLD {
            window.note_arrival(&anchor(i), t0);
        }
        assert!(
            !window.note_arrival(&anchor(999), t0 + WINDOW * 2),
            "two quiet windows later the storm is over and the drain takes anchors again"
        );
    }

    /// The window boundary must not leak. A machine still producing anchors reads
    /// high on the FIRST arrival of the new window, before it has had time to
    /// re-cross the threshold from zero; otherwise every boundary would hand the
    /// drain a fresh burst of walks, and the bound would only ever be partial.
    #[test]
    fn the_state_carries_across_a_boundary_while_anchors_keep_arriving() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..THRESHOLD {
            window.note_arrival(&anchor(i), t0);
        }
        assert!(
            window.note_arrival(&anchor(1_000), t0 + WINDOW),
            "the first anchor of the next window still reads high"
        );
        assert!(
            window.note_arrival(&anchor(1_001), t0 + WINDOW + Duration::from_secs(1)),
            "and so does the second"
        );
    }

    /// The crossing itself: the anchor that takes the window past the threshold is
    /// the first one routed, and every one after it stays routed.
    #[test]
    fn the_threshold_anchor_is_the_first_one_routed() {
        let t0 = Instant::now();
        let mut window = arrivals(t0);
        for i in 0..THRESHOLD - 1 {
            assert!(!window.note_arrival(&anchor(i), t0), "still under the threshold");
        }
        assert!(
            window.note_arrival(&anchor(THRESHOLD - 1), t0),
            "the {THRESHOLD}th distinct anchor crosses"
        );
        assert!(
            window.note_arrival(&anchor(THRESHOLD), t0),
            "and the volume stays routed for the rest of the window"
        );
    }
}
