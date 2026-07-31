//! A long-lived, bounded pool of 8 MB-stack OS threads for synchronous macOS
//! framework calls.
//!
//! See `file_system/CLAUDE.md`: NSURL / File Provider lookups make synchronous XPC
//! round-trips through provider override chains, so they need a big stack and must
//! never run on rayon (2 MB workers) or on tokio's blocking pool (which the runtime
//! also needs for real I/O). This pool gives them dedicated threads that are
//! **spawned once** instead of per call.
//!
//! ## Why a ceiling AND a target
//!
//! An XPC call into a wedged provider never returns, so a worker can be lost for
//! the process's lifetime. A plain fixed-size pool would therefore die permanently
//! the first time a provider hangs, and an unbounded pool is the leak we're fixing.
//! So: grow lazily to `target_workers` while everyone is busy, treat a worker that
//! has been on one job longer than `wedged_after` as lost, replace it, and never
//! exceed `max_workers` threads for the pool's whole lifetime. The leak is bounded
//! by construction, and a transient provider hang doesn't disable the feature.

use cmdr_fs::ignore_poison::IgnorePoison;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// 8 MB stack per thread: enough for deep FileProvider XPC call chains.
const THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// How a [`Pool`] is sized. Every field is explicit so tests can build a pool with
/// timings they can actually wait for.
#[derive(Clone, Copy, Debug)]
pub(super) struct PoolConfig {
    /// Thread-name prefix, so `sample` and Instruments name the wedged threads.
    pub name: &'static str,
    /// Worker count the pool grows to while every existing worker is busy.
    pub target_workers: usize,
    /// Hard ceiling on threads ever spawned. Never exceeded, whatever happens.
    pub max_workers: usize,
    /// A worker on the same job for longer than this counts as lost, so a
    /// replacement may be spawned (still within `max_workers`).
    pub wedged_after: Duration,
}

pub(super) struct Pool {
    inner: Arc<Inner>,
}

struct Inner {
    config: PoolConfig,
    state: Mutex<State>,
    /// Signalled when a job is queued.
    work_ready: Condvar,
}

struct State {
    jobs: VecDeque<Job>,
    /// One slot per spawned worker: when its current job started, `None` when idle.
    /// Length is the number of threads ever spawned; workers never exit.
    busy_since: Vec<Option<Instant>>,
}

impl Pool {
    pub(super) fn new(config: PoolConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                state: Mutex::new(State {
                    jobs: VecDeque::new(),
                    busy_since: Vec::new(),
                }),
                work_ready: Condvar::new(),
            }),
        }
    }

    /// Queues `job`, spawning a worker first if every existing one is busy and the
    /// ceiling allows it.
    pub(super) fn submit(&self, job: Job) {
        let worker_index = {
            let mut state = self.inner.state.lock_ignore_poison();
            state.jobs.push_back(job);
            self.inner.needs_worker(&state).then(|| {
                state.busy_since.push(None);
                state.busy_since.len() - 1
            })
        };
        self.inner.work_ready.notify_one();
        if let Some(index) = worker_index {
            self.inner.spawn_worker(index);
        }
    }

    /// Threads this pool has ever spawned. They never exit, so this is also the
    /// live count.
    pub(super) fn worker_count(&self) -> usize {
        self.inner.state.lock_ignore_poison().busy_since.len()
    }

    /// Workers currently inside a job.
    #[cfg(test)]
    pub(super) fn busy_count(&self) -> usize {
        self.inner
            .state
            .lock_ignore_poison()
            .busy_since
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }

    /// Workers that have been on the same job longer than `wedged_after`, so
    /// presumed lost inside a provider that never answered.
    pub(super) fn wedged_count(&self) -> usize {
        let state = self.inner.state.lock_ignore_poison();
        self.inner.wedged_count(&state)
    }

    /// Jobs queued but not yet picked up.
    pub(super) fn queue_len(&self) -> usize {
        self.inner.state.lock_ignore_poison().jobs.len()
    }
}

impl Inner {
    fn wedged_count(&self, state: &State) -> usize {
        state
            .busy_since
            .iter()
            .filter(|slot| slot.is_some_and(|started| started.elapsed() >= self.config.wedged_after))
            .count()
    }

    /// True when the queued job has nobody to run it soon and the ceiling allows
    /// one more thread.
    fn needs_worker(&self, state: &State) -> bool {
        if state.busy_since.len() >= self.config.max_workers {
            return false;
        }
        // An idle worker will pick the job up on the `notify_one` below; growing
        // past what the load needs is exactly the waste we're removing.
        if state.busy_since.iter().any(Option::is_none) {
            return false;
        }
        let healthy = state.busy_since.len() - self.wedged_count(state);
        healthy < self.config.target_workers
    }

    fn spawn_worker(self: &Arc<Self>, index: usize) {
        let inner = Arc::clone(self);
        let name = format!("{}-{index}", self.config.name);
        let spawned = std::thread::Builder::new()
            .stack_size(THREAD_STACK_SIZE)
            .name(name.clone())
            .spawn(move || inner.work_loop(index));
        if let Err(err) = spawned {
            // Losing the slot would permanently over-count workers and starve the
            // pool, so hand it back and let the next submit try again.
            let mut state = self.state.lock_ignore_poison();
            state.busy_since.truncate(index);
            log::warn!(target: "sync_status", "could not spawn worker {name}: {err}");
        }
    }

