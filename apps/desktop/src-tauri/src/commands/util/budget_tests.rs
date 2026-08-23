//! Two budgets, two guarantees.
//!
//! [`BlockingBudget`]: however many callers pile in, only `permits` of them hold a
//! blocking-pool thread at a time, and every caller still gets its answer.
//! [`Deadline`]: a command with several legs answers within ONE wall clock, not
//! within the sum of whatever timeouts its legs happen to carry.

use super::{BlockingBudget, Deadline, DeadlineError, timeout_detached_within};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// Tracks how many tasks were inside the budget at once, so a test can assert the
/// PEAK rather than a sample that could miss the overlap.
#[derive(Default)]
struct Concurrency {
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl Concurrency {
    fn enter(&self) {
        let live = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(live, Ordering::SeqCst);
    }
    fn leave(&self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

static TWO_AT_A_TIME: BlockingBudget = BlockingBudget::new(2);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_never_exceeds_the_budget_and_everyone_still_finishes() {
    // Pre-fix, an unbounded `spawn_blocking` per call is exactly what let one command
    // take all 512 pool threads and freeze every other subsystem behind it. 32 callers
    // against 2 permits is that burst in miniature.
    const CALLERS: usize = 32;
    let seen = Arc::new(Concurrency::default());

    let mut calls = tokio::task::JoinSet::new();
    for i in 0..CALLERS {
        let seen = Arc::clone(&seen);
        calls.spawn(TWO_AT_A_TIME.run(move || {
            seen.enter();
            // allowed-test-sleep: the occupancy IS the subject — the task has to hold its
            // pool thread long enough for the other 31 callers to pile up behind it.
            std::thread::sleep(Duration::from_millis(5));
            seen.leave();
            i
        }));
    }
    let results = calls.join_all().await;

    let peak = seen.peak.load(Ordering::SeqCst);
    assert!(peak <= 2, "the budget caps pool occupancy at 2, saw {peak} at once");
    assert!(peak > 1, "the budget must not serialize what it's allowed to overlap");

    let mut done: Vec<usize> = results.into_iter().map(|r| r.expect("no task panicked")).collect();
    done.sort_unstable();
    assert_eq!(
        done,
        (0..CALLERS).collect::<Vec<_>>(),
        "a caller past the cap waits its turn; it is never dropped"
    );
}

// ── Deadline: one wall-clock budget across a command's legs ─────────────────

/// The second leg of a command gets what the first one LEFT, which is the whole
/// point: otherwise the command's promise is the sum of its legs.
#[tokio::test(start_paused = true)]
async fn a_deadline_hands_each_leg_what_is_left() {
    let deadline = Deadline::new(Duration::from_secs(30));
    assert_eq!(deadline.remaining(), Duration::from_secs(30));

    // allowed-test-sleep: elapsed time IS the subject, and `start_paused` makes it
    // virtual, so this advances the clock 20 s without costing the suite anything.
    tokio::time::sleep(Duration::from_secs(20)).await;

    assert_eq!(deadline.remaining(), Duration::from_secs(10));
    assert_eq!(deadline.elapsed(), Duration::from_secs(20));
}

/// A spent deadline reports nothing left rather than wrapping around, and the
/// leg that asks for it doesn't run.
#[tokio::test(start_paused = true)]
async fn a_spent_deadline_refuses_the_next_leg_outright() {
    let deadline = Deadline::new(Duration::from_secs(5));
    // allowed-test-sleep: virtual time again (`start_paused`), spending the budget
    // and then some, which is the condition under test.
    tokio::time::sleep(Duration::from_secs(9)).await;
    assert_eq!(deadline.remaining(), Duration::ZERO);

    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let outcome: Result<(), DeadlineError> = timeout_detached_within(
        &deadline,
        || DeadlineError::TimedOut,
        |detail| DeadlineError::Unexpected { detail },
        async move {
            flag.store(true, Ordering::SeqCst);
            Ok::<(), DeadlineError>(())
        },
    )
    .await;

    assert!(matches!(
        outcome.expect_err("nothing is left to wait with"),
        DeadlineError::TimedOut
    ));
    assert!(
        !ran.load(Ordering::SeqCst),
        "work that would be abandoned a moment later is never started"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panicking_task_frees_its_permit() {
    // A permit leaked by a panic would shrink the budget to nothing over a session, and
    // the command family would wedge for good — the failure this is here to prevent.
    static ONE_AT_A_TIME: BlockingBudget = BlockingBudget::new(1);

    assert!(
        ONE_AT_A_TIME.run(|| panic!("boom")).await.is_err(),
        "a panicking task reports the JoinError"
    );
    assert_eq!(
        ONE_AT_A_TIME.run(|| 7).await.expect("the permit came back"),
        7,
        "the next caller still gets in"
    );
}

/// Every command family's refusal is distinguishable BY VARIANT, with its data
/// intact, once it crosses the wire.
///
/// The families were flattened into `{ message, timed_out }` before, so a
/// `Busy` and an OS refusal arrived as two English sentences a caller could only
/// tell apart by reading them. These pin the shape the frontend matches on: one
/// `type` tag, camelCase fields, and no free-form text carrying the
/// classification.
#[cfg(test)]
mod wire_shape_tests {
    use super::DeadlineError;
    use crate::commands::file_system::VolumeScanError;
    use crate::file_system::listing::fuzzy_jump::FuzzyJumpError;
    use crate::file_system::volume::reconnect_error::ReconnectError;
    use serde_json::json;

    #[test]
    fn a_deadline_tells_a_timeout_from_a_panicked_task() {
        assert_eq!(
            serde_json::to_value(DeadlineError::TimedOut).unwrap(),
            json!({ "type": "timedOut" })
        );
        assert_eq!(
            serde_json::to_value(DeadlineError::Unexpected {
                detail: "the task panicked".to_string()
            })
            .unwrap(),
            json!({ "type": "unexpected", "detail": "the task panicked" })
        );
    }

    #[test]
    fn a_reconnect_refusal_carries_the_volumes_own_answer_rather_than_a_sentence() {
        let json = serde_json::to_value(ReconnectError::Volume {
            error: cmdr_fs::volume::VolumeError::NotSupported,
        })
        .unwrap();
        assert_eq!(json["type"], "volume");
        // The nested `VolumeError` stays a VALUE, so the frontend's existing
        // volume factory words it rather than a second vocabulary being invented.
        assert!(json["error"].is_object() || json["error"].is_string());
    }

    #[test]
    fn a_scan_refusal_names_which_side_went_missing() {
        assert_eq!(
            serde_json::to_value(VolumeScanError::SourceVolumeNotFound {
                volume_id: "mtp-1:65537".to_string()
            })
            .unwrap(),
            json!({ "type": "sourceVolumeNotFound", "volumeId": "mtp-1:65537" })
        );
        assert_eq!(
            serde_json::to_value(VolumeScanError::DestinationVolumeNotFound {
                volume_id: "smb-naspi".to_string()
            })
            .unwrap(),
            json!({ "type": "destinationVolumeNotFound", "volumeId": "smb-naspi" })
        );
    }

    #[test]
    fn a_lost_listing_says_which_one() {
        assert_eq!(
            serde_json::to_value(FuzzyJumpError::ListingNotFound {
                listing_id: "listing-7".to_string()
            })
            .unwrap(),
            json!({ "type": "listingNotFound", "listingId": "listing-7" })
        );
    }
}
