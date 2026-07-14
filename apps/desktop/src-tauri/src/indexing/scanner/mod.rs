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
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;
use crate::indexing::firmlinks;
use crate::indexing::store::{EntryRow, IndexStore, resolve_scan_root};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use crate::pluralize::{pluralize, pluralize_with};

mod exclusions;
pub(in crate::indexing) use exclusions::*;

mod walker;
use walker::{DirTask, DirVisitor, RawDirEntry, RawFileType, ReadDirFn, WalkConfig, WalkReadError, std_read_dir, walk};

/// Per-directory read timeout for the LOCAL walk. Sits above any legitimate
/// slow-but-alive provider listing (an online cloud dir lists in well under a
/// second) while abandoning a disconnected File Provider mount quickly; a
/// timed-out dir prunes its subtree, so a dead mount costs a handful of frontier
/// dirs, not thousands. (The network scanner's 120 s is tuned for SMB-over-WAN.)
pub(in crate::indexing) const LOCAL_LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the walker watchdog checks for over-timeout reads (also the ceiling
/// on caller-cancel latency).
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests;

/// Number of dir ids per `MarkDirsListed` message (mirrors `volume_scanner`).
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
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            batch_size: 2000,
            num_threads: 0,
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
    cancelled: Arc<AtomicBool>,
}

impl ScanHandle {
    /// Build a handle around an existing progress + cancel pair. Used by the
    /// `Volume`-trait scanner (`volume_scanner`), which owns the walk itself and
    /// just needs the manager-facing progress/cancel surface.
    pub(crate) fn new(progress: Arc<ScanProgress>, cancelled: Arc<AtomicBool>) -> Self {
        Self { progress, cancelled }
    }

    /// Signal the scan to stop. Already-written data remains in the DB.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Summary returned when a scan completes (or is cancelled).
#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_entries: u64,
    pub total_dirs: u64,
    /// Resolved post-dedup physical bytes the scan summed (the final value of the
    /// `bytes_scanned` counter). Apples-to-apples with the stored physical-size sums.
    pub total_physical_bytes: u64,
    pub duration_ms: u64,
    pub was_cancelled: bool,
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
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let cancelled = Arc::new(AtomicBool::new(false));

    let handle = ScanHandle {
        progress: Arc::clone(&progress),
        cancelled: Arc::clone(&cancelled),
    };

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-scanner".into())
        .spawn(move || {
            let reader: ReadDirFn = Arc::new(std_read_dir);
            let result = run_scan(
                &config.root,
                &cancelled,
                &progress,
                &writer,
                config.batch_size,
                config.num_threads,
                true, // volume scan: root always maps to ROOT_ID
                reader,
                LOCAL_LIST_TIMEOUT,
            );

            // On a clean finish: stamp the listed dirs FIRST, then aggregate,
            // then trim the WAL. The mark→aggregate order is the ordering
            // invariant (a mark queued behind the final aggregate would leave
            // that dir at epoch 0 and roll the whole tree to incomplete). The
            // single in-order writer enforces it. The WAL checkpoint trims the
            // GB-scale post-scan spike now instead of waiting for the ticker.
            if let Ok((summary, listed_ids, epoch)) = &result
                && !summary.was_cancelled
            {
                send_marks(listed_ids, *epoch, &writer);
                if let Err(e) = writer.send(WriteMessage::ComputeAllAggregates) {
                    log::warn!("Scanner: failed to send ComputeAllAggregates: {e}");
                } else if let Err(e) = writer.send(WriteMessage::WalCheckpoint) {
                    log::warn!("Scanner: failed to send post-scan WalCheckpoint: {e}");
                }
            }

            result.map(|(summary, _, _)| summary)
        })
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Synchronous subtree scan. Runs in the caller's thread.
///
/// Used by post-replay background verification. After scanning, sends
/// `ComputeSubtreeAggregates` to the writer.
pub fn scan_subtree(root: &Path, writer: &IndexWriter, cancelled: &AtomicBool) -> Result<ScanSummary, ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let reader: ReadDirFn = Arc::new(std_read_dir);
    let (summary, listed_ids, epoch) = run_scan(root, cancelled, &progress, writer, 2000, 0, false, reader, LOCAL_LIST_TIMEOUT)?;

    if !summary.was_cancelled {
        // Stamp the listed dirs before the subtree aggregate (ordering invariant).
        send_marks(&listed_ids, epoch, writer);
        let root_str = root.to_string_lossy().to_string();
        if let Err(e) = writer.send(WriteMessage::ComputeSubtreeAggregates { root: root_str }) {
            log::warn!("Scanner: failed to send ComputeSubtreeAggregates: {e}");
        }
    }

    Ok(summary)
}

// ── Core scan logic ──────────────────────────────────────────────────

