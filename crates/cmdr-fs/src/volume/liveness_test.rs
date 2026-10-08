//! The silence ladder, on a paused clock with a probe that's a closure: no
//! server, no network, and every deadline exact.
//!
//! Every `sleep` here is VIRTUAL time on the paused clock: it advances the
//! clock by exactly that much, instantly, and the elapsed time is the subject.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::time::Instant;

use super::{Liveness, Timings, watch};

const T: Timings = Timings::PRODUCTION;

/// Probes that count themselves, and answer with what `answer(n)` says for
/// the `n`th (`None`: never answer at all, the black hole).
fn counted_probe(
    answer: impl Fn(usize) -> Option<bool> + Send + Sync + 'static,
) -> (
    Arc<AtomicUsize>,
    impl FnMut() -> std::pin::Pin<Box<dyn Future<Output = bool> + Send>>,
) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    let answer = Arc::new(answer);
    let probe = move || {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        let answer = Arc::clone(&answer);
        Box::pin(async move {
            match answer(n) {
                Some(answered) => answered,
                None => std::future::pending().await,
            }
        }) as std::pin::Pin<Box<dyn Future<Output = bool> + Send>>
    };
    (asked, probe)
}

/// Waits for the loss, with a virtual five-minute backstop so a watch that
/// never declares one fails here rather than hanging the runtime.
async fn lost_within_five_minutes(liveness: &Liveness) {
    tokio::time::timeout(Duration::from_secs(300), liveness.lost().cancelled())
        .await
        .expect("❗ five virtual minutes of unanswered probes and the server was never declared lost");
}

/// ❗ **Silence plus two unanswered probes is gone, at exactly 30 s**: 10 s
/// quiet, then two probes that each use their whole 10 s budget.
#[tokio::test(start_paused = true)]
async fn a_silent_server_is_declared_lost_at_the_deadline() {
    let liveness = Arc::new(Liveness::new());
    let waiting = liveness.begin();
    assert!(waiting.needs_a_watch(), "the first waiter starts the watch");
    let (asked, probe) = counted_probe(|_| None);
    let started = Instant::now();

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    lost_within_five_minutes(&liveness).await;

    assert_eq!(started.elapsed(), Duration::from_secs(30));
    assert_eq!(asked.load(Ordering::SeqCst), 2, "two probes, then it gave up");
}

/// ❗ **A server that answers its probes is never declared lost**, however long
/// the request it's busy with takes: this is the huge listing on a slow NAS.
#[tokio::test(start_paused = true)]
async fn a_server_that_answers_its_probes_is_never_lost() {
    let liveness = Arc::new(Liveness::new());
    let _waiting = liveness.begin();
    let (asked, probe) = counted_probe(|_| Some(true));

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    // allowed-test-sleep: virtual time on a paused clock; ten minutes of a busy server is the subject.
    tokio::time::sleep(Duration::from_secs(10 * 60)).await;

    assert!(
        !liveness.lost().is_cancelled(),
        "a server that answers is slow, not gone"
    );
    assert!(
        asked.load(Ordering::SeqCst) >= 50,
        "it kept asking, one probe per quiet stretch: {}",
        asked.load(Ordering::SeqCst)
    );
}

/// Bytes on the wire are the answer already: a server that keeps sending
/// (a slow download, a listing trickling in) is never even probed.
#[tokio::test(start_paused = true)]
async fn bytes_on_the_wire_keep_the_probe_from_going_out() {
    let liveness = Arc::new(Liveness::new());
    let _waiting = liveness.begin();
    let (asked, probe) = counted_probe(|_| None);

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    for _ in 0..24 {
        // allowed-test-sleep: virtual time on a paused clock; a byte every 5 s is the subject.
        tokio::time::sleep(Duration::from_secs(5)).await;
        liveness.heard();
    }

    assert!(!liveness.lost().is_cancelled());
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "nothing to ask a server that's talking"
    );
}

