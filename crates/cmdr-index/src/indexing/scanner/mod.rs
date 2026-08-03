//! Parallel directory walker for drive indexing.
//!
//! Drives the hang-tolerant [`walker`] engine with an [`InsertVisitor`] to run
//! both a full-volume scan (`scan_volume`) and a targeted subtree scan
//! (`scan_subtree`). Discovered entries are sent in batches to the [`IndexWriter`]
//! for insertion into the SQLite index.
//!
//! Scan exclusions (macOS system directories, virtual filesystems) are applied per
//! child in the visitor via `should_exclude`, so excluded subtrees are never
//! descended into.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::store::{IndexStore, resolve_scan_root};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
use cmdr_fs::pluralize::{pluralize, pluralize_with};

mod exclusions;
pub use exclusions::SYSTEM_DIR_EXCLUDES;
/// Recognizing macOS File Provider domain roots, the one probe `exclusions` needs
/// that isn't pure string work. macOS-only: no other platform has File Provider.
#[cfg(target_os = "macos")]
mod file_provider;
pub(in crate::indexing) use exclusions::*;

mod insert_visitor;
use insert_visitor::InsertVisitor;

mod walker;
use walker::{
    DEFAULT_GIVE_UP_AFTER, DEFAULT_PER_ENTRY_ALLOWANCE, DirTask, ReadDirFn, WalkConfig, default_reader, walk,
};

// The batched `getattrlistbulk` read and the child file-type vocabulary that goes
// with it, re-exported for the serial reconcile walk
// (`reconcile::reconciler::read_fs_children`), which reads directories exactly the
// way the fresh scan does but on its own guarded worker thread. macOS-only, because
// that reader is: no other platform has `getattrlistbulk`, and nothing else outside
// the scanner names either type. The walker engine stays private, and `RawDirEntry`
// with it: only the visitor ever sees one.
#[cfg(target_os = "macos")]
pub(in crate::indexing) use walker::RawFileType;
#[cfg(target_os = "macos")]
pub(in crate::indexing) use walker::bulk_read::{BulkDirRead, bulk_read_dir_unwatched};

/// How long one LOCAL directory read may go without producing anything before
/// it's abandoned. It measures a STALL, never total duration: a disconnected File
/// Provider mount blocks and delivers nothing (abandoned in 15 s, its subtree
/// pruned, so a dead mount costs a handful of frontier dirs), while a big healthy
/// directory keeps delivering and is read to completion however long it takes.
///
/// The serial reconcile's `GuardedReader` reuses this constant on a reader with no
/// progress signal, where it acts as a plain total cap — the same 15 s verdict for
/// a read that has produced nothing. (The network scanner's 120 s is tuned for
/// SMB-over-WAN.)
pub(in crate::indexing) const LOCAL_LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the walker watchdog checks for over-timeout reads (also the ceiling
/// on caller-cancel latency).
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests;

/// Number of dir ids per `MarkDirsListed` message (mirrors `network_scanner`).
const MARK_CHUNK: usize = 10_000;

/// Emit `MarkDirsListed` for the accumulated dir ids, chunked. A no-op when empty.
/// Sent by the completion paths (`scan_volume`/`scan_subtree`) after the final
/// `flush_batch` and before the final aggregate, so the ordering invariant holds.
fn send_marks(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) {
    for chunk in listed_ids.chunks(MARK_CHUNK) {
        if let Err(e) = writer.send(WriteMessage::MarkDirsListed {
            ids: chunk.to_vec(),
            epoch,
        }) {
            log::warn!("Scanner: failed to send MarkDirsListed: {e}");
        }
    }
}

// ── Types ────────────────────────────────────────────────────────────

/// Configuration for a scan operation.
pub struct ScanConfig {
    /// Root path to scan from.
    pub root: PathBuf,
    /// Batch size for sending entries to the writer.
    pub batch_size: usize,
    /// Number of walker worker threads (0 = auto-detect).
    pub num_threads: usize,
    /// The scanned volume's path space: where it's rooted (which selects the
    /// per-child exclusion tier) and whether its inodes are a trustworthy identity.
    /// `boot_disk` for the `/`-rooted scan (keeps it off `/Volumes/`, system trees);
    /// `mount_rooted` for an external drive scan rooted at `/Volumes/X` (skips only
    /// junk basenames, else it would exclude its own subtree and falsely complete
    /// empty). `pub(crate)` because `IndexPathSpace` is a crate-internal type and
    /// `ScanConfig` is only built in-crate.
    pub(crate) space: IndexPathSpace,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            batch_size: 2000,
            num_threads: 0,
            space: IndexPathSpace::root(),
        }
    }
}