/// Walk the local tree from `root` and insert every discovered entry, guarded so
/// a hung directory read can't stall the scan (see [`walker`]).
///
/// Parent attribution needs no path→id map: [`walk`] carries each directory's id
/// to its own read, so children take their parent's id directly. Ids come from the
/// shared `IndexWriter` counter. The scan root maps to `ROOT_ID` (volume scans) or
/// its existing entry id (subtree scans). `read_timeout` is the per-directory
/// wall-clock cap (production passes `LOCAL_LIST_TIMEOUT`; tests pass a short one).
#[allow(
    clippy::too_many_arguments,
    reason = "internal scan entry point threading writer/progress/config; a param struct would add indirection without clarity"
)]
fn run_scan(
    root: &Path,
    cancelled: &AtomicBool,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
    is_volume_root: bool,
    reader: ReadDirFn,
    read_timeout: Duration,
) -> Result<(ScanSummary, Vec<i64>, u64), ScanError> {
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

    let walk_cancel = Arc::new(AtomicBool::new(cancelled.load(Ordering::Relaxed)));
    let visitor = Arc::new(InsertVisitor::new(
        writer.clone(),
        is_volume_root,
        batch_size,
        progress,
        Arc::clone(&walk_cancel),
    ));

    // Watchdog ticks faster than the timeout (production 15s → 1s; a short test
    // timeout scales down, floored at 5ms).
    let watchdog_interval = (read_timeout / 15).clamp(Duration::from_millis(5), WATCHDOG_INTERVAL);
    let cfg = WalkConfig {
        num_threads,
        read_timeout,
        watchdog_interval,
    };
    let root_task = DirTask {
        path: root.to_path_buf(),
        id: root_id,
    };

    // The walker's workers are `'static`, so they need an `Arc` cancel flag, but
    // `run_scan` only borrows `cancelled`. A scoped bridge thread borrows it and
    // mirrors it into `walk_cancel`; it stops the moment the walk returns
    // (unparked) or the caller cancels.
    let walk_stats = std::thread::scope(|s| {
        let bridge_stop = Arc::new(AtomicBool::new(false));
        let bridge = {
            let bridge_stop = Arc::clone(&bridge_stop);
            let walk_cancel = Arc::clone(&walk_cancel);
            s.spawn(move || {
                loop {
                    if bridge_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if cancelled.load(Ordering::Relaxed) {
                        walk_cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                    std::thread::park_timeout(Duration::from_millis(100));
                }
            })
        };
        let stats = walk(root_task, cfg, reader, Arc::clone(&visitor), Arc::clone(&walk_cancel));
        bridge_stop.store(true, Ordering::Relaxed);
        bridge.thread().unpark();
        stats
    });

    // Flush the final batch and surface any writer-send failure.
    visitor.finish()?;

    if walk_stats.timed_out > 0 {
        log::warn!(
            "Scanner: {} skipped after {}s each (hung / disconnected dirs)",
            pluralize(walk_stats.timed_out, "dir"),
            read_timeout.as_secs(),
        );
    }

    let was_cancelled = cancelled.load(Ordering::Relaxed);
    // A cancelled scan emits no marks (the caller discards/heals the partial).
    let listed_ids = if was_cancelled { Vec::new() } else { visitor.take_listed_ids() };
    let snap = progress.snapshot();

    log::debug!(
        "Scanner: walk complete: {}, {} ({} listed) in {}ms",
        pluralize_with(snap.entries_scanned, "entry", "entries"),
        pluralize(snap.dirs_found, "dir"),
        listed_ids.len(),
        start.elapsed().as_millis()
    );

    Ok((
        ScanSummary {
            total_entries: snap.entries_scanned,
            total_dirs: snap.dirs_found,
            total_physical_bytes: snap.bytes_scanned,
            duration_ms: start.elapsed().as_millis() as u64,
            was_cancelled,
        },
        listed_ids,
        epoch,
    ))
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
    batch_size: usize,
    /// Live progress counters (shared with the manager-facing `ScanHandle`); the
    /// scan summary reads their final values.
    entries_scanned: Arc<AtomicU64>,
    dirs_found: Arc<AtomicU64>,
    bytes_scanned: Arc<AtomicU64>,
    /// Set when a writer send fails, to abort the walk promptly.
    walk_cancel: Arc<AtomicBool>,
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
        batch_size: usize,
        progress: &ScanProgress,
        walk_cancel: Arc<AtomicBool>,
    ) -> Self {
        let next_id = Arc::clone(writer.next_id());
        Self {
            writer,
            next_id,
            is_volume_root,
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
            self.walk_cancel.store(true, Ordering::Relaxed);
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
            // explicitly chosen, so global exclusions don't apply.
            if self.is_volume_root && should_exclude(&path_str) {
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

            let snap = match std::fs::symlink_metadata(&child.path) {
                Ok(meta) => super::metadata::extract_metadata(&meta, is_dir, is_symlink),
                Err(_) => super::metadata::MetadataSnapshot {
                    logical_size: None,
                    physical_size: None,
                    modified_at: None,
                    inode: None,
                    nlink: None,
                },
            };

            // Deduplicate hardlinks: if nlink > 1, count each inode's size once.
            let (logical_size, physical_size, modified_at, inode) =
                if !is_dir && !is_symlink && matches!(snap.nlink, Some(n) if n > 1) {
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

    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError) {
        match err {
            WalkReadError::Io(e) => {
                // Surface TCC-restricted paths so the sidebar can show the "limited
                // by macOS" styling. `record_denial` filters to known TCC prefixes.
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    crate::restricted_paths::record_denial(&dir.path);
                }
                log::debug!("Scanner: skipping errored dir {}: {e}", dir.path.display());
            }
            // Timeouts are already logged by the walker watchdog; left unmarked.
            WalkReadError::TimedOut => {}
        }
    }
}
