//! Coalescing coordinator tests (plan M2 TDD target): the pure `PassCoordinator`
//! contract, unit-tested without an app, a runtime, or the index fixtures.

use super::*;

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
