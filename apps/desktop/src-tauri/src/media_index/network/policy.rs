//! The conservative-fetch policy for network enrichment (plan Decision 6): the
//! typed knobs plus the PURE decisions the pass gates on, so "does it defer or
//! proceed?" is unit-testable over a fake clock / fake idle signal.
//!
//! "Conservative" has teeth here, not just intent:
//! - **Idle-gated**: enrich a network volume only while the app has been idle (no
//!   foreground activity) for [`ConservativeFetchPolicy::idle_threshold`], so a NAS is
//!   never dragged over the wire while the user is browsing.
//! - **Bandwidth-bounded**: after each image, sleep so the sustained fetch rate stays
//!   under [`ConservativeFetchPolicy::max_bytes_per_sec`] ([`throttle_delay`]).
//! - **Bounded concurrency**: [`ConservativeFetchPolicy::max_concurrency`] images in
//!   flight at once (the pass fetches serially = 1 today; the field bounds any future
//!   parallel fetch and is honest about the current shape).
//! - **Resumable**: each completed image persists immediately, so an interrupted pass
//!   resumes from the path-keyed store (see [`super::enrich`]).

use std::time::Duration;

/// The typed conservative-fetch knobs. Defaults are deliberately gentle for a NAS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConservativeFetchPolicy {
    /// Enrich a network volume only after the app has been idle this long.
    pub idle_threshold: Duration,
    /// Sustained fetch-rate ceiling in bytes/sec (`0` = unbounded). A gentle cap so
    /// background enrichment never saturates the link the user shares with the NAS.
    pub max_bytes_per_sec: u64,
    /// Maximum images fetched concurrently (`>= 1`). The pass fetches serially today,
    /// so this is effectively `1`; kept typed so a future parallel fetch is bounded.
    pub max_concurrency: usize,
    /// Per-image byte-read timeout: past this a fetch is treated as a disconnect and
    /// the volume pauses (never a false `Failed`).
    pub read_timeout: Duration,
}

impl Default for ConservativeFetchPolicy {
    fn default() -> Self {
        Self {
            idle_threshold: Duration::from_secs(5),
            // 8 MB/s: a few photos per second, well under a gigabit NAS link.
            max_bytes_per_sec: 8 * 1024 * 1024,
            max_concurrency: 1,
            read_timeout: Duration::from_secs(15),
        }
    }
}

/// The idle gate's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchGate {
    /// The app is idle enough; proceed with the fetch.
    Proceed,
    /// The user is active; defer (the pass pauses and retries later).
    DeferNotIdle,
}

/// The pure idle gate: proceed only when idle. (`is_idle` comes from
/// [`crate::priority::foreground`], itself tested over a fake clock.)
pub fn gate_on_idle(is_idle: bool) -> FetchGate {
    if is_idle {
        FetchGate::Proceed
    } else {
        FetchGate::DeferNotIdle
    }
}

/// The pure proceed-gate for one network-enrichment step, composing the two
/// higher-priority claims (`crate::priority`: interactive > transfers > indexing):
/// proceed only while the app is foreground-idle (app-WIDE — heavy on-device ML
/// with no deadline stands aside for any browsing) AND no user-initiated transfer
/// is touching THIS volume (per-volume — a copy elsewhere is no reason to wait).
/// A `false` pauses the pass exactly like a plain not-idle always has
/// (`PauseReason::NotIdle` → retry when clear).
pub fn volume_clear_for_enrichment(app_idle: bool, transfer_active_on_volume: bool) -> bool {
    app_idle && !transfer_active_on_volume
}

/// The bandwidth throttle: how long `bytes` should take at `max_bytes_per_sec`, i.e.
/// how long to sleep after fetching them to hold the sustained rate. `0` bytes/sec
/// means unbounded (no delay). Deliberately ignores the time OCR itself took, so it
/// slightly OVER-throttles — the conservative direction (never hammer the NAS).
pub fn throttle_delay(bytes: u64, max_bytes_per_sec: u64) -> Duration {
    if max_bytes_per_sec == 0 {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(bytes as f64 / max_bytes_per_sec as f64)
}

/// The pure per-image enrichment gate: enrich when the override covers it OR when its
/// importance meets the threshold. Without an override, a low-importance NAS folder
/// defers (the "navigation-based importance starves a photo archive" hazard — plan
/// Decision 6); the override is the escape hatch.
///
/// Before the importance slider lands the production importance oracle yields `None`,
/// so `covered_by_override` is the load-bearing input; the `importance` arg keeps
/// the importance gate behind the same seam.
pub fn should_enrich_image(covered_by_override: bool, importance: Option<f32>, threshold: f32) -> bool {
    covered_by_override || matches!(importance, Some(score) if score >= threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_defers_when_not_idle_proceeds_when_idle() {
        assert_eq!(gate_on_idle(false), FetchGate::DeferNotIdle);
        assert_eq!(gate_on_idle(true), FetchGate::Proceed);
    }

    /// Transfers trump indexing: an active transfer on the volume pauses enrichment
    /// even when the app is otherwise idle, and neither claim masks the other.
    #[test]
    fn a_transfer_on_the_volume_pauses_enrichment_even_when_idle() {
        assert!(!volume_clear_for_enrichment(true, true), "idle but transferring ⇒ wait");
        assert!(!volume_clear_for_enrichment(false, false), "browsing ⇒ wait");
        assert!(!volume_clear_for_enrichment(false, true));
        assert!(
            volume_clear_for_enrichment(true, false),
            "idle and no transfer ⇒ proceed"
        );
    }

    #[test]
    fn throttle_delay_scales_with_bytes_and_rate() {
        // 1 MB at 1 MB/s ⇒ 1 s.
        assert_eq!(throttle_delay(1_000_000, 1_000_000), Duration::from_secs(1));
        // 2 MB at 1 MB/s ⇒ 2 s.
        assert_eq!(throttle_delay(2_000_000, 1_000_000), Duration::from_secs(2));
        // Unbounded ⇒ no delay.
        assert_eq!(throttle_delay(9_999_999, 0), Duration::ZERO);
    }

    #[test]
    fn override_enriches_regardless_of_importance() {
        // Overridden low-importance folder ⇒ enriched.
        assert!(should_enrich_image(true, Some(0.0), 0.5));
        // Overridden with no importance signal at all ⇒ enriched (the network-enrichment case).
        assert!(should_enrich_image(true, None, 0.5));
    }

    #[test]
    fn without_override_low_importance_defers() {
        // Not overridden, below threshold ⇒ deferred.
        assert!(!should_enrich_image(false, Some(0.2), 0.5));
        // Not overridden, no importance signal ⇒ deferred (network-enrichment production default).
        assert!(!should_enrich_image(false, None, 0.5));
        // Not overridden but above threshold ⇒ enriched (the importance-slider path).
        assert!(should_enrich_image(false, Some(0.8), 0.5));
    }
}
