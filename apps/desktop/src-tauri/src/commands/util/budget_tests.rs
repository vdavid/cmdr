//! [`BlockingBudget`]'s one guarantee: however many callers pile in, only `permits` of
//! them hold a blocking-pool thread at a time — and every caller still gets its answer.

use super::BlockingBudget;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
            // Long enough that a genuinely unbounded pool would overlap far past 2.
            std::thread::sleep(std::time::Duration::from_millis(5));
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
