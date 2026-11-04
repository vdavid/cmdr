//! A keep-alive pool of dedicated `Utility`-QoS threads, for background work that
//! would otherwise spawn a fresh OS thread per job.
//!
//! ## Why it exists
//!
//! The index walker ran every walk on threads of its own and the rescan drain ran
//! every subtree reconcile on one: up to ~20,000 thread creations in half an hour on
//! a busy machine, each a `clone`, a stack mapping, and an allocator thread setup. A
//! pool turns that stream into a handful of long-lived threads. It's hygiene, not a
//! memory fix: pooling the walker left mimalloc's slack and the idle CPU where they
//! were (`docs/notes/performance/walker-thread-pool-2026-09-27.md`).
//!
//! ## What it promises
//!
//! - **A job never queues behind another job.** [`UtilityPool::execute`] hands the
//!   job to an idle thread, or spawns one. The pool is unbounded on purpose: its
//!   callers bound their own concurrency (a walk's `num_threads`, the drain's
//!   single flight), and a job that parks for a whole walk must not keep another
//!   walk's worker from starting.
//! - **An idle thread exits after `keep_alive`**, so a quiet app holds no threads
//!   for work it isn't doing. The most recently idled thread is reused first, which
//!   lets the rest age out when demand drops.
//! - **Every thread is `Utility` for its whole life.** That's what makes pooling
//!   compatible with `thread_qos`'s rule against lowering a shared thread: the
//!   lowered class can't leak onto unrelated work, because every job a pool runs
//!   asked for it.
//! - **Nothing leaks from one job to the next** except thread-locals. The job is
//!   dropped (with everything it captured) before the thread goes idle, so a pooled
//!   thread holds no `Arc` a job gave it.
//!
//! A job that panics unwinds its thread only, as a dedicated thread would.
//!
//! ## How a job reaches a thread
//!
//! Each idle thread parks on its own [`Handoff`] and is listed in `idle`. `execute`
//! pops one and writes the job into its slot while still holding the `idle` lock,
//! so a thread that finds itself no longer listed knows its job is already there.
//! That's what makes the race between a keep-alive timeout and a hand-off safe
//! without a second counter.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use crate::thread_qos::{QosClass, set_current_thread_qos};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// One idle thread's mailbox. `execute` fills `job` and signals `ready`.
// DEFAULT-OK: an empty mailbox is the true starting state; it carries no fact about the disk.
#[derive(Default)]
struct Handoff {
    job: Mutex<Option<Job>>,
    ready: Condvar,
}

/// A keep-alive pool of dedicated `Utility`-QoS threads. See the module docs.
///
/// Built `const` so it can be a `static`; [`Self::execute`] takes `&'static self`
/// because its threads outlive any borrow.
pub struct UtilityPool {
    name: &'static str,
    stack_size: usize,
    keep_alive: Duration,
    /// Parked threads, most recently idled last (popped first).
    idle: Mutex<VecDeque<Arc<Handoff>>>,
    threads_started: AtomicU64,
}

impl UtilityPool {
    /// A pool whose threads are named `name`, get `stack_size` bytes of stack, and
    /// exit after `keep_alive` without work.
    pub const fn new(name: &'static str, stack_size: usize, keep_alive: Duration) -> Self {
        Self {
            name,
            stack_size,
            keep_alive,
            idle: Mutex::new(VecDeque::new()),
            threads_started: AtomicU64::new(0),
        }
    }

