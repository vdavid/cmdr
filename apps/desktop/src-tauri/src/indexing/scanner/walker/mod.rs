//! Hang-tolerant parallel directory walker for the local drive scan.
//!
//! # Why this exists
//!
//! A local full-disk scan must survive a hung `readdir`. macOS File Provider
//! mounts (`~/Library/CloudStorage/…` for Dropbox / Google Drive / a MacDroid
//! phone, `~/Library/Mobile Documents/` for iCloud) block indefinitely on a
//! `readdir` when the provider is disconnected (`fileproviderd … FP -1004`).
//! The former third-party directory-walking crate's strict-ordered delivery froze
//! the whole scan on one such read, and the reconcile path's serial `read_dir`
//! froze every rescan.
//!
//! This engine walks directories in parallel and guards every single read. A read
//! the guard condemns is *abandoned*: the directory is reported as a read error
//! (its subtree pruned, the dir left unmarked so freshness stays honest), a
//! replacement worker is spawned to keep pool capacity, and the rest of the walk
//! proceeds. A hung dir therefore costs at most one worker for at most the
//! timeout — never the whole scan.
//!
//! # The guard measures PROGRESS, not elapsed time
//!
//! Elapsed time cannot tell a BIG directory from a BROKEN one. A total-duration
//! cap of 15 s did exactly that: a fresh scan reported "complete" with 6,001,637
//! entries while having silently dropped 661,411 rows in five directories whose
//! only sin was being large (up to 200,000 entries), all of which the serial
//! reconcile then read in under 11 s each. See `indexing/DETAILS.md`
//! § "The walker's progress timeout".
//!
//! So each read publishes what it has delivered through a [`ReadProgress`] handle,
//! and the watchdog judges THAT (see `Engine::verdict`): a read is abandoned when
//! it has delivered nothing for [`WalkConfig::stall_timeout`], or when its total
//! time has outrun the [`WalkConfig::per_entry_allowance`] its delivered entries
//! earn it. A disconnected mount blocks in the syscall and is abandoned exactly as
//! promptly as before; a 200,000-entry directory is read to completion however long
//! it honestly takes.
//!
//! # The abandon/replace protocol (the non-obvious part)
//!
//! A blocking `readdir` on a real OS thread can't be interrupted, so a worker
//! that calls it directly can't time itself out. Instead a **watchdog** thread
//! caps it from outside. Each in-flight read carries an `Arc<AtomicU8>` state:
//! `READING → COMPLETED` (won by the worker) or `READING → ABANDONED` (won by the
//! watchdog). Whoever wins the compare-and-swap owns the outcome exactly once:
//!
//! - Worker finishes its read, `CAS(READING → COMPLETED)`. On success it processes
//!   the result and accounts the task done. On failure (watchdog already abandoned
//!   it) it drops the result and exits — its slot was replaced.
//! - Watchdog condemns a read, `CAS(READING → ABANDONED)`. On
//!   success it reports the timeout, accounts the task done, and spawns a
//!   replacement worker. The stuck worker thread is left parked in the syscall; it
//!   exits on its own once the File Provider layer finally errors. That lingering
//!   thread is bounded (only genuinely-hung *frontier* dirs reach it, each pruning
//!   its subtree) and self-clearing, so it's a bounded cost, not a leak.
//!
//! Because the driver must never block on a parked worker, workers are **not**
//! joined; the walk returns when the outstanding-task count hits zero (only the
//! watchdog is joined — it runs on a timer, never on a syscall).
//!
//! # Testability
//!
//! The directory read is injected as a [`ReadDirFn`] and both thresholds live on
//! [`WalkConfig`], so the hang, big-but-healthy, trickle, honest-skip, and
//! parallel-correctness behaviors are unit-tested with a mock reader at
//! millisecond scale — no real hung mount required. Production passes the platform
//! [`default_reader`].

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

#[cfg(target_os = "macos")]
mod bulk_read;

#[cfg(test)]
mod tests;

/// Scoped log target for the walker.
const LOG_TARGET: &str = "cmdr::indexing::scanner::walker";

/// 8 MB worker stack, matching `file_system::sync_status`: a File Provider
/// `readdir` / `lstat` can descend deep XPC override chains that overflow
/// rayon's 2 MB default. This is also why the walk uses dedicated OS threads,
/// never rayon (project rule: never rayon for calls that reach macOS
/// frameworks).
const WORKER_STACK_SIZE: usize = 8 * 1024 * 1024;

