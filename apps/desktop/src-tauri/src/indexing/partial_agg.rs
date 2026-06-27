//! Send-decision logic and hot-path collection for mid-scan partial aggregation.
//!
//! The scan progress loop in `progress_reporter.rs` ticks every 500 ms. On each
//! tick it asks `should_send_partial_agg` whether to fire a
//! `ComputePartialAggregates` message; when it does, `collect_hot_paths` turns
//! the live listing snapshot into the message's `hot_paths`. Keeping both pure
//! lets them be unit-tested exhaustively while the timer loop itself stays a dumb
//! caller.

use crate::file_system::listing::caching::ListingSummary;
use crate::indexing::firmlinks;

/// How many 500 ms progress ticks between partial-aggregation passes.
///
/// 10 ticks = 5 s. Matches the frontend's 2 s/pane `index-dir-updated` refresh
/// throttle (so no emit is wasted), and yields ~28 reveals over a ~2.5 min scan
/// — frequent enough to feel live without measurably slowing the scan. Verified
/// on a real volume (Apple Silicon, 5.94M entries / 558K dirs, release build):
/// 28 passes, each ≤ 397 ms, total scan ~2m25s, indistinguishable from the
/// feature-off baseline. See the per-pass cost note on `PARTIAL_AGG_MAX_DEPTH`.
pub(super) const PARTIAL_AGG_TICK_INTERVAL: u64 = 10;

/// Skip a partial pass when the writer channel is backed up beyond this many
/// queued messages.
///
/// ~20% of the 20 000-message channel capacity. A deep backlog means the writer
/// is the bottleneck catching up on insert batches; partial sizes are a luxury,
/// so don't pile more work on. On the real-volume verification run the channel
/// never approached this depth (insert batches drained between passes), so the
/// cap acted purely as a safety valve — kept at 4 000 as headroom rather than
/// re-tuned down.
pub(super) const PARTIAL_AGG_MAX_QUEUE_DEPTH: usize = 4_000;

/// Decide whether the scan progress loop should send a `ComputePartialAggregates`
/// pass on this tick.
///
/// Fires on every `PARTIAL_AGG_TICK_INTERVAL`-th tick, never on tick 0 (the very
/// first tick has nothing scanned yet, and 0 is a multiple of the interval, so
/// it's excluded explicitly), and skips while the writer queue is deeper than
/// `PARTIAL_AGG_MAX_QUEUE_DEPTH`. A queue depth of exactly the max still sends:
/// the cap is "skip when *over* the threshold", so `== max` is the last sending
/// depth.
pub(super) fn should_send_partial_agg(tick: u64, queue_depth: usize) -> bool {
    if tick == 0 {
        return false;
    }
    if !tick.is_multiple_of(PARTIAL_AGG_TICK_INTERVAL) {
        return false;
    }
    queue_depth <= PARTIAL_AGG_MAX_QUEUE_DEPTH
}

