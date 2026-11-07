use std::path::PathBuf;

use super::{DropOutcome, ReceiveTally};
use crate::drop_in::events::PromiseDropFailure;

#[test]
fn the_last_callback_finishes_the_drop_once() {
    let mut tally = ReceiveTally::new(2);
    assert_eq!(tally.record(Some(PathBuf::from("/d/a.jpg"))), None);
    assert_eq!(
        tally.record(None),
        Some(DropOutcome::Ready {
            paths: vec![PathBuf::from("/d/a.jpg")],
            failed_count: 1
        })
    );
    assert_eq!(tally.record(Some(PathBuf::from("/d/late.jpg"))), None, "finished once");
    assert_eq!(tally.time_out(), None, "the watchdog can't finish it again");
}

#[test]
fn every_promise_failing_is_nothing_received() {
    let mut tally = ReceiveTally::new(1);
    assert_eq!(
        tally.record(None),
        Some(DropOutcome::Failed(PromiseDropFailure::NothingReceived))
    );
}

#[test]
fn the_watchdog_keeps_what_landed_and_counts_the_rest_as_failed() {
    let mut tally = ReceiveTally::new(3);
    tally.record(Some(PathBuf::from("/d/a.jpg")));
    assert_eq!(
        tally.time_out(),
        Some(DropOutcome::Ready {
            paths: vec![PathBuf::from("/d/a.jpg")],
            failed_count: 2
        })
    );
    assert_eq!(tally.record(Some(PathBuf::from("/d/b.jpg"))), None);
}

#[test]
fn the_watchdog_with_nothing_landed_is_a_timeout() {
    let mut tally = ReceiveTally::new(1);
    assert_eq!(
        tally.time_out(),
        Some(DropOutcome::Failed(PromiseDropFailure::TimedOut))
    );
}