// In-flight read state (see the module-level abandon/replace protocol).
const READING: u8 = 0;
const COMPLETED: u8 = 1;
const ABANDONED: u8 = 2;

// ── Public API ───────────────────────────────────────────────────────

/// One directory to read. `id` is opaque to the engine — it's the visitor's
/// handle for the directory (in production, the entry's integer index id), passed
/// back to the visitor so children can be attributed to their parent without any
/// path→id lookup. The engine only uses `path`, to read the directory.
#[derive(Debug, Clone)]
pub struct DirTask {
    pub path: PathBuf,
    pub id: i64,
}

/// File kind of a directory child, as reported by the reader without following
/// symlinks (an `lstat`-shaped classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFileType {
    Dir,
    File,
    Symlink,
    Other,
}

/// Raw filesystem stat a reader may supply inline with an entry, so the visitor
/// can skip a per-entry `lstat`. Plain primitives (the engine stays generic); the
/// visitor maps them via `metadata::metadata_from_raw`, the same rules a
/// `std::fs::Metadata` goes through. `physical_size` is bytes (`ALLOCSIZE` /
/// `st_blocks * 512`). The macOS `getattrlistbulk` reader fills this; `std_read_dir`
/// and the test mock leave it `None`, so the visitor stats the entry itself.
#[derive(Debug, Clone, Copy)]
pub struct InlineStat {
    pub logical_size: u64,
    pub physical_size: u64,
    pub modified_at: Option<u64>,
    pub inode: u64,
    pub nlink: u64,
}

/// A raw directory child yielded by the reader: its full path, its (non-followed)
/// file type, and optionally its inline stat (see [`InlineStat`]). The visitor
/// derives the name from `path`; when `stat` is `None` it does its own `lstat` for
/// sizes/mtime.
#[derive(Debug, Clone)]
pub struct RawDirEntry {
    pub path: PathBuf,
    pub file_type: RawFileType,
    pub stat: Option<InlineStat>,
}

/// Why a directory read didn't yield children.
#[derive(Debug)]
pub enum WalkReadError {
    /// `readdir` returned an error (permission denied, not a directory, …).
    Io(std::io::Error),
    /// The read stopped making progress and was abandoned. The directory's contents
    /// are unknown this walk; the subtree is pruned and the dir is left unmarked.
    TimedOut,
}

/// What an in-flight directory read has delivered so far, published by the reader
/// and read by the watchdog. This is the signal that separates a BIG directory
/// from a BROKEN one: a healthy 200,000-entry read keeps the count climbing for
/// however many seconds it honestly needs, while a disconnected mount blocks in
/// the syscall and never moves it.
///
/// A reader that can't report progress (one that only returns a whole `Vec` at
/// the end) simply leaves the count at zero, which collapses the watchdog's rules
/// back to a plain total-duration cap — bounded exactly as an unprogressed read is.
#[derive(Debug, Default)]
pub struct ReadProgress {
    entries: AtomicU64,
}

impl ReadProgress {
    /// Report `n` more entries delivered. Called by the reader after every batch
    /// (macOS `getattrlistbulk`) or entry (`std_read_dir`) — never at the end, or
    /// the watchdog learns nothing while the read is running.
    pub fn record_entries(&self, n: u64) {
        self.entries.fetch_add(n, Ordering::Relaxed);
    }

    fn entries(&self) -> u64 {
        self.entries.load(Ordering::Relaxed)
    }
}

/// Injected directory reader. Production uses [`default_reader`]; tests inject a
/// reader that can block, to exercise the timeout without a real hung mount. The
/// [`ReadProgress`] handle is the read's own; it must publish through it as it
/// goes (see [`ReadProgress`]).
pub type ReadDirFn = Arc<dyn Fn(&Path, &ReadProgress) -> std::io::Result<Vec<RawDirEntry>> + Send + Sync>;

/// Per-directory semantics, driven by the engine. Called concurrently from
/// worker threads, so implementors must be `Sync`.
pub trait DirVisitor: Send + Sync {
    /// Handle a directory whose read succeeded. Returns the child directories to
    /// descend into, each carrying the id the visitor assigned it (so the engine
    /// can schedule the read without knowing anything about ids). The visitor does
    /// its per-entry work (lstat, exclusions, row build, marking `dir` listed) here.
    fn visit_dir(&self, dir: &DirTask, children: Vec<RawDirEntry>) -> Vec<DirTask>;

