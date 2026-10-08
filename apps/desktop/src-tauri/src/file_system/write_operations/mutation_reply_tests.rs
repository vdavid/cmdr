//! The reply contract: an in-time answer is the answer, and one past the
//! deadline is `StillRunning` plus exactly one settle carrying the real end.

use std::time::Duration;

use tokio::sync::oneshot;

use super::*;

/// An `on_settled` that hands the event to the test, and the receiver to await it on.
fn settle_channel() -> (
    impl FnOnce(MutationSettled) + Send + 'static,
    oneshot::Receiver<MutationSettled>,
) {
    let (tx, rx) = oneshot::channel();
    (
        move |settled| {
            let _ = tx.send(settled);
        },
        rx,
    )
}

async fn after(delay: Duration) {
    // allowed-test-sleep: the fake latency IS the fixture (paused clock)
    tokio::time::sleep(delay).await;
}

#[tokio::test(start_paused = true)]
async fn an_answer_inside_the_deadline_is_the_reply_and_settles_nothing() {
    let (on_settled, rx) = settle_channel();
    let reply = reply_within(
        Duration::from_secs(2),
        async {
            after(Duration::from_millis(300)).await;
            Ok::<_, MutationError>("/docs/new")
        },
        on_settled,
    )
    .await;

    assert_eq!(reply.expect("landed in time"), MutationReply::Done);
    assert!(rx.await.is_err(), "an in-time reply never fires a settle");
}

#[tokio::test(start_paused = true)]
async fn a_refusal_inside_the_deadline_is_the_commands_own_error() {
    let (on_settled, _rx) = settle_channel();
    let reply = reply_within(
        Duration::from_secs(2),
        async { Err::<(), _>(MutationError::AlreadyExists { name: "docs".into() }) },
        on_settled,
    )
    .await;

    assert!(matches!(reply, Err(MutationError::AlreadyExists { .. })), "{reply:?}");
}

/// ERR-AREUV: the folder landed seconds after the dialog said it timed out.
/// Past the deadline the reply says "still running", never a failure, and the
/// settle reports the landing.
#[tokio::test(start_paused = true)]
async fn work_past_the_deadline_replies_still_running_then_settles_landed() {
    let (on_settled, rx) = settle_channel();
    let reply = reply_within(
        Duration::from_secs(2),
        async {
            after(Duration::from_secs(7)).await;
            Ok::<_, MutationError>(())
        },
        on_settled,
    )
    .await;

    let Ok(MutationReply::StillRunning { pending_id }) = reply else {
        panic!("a slow create is still running, not refused: {reply:?}");
    };
    let settled = rx.await.expect("the work settles");
    assert_eq!(settled.pending_id, pending_id);
    assert!(matches!(settled.outcome, MutationSettledOutcome::Landed), "{settled:?}");
}

#[tokio::test(start_paused = true)]
async fn a_refusal_past_the_deadline_settles_with_the_typed_reason() {
    let (on_settled, rx) = settle_channel();
    let reply = reply_within(
        Duration::from_secs(2),
        async {
            after(Duration::from_secs(7)).await;
            Err::<(), _>(MutationError::ParentNotWritable { path: "/docs".into() })
        },
        on_settled,
    )
    .await;

    assert!(matches!(reply, Ok(MutationReply::StillRunning { .. })), "{reply:?}");
    let settled = rx.await.expect("the work settles");
    assert!(
        matches!(
            settled.outcome,
            MutationSettledOutcome::Refused {
                error: MutationError::ParentNotWritable { .. }
            }
        ),
        "{settled:?}"
    );
}

/// A volume backend that panics mid-write.
fn fall_over() -> Result<(), MutationError> {
    panic!("the volume backend fell over")
}

#[tokio::test(start_paused = true)]
async fn a_panic_past_the_deadline_settles_as_unexpected() {
    let (on_settled, rx) = settle_channel();
    let reply = reply_within(
        Duration::from_secs(2),
        async {
            after(Duration::from_secs(7)).await;
            fall_over()
        },
        on_settled,
    )
    .await;

    assert!(matches!(reply, Ok(MutationReply::StillRunning { .. })), "{reply:?}");
    let settled = rx.await.expect("a panicked task still settles");
    assert!(
        matches!(
            settled.outcome,
            MutationSettledOutcome::Refused {
                error: MutationError::Unexpected { .. }
            }
        ),
        "{settled:?}"
    );
}
