//! The gathering engine: run a card's jobs somewhere else, wait a short grace for
//! them, then route every later answer down a late sink.
//!
//! The shape of the context menu's slow-fact engine, generic over the job key
//! and the answer. It knows nothing about a thread pool or an OS: the host
//! injects `submit`, so the grace-then-late routing is tested with plain threads.
//!
//! A worker stuck in a framework call is lost, never interrupted. So a deadline
//! here bounds how long the collector waits, never how long a job works; what
//! bounds the threads a dead mount can hold is the host pool's ceiling.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

/// A job: work that produces one answer.
pub type Job<F> = Box<dyn FnOnce() -> F + Send + 'static>;

/// What `submit` receives: one job wrapped with its delivery, to run anywhere
/// but on the caller's thread.
pub type Task = Box<dyn FnOnce() + Send + 'static>;

/// Where an answer goes once the collector stopped waiting.
type LateSink<K, F> = Arc<dyn Fn(K, F) + Send + Sync + 'static>;

/// Answers in flight for one request.
pub struct Gathering<K, F> {
    shared: Arc<Shared<K, F>>,
}

struct Shared<K, F> {
    state: Mutex<State<K, F>>,
    arrived: Condvar,
    /// Set once the request is gone: jobs still queued skip their work, and late
    /// answers drop.
    cancelled: Arc<AtomicBool>,
}

struct State<K, F> {
    /// Asked and not yet answered.
    pending: BTreeSet<K>,
    /// Answered while the collector was still waiting.
    ready: Vec<(K, F)>,
    /// `None` while the collector waits; the late route once it stopped.
    late: Option<LateSink<K, F>>,
}

impl<K, F> Shared<K, F> {
    /// A job that panicked while delivering poisons nothing worth refusing over:
    /// the state is a set and two lists, valid at every step.
    fn lock(&self) -> MutexGuard<'_, State<K, F>> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Cancels a gathering's unfinished work when the request that asked is gone.
#[derive(Clone)]
pub struct CancelGathering(Arc<AtomicBool>);

impl CancelGathering {
    /// Stops it: a job that hasn't started skips its work, and an answer still
    /// on its way is dropped.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Whether it was cancelled. A long job reads this between its steps.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// What the collector has when it stops waiting.
pub struct Collected<K, F> {
    /// Every answer that arrived in time, in arrival order.
    pub ready: Vec<(K, F)>,
    /// The jobs still out, each answered later through the late sink (or never,
    /// when the request is cancelled first).
    pub pending: BTreeSet<K>,
    /// Cancels what's still out.
    pub cancel: CancelGathering,
}

/// Starts `jobs` through `submit`, which runs each one somewhere other than the
/// caller. Each key names one job.
pub fn start_with<K, F>(jobs: Vec<(K, Job<F>)>, submit: impl Fn(Task)) -> Gathering<K, F>
where
    K: Ord + Copy + Send + 'static,
    F: Send + 'static,
{
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            pending: jobs.iter().map(|(key, _)| *key).collect(),
            ready: Vec::new(),
            late: None,
        }),
        arrived: Condvar::new(),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    for (key, job) in jobs {
        let shared = Arc::clone(&shared);
        submit(Box::new(move || {
            // A job queued behind a mount that stopped answering may only start after
            // its request is gone; its answer would go nowhere, so it doesn't ask at all.
            if shared.cancelled.load(Ordering::Acquire) {
                return;
            }
            let answer = job();
            deliver(&shared, key, answer);
        }));
    }
    Gathering { shared }
}

/// Routes one answer: to the waiting collector, or down the late route once it
/// stopped waiting.
fn deliver<K: Ord, F>(shared: &Shared<K, F>, key: K, answer: F) {
    let mut state = shared.lock();
    state.pending.remove(&key);
    match state.late.clone() {
        None => {
            state.ready.push((key, answer));
            drop(state);
            shared.arrived.notify_all();
        }
        Some(late) => {
            drop(state);
            if !shared.cancelled.load(Ordering::Acquire) {
                late(key, answer);
            }
        }
    }
}

impl<K: Ord + Copy, F> Gathering<K, F> {
    /// Cancels the gathering before anyone collected it.
    pub fn cancel_handle(&self) -> CancelGathering {
        CancelGathering(Arc::clone(&self.shared.cancelled))
    }