    /// Run `job` on a pooled thread: an idle one if there is one, otherwise a new
    /// one. Returns the spawn error when a new thread was needed and couldn't be
    /// created; the job is dropped unrun then.
    pub fn execute<F>(&'static self, job: F) -> std::io::Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        let job: Job = Box::new(job);
        {
            let mut idle = self.idle.lock_ignore_poison();
            if let Some(handoff) = idle.pop_back() {
                // Written under the `idle` lock: see the module docs.
                *handoff.job.lock_ignore_poison() = Some(job);
                handoff.ready.notify_one();
                return Ok(());
            }
        }
        std::thread::Builder::new()
            .name(self.name.into())
            .stack_size(self.stack_size)
            .spawn(move || {
                set_current_thread_qos(QosClass::Utility);
                self.run(job);
            })?;
        self.threads_started.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// How many OS threads this pool has created since the process started.
    pub fn threads_started(&self) -> u64 {
        self.threads_started.load(Ordering::Relaxed)
    }

    fn run(&self, first: Job) {
        let handoff = Arc::new(Handoff::default());
        let mut job = first;
        loop {
            job();
            match self.next_job(&handoff) {
                Some(next) => job = next,
                None => return,
            }
        }
    }

    /// Park until a job arrives or `keep_alive` passes. `None` means this thread
    /// took itself off the idle list and should exit.
    fn next_job(&self, handoff: &Arc<Handoff>) -> Option<Job> {
        self.idle.lock_ignore_poison().push_back(Arc::clone(handoff));
        let deadline = crate::rt::now() + self.keep_alive;
        let mut slot = handoff.job.lock_ignore_poison();
        loop {
            if let Some(job) = slot.take() {
                return Some(job);
            }
            let now = crate::rt::now();
            if now >= deadline {
                break;
            }
            slot = handoff
                .ready
                .wait_timeout(slot, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        drop(slot);
        let mut idle = self.idle.lock_ignore_poison();
        if let Some(at) = idle.iter().position(|h| Arc::ptr_eq(h, handoff)) {
            idle.remove(at);
            return None;
        }
        drop(idle);
        // `execute` claimed this thread between the timeout and the `idle` lock, so
        // its job is already in the slot.
        handoff.job.lock_ignore_poison().take()
    }
}

impl std::fmt::Debug for UtilityPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UtilityPool")
            .field("name", &self.name)
            .field("threads_started", &self.threads_started())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::mpsc;

    fn leaked(keep_alive: Duration) -> &'static UtilityPool {
        Box::leak(Box::new(UtilityPool::new("utility-pool-test", 64 * 1024, keep_alive)))
    }

    /// Runs `job` on `pool` and waits for it, returning the thread it ran on.
    fn run_and_wait(pool: &'static UtilityPool) -> std::thread::ThreadId {
        let (tx, rx) = mpsc::channel();
        pool.execute(move || tx.send(std::thread::current().id()).expect("receiver"))
            .expect("spawn");
        rx.recv_timeout(Duration::from_secs(5)).expect("the job ran")
    }

    #[test]
    fn sequential_jobs_reuse_one_thread() {
        let pool = leaked(Duration::from_secs(30));
        let mut threads = HashSet::new();
        for _ in 0..20 {
            threads.insert(run_and_wait(pool));
            // The job has sent, but its thread may not be idle yet; give it the
            // moment it needs to park so the next job finds it.
            crate::testing::wait_until(Duration::from_secs(5), "the thread parks", || {
                pool.idle.lock_ignore_poison().len() == 1
            });
        }
        assert_eq!(threads.len(), 1, "every job ran on the same thread");
        assert_eq!(pool.threads_started(), 1);
    }

    #[test]
    fn a_job_never_waits_for_a_busy_thread() {
        let pool = leaked(Duration::from_secs(30));
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let (started_tx, started_rx) = mpsc::channel();
        // Three jobs that all block until released: each must get its own thread.
        for _ in 0..3 {
            let release_rx = Arc::clone(&release_rx);
            let started_tx = started_tx.clone();
            pool.execute(move || {
                started_tx.send(()).expect("receiver");
                let _ = release_rx.lock_ignore_poison().recv();
            })
            .expect("spawn");
        }
        for _ in 0..3 {
            started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("every blocking job starts at once");
        }
        assert_eq!(pool.threads_started(), 3);
        for _ in 0..3 {
            release_tx.send(()).expect("a job is waiting");
        }
    }

    #[test]
    fn idle_threads_exit_after_keep_alive_and_the_pool_recovers() {
        let pool = leaked(Duration::from_millis(20));
        run_and_wait(pool);
        crate::testing::wait_until(Duration::from_secs(5), "the thread parks", || {
            pool.idle.lock_ignore_poison().len() == 1
        });
        // Nothing else runs on this pool, so the list empties only when the thread
        // ages out.
        crate::testing::wait_until(Duration::from_secs(5), "the thread exits", || {
            pool.idle.lock_ignore_poison().is_empty()
        });
        run_and_wait(pool);
        assert_eq!(
            pool.threads_started(),
            2,
            "a fresh thread replaced the one that aged out"
        );
    }

    #[test]
    fn a_job_drops_what_it_captured_before_its_thread_idles() {
        let pool = leaked(Duration::from_secs(30));
        let held = Arc::new(());
        let captured = Arc::clone(&held);
        let (tx, rx) = mpsc::channel();
        pool.execute(move || {
            let _keep = captured;
            tx.send(()).expect("receiver");
        })
        .expect("spawn");
        rx.recv_timeout(Duration::from_secs(5)).expect("the job ran");
        crate::testing::wait_until(Duration::from_secs(5), "the job's captures drop", || {
            Arc::strong_count(&held) == 1
        });
    }
}
