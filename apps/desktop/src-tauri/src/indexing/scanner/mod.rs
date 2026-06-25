//! Parallel directory walker for drive indexing.
//!
//! Uses `jwalk` for fast parallel directory traversal. Provides both full-volume scan
//! (`scan_volume`) and targeted subtree scan (`scan_subtree`). Discovered entries are
//! sent in batches to the [`IndexWriter`] for insertion into the SQLite index.
//!
//! Scan exclusions (macOS system directories, virtual filesystems) are filtered via
//! jwalk's `process_read_dir` callback so excluded subtrees are never descended into.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use jwalk::WalkDir;

use crate::ignore_poison::IgnorePoison;
use crate::indexing::firmlinks;
use crate::indexing::store::{EntryRow, IndexStore, ScanContext};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use crate::pluralize::{pluralize, pluralize_with};

mod exclusions;
pub(in crate::indexing) use exclusions::*;

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
    /// Number of jwalk rayon threads (0 = auto-detect).
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
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(f, "Writer send failed: {msg}"),
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
/// Spawns a `std::thread` that walks the directory tree using jwalk, sends batches
/// of [`ScannedEntry`] to the writer, and triggers `ComputeAllAggregates` on completion.
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
            let result = run_scan(
                &config.root,
                &cancelled,
                &progress,
                &writer,
                config.batch_size,
                config.num_threads,
                true, // volume scan: root always maps to ROOT_ID
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
    let (summary, listed_ids, epoch) = run_scan(root, cancelled, &progress, writer, 2000, 0, false)?;

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

/// Walk a directory tree and send discovered entries in batches to the writer.
///
/// Uses a `ScanContext` to assign integer IDs and parent IDs to each entry.
/// The context maintains a `HashMap<PathBuf, i64>` mapping directory paths
/// to their assigned IDs. For each entry:
/// 1. Look up parent_id from the map using the entry's parent path
/// 2. Assign `id = next_id; next_id += 1`
/// 3. If directory: add `(full_path, id)` to the map
fn run_scan(
    root: &Path,
    cancelled: &AtomicBool,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
    is_volume_root: bool,
) -> Result<(ScanSummary, Vec<i64>, u64), ScanError> {
    let start = Instant::now();
    let mut batch: Vec<EntryRow> = Vec::with_capacity(batch_size);
    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut total_physical_bytes: u64 = 0;
    // Tracks inodes with nlink > 1 so each hardlinked file's size is counted only once.
    // Files with nlink == 1 (the vast majority) skip the set entirely.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Initialize the scan context: seed root mapping and get the shared ID counter.
    // Volume-root scans need a write connection (to create the root sentinel).
    // Subtree scans only need a read connection (for resolve_path).
    //
    // Read `current_epoch` once here so every dir listed this scan is stamped
    // with it (a first scan stamps epoch 1). A volume-root scan has a write
    // connection and seeds meta to "1" if absent; a subtree scan runs after a
    // full scan (epoch already present) and just reads it on its read connection.
    let (mut scan_ctx, epoch) = {
        let db_path = writer.db_path();
        let conn = if is_volume_root {
            IndexStore::open_write_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::open_read_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
        let epoch = if is_volume_root {
            IndexStore::seed_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::read_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        let ctx = ScanContext::new(&conn, root, is_volume_root, Arc::clone(writer.next_id()))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
        (ctx, epoch)
    };

    // Raw paths of every directory whose readdir succeeded (incl. empty),
    // collected by `process_read_dir` on the rayon worker threads. Resolved to
    // entry ids after the walk and returned to the caller, which emits
    // `MarkDirsListed` before the final aggregate (the ordering invariant).
    let listed_paths: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));

    // For subtree rescans, delete existing descendants first to prevent orphaned entries.
    // The scan will re-insert fresh children with correct parent-child relationships.
    // The root entry itself is preserved (ScanContext resolved its existing ID).
    if !is_volume_root && let Some(&root_id) = scan_ctx.dir_ids.get(root) {
        writer
            .send(WriteMessage::DeleteDescendantsById(root_id))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
    }

    let walker = build_walker(root, num_threads, is_volume_root, Arc::clone(&listed_paths));

    for entry_result in walker {
        if cancelled.load(Ordering::Relaxed) {
            // Flush remaining batch before returning. A cancelled scan emits no
            // marks (the caller discards/heals the partial); the disconnect path handles keeping
            // an interrupted partial honest.
            flush_batch(&mut batch, writer)?;
            return Ok((
                ScanSummary {
                    total_entries,
                    total_dirs,
                    total_physical_bytes,
                    duration_ms: start.elapsed().as_millis() as u64,
                    was_cancelled: true,
                },
                Vec::new(),
                epoch,
            ));
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                // Surface TCC-restricted paths to the frontend store so the
                // sidebar can show the "this folder is limited by macOS"
                // styling. `record_denial` filters internally to known
                // TCC-restricted prefixes (USB drives etc. are ignored).
                if let Some(io_err) = e.io_error()
                    && io_err.kind() == std::io::ErrorKind::PermissionDenied
                    && let Some(p) = e.path()
                {
                    crate::restricted_paths::record_denial(p);
                }
                log::debug!("Scanner: skipping errored entry: {e}");
                continue;
            }
        };

        // Skip the root entry itself (depth 0) to avoid storing "/" as an entry
        if entry.depth == 0 {
            continue;
        }

        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // For volume-root scans, double-check exclusions on the iteration side
        // (process_read_dir callback prevents descent, but entries can leak through
        // before the rayon callback runs). Subtree scans skip this: the caller
        // explicitly chose the subtree, so global exclusions don't apply.
        if is_volume_root && should_exclude(&path_str) {
            continue;
        }

        // Normalize via firmlinks
        let normalized = firmlinks::normalize_path(&path_str);

        // Skip canonicalization aliases (the macOS root symlinks /tmp, /var, /etc, which
        // normalize to /private/...). The real directory owns the canonical slot; storing the
        // alias collides on INSERT OR IGNORE and risks a size-losing race. Skipping before the
        // metadata syscall also avoids a needless stat. See `is_canonicalization_alias`.
        if is_canonicalization_alias(&path_str, &normalized) {
            continue;
        }
        let normalized_path = PathBuf::from(&normalized);

        let is_dir = entry.file_type().is_dir();
        let is_symlink = entry.file_type().is_symlink();

        // Get metadata for size and modified time
        let snap = match std::fs::symlink_metadata(&path) {
            Ok(meta) => super::metadata::extract_metadata(&meta, is_dir, is_symlink),
            Err(_) => super::metadata::MetadataSnapshot {
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            },
        };

        // Deduplicate hardlinks: if nlink > 1, only count each inode's size once.
        // Files with nlink == 1 (the vast majority) skip the set entirely.
        let (logical_size, physical_size, modified_at, inode) =
            if !is_dir && !is_symlink && matches!(snap.nlink, Some(n) if n > 1) {
                let ino = snap.inode.unwrap_or(0);
                if !seen_inodes.insert(ino) {
                    (None, None, snap.modified_at, snap.inode)
                } else {
                    (snap.logical_size, snap.physical_size, snap.modified_at, snap.inode)
                }
            } else {
                (snap.logical_size, snap.physical_size, snap.modified_at, snap.inode)
            };

        // Look up parent_id from the scan context
        let parent_path = normalized_path.parent().unwrap_or(root);
        let parent_id = match scan_ctx.lookup_parent(parent_path) {
            Some(pid) => pid,
            None => {
                // Parent not in map -- this can happen if the parent was excluded
                // or if jwalk delivered entries out of order. Skip.
                log::debug!("Scanner: parent not found in context for {normalized}, skipping");
                continue;
            }
        };

        // Assign a fresh ID
        let id = scan_ctx.alloc_id();

        // Compute name
        let name = entry.file_name().to_string_lossy().to_string();

        // If directory, register in the scan context so children can find their parent
        if is_dir {
            scan_ctx.register_dir(normalized_path, id);
            total_dirs += 1;
            progress.dirs_found.fetch_add(1, Ordering::Relaxed);
        }

        let scanned = EntryRow {
            id,
            parent_id,
            name,
            is_directory: is_dir,
            is_symlink,
            logical_size,
            physical_size,
            modified_at,
            inode,
        };

        total_entries += 1;
        progress.entries_scanned.fetch_add(1, Ordering::Relaxed);

        // Accumulate the resolved post-dedup physical bytes once per stored entry.
        // Placed here (after the hardlink-dedup match, alongside the entry counters)
        // so it follows the exact dedup rules of the stored sums: directories,
        // symlinks, and second+ hardlinks resolved to `None` contribute 0.
        let entry_physical = physical_size.unwrap_or(0);
        total_physical_bytes += entry_physical;
        progress.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);

        batch.push(scanned);
        if batch.len() >= batch_size {
            flush_batch(&mut batch, writer)?;
        }
    }

    // Flush final batch
    flush_batch(&mut batch, writer)?;

    // Resolve the successfully-listed dir paths to entry ids via the scan
    // context. Paths from `process_read_dir` are raw FS paths; the context keys
    // child dirs by their firmlink-normalized path (the main loop's
    // normalization), so try the normalized form first. The scan root, however,
    // is seeded into the context under its RAW path (and skipped by the main
    // loop), so fall back to the raw path — this is what stamps ROOT_ID when the
    // root's own readdir succeeds. A path that resolves to neither (an
    // excluded/aliased dir never registered) is dropped — no row to stamp.
    let listed_ids: Vec<i64> = {
        let paths = listed_paths.lock_ignore_poison();
        let mut ids = Vec::with_capacity(paths.len());
        for p in paths.iter() {
            let normalized = firmlinks::normalize_path(&p.to_string_lossy());
            let id = scan_ctx
                .lookup_parent(&PathBuf::from(normalized))
                .or_else(|| scan_ctx.lookup_parent(p));
            if let Some(id) = id {
                ids.push(id);
            }
        }
        ids
    };

    log::debug!(
        "Scanner: walk complete: {}, {} ({} listed) in {}ms",
        pluralize_with(total_entries, "entry", "entries"),
        pluralize(total_dirs, "dir"),
        listed_ids.len(),
        start.elapsed().as_millis()
    );

    Ok((
        ScanSummary {
            total_entries,
            total_dirs,
            total_physical_bytes,
            duration_ms: start.elapsed().as_millis() as u64,
            was_cancelled: false,
        },
        listed_ids,
        epoch,
    ))
}

