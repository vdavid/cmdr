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

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

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
