//! Send-decision logic for mid-scan partial aggregation.
//!
//! The scan progress loop in `manager.rs` ticks every 500 ms. On each tick it
//! asks `should_send_partial_agg` whether to fire a `ComputePartialAggregates`
//! message. Keeping the decision in a pure function lets it be unit-tested
//! exhaustively while the timer loop itself stays a dumb caller.

// The scan progress loop in `manager.rs` is the sole non-test consumer of these
// items; until that call site lands they have no non-test reference, so
// dead-code analysis would flag them. The truth-table tests exercise every branch.
#![allow(dead_code, reason = "consumed by the scan progress loop in manager.rs")]

/// How many 500 ms progress ticks between partial-aggregation passes.
///
/// 10 ticks = 5 s. Matches the frontend's 2 s/pane `index-dir-updated` refresh
/// throttle (so no emit is wasted), and yields ~30 reveals over a ~2.5 min scan
/// — frequent enough to feel live without measurably slowing the scan.
pub(super) const PARTIAL_AGG_TICK_INTERVAL: u64 = 10;

/// Skip a partial pass when the writer channel is backed up beyond this many
/// queued messages.
///
/// ~20% of the 20 000-message channel capacity. A deep backlog means the writer
/// is the bottleneck catching up on insert batches; partial sizes are a luxury,
/// so don't pile more work on. Tuned with M4 measurements.
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

#[cfg(test)]
mod tests {
    use super::*;

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