/// Progress counters for an active scan. Atomically updated by the scan thread.
pub struct ScanProgress {
    /// Files and directories recorded so far.
    pub entries_scanned: Arc<AtomicU64>,
    /// Directories among them, the tier-1 progress numerator.
    pub dirs_found: Arc<AtomicU64>,
    /// Resolved post-dedup physical bytes seen so far. Each entry contributes its
    /// `physical_size.unwrap_or(0)` after hardlink dedup, so the live numerator
    /// follows the exact same rules as the stored physical-size sums (directories,
    /// symlinks, and second+ hardlinks contribute 0).
    pub bytes_scanned: Arc<AtomicU64>,
}

/// A point-in-time read of an active scan's progress counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgressSnapshot {
    pub entries_scanned: u64,
    pub dirs_found: u64,
    pub bytes_scanned: u64,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanProgress {
    /// A fresh set of counters for one scan.
    pub fn new() -> Self {
        Self {
            entries_scanned: Arc::new(AtomicU64::new(0)),
            dirs_found: Arc::new(AtomicU64::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Read current progress snapshot.
    pub fn snapshot(&self) -> ScanProgressSnapshot {
        ScanProgressSnapshot {
            entries_scanned: self.entries_scanned.load(Ordering::Relaxed),
            dirs_found: self.dirs_found.load(Ordering::Relaxed),
            bytes_scanned: self.bytes_scanned.load(Ordering::Relaxed),
        }
    }
}

/// Handle returned by `scan_volume` for progress tracking and cancellation.
pub struct ScanHandle {
    pub progress: Arc<ScanProgress>,
    cancel: CancellationToken,
}

impl ScanHandle {
    /// Build a handle around an existing progress + cancel pair. Used by the
    /// `Volume`-trait scanner (`network_scanner`), which owns the walk itself and
    /// just needs the manager-facing progress/cancel surface.
    pub(crate) fn new(progress: Arc<ScanProgress>, cancel: CancellationToken) -> Self {
        Self { progress, cancel }
    }

    /// Signal the scan to stop. Already-written data remains in the DB.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// What a walk covered. Reaching a caller as `Ok` means the walk ran to the END:
/// a cancelled walk's partial totals arrive inside
/// [`ScanError::Cancelled`](ScanError::Cancelled) instead, so no caller can treat
/// a partial as a completion by forgetting to check a flag.
#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_entries: u64,
    pub total_dirs: u64,
    /// Resolved post-dedup physical bytes the scan summed (the final value of the
    /// `bytes_scanned` counter). Apples-to-apples with the stored physical-size sums.
    pub total_physical_bytes: u64,
    pub duration_ms: u64,
}

/// Everything [`run_scan`] produced, INCLUDING whether the walk ran to the end.
///
/// Internal on purpose: the write sequences that follow a walk (stamping listed
/// dirs, the subtree aggregate) need the ids and the epoch on the cancel path
/// too, so the cancelled/completed split only becomes a `Result` at the public
/// entry points, via [`ScanOutcome::into_result`].
#[derive(Debug)]
struct ScanOutcome {
    summary: ScanSummary,
    /// True when the scan's token fired before the walk ended.
    cancelled: bool,
    /// Ids of the directories whose read succeeded. Empty on a cancel (honest
    /// partial coverage), so stamping them is a no-op there.
    listed_ids: Vec<i64>,
    epoch: u64,
    root_id: i64,
}

impl ScanOutcome {
    /// The caller-facing answer: totals for a finished walk, the typed
    /// cancellation for a stopped one.
    fn into_result(self) -> Result<ScanSummary, ScanError> {
        if self.cancelled {
            Err(ScanError::Cancelled(self.summary))
        } else {
            Ok(self.summary)
        }
    }
}

/// Errors that can occur during scanning.
#[derive(Debug)]
pub enum ScanError {
    Io(std::io::Error),
    WriterSend(String),
    /// The volume ROOT listing SUCCEEDED but returned zero children, so a
    /// reconcile rescan would see an empty live tree and delete every existing
    /// child (blanking the index). Surfaced by the LOCAL reconcile walker
    /// (`local_reconcile`) before it diffs the root, so the completion handler
    /// takes its `Err` arm and writes NO `scan_completed_at`: the prior
    /// stale-but-real index is kept and heals on the next launch. Mirrors the
    /// network path's `VolumeScanError::EmptyRoot`; see
    /// `indexing/DETAILS.md` § "No completion marker on an empty root".
    EmptyRoot,
    /// The volume ROOT itself couldn't be listed (its read errored or timed out),
    /// which for a mount-rooted external drive means the mount VANISHED mid-scan (a
    /// yanked USB stick / SD card). Distinct from [`EmptyRoot`](ScanError::EmptyRoot):
    /// an empty-but-readable root lists successfully (zero children), whereas a
    /// vanished root can't be read at all. The completion handler treats this as an
    /// aborted scan — it writes NO `scan_completed_at` and emits `index-scan-aborted`
    /// so the frontend clears the stuck "scanning" row — mirroring the network path's
    /// disconnect arm. Surfaced by the fresh guarded-walker scan (`run_scan`, when the root is
    /// the only dir and never read) and the LOCAL reconcile walk (`local_reconcile`,
    /// when its root read returns `None`).
    RootUnlistable,
    /// The walk stopped because its cancellation token fired, carrying the
    /// partial totals it had reached. A distinct variant rather than a flag on
    /// the summary: `scan_completed_at` is written only on the `Ok` path, so a
    /// caller cannot mark a half-built index complete by forgetting to check.
    /// It is NOT a failure — the completion handlers route it to its own arm,
    /// which keeps the prior freshness and fires no abort. See
    /// `lifecycle/scan_completion.rs`.
    Cancelled(ScanSummary),
    /// The reconcile walk panicked. `local_reconcile::start_local_reconcile`
    /// wraps the walk in `catch_unwind` and converts the panic payload into this
    /// typed variant (carrying the panic message), so the thread's `JoinHandle`
    /// resolves to `Ok(Err(ScanError::Panicked(_)))` instead of a raw thread
    /// panic. That routes it through the completion handler's `Ok(Err(_))` arm,
    /// which logs cleanly and fires `FreshnessEvent::ScanFailed` ⇒ Stale.
    Panicked(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(f, "Writer send failed: {msg}"),
            ScanError::EmptyRoot => write!(f, "root listing returned no children (treating as a failed rescan)"),
            ScanError::RootUnlistable => write!(f, "volume root became unlistable (mount vanished mid-scan)"),
            ScanError::Cancelled(s) => write!(
                f,
                "scan cancelled after {} entries in {} dirs",
                s.total_entries, s.total_dirs
            ),
            ScanError::Panicked(msg) => write!(f, "reconcile walk panicked: {msg}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(err: std::io::Error) -> Self {
        ScanError::Io(err)
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Start a full-volume scan on a background thread.
///
/// Spawns a `std::thread` that walks the directory tree via the guarded [`walker`],
/// sends batches of [`EntryRow`] to the writer, and triggers `ComputeAllAggregates`
/// on completion.
///
/// Returns a [`ScanHandle`] for progress/cancellation and a [`std::thread::JoinHandle`]
/// for the scan result.
pub fn scan_volume(
    config: ScanConfig,
    writer: &IndexWriter,
    cancel: CancellationToken,
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());

    let handle = ScanHandle {
        progress: Arc::clone(&progress),
        cancel: cancel.clone(),
    };

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-scanner".into())
        .spawn(move || {
            // Yield CPU to the UI: the whole scan runs on this thread and its walker pool.
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            let reader: ReadDirFn = default_reader();
            let result = run_scan(
                &config.root,
                &cancel,
                &progress,
                &writer,
                config.batch_size,
                config.num_threads,
                true, // volume scan: root always maps to ROOT_ID
                &config.space,
                reader,
                LOCAL_LIST_TIMEOUT,
            );

            // On a clean finish: stamp the listed dirs FIRST, then aggregate,
            // then trim the WAL. The mark→aggregate order is the ordering
            // invariant (a mark queued behind the final aggregate would leave
            // that dir at epoch 0 and roll the whole tree to incomplete). The
            // single in-order writer enforces it. The WAL checkpoint trims the
            // GB-scale post-scan spike now instead of waiting for the ticker.
            // A cancelled walk sends none of it: the caller discards or heals
            // the partial, and a truncate already ran.
            if let Ok(outcome) = &result
                && !outcome.cancelled
            {
                send_marks(&outcome.listed_ids, outcome.epoch, &writer);
                if let Err(e) = writer.send(WriteMessage::ComputeAllAggregates {
                    source: AggSource::Maps,
                }) {
                    log::warn!("Scanner: failed to send ComputeAllAggregates: {e}");
                } else if let Err(e) = writer.send(WriteMessage::WalCheckpoint) {
                    log::warn!("Scanner: failed to send post-scan WalCheckpoint: {e}");
                }
            }

            result.and_then(ScanOutcome::into_result)
        })
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Synchronous subtree scan. Runs in the caller's thread.
///
/// Used by post-replay background verification and by the per-navigation verifier.
/// After scanning, sends `ComputeSubtreeAggregates` to the writer.
///
/// `space` is the OWNING VOLUME's path space, and both halves of it matter here:
/// `root` is an absolute FS path, so a mount-rooted volume's subtree resolves to an
/// entry id only after the mount root is stripped, and a `BootDisk` scope would
/// exclude every `/Volumes/X/...` child and scan the subtree to nothing.
pub fn scan_subtree(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    cancel: &CancellationToken,
) -> Result<ScanSummary, ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let reader: ReadDirFn = default_reader();
    let outcome = run_scan(
        root,
        cancel,
        &progress,
        writer,
        2000,
        0,
        false,
        space,
        reader,
        LOCAL_LIST_TIMEOUT,
    )?;

    // Stamp the listed dirs before the subtree aggregate (the ordering invariant).
    // ❌ Both run on the CANCEL path too, which is why the outcome is unwrapped
    // to a `Result` only afterwards: `run_scan` already sent the destructive
    // `DeleteDescendantsById(root_id)`, so bailing early on cancel would strand a
    // half-rebuilt subtree with stale ancestors. A cancelled scan has no listed
    // ids (honest partial coverage), so the marks no-op and only the repair runs.
    send_marks(&outcome.listed_ids, outcome.epoch, writer);
    if let Err(e) = writer.send(WriteMessage::ComputeSubtreeAggregates {
        root_id: outcome.root_id,
    }) {
        log::warn!("Scanner: failed to send ComputeSubtreeAggregates: {e}");
    }

    outcome.into_result()
}

// ── Core scan logic ──────────────────────────────────────────────────

/// Walk the local tree from `root` and insert every discovered entry, guarded so
/// a hung directory read can't stall the scan (see [`walker`]).
///
/// Parent attribution needs no path→id map: [`walk`] carries each directory's id
/// to its own read, so children take their parent's id directly. Ids come from the
/// shared `IndexWriter` counter. The scan root maps to `ROOT_ID` (volume scans) or
/// its existing entry id (subtree scans). `stall_timeout` is how long one read may
/// go without delivering an entry (production passes `LOCAL_LIST_TIMEOUT`; tests
/// pass a short one).
#[allow(
    clippy::too_many_arguments,
    reason = "internal scan entry point threading writer/progress/config; a param struct would add indirection without clarity"
)]
fn run_scan(
    root: &Path,
    cancel: &CancellationToken,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
    is_volume_root: bool,
    space: &IndexPathSpace,
    reader: ReadDirFn,
    stall_timeout: Duration,
) -> Result<ScanOutcome, ScanError> {
    let start = Instant::now();

    // Resolve the scan root id and read the epoch every listed dir is stamped with
    // (a first scan seeds epoch 1). Volume-root scans need a write connection (to
    // create the root sentinel / seed the epoch); subtree scans read on a read
    // connection after the full scan already seeded both.
    let (root_id, epoch) = {
        let db_path = writer.db_path();
        let conn = if is_volume_root {
            IndexStore::open_write_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::open_read_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
        let epoch = if is_volume_root {
            IndexStore::seed_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::read_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        // A volume-root scan maps to `ROOT_ID` whatever its path is, so it hands
        // `root` over untouched. A SUBTREE scan resolves an EXISTING entry, so its
        // absolute root must first cross into the volume's index path space
        // (identity on the boot disk, mount-root-stripped elsewhere): an absolute
        // walk from `ROOT_ID` misses at the very first component on a mount-rooted
        // volume, and a subtree outside the mount root has no entry here at all.
        let root_id = if is_volume_root {
            resolve_scan_root(&conn, root, true).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            let index_root = space
                .index_relative(&root.to_string_lossy())
                .ok_or_else(|| ScanError::WriterSend(format!("{} is outside the volume's index", root.display())))?;
            resolve_scan_root(&conn, Path::new(&index_root), false).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        (root_id, epoch)
    };

    // Subtree rescans delete existing descendants first (the scan re-inserts fresh
    // children); the subtree root entry itself is preserved.
    if !is_volume_root {
        writer
            .send(WriteMessage::DeleteDescendantsById(root_id))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
    }

    // A CHILD of the scan's token: cancelling the parent stops the walk, and the
    // visitor can stop the walk on a writer-send failure WITHOUT that reading as
    // a user cancel (`was_cancelled` below asks the parent).
    let walk_cancel = cancel.child_token();
    let visitor = Arc::new(InsertVisitor::new(
        writer.clone(),
        is_volume_root,
        space.exclusion_scope().clone(),
        space.inodes_trustworthy(),
        batch_size,
        progress,
        walk_cancel.clone(),
    ));

    // Watchdog ticks faster than the timeout (production 15s → 1s; a short test
    // timeout scales down, floored at 5ms).
    let watchdog_interval = (stall_timeout / 15).clamp(Duration::from_millis(5), WATCHDOG_INTERVAL);
    let cfg = WalkConfig {
        num_threads,
        stall_timeout,
        per_entry_allowance: DEFAULT_PER_ENTRY_ALLOWANCE,
        watchdog_interval,
        give_up_after: DEFAULT_GIVE_UP_AFTER,
    };
    let root_task = DirTask {
        path: root.to_path_buf(),
        id: root_id,
    };

    let walk_stats = walk(root_task, cfg, reader, Arc::clone(&visitor), walk_cancel);

    // Flush the final batch and surface any writer-send failure.
    visitor.finish()?;

    if walk_stats.timed_out > 0 {
        log::warn!(
            "Scanner: {} skipped after producing nothing for {}s each (hung / disconnected dirs)",
            pluralize(walk_stats.timed_out, "dir"),
            stall_timeout.as_secs(),
        );
    }
    if walk_stats.subtrees_abandoned > 0 {
        log::warn!(
            "Scanner: gave up on {} after {DEFAULT_GIVE_UP_AFTER} consecutive failed reads each \
             (dead mounts / providers); their subtrees are left honestly unindexed",
            pluralize(walk_stats.subtrees_abandoned, "subtree"),
        );
    }

    let was_cancelled = cancel.is_cancelled();

    // A volume-root scan whose ROOT never listed (`dirs_read == 0`) means the mount
    // itself couldn't be read — it vanished or went unreadable mid-scan (a yanked
    // external drive). This is distinct from an empty-but-readable root, which reads
    // successfully (so `dirs_read == 1`). Surface it as the typed `RootUnlistable`
    // abort so the completion handler writes no `scan_completed_at` (no
    // false-complete of an empty index) and clears the stuck "scanning" row, instead
    // of silently "completing" with zero entries. Only for a volume-root scan (`/`
    // and mount roots); subtree scans have their own root handling.
    if is_volume_root && !was_cancelled && walk_stats.dirs_read == 0 {
        return Err(ScanError::RootUnlistable);
    }

    // A cancelled scan emits no marks (the caller discards/heals the partial).
    let listed_ids = if was_cancelled {
        Vec::new()
    } else {
        visitor.take_listed_ids()
    };
    let snap = progress.snapshot();

    log::debug!(
        "Scanner: walk complete: {}, {} ({} listed) in {}ms",
        pluralize_with(snap.entries_scanned, "entry", "entries"),
        pluralize(snap.dirs_found, "dir"),
        listed_ids.len(),
        start.elapsed().as_millis()
    );

    Ok(ScanOutcome {
        summary: ScanSummary {
            total_entries: snap.entries_scanned,
            total_dirs: snap.dirs_found,
            total_physical_bytes: snap.bytes_scanned,
            duration_ms: start.elapsed().as_millis() as u64,
        },
        cancelled: was_cancelled,
        listed_ids,
        epoch,
        root_id,
    })
}