    fn work_loop(&self, index: usize) {
        loop {
            let job = {
                let mut state = self.state.lock_ignore_poison();
                let job = loop {
                    if let Some(job) = state.jobs.pop_front() {
                        break job;
                    }
                    state = self
                        .work_ready
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                };
                state.busy_since[index] = Some(Instant::now());
                job
            };
            job();
            self.state.lock_ignore_poison().busy_since[index] = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::wait_until;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    const WAIT: Duration = Duration::from_secs(5);

    fn config(target: usize, max: usize) -> PoolConfig {
        PoolConfig {
            name: "test-pool",
            target_workers: target,
            max_workers: max,
            wedged_after: Duration::from_millis(50),
        }
    }

    /// Every submitted job runs, and its result comes back.
    #[test]
    fn runs_every_submitted_job() {
        let pool = Pool::new(config(4, 8));
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..20 {
            let done = Arc::clone(&done);
            pool.submit(Box::new(move || {
                done.fetch_add(1, Ordering::SeqCst);
            }));
        }
        wait_until(WAIT, "all 20 jobs ran", || done.load(Ordering::SeqCst) == 20);
    }

    /// The whole point of the pool: a burst of jobs, several times over, must not
    /// keep spawning threads. Pre-fix, `std::thread::scope` spawned a fresh set per
    /// call, which is how 21-23 8 MB-stack threads piled up in the incident.
    #[test]
    fn thread_count_stays_bounded_across_repeated_bursts() {
        let pool = Pool::new(config(4, 8));
        for _ in 0..10 {
            let done = Arc::new(AtomicUsize::new(0));
            for _ in 0..50 {
                let done = Arc::clone(&done);
                pool.submit(Box::new(move || {
                    done.fetch_add(1, Ordering::SeqCst);
                }));
            }
            wait_until(WAIT, "the burst finished", || done.load(Ordering::SeqCst) == 50);
        }
        assert!(
            pool.worker_count() <= 8,
            "500 jobs over 10 bursts spawned {} threads, ceiling is 8",
            pool.worker_count()
        );
    }

    /// A provider that never answers must not cost unbounded threads: the pool
    /// replaces the workers it presumes lost, then stops at the ceiling.
    #[test]
    fn never_exceeds_the_ceiling_when_every_job_wedges() {
        let pool = Pool::new(config(2, 5));
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let started = Arc::new(AtomicUsize::new(0));

        for _ in 0..30 {
            let release_rx = Arc::clone(&release_rx);
            let started = Arc::clone(&started);
            pool.submit(Box::new(move || {
                started.fetch_add(1, Ordering::SeqCst);
                // Blocks until the test releases it: our stand-in for an XPC call
                // into a File Provider that never replies.
                let _ = release_rx.lock_ignore_poison().recv();
            }));
        }

        // Give the pool every chance to over-spawn: each submit re-evaluates, and
        // past `wedged_after` all busy workers count as lost.
        wait_until(WAIT, "a worker is presumed lost", || pool.wedged_count() > 0);
        for _ in 0..30 {
            pool.submit(Box::new(|| {}));
        }
        assert!(
            pool.worker_count() <= 5,
            "60 wedging jobs spawned {} threads, ceiling is 5",
            pool.worker_count()
        );

        drop(release_tx);
        wait_until(WAIT, "every wedging job was released", || pool.busy_count() == 0);
        assert!(started.load(Ordering::SeqCst) > 0, "at least one wedging job started");
    }

    /// A worker presumed lost is replaced, so a transient provider hang doesn't
    /// disable the pool for the rest of the session.
    #[test]
    fn replaces_a_wedged_worker() {
        let pool = Pool::new(config(1, 4));
        let (release_tx, release_rx) = mpsc::channel::<()>();
        pool.submit(Box::new(move || {
            let _ = release_rx.recv();
        }));
        wait_until(WAIT, "the only worker picked up the job", || pool.busy_count() == 1);
        wait_until(WAIT, "the only worker is presumed lost", || pool.wedged_count() == 1);

        // With the only worker presumed lost, a new job must still get served.
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_in_job = Arc::clone(&ran);
        pool.submit(Box::new(move || {
            ran_in_job.fetch_add(1, Ordering::SeqCst);
        }));
        wait_until(WAIT, "the replacement worker ran the job", || {
            ran.load(Ordering::SeqCst) == 1
        });
        assert_eq!(pool.worker_count(), 2, "the lost worker was replaced, not duplicated");

        drop(release_tx);
    }

    /// One job at a time needs one thread, not `available_parallelism()` of them.
    #[test]
    fn a_single_job_does_not_fan_out() {
        let pool = Pool::new(config(4, 8));
        let done = Arc::new(AtomicUsize::new(0));
        let done_in_job = Arc::clone(&done);
        pool.submit(Box::new(move || {
            done_in_job.fetch_add(1, Ordering::SeqCst);
        }));
        wait_until(WAIT, "the single job ran", || done.load(Ordering::SeqCst) == 1);
        assert_eq!(pool.worker_count(), 1);
        assert_eq!(pool.queue_len(), 0);
    }
}
