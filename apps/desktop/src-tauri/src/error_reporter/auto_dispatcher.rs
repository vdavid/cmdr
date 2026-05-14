//! Auto-dispatcher for error reports (Flow B).
//!
//! When the user opts in to `updates.errorReports`, calls to [`crate::log_error!`] route
//! through [`on_error_logged`]. The first error in a window starts a 60 s ± 10 s debounce
//! timer; subsequent errors within the window only bump a counter (the first call's
//! metadata is captured for the user-facing note). When the timer fires, [`flush`] builds
//! a 1 MB-tail bundle and uploads it via the same pipeline Phase 4 uses, then emits an
//! `error-report-auto-sent` Tauri event so the frontend can show a confirmation toast.
//!
//! ## Why not retry on upload failure
//!
//! We're already debounced at 60 s; the network may be flaky, but the user is going to
//! hit other errors soon enough if they keep hitting the same code path. Retrying inside
//! a single dispatch risks flooding the server during outages, with no benefit (the user
//! still has the manual flow if they want to be sure their report lands).
//!
//! ## Crash-loop interaction
//!
//! If the app exits inside the 60 s debounce window, the report does NOT fire. This
//! mirrors the frontend log bridge's `beforeunload` semantics; flushed-on-shutdown
//! reports require a separate codepath we don't ship. Crashes are still covered: panics
//! route through `crash_reporter`, which writes to disk synchronously and uploads on the
//! next launch. The auto-dispatcher is for soft, recoverable errors.
//!
//! ## AppHandle wiring
//!
//! The macro can't pass an `AppHandle` (it'd require every `log_error!` site to thread
//! one in). We stash a `tauri::AppHandle<tauri::Wry>` in [`APP_HANDLE`] at startup via
//! [`set_app_handle`], called from `lib.rs::setup`. If the handle isn't set yet (before
//! setup runs, or in unit tests), [`on_error_logged`] still bumps the counter and stores
//! the debounce state, but skips the spawn (no handle to clone). The state's
//! `flush_spawned` flag tracks this; when [`set_app_handle`] later runs, it picks up the
//! orphaned window and spawns the flush task with the remaining time. If the deadline
//! has already elapsed, [`sleep_until`] is a no-op and `flush` runs immediately.

use crate::error_reporter::{self, BundleKind, BundleScope, FLOW_B_BUNDLE_CAP_MB};
use chrono::{DateTime, Utc};
use rand::RngExt;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Wry};

/// Debounce window: first error schedules a flush this far in the future, plus jitter.
const DEBOUNCE_BASE: Duration = Duration::from_secs(60);
/// Jitter range: schedule_at = first_seen + DEBOUNCE_BASE ± uniform(0, JITTER).
/// Avoids lock-step reporting under global outages where many users hit the same error
/// at the same time.
const JITTER: Duration = Duration::from_secs(10);

/// Tail size for the auto-send bundle, re-exported from the error_reporter module so
/// the cap stays in lockstep with the bundle scope (Flow B fires without per-event
/// consent; small bundle, anchored on the actual error).
const AUTO_BUNDLE_CAP_MB: usize = FLOW_B_BUNDLE_CAP_MB;

/// Server URL for error report ingestion. Mirrors the commands layer constant.
#[cfg(debug_assertions)]
const ERROR_REPORT_URL: &str = "http://localhost:8787/error-report";
#[cfg(not(debug_assertions))]
const ERROR_REPORT_URL: &str = "https://api.getcmdr.com/error-report";

/// Tauri event emitted after a successful auto-send. Frontend listens for this and shows
/// the confirmation toast.
pub const AUTO_SENT_EVENT: &str = "error-report-auto-sent";

/// Master switch driven by the `updates.errorReports` setting. Default: off (opt-in).
///
/// Read on the hot path of every `log_error!` call, so it's an atomic with `Relaxed`
/// ordering; no synchronization needed beyond "eventually visible to other threads".
static ENABLED: AtomicBool = AtomicBool::new(false);

/// AppHandle stashed at startup so the macro doesn't have to thread one in.
static APP_HANDLE: OnceLock<AppHandle<Wry>> = OnceLock::new();

/// Per-window debounce state, captured on the first error in the window.
struct DebounceState {
    first_category: String,
    first_message: String,
    error_count: usize,
    /// Wall-clock target for the flush. Read by the late-spawn path in
    /// [`set_app_handle`] to compute the remaining delay when a window opened before
    /// the AppHandle was ready, and by tests to assert jitter bounds.
    scheduled_send_at: Instant,
    /// UTC wall-clock of the first error in the window. Anchors the Flow B bundle
    /// scope (`first_error_at - 30 min` lower bound). Captured at first-error time so
    /// the window is stable if the system clock drifts between record and flush.
    first_error_at: DateTime<Utc>,
    /// True once a flush task has been spawned for this window. If `set_app_handle`
    /// runs after a window opened without the handle, this lets us spawn exactly once
    /// without racing with [`on_error_logged`].
    flush_spawned: bool,
}

