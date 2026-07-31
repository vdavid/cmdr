//! Coalescing coordinator tests (plan M2 TDD target): the pure `PassCoordinator`
//! contract, unit-tested without an app, a runtime, or the index fixtures.

use super::*;
use std::time::{Duration, Instant};

// ── Coalescing coordinator (plan M2 TDD target) ──────────────────────────

/// The core contract: a request while a pass runs does NOT start a second pass —
/// it sets the re-run flag, so the sweep + a concurrent `ScanCompleted` collapse
/// to one pass, then one re-run. This is the "sweep + concurrent completion ⇒ one
/// pass" guarantee (plan Decision 4), unit-tested without an app or a runtime.
#[test]
fn concurrent_requests_coalesce_into_one_pass_plus_one_rerun() {
    let coord = PassCoordinator::new();

    // First request starts a pass.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A second request while it runs coalesces (no second pass).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    // A third also coalesces onto the SAME pending re-run (not two re-runs).
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);

    // The pass finishes: because a request arrived mid-pass, run once more.
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    // That re-run finishes with nothing further pending ⇒ done.
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
}

/// After a pass fully finishes (Done), the next request starts a fresh pass — the
/// slot resets, so a later scan completion isn't wrongly coalesced.
#[test]
fn a_new_request_after_done_starts_a_fresh_pass() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    // A later completion starts a new pass, not a coalesce.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
}

/// Two volumes are independent: a pass running for one never coalesces the other.
#[test]
fn coalescing_is_per_volume() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // A different volume starts its own pass, not coalesced onto root's.
    assert_eq!(coord.request("smb-nas"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    assert_eq!(coord.finish("smb-nas"), FinishOutcome::Done);
}

/// Only ONE re-run is buffered no matter how many requests pile up mid-pass: the
/// re-run reruns once and then, with nothing new, is done. (A pathological event
/// storm can't queue N re-runs.)
#[test]
fn many_midpass_requests_buffer_exactly_one_rerun() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    for _ in 0..100 {
        assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    }
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    assert_eq!(coord.finish("root"), FinishOutcome::Done, "exactly one re-run, not 100");
}

// ── Incremental debounce (leading + trailing throttle) ────────────────────

/// The FIRST incremental of a burst runs immediately: a genuine edit scores
/// without waiting out a window (leading edge).
#[test]
fn debounce_first_pass_runs_immediately() {
    let now = Instant::now();
    assert_eq!(
        incremental_debounce_wait(None, now, Duration::from_secs(60)),
        Duration::ZERO,
        "no prior pass this run ⇒ run now (leading edge)"
    );
}

/// A second incremental that wants to run mid-window waits out the remainder, so
/// sustained change fires at most once per window (trailing edge, the throttle
/// guarantee — NOT a debounce that never fires under constant change).
#[test]
fn debounce_within_window_waits_the_remainder() {
    let window = Duration::from_secs(60);
    let started = Instant::now();
    // 20 s into the window ⇒ ~40 s left before the next pass may start.
    let now = started + Duration::from_secs(20);
    let wait = incremental_debounce_wait(Some(started), now, window);
    assert!(
        wait >= Duration::from_secs(39) && wait <= Duration::from_secs(40),
        "≈40 s remaining, got {wait:?}"
    );
}

/// Once the window has fully elapsed since the last pass started, the next runs
/// immediately — the throttle spaces passes, it never stalls a lone late change.
#[test]
fn debounce_after_window_runs_immediately() {
    let window = Duration::from_secs(60);
    let started = Instant::now();
    let now = started + Duration::from_secs(90);
    assert_eq!(
        incremental_debounce_wait(Some(started), now, window),
        Duration::ZERO,
        "window elapsed ⇒ no wait"
    );
}
