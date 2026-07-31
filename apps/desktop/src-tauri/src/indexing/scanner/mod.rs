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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::indexing::events::{Diagnostic, IndexErrorReport, IndexEvent};
use crate::indexing::paths::firmlinks;
use crate::indexing::store::{EntryRow, IndexStore, resolve_scan_root};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::{pluralize, pluralize_with};

mod exclusions;
pub use exclusions::SYSTEM_DIR_EXCLUDES;
/// Recognizing macOS File Provider domain roots, the one probe `exclusions` needs
/// that isn't pure string work. macOS-only: no other platform has File Provider.
#[cfg(target_os = "macos")]
mod file_provider;
pub(in crate::indexing) use exclusions::*;

mod walker;
use walker::{
    DEFAULT_GIVE_UP_AFTER, DEFAULT_PER_ENTRY_ALLOWANCE, DirTask, DirVisitor, ReadDirFn, WalkConfig, WalkReadError,
    default_reader, walk,
};

// The reader's per-child vocabulary, and (on macOS) the batched `getattrlistbulk`
// read itself, re-exported for the serial reconcile walk
// (`reconcile::reconciler::read_fs_children`), which reads directories exactly the
// way the fresh scan does but on its own guarded worker thread. The walker engine
// stays private to the scanner.
#[cfg(target_os = "macos")]
pub(in crate::indexing) use walker::bulk_read::{BulkDirRead, bulk_read_dir_unwatched};
pub(in crate::indexing) use walker::{RawDirEntry, RawFileType};

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
    /// Exclusion scope for the per-child gate. `BootDisk` for the `/`-rooted boot
    /// scan (keeps it off `/Volumes/`, system trees); `MountRooted` for an external
    /// drive scan rooted at `/Volumes/X` (skips only junk basenames, else it would
    /// exclude its own subtree and falsely complete empty). `pub(crate)` because
    /// `ExclusionScope` is a crate-internal type and `ScanConfig` is only built
    /// in-crate.
    pub(crate) scope: ExclusionScope,
    /// Whether the scanned volume's inode is a trustworthy identity. `false` only
    /// for a local external drive on FAT/exFAT, whose derived inodes are unstable:
    /// the visitor then stores `inode: None` for every entry (and skips hardlink
    /// dedup, inert at `nlink == 1` anyway) so the live rename pre-pass can never
    /// match a reused inode. Defaults to `true`; the manager feeds it from the
    /// volume's `IndexPathSpace`. See `filesystem_kind::has_stable_inodes`.
    pub(crate) inodes_trustworthy: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            batch_size: 2000,
            num_threads: 0,
            scope: ExclusionScope::boot_disk(),
            inodes_trustworthy: true,
        }
    }
}

