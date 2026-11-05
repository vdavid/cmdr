//! `blocking_typed_result_with_timeout_on`: the work runs on the HELD runtime's
//! blocking pool (not the caller's), and answers in the caller's type for each
//! of its three ends. The browser half (inline) runs in `cmdr-listing-wasm`'s
//! `rt_self_test.rs`.

use super::blocking_typed_result_with_timeout_on;
use tokio::time::Duration;

#[derive(Debug, PartialEq)]
enum Refusal {
    Own,
    TimedOut,
    Joined,
}

const HELD_THREAD: &str = "held-runtime-worker";

fn held_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name(HELD_THREAD)
        .enable_all()
        .build()
        .expect("a runtime builds")
}

#[tokio::test]
async fn work_runs_on_the_held_runtimes_pool() {
    let held = held_runtime();
    let thread = blocking_typed_result_with_timeout_on(
        held.handle(),
        Duration::from_secs(5),
        || Refusal::TimedOut,
        |_| Refusal::Joined,
        || Ok::<_, Refusal>(std::thread::current().name().map(str::to_owned)),
    )
    .await;
    assert_eq!(thread, Ok(Some(HELD_THREAD.to_owned())));
    held.shutdown_background();
}

#[tokio::test]
async fn the_works_own_refusal_crosses_unchanged() {
    let held = held_runtime();
    let answer = blocking_typed_result_with_timeout_on(
        held.handle(),
        Duration::from_secs(5),
        || Refusal::TimedOut,
        |_| Refusal::Joined,
        || Err::<(), _>(Refusal::Own),
    )
    .await;
    assert_eq!(answer, Err(Refusal::Own));
    held.shutdown_background();
}

#[tokio::test]
async fn an_overrun_answers_on_timeout() {
    let held = held_runtime();
    let answer = blocking_typed_result_with_timeout_on(
        held.handle(),
        Duration::from_millis(50),
        || Refusal::TimedOut,
        |_| Refusal::Joined,
        || {
            // allowed-test-sleep: this closure fakes slow blocking work; overrunning the
            // 50 ms timeout is what makes the helper answer `on_timeout`. 500 ms is a 10x
            // margin, and `thread::sleep` never returns early.
            std::thread::sleep(Duration::from_millis(500));
            Ok(())
        },
    )
    .await;
    assert_eq!(answer, Err(Refusal::TimedOut));
    // Detached, so the sleeping work isn't waited out here.
    held.shutdown_background();
}

#[tokio::test]
async fn a_panic_answers_on_join_failure() {
    let held = held_runtime();
    let answer = blocking_typed_result_with_timeout_on(
        held.handle(),
        Duration::from_secs(5),
        || Refusal::TimedOut,
        |_| Refusal::Joined,
        || -> Result<(), Refusal> { panic!("the work panicked") },
    )
    .await;
    assert_eq!(answer, Err(Refusal::Joined));
    held.shutdown_background();
}