static STATE: Mutex<Option<DebounceState>> = Mutex::new(None);

/// Enable or disable auto-send. Driven by the `updates.errorReports` setting.
pub fn set_enabled(value: bool) {
    ENABLED.store(value, Ordering::Relaxed);
}

/// Returns whether auto-send is currently enabled.
#[allow(
    dead_code,
    reason = "Public API; useful for diagnostics and the macro's hot-path peek"
)]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Stash the app handle so the macro-driven entry point can spawn flush tasks without
/// receiving an `AppHandle` argument. Called once from `lib.rs::setup`.
///
/// If a debounce window is already active and never got its flush task (because an error
/// fired before the handle was wired up), spawn one now. Compute the remaining time
/// from `scheduled_send_at`; if it's already past, fire immediately.
pub fn set_app_handle(handle: AppHandle<Wry>) {
    if APP_HANDLE.set(handle.clone()).is_err() {
        // Already set; nothing more to do. Tests reset the handle differently; in prod
        // setup runs once.
        return;
    }
    // Atomically peek at the state under the lock. If a window is open without a
    // spawned flush, mark it as spawned and kick the task off.
    let scheduled_at = {
        let mut guard = match STATE.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match guard.as_mut() {
            Some(state) if !state.flush_spawned => {
                state.flush_spawned = true;
                Some(state.scheduled_send_at)
            }
            _ => None,
        }
    };
    if let Some(deadline) = scheduled_at {
        tauri::async_runtime::spawn(async move {
            sleep_until(deadline).await;
            flush(handle).await;
        });
    }
}

/// Throttles `cmdr://state` snapshots so an error storm in a tight loop doesn't fill
/// the log file with thousands of YAML blobs. One snapshot every 30 s is plenty for
/// triage; if the same error keeps firing, the *first* snapshot is the load-bearing
/// one anyway.
static LAST_STATE_SNAPSHOT_AT: Mutex<Option<Instant>> = Mutex::new(None);
const STATE_SNAPSHOT_THROTTLE: Duration = Duration::from_secs(30);

/// Spawn a background task that reads `cmdr://state` and writes it to the log file as
/// a debug-level record. Always runs (regardless of the Flow B opt-in) so that
/// user-initiated bundles built minutes after a failure still have a state snapshot
/// to read. File-only because of the dispatch tree: stdout's Info default drops debug
/// records on the floor, file chain stays at Debug. No-op if the AppHandle isn't wired
/// yet (early startup, unit tests) or the throttle window is still active.
fn emit_state_snapshot() {
    {
        let mut guard = match LAST_STATE_SNAPSHOT_AT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = Instant::now();
        if let Some(last) = *guard
            && now.duration_since(last) < STATE_SNAPSHOT_THROTTLE
        {
            return;
        }
        *guard = Some(now);
    }
    let Some(app) = APP_HANDLE.get().cloned() else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        match crate::mcp::resources::read_resource(&app, "cmdr://state").await {
            Ok(content) => log::debug!(
                target: "cmdr_lib::error_reporter::state_snapshot",
                "State at error time:\n{}",
                content.text,
            ),
            Err(e) => log::debug!(
                target: "cmdr_lib::error_reporter::state_snapshot",
                "(state snapshot unavailable: {e})",
            ),
        }
    });
}

/// Records an error against the auto-dispatcher. If the opt-in flag is off, returns
/// immediately. Otherwise locks the state, registers the error, and (if this call
/// started a fresh debounce window) spawns a tokio task that fires [`flush`] when the
/// timer expires.
///
/// Hot-path constraint: the disabled fast-path must do no work and no allocation. The
/// `format!()` in the macro happens regardless (cheap for short error strings; the
/// macro user controls the size), but everything past the `is_enabled()` check is gated.
pub fn on_error_logged(category: &str, message: &str) {
    // State snapshot fires regardless of the Flow B opt-in: a user who later runs the
    // manual "Send error report" flow needs the snapshot in their bundle too.
    // Throttled internally; cheap when throttled.
    emit_state_snapshot();
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let scheduled_send_at = match record_error(category, message) {
        Some(t) => t,
        None => return, // Already had an active debounce; only the counter changed.
    };

    // Spawn the flush task only if the AppHandle has been wired up. If it's not, the
    // debounce state is preserved with `flush_spawned = false`; when `set_app_handle`
    // eventually runs, it'll spawn the task with the remaining time (or fire immediately
    // if the deadline has already passed).
    let Some(app) = APP_HANDLE.get().cloned() else {
        return;
    };
    if !mark_flush_spawned() {
        // Lost the race: someone else (e.g. set_app_handle catching up) already spawned
        // the flush task for this window. Don't double-spawn.
        return;
    }
    tauri::async_runtime::spawn(async move {
        sleep_until(scheduled_send_at).await;
        flush(app).await;
    });
}

/// Atomically mark the active window as having a spawned flush task. Returns `true` if
/// this caller is the one that flipped the flag (and so should spawn), `false` if it was
/// already set (someone else won the race).
fn mark_flush_spawned() -> bool {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match guard.as_mut() {
        Some(state) if !state.flush_spawned => {
            state.flush_spawned = true;
            true
        }
        _ => false,
    }
}