/// Progress counters for an active scan. Atomically updated by the scan thread.
pub struct ScanProgress {
    pub entries_scanned: Arc<AtomicU64>,
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

impl ScanProgress {
    pub(crate) fn new() -> Self {
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
                config.scope.clone(),
                config.inodes_trustworthy,
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
/// Used by post-replay background verification. After scanning, sends
/// `ComputeSubtreeAggregates` to the writer.
pub fn scan_subtree(root: &Path, writer: &IndexWriter, cancel: &CancellationToken) -> Result<ScanSummary, ScanError> {
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
        // Subtree scans don't apply global exclusions (the subtree was chosen
        // explicitly), so the scope is inert here; pass the boot-disk one.
        ExclusionScope::boot_disk(),
        // Subtree scans back post-replay background verification, which is
        // root-only (the boot disk, APFS) — trustworthy inodes.
        true,
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
    scope: ExclusionScope,
    inodes_trustworthy: bool,
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
        let root_id =
            resolve_scan_root(&conn, root, is_volume_root).map_err(|e| ScanError::WriterSend(e.to_string()))?;
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
        scope,
        inodes_trustworthy,
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

/// Fresh-scan [`DirVisitor`]: inserts every discovered entry as a new row,
/// attributing children to the directory being read via the carried `parent_id`.
///
/// A directory whose read SUCCEEDS is recorded in `listed_ids` (marked listed at
/// the current epoch after the walk); a timed-out or errored dir is never
/// recorded, so it stays `listed_epoch = 0` (honest "unknown"). Runs concurrently
/// on the walker's worker threads, so shared state is behind mutexes / atomics.
struct InsertVisitor {
    writer: IndexWriter,
    /// Shared id counter from `IndexWriter` (the single allocation source).
    next_id: Arc<AtomicI64>,
    is_volume_root: bool,
    /// Exclusion scope for the per-child gate (see `ScanConfig::scope`).
    scope: ExclusionScope,
    /// Whether the scanned volume's inode is a trustworthy identity (see
    /// `ScanConfig::inodes_trustworthy`). `false` on FAT/exFAT ⇒ every stored
    /// `inode` is nulled and hardlink dedup is skipped.
    inodes_trustworthy: bool,
    batch_size: usize,
    /// Live progress counters (shared with the manager-facing `ScanHandle`); the
    /// scan summary reads their final values.
    entries_scanned: Arc<AtomicU64>,
    dirs_found: Arc<AtomicU64>,
    bytes_scanned: Arc<AtomicU64>,
    /// A child of the scan's token, cancelled when a writer send fails so the
    /// walk stops promptly. Cancelling it does NOT mark the scan user-cancelled.
    walk_cancel: CancellationToken,
    /// Accumulating insert batch, flushed at `batch_size`.
    batch: Mutex<Vec<EntryRow>>,
    /// Inodes seen with nlink > 1, so each hardlink's size counts once.
    seen_inodes: Mutex<HashSet<u64>>,
    /// Ids of directories whose read succeeded (marked listed after the walk).
    listed_ids: Mutex<Vec<i64>>,
    /// First writer-send error, surfaced as the scan result.
    send_error: Mutex<Option<String>>,
}

impl InsertVisitor {
    fn new(
        writer: IndexWriter,
        is_volume_root: bool,
        scope: ExclusionScope,
        inodes_trustworthy: bool,
        batch_size: usize,
        progress: &ScanProgress,
        walk_cancel: CancellationToken,
    ) -> Self {
        let next_id = Arc::clone(writer.next_id());
        Self {
            writer,
            next_id,
            is_volume_root,
            scope,
            inodes_trustworthy,
            batch_size,
            entries_scanned: Arc::clone(&progress.entries_scanned),
            dirs_found: Arc::clone(&progress.dirs_found),
            bytes_scanned: Arc::clone(&progress.bytes_scanned),
            walk_cancel,
            batch: Mutex::new(Vec::with_capacity(batch_size)),
            seen_inodes: Mutex::new(HashSet::new()),
            listed_ids: Mutex::new(Vec::new()),
            send_error: Mutex::new(None),
        }
    }

    fn send_entries(&self, entries: Vec<EntryRow>) {
        if entries.is_empty() {
            return;
        }
        if let Err(e) = self.writer.send(WriteMessage::InsertEntriesV2(entries)) {
            // A send failure means the writer is gone — abort the walk and keep the
            // first error to return from the scan.
            self.walk_cancel.cancel();
            let mut slot = self.send_error.lock_ignore_poison();
            if slot.is_none() {
                *slot = Some(e.to_string());
            }
        }
    }

    fn push_row(&self, row: EntryRow) {
        let full = {
            let mut batch = self.batch.lock_ignore_poison();
            batch.push(row);
            if batch.len() >= self.batch_size {
                std::mem::take(&mut *batch)
            } else {
                Vec::new()
            }
        };
        self.send_entries(full);
    }

    /// Flush the final partial batch and surface any captured send error.
    fn finish(&self) -> Result<(), ScanError> {
        let remaining = std::mem::take(&mut *self.batch.lock_ignore_poison());
        self.send_entries(remaining);
        match self.send_error.lock_ignore_poison().take() {
            Some(msg) => Err(ScanError::WriterSend(msg)),
            None => Ok(()),
        }
    }

    fn take_listed_ids(&self) -> Vec<i64> {
        std::mem::take(&mut *self.listed_ids.lock_ignore_poison())
    }
}

impl DirVisitor for InsertVisitor {
    fn visit_dir(&self, dir: &DirTask, children: Vec<RawDirEntry>) -> Vec<DirTask> {
        // This directory's read succeeded → mark it listed at scan end.
        self.listed_ids.lock_ignore_poison().push(dir.id);

        let mut subdirs = Vec::new();
        for child in children {
            let path_str = child.path.to_string_lossy();

            // Volume-root scans apply the exclusion policy; subtree scans were
            // explicitly chosen, so global exclusions don't apply. The scope comes
            // from the volume kind: `BootDisk` for the `/`-rooted boot scan,
            // `MountRooted` for an external drive rooted at `/Volumes/X` (which must
            // index its own subtree, skipping only junk basenames).
            if self.is_volume_root && should_exclude(&path_str, &self.scope) {
                continue;
            }
            // Skip canonicalization aliases (/tmp, /var, /etc, Data-volume
            // firmlinks): the real dir owns the canonical slot. Everything we
            // actually store normalizes to itself, so the carried `parent_id` is exact.
            let normalized = firmlinks::normalize_path(&path_str);
            if is_canonicalization_alias(&path_str, &normalized) {
                continue;
            }

            let is_dir = child.file_type == RawFileType::Dir;
            let is_symlink = child.file_type == RawFileType::Symlink;

            // Prefer the reader's inline stat (macOS `getattrlistbulk` supplies it,
            // avoiding a per-entry `lstat` — the dominant local-walk cost). When the
            // reader didn't provide it (`std_read_dir`, or a bulk-read fallback),
            // stat the entry here. Both funnel through `metadata`'s rules.
            let snap = match child.stat {
                Some(s) => super::metadata::metadata_from_raw(
                    s.logical_size,
                    s.physical_size,
                    s.modified_at,
                    s.inode,
                    s.nlink,
                    is_dir,
                    is_symlink,
                ),
                None => match std::fs::symlink_metadata(&child.path) {
                    Ok(meta) => super::metadata::extract_metadata(&meta, is_dir, is_symlink),
                    Err(_) => super::metadata::MetadataSnapshot {
                        logical_size: None,
                        physical_size: None,
                        modified_at: None,
                        inode: None,
                        nlink: None,
                    },
                },
            };

            // Deduplicate hardlinks: if nlink > 1, count each inode's size once.
            //
            // On a volume without stable inodes (FAT/exFAT), never STORE the
            // derived inode: it's an unstable identity that would let the live
            // rename pre-pass false-match a reused inode. Hardlink dedup is skipped
            // there for the same reason (it keys off the inode) — and `nlink` is
            // always 1 on those formats anyway, so the branch would never fire.
            let (logical_size, physical_size, modified_at, inode) = if !self.inodes_trustworthy {
                (snap.logical_size, snap.physical_size, snap.modified_at, None)
            } else if !is_dir && !is_symlink && matches!(snap.nlink, Some(n) if n > 1) {
                let ino = snap.inode.unwrap_or(0);
                if !self.seen_inodes.lock_ignore_poison().insert(ino) {
                    (None, None, snap.modified_at, snap.inode)
                } else {
                    (snap.logical_size, snap.physical_size, snap.modified_at, snap.inode)
                }
            } else {
                (snap.logical_size, snap.physical_size, snap.modified_at, snap.inode)
            };

            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let name = child
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if is_dir {
                subdirs.push(DirTask {
                    path: child.path.clone(),
                    id,
                });
                self.dirs_found.fetch_add(1, Ordering::Relaxed);
            }

            self.entries_scanned.fetch_add(1, Ordering::Relaxed);
            // Post-dedup physical bytes: dirs, symlinks, and 2nd+ hardlinks resolved
            // to `None` contribute 0, matching the stored sums.
            let entry_physical = physical_size.unwrap_or(0);
            self.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);

            self.push_row(EntryRow {
                id,
                parent_id: dir.id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
            });
        }
        subdirs
    }

    fn note_worker_spawn_failure(&self, error: &std::io::Error) {
        // Worth a report: the walk still finishes, but at reduced parallelism, and
        // a machine that can't spawn threads has a bigger problem than this scan.
        self.writer.events().emit(IndexEvent::Error {
            report: IndexErrorReport::WalkWorkerSpawnFailed {
                detail: Diagnostic(error.to_string()),
            },
        });
    }

    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError) {
        match err {
            WalkReadError::Io(e) => {
                // Surface TCC-restricted paths so the sidebar can show the "limited
                // by macOS" styling. The host filters to known TCC prefixes.
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    self.writer
                        .events()
                        .emit(IndexEvent::PathAccessDenied { path: dir.path.clone() });
                }
                log::debug!("Scanner: skipping errored dir {}: {e}", dir.path.display());
            }
            // Timeouts are already logged by the walker watchdog; left unmarked.
            WalkReadError::TimedOut => {}
        }
    }
}