/// A byte arriving while a probe is out counts as the answer, so one late
/// probe doesn't push a talking server toward the limit.
#[tokio::test(start_paused = true)]
async fn a_byte_during_an_unanswered_probe_resets_the_count() {
    let liveness = Arc::new(Liveness::new());
    let _waiting = liveness.begin();
    let (asked, probe) = counted_probe(|_| None);
    let started = Instant::now();

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    // Probe 1 goes out at 10 s; a byte lands at 15 s, inside its budget.
    // allowed-test-sleep: virtual time on a paused clock; the byte's moment is the subject.
    tokio::time::sleep(Duration::from_secs(15)).await;
    liveness.heard();
    lost_within_five_minutes(&liveness).await;

    // Quiet again from 15 s: probe 2 at 25 s, probe 3 at 35 s, gone at 45 s.
    // Without the reset, probe 1's miss would have counted and it'd be 30 s.
    assert_eq!(started.elapsed(), Duration::from_secs(45));
    assert_eq!(asked.load(Ordering::SeqCst), 3);
}

/// Answered, unanswered, answered: the count is "in a ROW", so a server that
/// drops the odd probe on a busy link stays up.
#[tokio::test(start_paused = true)]
async fn an_answer_between_misses_starts_the_count_over() {
    let liveness = Arc::new(Liveness::new());
    let _waiting = liveness.begin();
    let (asked, probe) = counted_probe(|n| if n % 2 == 0 { None } else { Some(true) });

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    // allowed-test-sleep: virtual time on a paused clock; five minutes of alternating answers is the subject.
    tokio::time::sleep(Duration::from_secs(5 * 60)).await;

    assert!(!liveness.lost().is_cancelled());
    assert!(asked.load(Ordering::SeqCst) >= 10);
}

/// A probe refused outright (the port's closed) is as unanswered as a silent
/// one, and comes back at once, so the loss lands as soon as the quiet does.
#[tokio::test(start_paused = true)]
async fn a_refused_probe_counts_as_unanswered() {
    let liveness = Arc::new(Liveness::new());
    let _waiting = liveness.begin();
    let (_asked, probe) = counted_probe(|_| Some(false));
    let started = Instant::now();

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    lost_within_five_minutes(&liveness).await;

    assert_eq!(started.elapsed(), T.quiet);
}

/// ❗ **Nothing waiting, nothing probed.** An idle volume costs the server
/// nothing, and the watch ends with the last waiter.
#[tokio::test(start_paused = true)]
async fn nothing_waiting_means_nothing_probed() {
    let liveness = Arc::new(Liveness::new());
    let waiting = liveness.begin();
    let (asked, probe) = counted_probe(|_| None);

    let watcher = tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    drop(waiting);
    // allowed-test-sleep: virtual time on a paused clock; five idle minutes with no probe is the subject.
    tokio::time::sleep(Duration::from_secs(5 * 60)).await;

    assert!(watcher.is_finished(), "the watch ends with the last waiter");
    assert_eq!(asked.load(Ordering::SeqCst), 0);
    assert!(!liveness.lost().is_cancelled());
    assert!(
        liveness.begin().needs_a_watch(),
        "and the next waiter starts a new one rather than finding a stale flag"
    );
}

/// ❗ **The silence clock starts when somebody starts waiting.** An hour of
/// nobody asking anything is not an hour of the server being quiet.
#[tokio::test(start_paused = true)]
async fn idle_time_before_a_request_is_not_silence() {
    let liveness = Arc::new(Liveness::new());
    // allowed-test-sleep: virtual time on a paused clock; an idle hour before the request is the subject.
    tokio::time::sleep(Duration::from_secs(60 * 60)).await;
    let _waiting = liveness.begin();
    let (asked, probe) = counted_probe(|_| None);

    tokio::spawn(watch(Arc::clone(&liveness), T, "test", probe));
    // allowed-test-sleep: virtual time on a paused clock; stopping just short of the quiet stretch is the subject.
    tokio::time::sleep(T.quiet - Duration::from_secs(1)).await;

    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "the first probe waits a full quiet stretch"
    );
}

/// A second waiter joins the running watch rather than starting another.
#[tokio::test(start_paused = true)]
async fn one_watch_covers_every_waiter() {
    let liveness = Arc::new(Liveness::new());
    let first = liveness.begin();
    let second = liveness.begin();

    assert!(first.needs_a_watch());
    assert!(!second.needs_a_watch());
}