    /// Waits until every job answered or `deadline` passed, whichever comes
    /// first, and hands every answer after that to `late`, on the thread that
    /// produced it.
    pub fn collect(self, deadline: Instant, late: impl Fn(K, F) + Send + Sync + 'static) -> Collected<K, F> {
        let mut state = self.shared.lock();
        while !state.pending.is_empty() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            state = self
                .shared
                .arrived
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        // Flipped under the same lock the jobs deliver under, so every answer lands in
        // exactly one place: `ready` before this line, the late route after it.
        state.late = Some(Arc::new(late));
        Collected {
            ready: std::mem::take(&mut state.ready),
            pending: state.pending.clone(),
            cancel: CancelGathering(Arc::clone(&self.shared.cancelled)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// Far longer than any step here takes, so a test that waits this long has failed.
    const LONG: Duration = Duration::from_secs(10);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Fast,
        Slow,
    }

    fn on_threads(task: Task) {
        std::thread::spawn(task);
    }

    fn instant(key: Key, answer: &'static str) -> (Key, Job<&'static str>) {
        (key, Box::new(move || answer))
    }

    /// A job that says when it started and answers only once released.
    fn held(key: Key, answer: &'static str) -> ((Key, Job<&'static str>), mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let job: Job<&'static str> = Box::new(move || {
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(LONG);
            answer
        });
        ((key, job), started_rx, release_tx)
    }

    #[test]
    fn answers_inside_the_grace_are_ready_and_nothing_is_pending() {
        let gathering = start_with(vec![instant(Key::Fast, "a"), instant(Key::Slow, "b")], on_threads);
        let mut collected = gathering.collect(Instant::now() + LONG, |_, _| {});
        collected.ready.sort();
        assert_eq!(collected.ready, [(Key::Fast, "a"), (Key::Slow, "b")]);
        assert!(collected.pending.is_empty());
    }

    #[test]
    fn the_wait_ends_at_the_deadline_and_a_slow_answer_goes_down_the_late_route() {
        let (slow, started, release) = held(Key::Slow, "late");
        let gathering = start_with(vec![instant(Key::Fast, "soon"), slow], on_threads);
        started.recv_timeout(LONG).expect("the held job starts");
        let began = Instant::now();
        let (late_tx, late_rx) = mpsc::channel();
        let collected = gathering.collect(Instant::now() + Duration::from_millis(50), move |key, answer| {
            let _ = late_tx.send((key, answer));
        });
        assert!(
            began.elapsed() < LONG,
            "the wait must end at the deadline, not when the slow job answers"
        );
        assert!(collected.pending.contains(&Key::Slow));
        assert!(collected.ready.iter().all(|(key, _)| *key == Key::Fast));
        release.send(()).expect("the held job is waiting");
        // On a loaded machine the fast job may answer after the deadline too; either
        // way it arrives exactly once, and the slow one arrives late.
        let mut late = Vec::new();
        while !late.contains(&(Key::Slow, "late")) {
            late.push(late_rx.recv_timeout(LONG).expect("the slow answer arrives late"));
        }
        let fast_answers = collected.ready.len() + late.iter().filter(|(key, _)| *key == Key::Fast).count();
        assert_eq!(fast_answers, 1);
    }

    /// Once the request is gone, a late answer goes nowhere, and a job that hadn't
    /// started yet doesn't do its work at all.
    #[test]
    fn a_cancelled_gathering_drops_late_answers_and_skips_queued_work() {
        let (slow, started, release) = held(Key::Slow, "late");
        let gathering = start_with(vec![slow], on_threads);
        let (late_tx, late_rx) = mpsc::channel();
        let collected = gathering.collect(Instant::now(), move |key, answer| {
            let _ = late_tx.send((key, answer));
        });
        // Cancel only once the job is RUNNING: its thread may start after the cancel, and
        // then it rightly skips its work (the second half below).
        started.recv_timeout(LONG).expect("the held job starts");
        collected.cancel.cancel();
        assert!(collected.cancel.is_cancelled());
        release.send(()).expect("the held job is waiting");
        // The late route is the last owner of `late_tx`: once the job has answered and
        // the gathering is gone, the channel closes, so "nothing arrived" needs no wait.
        drop(collected);
        assert_eq!(
            late_rx.recv_timeout(LONG),
            Err(mpsc::RecvTimeoutError::Disconnected),
            "a late answer for a closed card must go nowhere"
        );

        // A job that only starts after the cancel never runs.
        let ran = Arc::new(AtomicBool::new(false));
        let ran_in_job = Arc::clone(&ran);
        let (queued, run_queued) = mpsc::channel::<Task>();
        let job: Job<&'static str> = Box::new(move || {
            ran_in_job.store(true, Ordering::SeqCst);
            "never"
        });
        let gathering = start_with(vec![(Key::Fast, job)], |task| {
            queued.send(task).expect("the queue is open");
        });
        gathering.cancel_handle().cancel();
        (run_queued.recv().expect("one job was queued"))();
        assert!(!ran.load(Ordering::SeqCst), "work for a closed card must be skipped");
    }

    /// Every answer lands in exactly one place, whichever side of the deadline it
    /// falls on.
    #[test]
    fn no_answer_is_lost_or_delivered_twice_around_the_deadline() {
        for _ in 0..200 {
            let gathering = start_with(vec![instant(Key::Fast, "a"), instant(Key::Slow, "b")], on_threads);
            let (late_tx, late_rx) = mpsc::channel();
            let collected = gathering.collect(Instant::now(), move |key, _| {
                let _ = late_tx.send(key);
            });
            let mut seen: Vec<Key> = collected.ready.iter().map(|(key, _)| *key).collect();
            for _ in 0..collected.pending.len() {
                seen.push(late_rx.recv_timeout(LONG).expect("a pending job answers late"));
            }
            seen.sort();
            assert_eq!(seen, [Key::Fast, Key::Slow]);
        }
    }
}
