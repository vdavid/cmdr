//! Coalescing coordinator tests (a TDD target): the pure `PassCoordinator`
//! contract, unit-tested without an app, a runtime, or an index — ported from
//! `importance`'s coalescing tests.

use super::*;

/// The core contract: a request while a pass runs does NOT start a second pass — it
/// sets the re-run flag, so the sweep + a concurrent `ScanCompleted` collapse to one
/// pass, then one re-run.
#[test]
fn concurrent_requests_coalesce_into_one_pass_plus_one_rerun() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
}

/// After a pass fully finishes (Done), the next request starts a fresh pass.
#[test]
fn a_new_request_after_done_starts_a_fresh_pass() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    assert_eq!(coord.request("root"), BeginOutcome::Start);
}

/// Two volumes are independent.
#[test]
fn coalescing_is_per_volume() {
    let coord = PassCoordinator::new();
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    assert_eq!(coord.request("smb-nas"), BeginOutcome::Start);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
    assert_eq!(coord.finish("smb-nas"), FinishOutcome::Done);
}

/// Item 1's double-kick is safe: the `start()`-time sweep kick and the `wire_volume`
/// registration kick both request the same volume, and a concurrent `ScanCompleted`
/// can pile on too. Whichever lands first starts the pass; the rest fold into ONE
/// re-run, never a second concurrent pass.
#[test]
fn a_kick_during_a_running_pass_folds_into_one_rerun() {
    let coord = PassCoordinator::new();
    // The first kick (sweep or registration) starts the pass.
    assert_eq!(coord.request("root"), BeginOutcome::Start);
    // The other kick + a concurrent scan-completion arrive mid-pass.
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    assert_eq!(coord.request("root"), BeginOutcome::Coalesced);
    // The pass runs exactly once more for the coalesced kicks, then idles.
    assert_eq!(coord.finish("root"), FinishOutcome::RunAgain);
    assert_eq!(coord.finish("root"), FinishOutcome::Done);
}

/// Only ONE re-run is buffered no matter how many requests pile up mid-pass.
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