    /// Handle a directory whose read failed or timed out. The engine has already
    /// decided not to descend and not to mark the dir listed; this is for the
    /// visitor's own bookkeeping (logging, denial recording).
    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError);
}

/// Default per-subtree consecutive-read-failure budget. Mirrors the network
/// scanner's `CONSECUTIVE_FAILURE_ABORT` (`network_scanner/mod.rs`) so the two give-up
/// thresholds stay consistent; the count is stronger evidence here (every failure
/// is under ONE parent, and any successful sibling resets it), so reusing the
/// value is if anything conservative.
pub const DEFAULT_GIVE_UP_AFTER: usize = 32;

/// Default per-entry time allowance (see [`WalkConfig::per_entry_allowance`]).
///
/// Deliberately enormous next to reality so it can never fire on a healthy read:
/// the `getattrlistbulk` reader delivers a boot-volume directory at ~2 µs per
/// entry (verified on macOS 15, `bulk_vs_std_walk_bench`, 2026-07-21), and the
/// serial reconcile's per-entry allowance for calling a read *pathological* is
/// 100 µs (`local_reconcile/cost_budget.rs`). 1 ms is 500× the measured cost and
/// 10× that threshold, so it only ever catches a read moving orders of magnitude
/// slower than any filesystem we've measured, while still bounding one that
/// trickles below the stall rule's radar forever.
pub const DEFAULT_PER_ENTRY_ALLOWANCE: Duration = Duration::from_millis(1);

/// Walk tuning.
#[derive(Debug, Clone)]
pub struct WalkConfig {
    /// Worker threads. `0` = derive from available parallelism.
    pub num_threads: usize,
    /// How long a single read may go WITHOUT delivering an entry before it's
    /// abandoned. Not a cap on the read's total duration: a big directory that
    /// keeps delivering is read to completion however long it honestly takes.
    pub stall_timeout: Duration,
    /// How much total time a read earns per entry it has delivered, on top of
    /// [`Self::stall_timeout`]. The backstop against a read that trickles forever
    /// without ever stalling long enough to trip the stall rule; a healthy read
    /// clears it by orders of magnitude. `0` disables it, leaving only the stall
    /// rule.
    pub per_entry_allowance: Duration,
    /// How often the watchdog checks for over-timeout reads. Smaller = tighter
    /// abandon latency and cancellation latency, at a little more wakeup cost.
    pub watchdog_interval: Duration,
    /// Per-subtree consecutive-read-failure budget (see [`SubtreeBudget`]). Once
    /// the children of one successfully-listed directory rack up this many failed
    /// reads (timeouts + IO errors) with no successful read in between, the whole
    /// remaining subtree is pruned unread. `0` disables the budget.
    pub give_up_after: usize,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            num_threads: 0,
            stall_timeout: Duration::from_secs(15),
            per_entry_allowance: DEFAULT_PER_ENTRY_ALLOWANCE,
            watchdog_interval: Duration::from_secs(1),
            give_up_after: DEFAULT_GIVE_UP_AFTER,
        }
    }
}

/// Engine-level outcome of a walk (visitor-level totals live in the visitor).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub dirs_read: u64,
    pub timed_out: u64,
    pub io_errors: u64,
    /// Subtrees abandoned by the give-up budget (one per trip, not per pruned
    /// descendant). Each corresponds to a single give-up log line.
    pub subtrees_abandoned: u64,
}