/// Build the jwalk walker with exclusion filtering in `process_read_dir`.
///
/// `listed_paths` collects the raw path of every directory whose `readdir`
/// SUCCEEDED (jwalk calls `process_read_dir` only after a successful
/// `fs::read_dir`, including an empty-but-successful read; a wholly-errored
/// readdir surfaces as an `Err` entry in the parent and never reaches here).
/// `run_scan` later resolves these to entry ids and stamps `listed_epoch`. A
/// dir whose readdir failed is therefore never marked → it stays
/// `listed_epoch=0` (honest "unknown", not a misleading empty).
fn build_walker(
    root: &Path,
    num_threads: usize,
    is_volume_root: bool,
    listed_paths: Arc<Mutex<Vec<PathBuf>>>,
) -> WalkDir {
    let parallelism = if num_threads == 0 {
        jwalk::Parallelism::RayonNewPool(0)
    } else {
        jwalk::Parallelism::RayonNewPool(num_threads)
    };

    WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(parallelism)
        .process_read_dir(move |depth, path, _read_dir_state, children| {
            // `depth == None` is the synthetic pre-pass over the root entry's
            // own results (its parent's listing), not a directory read — skip it.
            // Every other call means `path`'s readdir succeeded, so record it.
            if depth.is_some() {
                listed_paths.lock_ignore_poison().push(path.to_path_buf());
            }
            if !is_volume_root {
                return;
            }
            // Filter out excluded directories to prevent descent into them
            children.retain(|entry_result| {
                if let Ok(entry) = entry_result {
                    let path = entry.path();
                    let path_str = path.to_string_lossy();
                    !should_exclude(&path_str)
                } else {
                    true // Keep errors so they can be logged in the main loop
                }
            });
        })
}

/// Compute parent path from a normalized path. No trailing slashes.
///
/// Examples:
/// - `/Users/foo/bar.txt` -> `/Users/foo`
/// - `/Users` -> `/`
/// - `/` -> `` (empty, should not happen since we skip root)
#[cfg(test)]
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Send a batch of entries to the writer and clear the batch buffer.
fn flush_batch(batch: &mut Vec<EntryRow>, writer: &IndexWriter) -> Result<(), ScanError> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    writer
        .send(WriteMessage::InsertEntriesV2(entries))
        .map_err(|e| ScanError::WriterSend(e.to_string()))
}