/// Turn a snapshot of the live listing cache into the firmlink-normalized "hot
/// paths" for a partial-aggregation pass: the directories a pane is currently
/// showing, so the handler can punch their `dir_stats` through the depth cap.
///
/// Keeps only listings on the volume being scanned (matched by `volume_id`, the
/// only reliable signal — `ListingSummary` carries no volume root). This drops,
/// by construction, every `network` / `search-results` / `mtp-*` listing, SMB
/// shares, and **other local volumes** like `/Volumes/OtherDisk`: those carry
/// absolute-looking paths that would otherwise be resolved against the scanned
/// volume's per-volume DB, matching the wrong subtree. `path.is_absolute()` is a
/// belt-and-braces second filter.
///
/// Surviving paths are mapped through `firmlinks::normalize_path` so they match
/// the index's canonical form (`/tmp` → `/private/tmp` etc.), then deduplicated
/// (two panes on the same dir collapse to one) while preserving first-seen order.
pub(super) fn collect_hot_paths(listings: &[ListingSummary], scanned_volume_id: &str) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for listing in listings {
        if listing.volume_id != scanned_volume_id || !listing.path.is_absolute() {
            continue;
        }
        let normalized = firmlinks::normalize_path(&listing.path.to_string_lossy());
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn summary(volume_id: &str, path: &str) -> ListingSummary {
        ListingSummary {
            listing_id: format!("listing-{path}"),
            volume_id: volume_id.to_string(),
            path: PathBuf::from(path),
            entry_count: 0,
            age_ms: 0,
        }
    }

    #[test]
    fn keeps_matching_volume_absolute_paths_normalized_and_deduped() {
        let listings = vec![
            // On the scanned volume, absolute: kept.
            summary("vol-main", "/Users/david"),
            // Same path on another local volume: dropped (would resolve against
            // the wrong per-volume DB).
            summary("vol-other", "/Users/david"),
            // Virtual / device listings: dropped by the volume-id filter.
            summary("mtp-1234", "/DCIM"),
            summary("search-results", "/anything"),
            summary("network", "/share/docs"),
            // Relative / SMB-share-shaped path on the scanned volume: dropped by
            // the absolute-path filter.
            summary("vol-main", "relative/dir"),
            // Needs firmlink normalization.
            summary("vol-main", "/tmp/scratch"),
            // Duplicate of the first (two panes on the same dir): deduped.
            summary("vol-main", "/Users/david"),
        ];

        let hot = collect_hot_paths(&listings, "vol-main");

        // First-seen order preserved; the /tmp entry normalized; duplicate gone;
        // every other-volume / virtual / relative entry filtered out.
        let expected_tmp = firmlinks::normalize_path("/tmp/scratch");
        assert_eq!(hot, vec!["/Users/david".to_string(), expected_tmp]);
    }

    #[test]
    fn empty_when_no_listing_matches_the_scanned_volume() {
        let listings = vec![summary("vol-other", "/Users/david"), summary("mtp-1", "/DCIM")];
        assert!(collect_hot_paths(&listings, "vol-main").is_empty());
    }

    #[test]
    fn never_sends_on_tick_zero() {
        // Tick 0 satisfies `tick % interval == 0` but nothing is scanned yet.
        assert!(!should_send_partial_agg(0, 0));
    }

    #[test]
    fn skips_non_interval_ticks() {
        for tick in [1, 5, 9, 11, 19, 21] {
            assert!(
                !should_send_partial_agg(tick, 0),
                "tick {tick} is not a multiple of the interval; should skip"
            );
        }
    }

    #[test]
    fn fires_on_interval_ticks_with_shallow_queue() {
        for tick in [
            PARTIAL_AGG_TICK_INTERVAL,
            PARTIAL_AGG_TICK_INTERVAL * 2,
            PARTIAL_AGG_TICK_INTERVAL * 3,
        ] {
            assert!(
                should_send_partial_agg(tick, 0),
                "tick {tick} is an interval multiple with a shallow queue; should fire"
            );
        }
    }

    #[test]
    fn skips_interval_tick_when_queue_is_deep() {
        assert!(!should_send_partial_agg(
            PARTIAL_AGG_TICK_INTERVAL,
            PARTIAL_AGG_MAX_QUEUE_DEPTH + 1
        ));
    }

    #[test]
    fn queue_depth_boundary_is_inclusive() {
        // Exactly at the cap still sends; one over does not. The cap is "skip
        // when *over* the threshold" (`> max`), so `== max` is the last depth
        // that fires.
        assert!(should_send_partial_agg(
            PARTIAL_AGG_TICK_INTERVAL,
            PARTIAL_AGG_MAX_QUEUE_DEPTH
        ));
        assert!(!should_send_partial_agg(
            PARTIAL_AGG_TICK_INTERVAL,
            PARTIAL_AGG_MAX_QUEUE_DEPTH + 1
        ));
    }
}
