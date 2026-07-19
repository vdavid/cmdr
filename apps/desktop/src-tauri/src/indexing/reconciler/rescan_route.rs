//! Depth-split routing for `MustScanSubDirs` anchors, plus the per-volume
//! root-rescan cooldown.
//!
//! macOS drops fine-grained FSEvents under heavy churn and raises
//! `MustScanSubDirs` on ever-higher paths, up to `/`. A deep/narrow anchor (a
//! single `target/`) is exactly what the throttled `reconcile_subtree` drain is
//! good at. A shallow/root-scale anchor is NOT: reconciling `/` is a ~20-min walk
//! that holds the per-dir hourglass the whole time (`reconciler/rescan.rs`), and
//! under continuous root churn the anchor never leaves `pending_rescans`, so the
//! hold never clears. A channel overflow — the SAME "we lost events" signal —
//! already takes the VISIBLE scanner path (single-flight, updates freshness,
//! self-clearing). This routes shallow anchors there too, so the two equivalent
//! signals stop diverging.
//!
//! Everything here is pure + clock-injected so the split and the cooldown are
//! deterministically unit-tested.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

/// Anchor-depth threshold: an anchor at or below this depth routes to the visible
/// scanner; a deeper one keeps the throttled reconcile. `path_prefix::depth`
/// counts components, so `depth("/") = 0`, `depth("/Users") = 1`,
/// `depth("/Users/<me>") = 2`. A threshold of 2 therefore catches the top ~3
/// levels a root-scale `MustScanSubDirs` collapses to, while a project's `target/`
/// (three levels deep or more, e.g. `/Users/me/projects/foo/target`) still
/// reconciles in place. Depth stands in for walk-the-world cost: two-to-three
/// levels is the band where a reconcile stops being cheap and starts holding the
/// hourglass for the better part of a full scan.
pub(in crate::indexing) const SHALLOW_RESCAN_MAX_DEPTH: usize = 2;

/// Minimum interval between scanner rescans triggered by shallow routing, PER
/// volume. A machine that overflows `/` every few minutes must not scanner-rescan
/// continuously; within this window the redundant shallow demand is coalesced
/// (dropped). Dropping is safe: `start_scan` is single-flight and the FSEvents
/// stream replays from the last event id, so a coalesced burst is caught by the
/// next scan anyway. This is the one genuine staleness knob the split introduces.
pub(in crate::indexing) const ROOT_RESCAN_COOLDOWN: Duration = Duration::from_secs(45);

/// Where a `MustScanSubDirs` anchor should go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum RescanRoute {
    /// Shallow/root-scale: take the visible `start_scan` path (like a channel
    /// overflow) — single-flight, updates freshness, no per-dir hourglass hold.
    Scanner,
    /// Deep/narrow: keep the throttled `reconcile_subtree` drain.
    Reconcile,
}

/// Classify a `MustScanSubDirs` anchor by its component depth. Pure so the split
/// is one testable decision, independent of the trigger, the reconciler state, and
/// the cooldown.
pub(in crate::indexing) fn classify(anchor_depth: usize) -> RescanRoute {
    if anchor_depth <= SHALLOW_RESCAN_MAX_DEPTH {
        RescanRoute::Scanner
    } else {
        RescanRoute::Reconcile
    }
}

/// Per-volume time of the last scanner rescan triggered by shallow routing.
/// Module-global, NOT per-reconciler: a reconciler is recreated on every scan
/// cycle, so a per-instance field would reset the cooldown each time and let a
/// churning root re-scan back-to-back across cycles. Keyed by volume id and lived
/// for the process, it survives that recreation.
static LAST_SCANNER_RESCAN: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether a scanner rescan is allowed for `volume_id` at `now`. Returns `true`
/// and records `now` when the cooldown has elapsed (or on the first trigger);
/// returns `false` within [`ROOT_RESCAN_COOLDOWN`] of the last trigger, so the
/// caller coalesces (drops) the redundant shallow demand. Clock-injected.
pub(in crate::indexing) fn allow_scanner_rescan(volume_id: &str, now: Instant) -> bool {
    let mut map = LAST_SCANNER_RESCAN.lock_ignore_poison();
    match map.get(volume_id) {
        Some(&last) if now.duration_since(last) < ROOT_RESCAN_COOLDOWN => false,
        _ => {
            map.insert(volume_id.to_string(), now);
            true
        }
    }
}

/// Test-only: clear the cooldown ledger so a test starts from a clean leading edge.
#[cfg(test)]
pub(in crate::indexing) fn reset_cooldown_for_test() {
    LAST_SCANNER_RESCAN.lock_ignore_poison().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shallow_anchors_route_to_the_scanner() {
        // The top ~3 levels a root-scale MustScanSubDirs collapses to.
        assert_eq!(classify(0), RescanRoute::Scanner, "/ routes to the scanner");
        assert_eq!(classify(1), RescanRoute::Scanner, "/Users routes to the scanner");
        assert_eq!(classify(2), RescanRoute::Scanner, "/Users/<me> routes to the scanner");
    }

    #[test]
    fn deep_anchors_keep_the_reconcile_drain() {
        // A project's target/ and anything deeper: the throttled reconcile is good
        // at these, so they must NOT take the visible scanner path.
        assert_eq!(classify(3), RescanRoute::Reconcile, "a depth-3 anchor reconciles");
        assert_eq!(classify(6), RescanRoute::Reconcile, "a deep anchor reconciles");
    }

    #[test]
    fn cooldown_allows_the_leading_edge_then_coalesces_within_the_window() {
        reset_cooldown_for_test();
        let vol = "cooldown-vol-a";
        let t0 = Instant::now();
        assert!(
            allow_scanner_rescan(vol, t0),
            "first shallow trigger is the leading edge"
        );
        assert!(
            !allow_scanner_rescan(vol, t0 + Duration::from_secs(1)),
            "a second trigger 1 s later is coalesced (dropped)"
        );
        assert!(
            !allow_scanner_rescan(vol, t0 + ROOT_RESCAN_COOLDOWN - Duration::from_millis(1)),
            "still within the window just before it elapses"
        );
    }

    #[test]
    fn cooldown_reallows_after_the_window_elapses() {
        reset_cooldown_for_test();
        let vol = "cooldown-vol-b";
        let t0 = Instant::now();
        assert!(allow_scanner_rescan(vol, t0));
        assert!(
            allow_scanner_rescan(vol, t0 + ROOT_RESCAN_COOLDOWN),
            "exactly one window later is allowed again (never starves)"
        );
    }

    #[test]
    fn cooldown_is_tracked_independently_per_volume() {
        reset_cooldown_for_test();
        let t0 = Instant::now();
        assert!(allow_scanner_rescan("cooldown-vol-c", t0));
        assert!(
            allow_scanner_rescan("cooldown-vol-d", t0),
            "a different volume has its own leading edge"
        );
        assert!(
            !allow_scanner_rescan("cooldown-vol-c", t0 + Duration::from_secs(1)),
            "the first volume is still cooling down"
        );
    }
}
