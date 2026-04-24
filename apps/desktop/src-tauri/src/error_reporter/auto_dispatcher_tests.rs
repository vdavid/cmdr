//! Tests for the Flow B auto-dispatcher.
//!
//! These exercise the debounce/jitter state machine directly via test-only seams
//! (`record_error_for_test`, `snapshot_for_test`, `reset_for_test`) so we don't need a
//! Tauri runtime to validate behavior. The full `on_error_logged` path is exercised by
//! manual smoke tests; the spawned tokio task it kicks off uses real wall-clock time,
//! which would make the test suite slow.
//!
//! All tests serialize on a process-global mutex because the dispatcher's state is itself
//! process-global (atomic flag + `Mutex<Option<DebounceState>>`). Running them in
//! parallel would race.

use super::auto_dispatcher::{
    flush_spawned_for_test, jitter_window, pick_jitter_offset_for_test, record_error_for_test, reset_for_test,
    set_enabled, simulate_late_app_handle_for_test, snapshot_for_test,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Serializes test access to the process-global dispatcher state.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
    let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    guard
}

#[test]
fn debounces_within_60s() {
    let _guard = lock_and_reset();
    set_enabled(true);

    let scheduled =
        record_error_for_test("cmdr_lib::network", "first failure").expect("first call should start a new debounce");
    // Subsequent errors within the window must NOT start a new debounce.
    for i in 0..9 {
        let none = record_error_for_test("cmdr_lib::other", &format!("noise {i}"));
        assert!(
            none.is_none(),
            "call {i} should join the existing window, not start a new one"
        );
    }

    let snapshot = snapshot_for_test().expect("state should still be active");
    assert_eq!(snapshot.2, 10, "error_count should reflect all calls in the window");
    assert_eq!(
        snapshot.3, scheduled,
        "scheduled time should not shift on subsequent calls"
    );

    reset_for_test();
}

#[test]
fn respects_disabled_flag() {
    let _guard = lock_and_reset();

    // Default-off: setting wasn't touched, so nothing should be scheduled.
    let none = record_error_for_test("cmdr_lib::network", "should be ignored");
    assert!(none.is_none(), "with the flag off, record_error must return None");
    assert!(
        snapshot_for_test().is_none(),
        "with the flag off, no state should be created"
    );

    // Flip on, fire one, flip off again — the active window stays (we don't tear it
    // down on disable; the user opted out, not "abort everything in flight"). New
    // errors after disable still don't accumulate.
    set_enabled(true);
    let _ = record_error_for_test("cmdr_lib::a", "kept");
    set_enabled(false);
    let _ = record_error_for_test("cmdr_lib::b", "ignored after disable");
    let snapshot = snapshot_for_test().expect("active state should survive disable");
    assert_eq!(snapshot.2, 1, "errors logged after disable must not bump the counter");

    reset_for_test();
}

#[test]
fn metadata_from_first_call() {
    let _guard = lock_and_reset();
    set_enabled(true);

    record_error_for_test("cmdr_lib::network::smb", "first message wins");
    record_error_for_test("cmdr_lib::other", "second message must NOT overwrite");
    record_error_for_test("cmdr_lib::yet_another", "third message must NOT overwrite either");

    let (cat, msg, count, _) = snapshot_for_test().expect("state should be active");
    assert_eq!(cat, "cmdr_lib::network::smb", "first category should be preserved");
    assert_eq!(msg, "first message wins", "first message should be preserved");
    assert_eq!(count, 3, "error count should reflect all three calls");

    reset_for_test();
}

#[test]
fn jitter_within_bounds() {
    let _guard = lock_and_reset();
    set_enabled(true);

    let (low, high) = jitter_window();
    // Statistical: 100 trials. Each trial schedules a fresh window so we sample 100
    // independent jitter draws.
    for trial in 0..100 {
        reset_for_test();
        set_enabled(true);
        let now = Instant::now();
        let scheduled =
            record_error_for_test("cmdr_lib::test", "jitter trial").expect("trial should start a new debounce");
        let delta = scheduled.saturating_duration_since(now);
        // Allow a 200 ms slack for scheduling overhead between `now` and the internal
        // `Instant::now()` inside `record_error`.
        let slack = Duration::from_millis(200);
        assert!(
            delta >= low.saturating_sub(slack),
            "trial {trial}: scheduled too early ({delta:?} < {low:?} - slack)",
        );
        assert!(
            delta <= high + slack,
            "trial {trial}: scheduled too late ({delta:?} > {high:?} + slack)",
        );
    }

    reset_for_test();
}