/// Non-macOS reader: `std::fs::read_dir`, classifying each child without
/// following symlinks (the visitor stats each for sizes/mtime). Read errors on
/// individual entries are skipped (the directory read as a whole still succeeds);
/// a failure to open the directory propagates as the `Err`. On macOS
/// [`default_reader`] uses the `getattrlistbulk` reader instead, so this is unused
/// there.
#[cfg_attr(
    target_os = "macos",
    allow(
        dead_code,
        reason = "macOS uses the getattrlistbulk reader; std_read_dir is the reader for other platforms"
    )
)]
pub fn std_read_dir(path: &Path, progress: &ReadProgress) -> std::io::Result<Vec<RawDirEntry>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(path)? {
        // The iterator yields one entry per `readdir` step, so every turn of this
        // loop is real progress the watchdog can see.
        progress.record_entries(1);
        let Ok(entry) = entry else { continue };
        let file_type = match entry.file_type() {
            Ok(ft) if ft.is_dir() => RawFileType::Dir,
            Ok(ft) if ft.is_symlink() => RawFileType::Symlink,
            Ok(ft) if ft.is_file() => RawFileType::File,
            Ok(_) => RawFileType::Other,
            // A per-entry file_type() failure is rare (the dirent usually carries
            // the type); treat it as Other rather than dropping the entry.
            Err(_) => RawFileType::Other,
        };
        out.push(RawDirEntry {
            path: entry.path(),
            file_type,
            stat: None, // the visitor stats each entry itself
        });
    }
    Ok(out)
}

/// The production directory reader for this platform. On macOS this is the
/// `getattrlistbulk` bulk reader (name + type + sizes + mtime + inode + nlink in
/// one batched syscall, so the visitor skips a per-entry `lstat` — the dominant
/// cost of a local walk); everywhere else it's [`std_read_dir`] plus per-entry
/// `symlink_metadata`.
pub fn default_reader() -> ReadDirFn {
    #[cfg(target_os = "macos")]
    {
        Arc::new(bulk_read::bulk_read_dir)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(std_read_dir)
    }
}

/// Walk `root` and everything under it, calling `visitor` per directory. Blocks
/// until the walk completes (outstanding tasks reach zero) or `cancelled` is set.
/// Never blocks on a hung directory: see the module docs.
pub fn walk<V: DirVisitor + 'static>(
    root: DirTask,
    cfg: WalkConfig,
    reader: ReadDirFn,
    visitor: Arc<V>,
    cancelled: Arc<AtomicBool>,
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
        cancelled,
        reader,
        visitor,
        stall_timeout: cfg.stall_timeout,
        per_entry_allowance: cfg.per_entry_allowance,
        give_up_after: cfg.give_up_after,
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
                crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
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

// ── Internals ────────────────────────────────────────────────────────

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
    cancelled: Arc<AtomicBool>,
    reader: ReadDirFn,
    visitor: Arc<V>,
    stall_timeout: Duration,
    per_entry_allowance: Duration,
    /// Per-subtree give-up budget threshold (see [`SubtreeBudget`]). Copied onto
    /// every budget the engine mints.
    give_up_after: usize,
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
            let _guard = self.queue.lock_ignore_poison();
            self.done.store(true, Ordering::SeqCst);
            drop(_guard);
            self.cv.notify_all();
        }
    }

    /// Mark the walk done and wake everyone (used by the cancel path).
    fn signal_done(&self) {
        let _guard = self.queue.lock_ignore_poison();
        self.done.store(true, Ordering::SeqCst);
        drop(_guard);
        self.cv.notify_all();
    }

    fn spawn_worker(self: Arc<Self>, slot: Slot) {
        let spawned = std::thread::Builder::new()
            .name("index-walk".into())
            .stack_size(WORKER_STACK_SIZE)
            .spawn(move || self.run_worker(slot));
        if let Err(e) = spawned {
            // A failed spawn only reduces capacity; the remaining workers still
            // drain the queue. Never panic a replacement (it'd abort mid-scan).
            crate::log_error!(target: LOG_TARGET, "failed to spawn walk worker: {e}");
        }
    }

    fn run_worker(self: Arc<Self>, slot: Slot) {
        // Yield CPU to the UI: directory-walking is heavy background work. Set once per
        // worker thread (covers both initial and replacement workers).
        crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
        loop {
            // Pop the next task, or exit when the walk is done/cancelled.
            let scheduled = {
                let mut q = self.queue.lock_ignore_poison();
                loop {
                    if self.done.load(Ordering::SeqCst) || self.cancelled.load(Ordering::SeqCst) {
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
            if scheduled.budget.is_given_up() {
                self.complete_one();
                continue;
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

            if self.cancelled.load(Ordering::SeqCst) {
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
            std::thread::sleep(interval);
            if self.done.load(Ordering::SeqCst) {
                return;
            }
            if self.cancelled.load(Ordering::SeqCst) {
                self.signal_done();
                return;
            }

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
                let delivered = crate::pluralize::pluralize_with(entries, "entry", "entries");
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
