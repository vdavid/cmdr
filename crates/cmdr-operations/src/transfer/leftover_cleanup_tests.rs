//! The background pass's own rules. What a stop hands it, and how the stop
//! reports, is pinned where the stop happens (`volume/cleanup_tests.rs`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;
use crate::in_flight_temps::{Ledger, RecordHome};
use crate::outage_slot::{CleanupPolicy, OutagePolicy};
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::transfer::volume_outage::{OutageEnd, wait_while_out};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;

const CADENCE: &[Duration] = &[Duration::from_millis(10)];

/// ❗ An outage the operation already waited out (its fifteen-minute wait gave
/// up) gets ONE look from the cleanup, ❌ never a second wait: the pass makes
/// its one dial and hands the temp to the ledger, well inside its wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outage_the_operation_gave_up_on_gets_one_look_and_no_wait() {
    const VOLUME_ID: &str = "leftover-cleanup-test-gave-up";
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    let nas = OutageVolume::wrap(Arc::clone(&inner), "leftover-cleanup-test-gave-up");
    let volume: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-9b62");
    inner.create_file(&temp, b"half a photo").await.unwrap();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("leftover-cleanup-test-source".to_string(), VOLUME_ID.to_string())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(2),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_millis(100),
            })
            .with_cleanup_policy(CleanupPolicy {
                inline_budget: Duration::from_secs(5),
                wait: Duration::from_secs(60),
                retry_after: Duration::from_millis(20),
            }),
    );
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(VOLUME_ID)), &temp);
    nas.go_away();
    let ended = wait_while_out(&state, &[&volume], None).await;
    assert!(
        matches!(ended, OutageEnd::GaveUp { .. }),
        "the fixture must give up: {ended:?}"
    );
    let dialed_while_waiting = nas.reconnect_attempts();

    finish_in_background(
        &volume,
        &state,
        in_flight_temps::dest_home(&state),
        LeftBehind {
            out: vec![temp.clone()],
            refused: Vec::new(),
        },
    );
    tokio::time::timeout(Duration::from_secs(5), engine.leftover_cleanups_settled())
        .await
        .expect("one look, ❌ never the cleanup's full wait");

    assert_eq!(
        nas.reconnect_attempts(),
        dialed_while_waiting + 1,
        "the pass dials exactly once"
    );
    assert_eq!(
        ledger.pending_paths(),
        vec![temp.clone()],
        "and hands the temp to the reconnect sweep"
    );
    assert!(inner.exists(Path::new(&temp)).await);
}

/// ❗ A temp the share refuses the moment it's back gets the same retries as a
/// temp refused at the stop, and more than one. The session the outage dropped
/// still holds the temp open until the server notices it's gone, which on
/// Samba is a moment after the new session is up, and past a single 3 s retry
/// on a busy one (`STATUS_SHARING_VIOLATION`, `#578`). Without the retries the
/// temp sat in the ledger until the share's NEXT reconnect, while the stop had
/// said it goes once the share is back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_the_share_refuses_the_moment_it_is_back_goes_on_the_retries() {
    const VOLUME_ID: &str = "leftover-cleanup-test-held-by-the-dropped-session";
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    let nas = OutageVolume::wrap(Arc::clone(&inner), VOLUME_ID);
    let volume: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-5c17");
    inner.create_file(&temp, b"half a photo").await.unwrap();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("leftover-cleanup-test-source".to_string(), VOLUME_ID.to_string())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(60),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_millis(100),
            })
            .with_cleanup_policy(CleanupPolicy {
                inline_budget: Duration::from_secs(5),
                wait: Duration::from_secs(60),
                retry_after: Duration::from_millis(20),
            }),
    );
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(VOLUME_ID)), &temp);
    nas.go_away();
    // Refused the moment it's back, and on the first retry too.
    nas.refuse_deletes_of(&temp, 2);

    finish_in_background(
        &volume,
        &state,
        in_flight_temps::dest_home(&state),
        LeftBehind {
            out: vec![temp.clone()],
            refused: Vec::new(),
        },
    );
    wait_until_async(Duration::from_secs(5), "the pass to dial the share", || {
        nas.reconnect_attempts() >= 1
    })
    .await;
    nas.reconnected_elsewhere();
    tokio::time::timeout(Duration::from_secs(5), engine.leftover_cleanups_settled())
        .await
        .expect("the pass settles once the share is back");

    assert!(!inner.exists(Path::new(&temp)).await, "a later retry takes the temp");
    assert!(
        ledger.pending_paths().is_empty(),
        "❌ not left for the share's next reconnect"
    );
    assert!(!ledger.live_paths().contains(&temp), "its record retired");
}