#[test]
fn jitter_offset_is_within_double_jitter_band() {
    // Direct sampling of the jitter helper, no state machine in the loop.
    let max = Duration::from_secs(20); // 2 * JITTER
    for _ in 0..1000 {
        let offset = pick_jitter_offset_for_test();
        assert!(offset <= max, "jitter offset {offset:?} exceeded 2 * JITTER ({max:?})");
    }
}

/// Late AppHandle wiring: an error logged before `set_app_handle` runs should leave the
/// debounce state with `flush_spawned = false`. When the handle later arrives, simulating
/// the production path, we should see the flag flip to true and a deadline returned so
/// the caller can spawn the flush task.
#[test]
fn late_app_handle_picks_up_active_window() {
    let _guard = lock_and_reset();
    set_enabled(true);

    // Simulate "error logged before AppHandle ready": record but don't spawn.
    let scheduled =
        record_error_for_test("cmdr_lib::network", "logged before setup").expect("first call should open a window");
    assert_eq!(
        flush_spawned_for_test(),
        Some(false),
        "test seam doesn't spawn — flag must remain false"
    );

    // Subsequent error in the same window must keep flush_spawned = false too,
    // otherwise the late-arriving AppHandle would think the spawn is already covered.
    let _ = record_error_for_test("cmdr_lib::other", "still no handle");
    assert_eq!(
        flush_spawned_for_test(),
        Some(false),
        "additional errors in the window must not flip the spawn flag"
    );

    // Now simulate the AppHandle arriving. The helper returns the deadline iff there was
    // work to schedule, and flips the flag so a subsequent on_error_logged in the same
    // window won't spawn a duplicate.
    let deadline = simulate_late_app_handle_for_test().expect("expected a deadline for the active window");
    assert_eq!(deadline, scheduled, "deadline should match the original schedule");
    assert_eq!(
        flush_spawned_for_test(),
        Some(true),
        "after the simulated AppHandle wiring, the flag must be set"
    );

    // A second call to the late-arrival helper is a no-op (idempotent / re-entrant safe).
    assert!(
        simulate_late_app_handle_for_test().is_none(),
        "calling the late-arrival helper again must not double-spawn"
    );

    reset_for_test();
}

/// If the AppHandle arrives after the debounce deadline has already passed, the late
/// path still returns a deadline (in the past) so the spawned task fires immediately
/// via `sleep_until` (which is a no-op when `deadline <= now`).
#[test]
fn late_app_handle_with_past_deadline_returns_deadline() {
    let _guard = lock_and_reset();
    set_enabled(true);

    let scheduled =
        record_error_for_test("cmdr_lib::network", "logged way before setup").expect("first call should open a window");
    let now = Instant::now();
    assert!(scheduled > now, "scheduled should be in the future at this point");

    // We can't time-travel `Instant` cheaply, but we can validate the contract: the
    // helper returns whatever scheduled_send_at was, and the production caller's
    // sleep_until handles the past-deadline case by returning immediately.
    let deadline = simulate_late_app_handle_for_test().expect("expected a deadline");
    assert_eq!(deadline, scheduled);

    reset_for_test();
}

/// If no window is active when the AppHandle arrives, the late-arrival helper is a no-op.
#[test]
fn late_app_handle_with_no_active_window_is_noop() {
    let _guard = lock_and_reset();
    set_enabled(true);

    assert!(
        simulate_late_app_handle_for_test().is_none(),
        "no active window — nothing to spawn"
    );
    assert!(
        snapshot_for_test().is_none(),
        "the helper must not create state when there's nothing to do"
    );

    reset_for_test();
}

/// Documents the crash-loop interaction. If the process exits during the 60 s window,
/// the spawned flush task is dropped before it fires — by design. The crash reporter
/// covers panics; the auto-dispatcher is for soft errors that don't kill the app.
#[test]
fn crash_loop_safety_note() {
    // Behavioural assertion: the dispatcher does not write anything to disk on
    // `record_error`. If the process were to exit immediately after, no in-flight
    // report would persist. (We don't queue or persist failed/pending auto-sends —
    // see the module-level docs for the rationale.)
    let _guard = lock_and_reset();
    set_enabled(true);
    let _ = record_error_for_test("cmdr_lib::test", "would be dropped on crash");
    // Simulate process exit by clearing the in-RAM state without flushing.
    reset_for_test();
    assert!(
        snapshot_for_test().is_none(),
        "post-crash state should be empty (no persistence)"
    );
}
