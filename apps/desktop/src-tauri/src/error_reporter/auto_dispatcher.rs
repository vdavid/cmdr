//! Auto-dispatcher for error reports (Flow B).
//!
//! When the user opts in to `updates.errorReports`, calls to [`crate::log_error!`] route
//! through [`on_error_logged`]. The first error in a window starts a 60 s ± 10 s debounce
//! timer; subsequent errors within the window only bump a counter (the first call's
//! category and message are captured for the automatic note). When the timer fires, [`flush`] builds
//! a 1 MB-tail bundle and uploads it via the same pipeline Phase 4 uses, records what it sent
//! in [`super::auto_sent`], then emits an `error-report-auto-sent` Tauri event so the frontend
//! can show a confirmation toast. The stash comes first: the toast offers to show the report
//! and add a note to it, and both read what the flush left behind.
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

use crate::error_reporter::{self, BundleKind, BundleScope, FLOW_B_BUNDLE_CAP_MB, auto_sent};
use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Wry};
use tauri_specta::Event as _;

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

/// `error-report-auto-sent`: emitted after a successful Flow B auto-send. The
/// frontend listens for this and shows the confirmation toast. `id` is the
/// server-issued `ERR-XXXXX` report id (the same one the manifest carried).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReportAutoSent {
    pub id: String,
}

/// Master switch driven by the `updates.errorReports` setting. Default: off (opt-in).
///
/// Read on the hot path of every `log_error!` call, so it's an atomic with `Relaxed`
/// ordering; no synchronization needed beyond "eventually visible to other threads".
static ENABLED: AtomicBool = AtomicBool::new(false);

/// AppHandle stashed at startup so the macro doesn't have to thread one in.
static APP_HANDLE: OnceLock<AppHandle<Wry>> = OnceLock::new();

/// Per-window debounce state, captured on the first error in the window.
struct DebounceState {
    /// The first error's log target: a fixed string, safe as is.
    first_category: String,
    /// The first error's message, whole. The note carries it as a `detail=` field, which the
    /// bundle's report pass redacts and caps like any external text.
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

/// Ask the report-local history to capture typed state. It owns the 30-second throttle and
/// bounded ring; no snapshot enters the log file or any other persisted store.
fn capture_state_snapshot() {
    if let Some(app) = APP_HANDLE.get().cloned() {
        super::state_history::capture_if_due(app);
    }
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
    // State capture fires regardless of the Flow B opt-in: a user who later runs the manual
    // flow needs the history too. The collector is process-local and throttled internally.
    capture_state_snapshot();
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
    let Some(state) = take_window_to_send().await else {
        return;
    };

    let note = automatic_note(&state);

    let scope = BundleScope::Window {
        first_error_at: state.first_error_at,
    };
    // Flow B NEVER attaches an email: the user opted into auto-send, not into shipping their
    // address on every report. There's nothing to pass here even by mistake, since only
    // `AttachedEmail::from_flow_a_dialog` mints one and this path never sees a dialog.
    let request = error_reporter::BundleRequest {
        kind: BundleKind::Auto,
        scope,
        // No preview to keep an id in step with; the manifest's minted id is the only one
        // anyone ever sees, via the `error-report-auto-sent` event.
        id: None,
        user_note: Some(note),
        email: None,
    };
    let bundle = match error_reporter::build_bundle(&app, request).await {
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
    let size_bytes = capped.len();
    // Bound to a `let` rather than awaited in the `match` scrutinee: the scrutinee's temporaries
    // (including the borrow of `bundle.manifest`) would outlive the arms, and the success arm
    // moves the manifest into the stash.
    let uploaded = error_reporter::upload(capped, &bundle.manifest, &error_reporter::error_report_url()).await;
    match uploaded {
        Ok(result) => {
            log::info!(
                target: "cmdr_lib::error_reporter",
                "Auto-send: error report uploaded, id={}",
                result.id,
            );
            // Stash before the event: the toast the event raises can offer "see what was sent"
            // and "add a note" the instant it renders, and both read this.
            auto_sent::record(
                result.id.clone(),
                result.amend_key,
                auto_sent::AutoSentPreview {
                    size_bytes,
                    manifest: bundle.manifest,
                    sample_first: bundle.sample_first,
                    sample_last: bundle.sample_last,
                    total_redacted_lines: bundle.total_redacted_lines,
                },
            );
            // This bundle carries the session's log tail, so a panic logged before now went out
            // with it, and the next launch shouldn't offer the same panic a second time. THIS
            // line, not one higher up: everything above can fail, and only a landed upload means
            // the user actually heard about it. See `crash_reporter/survival.rs`.
            crate::crash_reporter::note_in_session_report_delivered();
            if let Err(e) = (ErrorReportAutoSent { id: result.id.clone() }).emit(&app) {
                log::warn!(
                    target: "cmdr_lib::error_reporter",
                    "Auto-send: succeeded but couldn't emit `error-report-auto-sent`: {e}",
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

/// Drains the debounce window, and hands it back only if it may go out now.
///
/// The `ENABLED` switch is seeded from the stored `updates.errorReports`; the organization's policy
/// is asked here, at send time, so a managed off wins over a stored on. A refused window is dropped,
/// like a failed upload (no retry, no queue).
async fn take_window_to_send() -> Option<DebounceState> {
    let state = {
        let mut guard = match STATE.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.take()
    }?;
    if crate::server_request::check_policy(crate::managed_policy::Egress::ErrorReport)
        .await
        .is_err()
    {
        return None;
    }
    Some(state)
}

/// Manifest note for an automatic report: the count, the first error's category, and its
/// message as a `detail=` field. `build_bundle` runs automatic notes through the report's
/// redaction context, which redacts and caps that field (`redact/DETAILS.md` § "External-text
/// fields"), so the note never ships more than the report's own logs do.
fn automatic_note(state: &DebounceState) -> String {
    let count = state.error_count;
    let plural = if count == 1 { "" } else { "s" };
    format!(
        "auto-send: {count} error{plural} within 60s, first: {} detail={:?}",
        state.first_category,
        cmdr_fs::log_detail::LogDetail(&state.first_message)
    )
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

/// Serializes every test that drives the dispatcher's statics.
///
/// `STATE` and `ENABLED` are process-global, so two such tests running in
/// parallel read each other's debounce window and both flake. Hold this across
/// any `set_enabled` / `reset_for_test` / `snapshot_for_test` sequence.
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn record_error_for_test(category: &str, message: &str) -> Option<Instant> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    record_error(category, message)
}

#[cfg(test)]
pub async fn take_window_to_send_for_test() -> Option<usize> {
    take_window_to_send().await.map(|state| state.error_count)
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
pub fn snapshot_for_test() -> Option<(usize, Instant)> {
    let guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard.as_ref().map(|s| (s.error_count, s.scheduled_send_at))
}

#[cfg(test)]
pub fn note_for_test() -> Option<String> {
    let guard = match STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard.as_ref().map(automatic_note)
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
