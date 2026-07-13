//! The two process-global runtime flags the enrichment scheduler gates on.
//!
//! - **[`is_enabled`]**: the master "Index image contents" toggle, seeded from
//!   settings at startup (OFF by default) and flipped by the live-apply settings
//!   command. Every pass checks it before doing any work.
//! - **[`is_cancelled`]**: the emergency stop the indexing memory watchdog sets via
//!   its subsystem-stop hook (media_index shares the ONE resident-memory ceiling,
//!   it does not stand up a second one — see the plan's Resources cross-cutting).
//!   The pass checks it BETWEEN images so it yields promptly under memory pressure.
//!   Enabling the feature clears it, so re-enabling recovers.
//!
//! The enrichment core takes the cancel decision as an argument (a closure), so it
//! stays unit-testable without touching these globals; only the live scheduler
//! reads them.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// The lowest folder-importance level (`0.0..=1.0`) the user wants image-indexed —
/// the M2 settings slider's typed value (plan M2). Stored as `f64` bits in an atomic
/// so the scheduler reads it lock-free on every pass. The default is
/// [`DEFAULT_IMPORTANCE_THRESHOLD`]; the slider live-applies a new value.
static IMPORTANCE_THRESHOLD_BITS: AtomicU64 = AtomicU64::new(DEFAULT_IMPORTANCE_THRESHOLD.to_bits());

/// The default importance threshold before the user touches the slider: `0.0`, i.e.
/// enrich every folder importance scores at all. Importance already floors junk
/// (`node_modules`, caches, hidden/system) to no row, so `0.0` still skips junk while
/// preserving the pre-slider behavior of covering all real folders. The slider raises
/// it to defer low-importance folders.
pub const DEFAULT_IMPORTANCE_THRESHOLD: f64 = 0.0;

/// Set the master toggle. Enabling also clears any prior emergency-stop so the
/// scheduler resumes.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        CANCELLED.store(false, Ordering::SeqCst);
    }
}

/// Whether image indexing is enabled (the master toggle).
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Request that in-flight enrichment yield (the memory watchdog's stop hook calls
/// this). Idempotent; cleared by [`set_enabled(true)`](set_enabled).
pub fn request_cancel() {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Whether an emergency stop is in effect. The pass checks this between images.
pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Set the importance threshold (clamped to `0.0..=1.0`). Seeded from settings at
/// startup and live-applied by the slider's settings command.
pub fn set_importance_threshold(threshold: f64) {
    IMPORTANCE_THRESHOLD_BITS.store(threshold.clamp(0.0, 1.0).to_bits(), Ordering::SeqCst);
}

/// The current importance threshold (`0.0..=1.0`). The scheduler enriches a folder
/// only when its importance is at or above this (or an override covers it).
pub fn importance_threshold() -> f64 {
    f64::from_bits(IMPORTANCE_THRESHOLD_BITS.load(Ordering::SeqCst))
}