/// Lock the state, register the error, and return the scheduled flush time iff this
/// call started a new debounce window. Returns `None` if a window was already active
/// (in which case the caller should NOT spawn a duplicate flush task).
///
/// Split out of [`on_error_logged`] so tests can drive the state machine without
/// needing a Tauri runtime.
fn record_error(category: &str, message: &str) -> Option<Instant> {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(state) = guard.as_mut() {
        state.error_count = state.error_count.saturating_add(1);
        return None;
    }
    let scheduled_send_at = Instant::now() + DEBOUNCE_BASE - JITTER + jitter_offset();
    *guard = Some(DebounceState {
        first_category: category.to_string(),
        first_message: message.to_string(),
        error_count: 1,
        scheduled_send_at,
        first_error_at: Utc::now(),
        flush_spawned: false,
    });
    Some(scheduled_send_at)
}

/// Drain the debounce state and ship a single bundle. No-op if the state is empty (can
/// happen if the test harness cleared it between scheduling and firing).
async fn flush(app: AppHandle<Wry>) {
    let snapshot = {
        let mut guard = match STATE.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.take()
    };
    let Some(state) = snapshot else {
        return;
    };

    let plural = if state.error_count == 1 { "" } else { "s" };
    let note = format!(
        "auto-send: {count} error{plural} within 60s, first: {cat} | {msg}",
        count = state.error_count,
        plural = plural,
        cat = state.first_category,
        msg = state.first_message,
    );

    let scope = BundleScope::Window {
        first_error_at: state.first_error_at,
    };
    let bundle = match error_reporter::build_bundle(&app, BundleKind::Auto, Some(note), scope) {
        Ok(b) => b,
        Err(e) => {
            log::warn!(
                target: "cmdr_lib::error_reporter",
                "Auto-send: build_bundle failed: {e}",
            );
            return;
        }
    };

    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, AUTO_BUNDLE_CAP_MB);
    match error_reporter::upload(capped, &bundle.manifest, ERROR_REPORT_URL).await {
        Ok(result) => {
            log::info!(
                target: "cmdr_lib::error_reporter",
                "Auto-send: error report uploaded, id={}",
                result.id,
            );
            if let Err(e) = app.emit(AUTO_SENT_EVENT, &result.id) {
                log::warn!(
                    target: "cmdr_lib::error_reporter",
                    "Auto-send: succeeded but couldn't emit `{AUTO_SENT_EVENT}`: {e}",
                );
            }
        }
        Err(e) => {
            log::warn!(
                target: "cmdr_lib::error_reporter",
                "Auto-send: upload failed (dropping report, no retry): {e}",
            );
        }
    }
}

/// Returns a uniformly-distributed `Duration` in `[0, 2 * JITTER]`. The caller adds this
/// to `DEBOUNCE_BASE - JITTER` so the resulting schedule sits in
/// `[DEBOUNCE_BASE - JITTER, DEBOUNCE_BASE + JITTER]`.
fn jitter_offset() -> Duration {
    let max_millis = (2 * JITTER.as_millis()) as u64;
    let mut rng = rand::rng();
    Duration::from_millis(rng.random_range(0..=max_millis))
}

async fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if deadline > now {
        tokio::time::sleep(deadline - now).await;
    }
}

#[cfg(test)]
pub fn record_error_for_test(category: &str, message: &str) -> Option<Instant> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    record_error(category, message)
}

#[cfg(test)]
pub fn reset_for_test() {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    *guard = None;
    ENABLED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
pub fn snapshot_for_test() -> Option<(String, String, usize, Instant)> {
    let guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard.as_ref().map(|s| {
        (
            s.first_category.clone(),
            s.first_message.clone(),
            s.error_count,
            s.scheduled_send_at,
        )
    })
}

/// Test seam: returns `Some(true)` if a window is active and its flush task has been
/// spawned, `Some(false)` if a window is active but no spawn happened yet, `None` if
/// no window is active.
#[cfg(test)]
pub fn flush_spawned_for_test() -> Option<bool> {
    let guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard.as_ref().map(|s| s.flush_spawned)
}

/// Test seam: simulates the late-arriving AppHandle path without needing a Tauri runtime.
/// Returns `Some(deadline)` if a window was active and not yet spawned (so the production
/// `set_app_handle` would spawn a task for it), `None` otherwise.
#[cfg(test)]
pub fn simulate_late_app_handle_for_test() -> Option<Instant> {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match guard.as_mut() {
        Some(state) if !state.flush_spawned => {
            state.flush_spawned = true;
            Some(state.scheduled_send_at)
        }
        _ => None,
    }
}

#[cfg(test)]
pub fn jitter_window() -> (Duration, Duration) {
    (DEBOUNCE_BASE - JITTER, DEBOUNCE_BASE + JITTER)
}

#[cfg(test)]
pub fn pick_jitter_offset_for_test() -> Duration {
    jitter_offset()
}
