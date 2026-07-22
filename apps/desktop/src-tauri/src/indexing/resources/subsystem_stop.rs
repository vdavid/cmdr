//! A tiny registry of subsystem stop hooks the global memory watchdog runs
//! ALONGSIDE `stop_all_indexing`.
//!
//! The 16 GB resident-memory watchdog (`resources/memory_watchdog.rs`) measures the WHOLE
//! process but historically only stopped indexing — `stop_all_indexing` "does not
//! know about other subsystems". A subsystem that shares the same resident pool
//! (media_index enrichment, which decodes HEIC/RAW and can spike RAM) must yield to
//! the same ceiling rather than standing up a SECOND independent 16 GB budget: two
//! ceilings over one pool each see headroom and can sum to ~2× (plan Resources
//! cross-cutting).
//!
//! So a subsystem registers a stop hook once at startup; the watchdog's stop action
//! runs them all. Hooks must be cheap and non-blocking (they run inline in the
//! watchdog's stop path) — e.g. flip an atomic cancel flag.

use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;

type StopHook = Box<dyn Fn() + Send + Sync>;

/// The registered hooks. Process-global and append-only; a subsystem registers once
/// at startup and never unregisters (it lives for the process).
static STOP_HOOKS: Mutex<Vec<StopHook>> = Mutex::new(Vec::new());

/// Register a stop hook the memory watchdog runs when the global budget is hit.
/// Call once per subsystem at startup. The hook must be cheap and non-blocking.
pub fn register_subsystem_stop_hook(hook: StopHook) {
    STOP_HOOKS.lock_ignore_poison().push(hook);
}

/// Run every registered stop hook. Called from `stop_all_indexing` (the memory
/// watchdog's stop action), so a shared-pool subsystem yields under the same
/// ceiling.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "only the macOS memory watchdog stops indexing and runs the hooks"
    )
)]
pub(crate) fn run_subsystem_stop_hooks() {
    for hook in STOP_HOOKS.lock_ignore_poison().iter() {
        hook();
    }
}
