//! The walk driver and the guarded engine behind it: the worker pool, the
//! watchdog that abandons a read the moment it stops making progress, and the
//! per-subtree give-up budget. The types a caller and a visitor see live in
//! `mod.rs`, along with the module docs that explain the abandon/replace
//! protocol this file implements.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{DirTask, DirVisitor, ReadDirFn, ReadProgress, WalkConfig, WalkReadError, WalkStats};
use crate::indexing::scanner::WalkHeartbeat;

/// Scoped log target for the walker.
const LOG_TARGET: &str = "cmdr::indexing::scanner::walker";

/// 8 MB worker stack, matching `file_system::sync_status`: a File Provider
/// `readdir` / `lstat` can descend deep XPC override chains that overflow
/// rayon's 2 MB default. This is also why the walk uses dedicated OS threads,
/// never rayon (project rule: never rayon for calls that reach macOS
/// frameworks).
const WORKER_STACK_SIZE: usize = 8 * 1024 * 1024;

// In-flight read state (see the abandon/replace protocol in the module docs).
const READING: u8 = 0;
const COMPLETED: u8 = 1;
const ABANDONED: u8 = 2;

/// Walk `root` and everything under it, calling `visitor` per directory. Blocks
/// until the walk completes (outstanding tasks reach zero) or `cancel` fires.
/// Never blocks on a hung directory: see the module docs.
pub fn walk<V: DirVisitor + 'static>(
    root: DirTask,
    cfg: WalkConfig,
    reader: ReadDirFn,
    visitor: Arc<V>,
    cancel: CancellationToken,
) -> WalkStats {
    let num_threads = if cfg.num_threads == 0 {
        std::thread::available_parallelism().map_or(4, |n| n.get())
    } else {
        cfg.num_threads
    };

    let engine = Arc::new(Engine {
        queue: Mutex::new(VecDeque::new()),
        cv: Condvar::new(),
        outstanding: AtomicUsize::new(0),
        done: AtomicBool::new(false),
        watchdog_wake: Condvar::new(),
        watchdog_lock: Mutex::new(()),
        cancel,
        reader,
        visitor,
        stall_timeout: cfg.stall_timeout,
        per_entry_allowance: cfg.per_entry_allowance,
        give_up_after: cfg.give_up_after,
        heartbeat: cfg.heartbeat,
        per_dir_delay: cfg.per_dir_delay,
        slots: Mutex::new(Vec::with_capacity(num_threads)),
        dirs_read: AtomicU64::new(0),
        timed_out: AtomicU64::new(0),
        io_errors: AtomicU64::new(0),
        subtrees_abandoned: AtomicU64::new(0),
    });

    // The scan root and its direct children share a budget rooted at the root path;
    // each successfully-listed dir mints a fresh budget for its own children.
    let root_budget = SubtreeBudget::new(root.path.clone(), cfg.give_up_after);
    engine.enqueue(ScheduledTask {
        task: root,
        budget: root_budget,
    });

    // Give each initial worker its own slot up front so the watchdog can see it.
    let initial_slots: Vec<Slot> = {
        let mut slots = engine.slots.lock_ignore_poison();
        for _ in 0..num_threads {
            slots.push(Arc::new(Mutex::new(None)));
        }
        slots.clone()
    };
    for slot in initial_slots {
        engine.clone().spawn_worker(slot);
    }

    let watchdog = {
        let engine = engine.clone();
        let interval = cfg.watchdog_interval;
        std::thread::Builder::new()
            .name("index-walk-watchdog".into())
            .spawn(move || {
                // Utility tier: the whole walk (workers + this watchdog) yields CPU to the UI.
                cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
                engine.run_watchdog(interval)
            })
            .expect("failed to spawn walker watchdog thread")
    };

    // Wait for completion. Workers are intentionally not joined — an abandoned
    // one is parked in a syscall and would block forever. The watchdog runs on a
    // timer, so it's safe to join.
    {
        let mut q = engine.queue.lock_ignore_poison();
        while !engine.done.load(Ordering::SeqCst) {
            q = engine.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
    let _ = watchdog.join();

    WalkStats {
        dirs_read: engine.dirs_read.load(Ordering::Relaxed),
        timed_out: engine.timed_out.load(Ordering::Relaxed),
        io_errors: engine.io_errors.load(Ordering::Relaxed),
        subtrees_abandoned: engine.subtrees_abandoned.load(Ordering::Relaxed),
    }
}

/// Per-subtree give-up budget: the consecutive failed-read count among the
/// children of ONE successfully-listed directory. Any successful sibling read
/// resets it; once it reaches `limit` the budget is *given up* — sticky — and
/// every still-queued sibling sharing it is pruned unread. This bounds a dead
/// mount to ~`limit` probes per level instead of one abandon per descendant,
/// and it falls naturally on a dead `Library/CloudStorage/<provider>-*` root
/// (reads fail, nothing resets). A healthy provider is untouched: its reads
/// succeed, so the counter never climbs. Shared (`Arc`) by all children of the
/// directory that minted it. Pruned dirs are never marked listed, so they stay
/// honest-stale (unknown size), never false-complete.
struct SubtreeBudget {
    /// Consecutive failed reads with no success in between (reset by any success).
    consecutive_failures: AtomicUsize,
    /// Sticky once the budget trips; makes the give-up idempotent and prunes the
    /// remaining siblings.
    given_up: AtomicBool,
    /// The directory whose children this budget covers — the subject of the single
    /// give-up log line.
    root: PathBuf,
    /// Trip threshold, copied from [`WalkConfig::give_up_after`]. `0` disables it.
    limit: usize,
}

impl SubtreeBudget {
    fn new(root: PathBuf, limit: usize) -> Arc<Self> {
        Arc::new(Self {
            consecutive_failures: AtomicUsize::new(0),
            given_up: AtomicBool::new(false),
            root,
            limit,
        })
    }

    /// Record a failed read under this subtree. Returns `true` exactly once — on
    /// the read that trips the budget — so the caller logs the give-up a single
    /// time. Under concurrency "consecutive" is loose (up to `num_threads` reads
    /// can be in flight against one budget), the same caveat the network scanner
    /// notes: a genuinely dead subtree piles failures with no success to reset it,
    /// so it still trips; a lone bad dir is reset by its many healthy peers.
    fn record_failure(&self) -> bool {
        if self.limit == 0 {
            return false;
        }
        let n = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        n >= self.limit && !self.given_up.swap(true, Ordering::SeqCst)
    }

    /// A successful read broke the streak — reset the counter. Leaves an
    /// already-tripped budget given up (its siblings are already being pruned).
    fn reset(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
    }

    fn is_given_up(&self) -> bool {
        self.given_up.load(Ordering::SeqCst)
    }
}

/// A directory scheduled for reading: the visitor-facing [`DirTask`] plus the
/// give-up budget it shares with its siblings. Internal to the engine — the
/// public visitor API still sees a bare `DirTask`.
#[derive(Clone)]
struct ScheduledTask {
    task: DirTask,
    budget: Arc<SubtreeBudget>,
}

/// An in-flight directory read, registered in a worker's slot so the watchdog
/// can time it out.
struct InFlight {
    state: Arc<AtomicU8>,
    task: ScheduledTask,
    started: Instant,
    /// What the read has delivered so far, published by the reader itself.
    progress: Arc<ReadProgress>,
    /// The watchdog's own bookkeeping (only it touches these, under the slot
    /// lock): the entry count it last saw, and when it saw it move.
    seen_entries: u64,
    seen_at: Instant,
}

/// Why the watchdog abandoned a read (see [`Engine::verdict`]). Typed rather than
/// inferred from the log line: the two cases mean different things in the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbandonReason {
    /// Delivered nothing for a whole `stall_timeout` — a hung or dead mount.
    Stalled,
    /// Kept delivering, but far too slowly for the work it did.
    OverAllowance,
}

/// A worker's current-read slot. `None` between reads. Each worker owns one; the
/// watchdog scans all of them.
type Slot = Arc<Mutex<Option<InFlight>>>;

struct Engine<V: DirVisitor> {
    /// Directories still to read. Drained by workers, grown as dirs are discovered.
    queue: Mutex<VecDeque<ScheduledTask>>,
    /// Signals queue-non-empty and walk-done. Paired with `queue`'s mutex.
    cv: Condvar,
    /// Tasks enqueued but not yet accounted done. Walk completes when this hits 0.
    outstanding: AtomicUsize,
    /// Set (under the `queue` lock) when the walk is finished or cancelled.
    done: AtomicBool,
    /// Wakes the watchdog the moment the walk is done, so a SHORT walk doesn't
    /// pay a whole `watchdog_interval` of dead time before `walk` can return.
    /// It cost a flat ~1 s per walk on a plain `sleep`, which is invisible on a
    /// full volume scan and ruinous for a search covering many small frontier
    /// nodes one after another. Paired with `watchdog_lock`, not `queue`'s
    /// mutex: `cv` is notified once per enqueued directory, and waiting on that
    /// would wake the watchdog tens of thousands of times a walk.
    watchdog_wake: Condvar,
    /// Held while the watchdog decides whether to sleep, and taken-then-released
    /// by `signal_done` before it notifies, so the wake can't be missed in the
    /// window between the check and the wait.
    watchdog_lock: Mutex<()>,
    /// The walk's stop signal. Workers check it between tasks and between
    /// entries, so a cancel lands within one directory read.
    cancel: CancellationToken,
    reader: ReadDirFn,
    visitor: Arc<V>,
    stall_timeout: Duration,
    per_entry_allowance: Duration,
    /// Per-subtree give-up budget threshold (see [`SubtreeBudget`]). Copied onto
    /// every budget the engine mints.
    give_up_after: usize,
    /// Where each starting read is reported, for a consumer watching this walk.
    heartbeat: Option<WalkHeartbeat>,
    /// The E2E throttle, applied before each read (see [`WalkConfig::per_dir_delay`]).
    per_dir_delay: Option<Duration>,
    /// One slot per live worker (initial + replacements). Grows on abandonment.
    slots: Mutex<Vec<Slot>>,
    dirs_read: AtomicU64,
    timed_out: AtomicU64,
    io_errors: AtomicU64,
    subtrees_abandoned: AtomicU64,
}

impl<V: DirVisitor + 'static> Engine<V> {
    /// Push a directory to read. Bumps the outstanding count first so completion
    /// can't race to zero before the child is queued.
    fn enqueue(&self, task: ScheduledTask) {
        self.outstanding.fetch_add(1, Ordering::SeqCst);
        self.queue.lock_ignore_poison().push_back(task);
        self.cv.notify_one();
    }

    /// Record a failed read against its subtree budget. On the read that trips the
    /// budget (returns `true` exactly once), log the give-up a single time and
    /// count it; the remaining still-queued siblings are pruned unread by the
    /// pre-read check in [`Self::run_worker`].
    fn record_subtree_failure(&self, scheduled: &ScheduledTask) {
        if scheduled.budget.record_failure() {
            self.subtrees_abandoned.fetch_add(1, Ordering::Relaxed);
            log::warn!(
                target: LOG_TARGET,
                "giving up on subtree {} after {} consecutive failed reads (timeouts / IO errors); \
                 pruning its remaining unread directories (left honest-stale, not indexed)",
                scheduled.budget.root.display(),
                scheduled.budget.limit,
            );
        }
    }

    /// Account one task done. When the last one completes, mark the walk done
    /// (under the queue lock, so a worker mid-`wait` can't miss the wakeup).
    fn complete_one(&self) {
        if self.outstanding.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.signal_done();
        }
    }

    /// Mark the walk done and wake everyone (used by the cancel path).
    fn signal_done(&self) {
        let _guard = self.queue.lock_ignore_poison();
        self.done.store(true, Ordering::SeqCst);
        drop(_guard);
        self.cv.notify_all();
        // Take and release the watchdog's lock before notifying: that's what
        // orders this against its check-then-wait, so a walk that finishes
        // between the two doesn't leave it asleep for a whole interval.
        drop(self.watchdog_lock.lock_ignore_poison());
        self.watchdog_wake.notify_all();
    }

    fn spawn_worker(self: Arc<Self>, slot: Slot) {
        let visitor = Arc::clone(&self.visitor);
        let spawned = std::thread::Builder::new()
            .name("index-walk".into())
            .stack_size(WORKER_STACK_SIZE)
            .spawn(move || self.run_worker(slot));
        if let Err(e) = spawned {
            // A failed spawn only reduces capacity; the remaining workers still
            // drain the queue. Never panic a replacement (it'd abort mid-scan).
            visitor.note_worker_spawn_failure(&e);
        }
    }

    fn run_worker(self: Arc<Self>, slot: Slot) {
        // Yield CPU to the UI: directory-walking is heavy background work. Set once per
        // worker thread (covers both initial and replacement workers).
        cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
        loop {
            // Pop the next task, or exit when the walk is done/cancelled.
            let scheduled = {
                let mut q = self.queue.lock_ignore_poison();
                loop {
                    if self.done.load(Ordering::SeqCst) || self.cancel.is_cancelled() {
                        return;
                    }
                    if let Some(task) = q.pop_front() {
                        break task;
                    }
                    q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
                }
            };

            // Prune: this task's subtree was given up (its siblings racked up the
            // failure budget). Skip the read entirely — no probe, no per-dir log,
            // the dir left unlisted (honest-stale). This is what replaces the
            // per-descendant abandon flood with one give-up line.
            //
            // The visitor still hears about it: this is the only mention a pruned
            // directory ever gets, and without it nothing can record that Cmdr gave
            // up here, so it sits in the coverage frontier and every later search
            // pays to rediscover the same dead mount.
            if scheduled.budget.is_given_up() {
                self.visitor.visit_pruned(&scheduled.task);
                self.complete_one();
                continue;
            }

            // Say where the walk is BEFORE the read, not after it: a read that
            // hangs is exactly the one a watcher wants named, and it would never
            // report itself from the other side.
            if let Some(heartbeat) = &self.heartbeat {
                heartbeat.entering(&scheduled.task.path);
            }
            if let Some(delay) = self.per_dir_delay {
                std::thread::sleep(delay);
            }

            // Register the read so the watchdog can time it out, then do the
            // (potentially blocking) read.
            let state = Arc::new(AtomicU8::new(READING));
            let progress = Arc::new(ReadProgress::default());
            let started = Instant::now();
            *slot.lock_ignore_poison() = Some(InFlight {
                state: Arc::clone(&state),
                task: scheduled.clone(),
                started,
                progress: Arc::clone(&progress),
                seen_entries: 0,
                seen_at: started,
            });
            let result = (self.reader)(&scheduled.task.path, &progress);

            // Resolve the race with the watchdog. If it already abandoned this
            // read, drop the result and exit — a replacement worker took over.
            if state
                .compare_exchange(READING, COMPLETED, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }
            *slot.lock_ignore_poison() = None;

            if self.cancel.is_cancelled() {
                self.complete_one();
                continue;
            }

            match result {
                Ok(children) => {
                    self.dirs_read.fetch_add(1, Ordering::Relaxed);
                    // A successful read breaks the failure streak among this dir's
                    // siblings, and its own children start a fresh budget rooted here.
                    scheduled.budget.reset();
                    let child_budget = SubtreeBudget::new(scheduled.task.path.clone(), self.give_up_after);
                    for sub in self.visitor.visit_dir(&scheduled.task, children) {
                        self.enqueue(ScheduledTask {
                            task: sub,
                            budget: Arc::clone(&child_budget),
                        });
                    }
                }
                Err(e) => {
                    self.io_errors.fetch_add(1, Ordering::Relaxed);
                    self.record_subtree_failure(&scheduled);
                    self.visitor.visit_read_error(&scheduled.task, &WalkReadError::Io(e));
                }
            }
            self.complete_one();
        }
    }

    /// Should this in-flight read be abandoned, and why? `None` means "still
    /// working, leave it alone". Two rules, either of which fires:
    ///
    /// - **Stalled**: it has delivered nothing for a whole `stall_timeout`. This is
    ///   the hung-mount rule, and it applies whether the read has produced a
    ///   million entries or none — a mount that drops mid-listing is abandoned as
    ///   promptly as one that never starts.
    /// - **Over allowance**: its total time has outrun `stall_timeout` plus
    ///   `per_entry_allowance` per entry delivered. The backstop for a read that
    ///   trickles just fast enough to keep resetting the stall rule forever.
    ///
    /// A reader that publishes no progress leaves `entries` at 0, which makes both
    /// rules the same plain total-duration cap. That's the honest verdict: a read
    /// we cannot observe is indistinguishable from one that has produced nothing.
    fn verdict(&self, f: &InFlight, now: Instant) -> Option<AbandonReason> {
        if now.duration_since(f.seen_at) >= self.stall_timeout {
            return Some(AbandonReason::Stalled);
        }
        let earned = self
            .per_entry_allowance
            .saturating_mul(u32::try_from(f.seen_entries).unwrap_or(u32::MAX));
        if now.duration_since(f.started) >= self.stall_timeout.saturating_add(earned) {
            return Some(AbandonReason::OverAllowance);
        }
        None
    }

    fn run_watchdog(self: Arc<Self>, interval: Duration) {
        loop {
            {
                // Check under the lock, then wait on it: `signal_done` takes the
                // same lock before notifying, so the walk can't finish inside the
                // gap and leave us sleeping out the interval.
                let guard = self.watchdog_lock.lock_ignore_poison();
                if self.done.load(Ordering::SeqCst) {
                    return;
                }
                let _ = self
                    .watchdog_wake
                    .wait_timeout(guard, interval)
                    .unwrap_or_else(|e| e.into_inner());
            }
            if self.done.load(Ordering::SeqCst) {
                return;
            }
            if self.cancel.is_cancelled() {
                self.signal_done();
                return;
            }

            // Before judging the reads: whatever the visitor owes on a clock. A
            // walk parked on one directory reaches nothing else, and this is the
            // thread still moving.
            self.visitor.on_watchdog_tick();

            let now = Instant::now();
            // Snapshot the slot handles (cheap Arc clones) so we don't hold the
            // slots lock across per-slot work or a worker spawn.
            let slots = self.slots.lock_ignore_poison().clone();
            for slot in slots {
                // Observe progress and judge under the slot lock (the watchdog is
                // the only reader/writer of the `seen_*` fields), then act outside it.
                let claim = {
                    let mut guard = slot.lock_ignore_poison();
                    guard.as_mut().and_then(|f| {
                        let entries = f.progress.entries();
                        if entries > f.seen_entries {
                            f.seen_entries = entries;
                            f.seen_at = now;
                        }
                        self.verdict(f, now)
                            .map(|reason| (Arc::clone(&f.state), f.task.clone(), reason, entries))
                    })
                };
                let Some((state, task, reason, entries)) = claim else {
                    continue;
                };
                // Try to claim the abandonment. If the worker just finished, its
                // CAS won and this fails — leave it alone.
                if state
                    .compare_exchange(READING, ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    continue;
                }
                *slot.lock_ignore_poison() = None;
                self.timed_out.fetch_add(1, Ordering::Relaxed);
                let delivered = cmdr_fs::pluralize::pluralize_with(entries, "entry", "entries");
                match reason {
                    AbandonReason::Stalled => log::warn!(
                        target: LOG_TARGET,
                        "read produced nothing for {:?} ({delivered} so far), abandoning {} \
                         (subtree skipped this scan)",
                        self.stall_timeout,
                        task.task.path.display(),
                    ),
                    AbandonReason::OverAllowance => log::warn!(
                        target: LOG_TARGET,
                        "read is trickling ({delivered}, past its {:?}-per-entry allowance), abandoning {} \
                         (subtree skipped this scan)",
                        self.per_entry_allowance,
                        task.task.path.display(),
                    ),
                }
                self.record_subtree_failure(&task);
                self.visitor.visit_read_error(&task.task, &WalkReadError::TimedOut);

                // Restore capacity: the parked worker is gone, so add a fresh slot
                // and a replacement worker.
                let new_slot: Slot = Arc::new(Mutex::new(None));
                self.slots.lock_ignore_poison().push(Arc::clone(&new_slot));
                Arc::clone(&self).spawn_worker(new_slot);

                self.complete_one();
            }
        }
    }
}
