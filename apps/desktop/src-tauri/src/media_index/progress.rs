//! The enrichment progress seam: a pure emit-throttle decision plus the sink trait
//! the enrich cores report through.
//!
//! Kept Tauri-free so the registry-free enrich cores (`scheduler/enrich.rs`,
//! `network/enrich.rs`) can report progress without depending on an `AppHandle` — a
//! test injects a recording sink, production injects the throttled Tauri emitter
//! (`events/mod.rs`). Progress rides the top-right indexing indicator as a second
//! publisher alongside the drive indexer; see
//! [`media_index/DETAILS.md`](DETAILS.md) § Progress events.

/// A snapshot of one pass's progress WITHIN its enrichable subset (images passing the
/// coverage gates, never the full walked set — a raw walked-set denominator rebuilds
/// the never-finishes bug inside the indicator). `done` counts every subset
/// image the pass has finished handling (enriched, already-current, or quietly skipped
/// — vanished / too-small / phantom), so it reaches `total` on a completed pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EnrichProgress {
    /// Subset images processed so far.
    pub(crate) done: u64,
    /// Total images in the enrichable subset (the honest denominator).
    pub(crate) total: u64,
    /// Bytes processed so far (summed `ImageEntry.size`; a `None` size counts 0, so the
    /// bytes bar under-counts rather than lies).
    pub(crate) bytes_done: u64,
    /// Total bytes across the enrichable subset.
    pub(crate) bytes_total: u64,
}

/// The seam the enrich cores report progress through. Called once per processed subset
/// image (cheap — the implementation throttles the actual emission), so it stays out of
/// the per-image hot path except a counter + time check. Tests inject a
/// recorder; production injects the throttled Tauri emitter.
pub(crate) trait EnrichProgressSink {
    /// Report cumulative progress. The sink decides whether to actually emit.
    fn report(&self, progress: EnrichProgress);
}

/// A no-op sink for passes with no app handle (unit tests that don't assert progress,
/// and the scheduler when it isn't wired to an app).
pub(crate) struct NoopProgressSink;

impl EnrichProgressSink for NoopProgressSink {
    fn report(&self, _progress: EnrichProgress) {}
}

/// Whether a progress tick should be EMITTED now, given the last emit's time and count.
///
/// Emits on the first tick (`last_emit_ms` is `None`), when at least `min_step` images
/// have been processed since the last emit, or when at least `min_interval_ms` has
/// elapsed. Pure over its inputs (a fake clock in tests) so the "start, then every
/// 500 ms or 100 images" cadence is unit-testable without an app or a real timer.
/// Saturating arithmetic so a clock that appears to go backwards can't underflow into a
/// spurious emit.
pub(crate) fn should_emit_progress(
    last_emit_ms: Option<u64>,
    last_done: u64,
    now_ms: u64,
    done: u64,
    min_interval_ms: u64,
    min_step: u64,
) -> bool {
    match last_emit_ms {
        None => true,
        Some(last) => done.saturating_sub(last_done) >= min_step || now_ms.saturating_sub(last) >= min_interval_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: u64 = 500;
    const STEP: u64 = 100;

    #[test]
    fn first_tick_always_emits() {
        // No prior emit ⇒ the pass-start tick always fires (the row appears at once).
        assert!(should_emit_progress(None, 0, 0, 0, INTERVAL, STEP));
    }

    #[test]
    fn emits_after_the_step_threshold_of_images() {
        // 100 images since the last emit ⇒ emit, even within the interval.
        assert!(should_emit_progress(Some(1_000), 0, 1_010, 100, INTERVAL, STEP));
        // 99 images and only 10 ms elapsed ⇒ hold.
        assert!(!should_emit_progress(Some(1_000), 0, 1_010, 99, INTERVAL, STEP));
    }

    #[test]
    fn emits_after_the_time_interval() {
        // 500 ms elapsed with only a few images ⇒ emit (a slow pass still ticks).
        assert!(should_emit_progress(Some(1_000), 40, 1_500, 45, INTERVAL, STEP));
        // 499 ms and only five images ⇒ hold.
        assert!(!should_emit_progress(Some(1_000), 40, 1_499, 45, INTERVAL, STEP));
    }

    #[test]
    fn a_backwards_clock_does_not_spuriously_emit() {
        // now < last (a clock adjustment): saturating_sub ⇒ 0, no interval trip; and no
        // step trip ⇒ hold, never an underflow-driven emit.
        assert!(!should_emit_progress(Some(2_000), 50, 1_000, 60, INTERVAL, STEP));
    }
}
