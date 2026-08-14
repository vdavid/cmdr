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
//! and the watchdog judges THAT (see `Engine::verdict` in `engine.rs`): a read is
//! abandoned when it has delivered nothing for [`WalkConfig::stall_timeout`], or when its total
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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(target_os = "macos")]
pub(super) mod bulk_read;

mod engine;
pub use engine::walk;

#[cfg(test)]
mod tests;

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

    /// A directory this walk will never read: its subtree was given up after
    /// [`WalkConfig::give_up_after`] consecutive failed reads, so the task was
    /// dropped from the queue unread.
    ///
    /// Nothing was tried, so there's no error to report — and that's exactly why
    /// the hook exists: a pruned task gets no other mention anywhere, so a visitor
    /// recording ground nothing can read would silently miss the whole pruned
    /// majority of a dead mount. ❌ Don't log per call; killing that flood is what
    /// the budget is for. Defaults to nothing, like the other bookkeeping hooks.
    fn visit_pruned(&self, _dir: &DirTask) {}

    /// A worker thread failed to spawn, so the walk runs with less parallelism.
    /// The engine carries on either way (the remaining workers still drain the
    /// queue); how loudly to report it is the visitor's call, so this defaults to
    /// silence and the production visitor overrides it.
    fn note_worker_spawn_failure(&self, _error: &std::io::Error) {}

    /// The walk's watchdog came round, so anything the visitor owes on a CLOCK
    /// rather than on an entry can go out now.
    ///
    /// It exists because the visitor's own hooks all fire on discovery: a walk
    /// parked on one slow directory calls none of them, and whatever it found
    /// before it parked would sit until the walk ended. The watchdog is the one
    /// thread still moving then, and it already wakes on
    /// [`WalkConfig::watchdog_interval`], so this costs no thread of its own.
    /// Defaults to nothing; a walk with a live consumer is the only one with
    /// anything to do here.
    fn on_watchdog_tick(&self) {}
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
/// 100 µs (`reconcile/local_reconcile/cost_budget.rs`). 1 ms is 500× the measured cost and
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
    /// Per-subtree consecutive-read-failure budget (`SubtreeBudget`, `engine.rs`).
    /// Once
    /// the children of one successfully-listed directory rack up this many failed
    /// reads (timeouts + IO errors) with no successful read in between, the whole
    /// remaining subtree is pruned unread. `0` disables the budget.
    pub give_up_after: usize,
    /// Where to report each directory read as it starts, so a consumer watching a
    /// cover walk sees it moving between batches. `None` for the background scans,
    /// which report through their own [`ScanProgress`](super::ScanProgress).
    pub heartbeat: Option<super::WalkHeartbeat>,
    /// A pause before each directory read, from `CMDR_E2E_WALK_THROTTLE_MS`. Only
    /// a cover walk ever sets it, and only under an E2E run; see
    /// [`cover_walk_throttle`](super::cover_walk_throttle).
    pub per_dir_delay: Option<Duration>,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            num_threads: 0,
            stall_timeout: Duration::from_secs(15),
            per_entry_allowance: DEFAULT_PER_ENTRY_ALLOWANCE,
            watchdog_interval: Duration::from_secs(1),
            give_up_after: DEFAULT_GIVE_UP_AFTER,
            heartbeat: None,
            per_dir_delay: None,
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
