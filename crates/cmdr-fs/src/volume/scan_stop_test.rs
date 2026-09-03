//! `ScanStop`: the one boundary a copy scan asks, and the ordering it owes.

use super::*;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[tokio::test]
async fn an_unarmed_stop_lets_every_walk_carry_on() {
    let stop = ScanStop::none();
    assert!(!stop.should_stop().await, "nothing can stop a scan with no owner");
    assert!(!stop.should_stop_blocking());
    assert!(!stop.is_armed());
}

#[tokio::test]
async fn a_live_owner_lets_the_walk_carry_on() {
    let signal = TestScanStop::new();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);
    assert!(!stop.should_stop().await);
    assert_eq!(signal.asks(), 1, "the cheap check is what an unpaused boundary costs");
}

#[tokio::test]
async fn a_stopping_owner_stops_the_walk_without_parking_it() {
    let signal = TestScanStop::already_stopping();
    signal.pause();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);
    // Paused AND stopping: the answer has to come back rather than park, or a
    // cancelled scan would sit on a gate nobody is going to open.
    let answered = tokio::time::timeout(Duration::from_secs(5), stop.should_stop())
        .await
        .expect("a stopping boundary must answer without parking");
    assert!(answered, "stop outranks pause");
}

#[tokio::test]
async fn a_paused_owner_holds_the_walk_until_it_resumes() {
    let signal = TestScanStop::new();
    signal.pause();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);

    let parked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let parked_in_task = Arc::clone(&parked);
    let waiter = tokio::spawn(async move {
        let answer = stop.should_stop().await;
        parked_in_task.store(true, Ordering::Release);
        answer
    });

    // allowed-test-sleep: the wait IS the subject — a park has nothing to poll
    // for, and the only way to show one is happening is to let time pass and find
    // the task still standing there.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !parked.load(Ordering::Acquire),
        "a paused walk must still be standing at its boundary"
    );

    signal.resume();
    let answer = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("resume must wake the parked walk")
        .expect("the waiter task doesn't panic");
    assert!(!answer, "a resumed walk carries on rather than stopping");
}

#[tokio::test]
async fn a_stop_landing_while_parked_is_answered_at_the_same_boundary() {
    let signal = TestScanStop::new();
    signal.pause();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);

    let waiter = tokio::spawn(async move { stop.should_stop().await });
    // allowed-test-sleep: the stop has to land while the task is ALREADY parked,
    // so it needs a head start; there's nothing observable to wait on.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Cancel, with the pause flag still set: the walk must not have to wait for
    // a resume that is never coming, and must not spend one more entry first.
    signal.stop();
    let answer = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("a cancel while parked must wake the walk")
        .expect("the waiter task doesn't panic");
    assert!(answer, "the stop is re-read after waking");
}

#[test]
fn the_blocking_boundary_answers_a_stop_that_lands_while_the_thread_is_parked() {
    let signal = TestScanStop::new();
    signal.pause();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);

    let worker = std::thread::spawn(move || stop.should_stop_blocking());
    // allowed-test-sleep: same head start, for the thread-parking twin — the stop
    // has to land while the worker is already inside its park.
    std::thread::sleep(Duration::from_millis(20));
    signal.stop();
    let answer = worker.join().expect("the worker thread doesn't panic");
    assert!(answer, "the blocking boundary honors a stop issued during a pause");
}

#[test]
fn debug_says_whether_a_stop_is_armed_without_claiming_more() {
    assert!(format!("{:?}", ScanStop::none()).contains("none"));
    let armed = ScanStop::new(TestScanStop::new() as Arc<dyn ScanStopSignal>);
    assert!(format!("{armed:?}").contains("armed"));
}
