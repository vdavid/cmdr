//! Tests for `blocking_typed_result_until_stalled`: the waiter with no total deadline
//! that gives up on work only once it stops moving, and never drops it.

use std::sync::mpsc;
use std::time::Duration;

use super::{StallWatch, blocking_typed_result_until_stalled};

/// A watch that reports a fixed idleness and answers `give_up` with `verdict`,
/// optionally releasing the work at that moment.
struct FixedWatch {
    idle: Duration,
    verdict: Option<&'static str>,
    release_on_give_up: Option<mpsc::Sender<()>>,
    gave_up: bool,
}

impl StallWatch<&'static str> for FixedWatch {
    fn idle_for(&self) -> Duration {
        self.idle
    }

    fn on_poll(&mut self) {}

    fn give_up(&mut self) -> Option<&'static str> {
        self.gave_up = true;
        if let Some(release) = self.release_on_give_up.take() {
            let _ = release.send(());
        }
        self.verdict
    }
}

#[tokio::test]
async fn work_that_keeps_moving_returns_its_own_result() {
    let mut watch = FixedWatch {
        idle: Duration::ZERO,
        verdict: Some("stalled"),
        release_on_give_up: None,
        gave_up: false,
    };
    let result = blocking_typed_result_until_stalled(
        Duration::from_millis(50),
        &mut watch,
        |_| "join failed",
        || Ok::<_, &'static str>(7),
    )
    .await;
    assert_eq!(result, Ok(7));
    assert!(!watch.gave_up, "moving work is never given up on");
}

/// Idle past the limit, the waiter answers the watch's error without waiting for the
/// work, and the work keeps running to its own end: giving up detaches, it doesn't
/// drop, so a pull mid-transaction still reaches its cleanup.
#[tokio::test]
async fn idle_work_is_given_up_with_the_watchs_error_and_keeps_running() {
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (finished_tx, finished_rx) = mpsc::channel::<()>();
    let mut watch = FixedWatch {
        idle: Duration::from_secs(3600),
        verdict: Some("stalled"),
        release_on_give_up: None,
        gave_up: false,
    };

    let result = blocking_typed_result_until_stalled(
        Duration::from_millis(50),
        &mut watch,
        |_| "join failed",
        move || {
            release_rx.recv().expect("the test releases the work");
            finished_tx.send(()).expect("the test listens");
            Ok::<_, &'static str>(7)
        },
    )
    .await;

    assert_eq!(result, Err("stalled"));
    release_tx.send(()).expect("the work is still waiting");
    finished_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("given-up work runs to its own end");
}

/// When `give_up` finds the work already delivered, the waiter takes the work's real
/// result: the session it built belongs to its window, and answering an error instead
/// would strand it.
#[tokio::test]
async fn work_that_delivered_before_the_give_up_answers_its_result() {
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let mut watch = FixedWatch {
        idle: Duration::from_secs(3600),
        verdict: None,
        release_on_give_up: Some(release_tx),
        gave_up: false,
    };

    let result = blocking_typed_result_until_stalled(
        Duration::from_millis(50),
        &mut watch,
        |_| "join failed",
        move || {
            release_rx.recv().expect("give_up releases the work");
            Ok::<_, &'static str>(7)
        },
    )
    .await;

    assert!(watch.gave_up);
    assert_eq!(result, Ok(7));
}
