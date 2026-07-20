//! The LOCAL full-tree reconcile rescan for the local volume.
//!
//! A LOCAL rescan of an already-populated index reconciles in place instead of
//! truncating and rebuilding: it BFS-walks the tree from the volume root over
//! `std::fs::read_dir`, diffs each directory against the DB
//! ([`reconciler::diff_dir_against_db`], shared with the live `reconcile_subtree`
//! and the network `reconcile_volume_via_trait`), and writes only the changes — so
//! the last-good directory sizes stay visible (marked stale) throughout, and a
//! rescan never mints the large freelist a mass-DELETE + bulk-reinsert does. A
//! FIRST/empty scan keeps today's truncate + parallel-walk path (the onboarding
//! moment stays fast); the `manager::start_scan` predicate picks between them.
//!
//! ## Why a separate serial walk
//!
//! The guarded parallel walker (`scanner::walker`) builds the fresh scan. The
//! reconcile is a separate serial BFS used only on the rare rescan (journal gap /
//! overflow / stale-on-launch / forced); it reuses proven per-dir diff code and a
//! single read connection, so there are no id races. Speed of the rare walk is
//! secondary to safety here, so it stays serial. Each directory read is capped by
//! a [`GuardedReader`] (15 s) so a hung File Provider mount can't freeze it; see
//! `indexing/DETAILS.md` § "The guarded local walker".
//!
//! ## Integration shape
//!
//! [`start_local_reconcile`] returns the SAME `(ScanHandle, JoinHandle<Result<
//! ScanSummary, ScanError>>)` shape as [`scanner::scan_volume`] and runs the
//! synchronous walk on a `std::thread` (NOT a tokio task). `manager::start_scan`
//! swaps it in for the `scanner::scan_volume` call on the reconcile branch, so the
//! existing completion handler — FSEvents drain → replay → `run_live_event_loop` —
//! is reused LITERALLY UNCHANGED. The shared finish (marks → one
//! `ComputeAllAggregates`) runs IN-THREAD, exactly as `scan_volume` does its
//! marks + aggregate before the thread joins.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

mod latency_probe;

use latency_probe::LatencyProbe;

use super::DEBUG_STATS;
use super::IndexPathSpace;
use super::metadata::extract_metadata;
use super::reconciler::{self, LiveChild};
use super::scanner::{LOCAL_LIST_TIMEOUT, ScanError, ScanHandle, ScanProgress, ScanSummary};
use super::store::IndexStore;
use super::writer::{IndexWriter, WriteMessage};

/// One directory's normalized filesystem children (name, metadata, is_symlink), or
/// `None` when the directory can't be listed.
type FsChildrenResult = Option<Vec<(String, std::fs::Metadata, bool)>>;

/// The read closure a [`GuardedReader`] runs on its worker thread.
type ReadFn = Arc<dyn Fn(&Path) -> FsChildrenResult + Send + Sync>;

/// 8 MB worker stack, matching the parallel scanner: a File Provider `readdir` /
/// `lstat` can descend deep XPC override chains that overflow a smaller stack.
const READER_STACK_SIZE: usize = 8 * 1024 * 1024;

/// A serial directory reader with a per-read wall-clock cap.
///
/// The reconcile walk is serial, but a single `read_dir` on a disconnected File
/// Provider mount (a hung cloud / phone provider) blocks forever, which would
/// freeze the whole rescan. This runs each read on a persistent helper thread and
/// waits at most `timeout`: a read that overruns is *abandoned* (the helper is
/// left parked in the syscall and a fresh one is spawned for the next read) and
/// reported as `None`, so the walk treats a hung dir exactly like any other
/// unlistable one — skip it, keep its prior `listed_epoch` (honest), heal later —
/// and moves on. Only a genuinely hung read ever spawns a replacement, so the cost
/// is bounded and self-clearing. Reusing one persistent worker (not a thread per
/// read) keeps a healthy full rescan free of per-directory thread churn.
struct GuardedReader {
    read_fn: ReadFn,
    timeout: Duration,
    req_tx: Sender<PathBuf>,
    res_rx: Receiver<FsChildrenResult>,
    /// Per-directory latency observability, `None` unless
    /// `CMDR_RECONCILE_LATENCY_SPIKE` is set. See [`latency_probe`].
    latency: Option<LatencyProbe>,
}

impl GuardedReader {
    /// Guard the production filesystem read (`reconciler::read_fs_children`) for the
    /// given volume path space (so a mount-rooted drive uses the right exclusion
    /// scope and skips firmlink normalization).
    fn for_fs(timeout: Duration, space: IndexPathSpace) -> Self {
        Self::with_read_fn(
            timeout,
            Arc::new(move |p: &Path| reconciler::read_fs_children(p, &space)),
        )
    }

    fn with_read_fn(timeout: Duration, read_fn: ReadFn) -> Self {
        let (req_tx, res_rx) = Self::spawn_worker(&read_fn);
        Self {
            read_fn,
            timeout,
            req_tx,
            res_rx,
            latency: LatencyProbe::from_env(Instant::now()),
        }
    }

    fn spawn_worker(read_fn: &ReadFn) -> (Sender<PathBuf>, Receiver<FsChildrenResult>) {
        let (req_tx, req_rx) = channel::<PathBuf>();
        let (res_tx, res_rx) = channel::<FsChildrenResult>();
        let read_fn = Arc::clone(read_fn);
        std::thread::Builder::new()
            .name("reconcile-read".into())
            .stack_size(READER_STACK_SIZE)
            .spawn(move || {
                // Yield CPU to the UI: this thread reads directories in the background.
                crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
                while let Ok(path) = req_rx.recv() {
                    let result = read_fn(&path);
                    // If the caller abandoned us (timed out and dropped the receiver),
                    // stop rather than spin.
                    if res_tx.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn reconcile reader thread");
        (req_tx, res_rx)
    }

    fn respawn(&mut self) {
        let (req_tx, res_rx) = Self::spawn_worker(&self.read_fn);
        self.req_tx = req_tx;
        self.res_rx = res_rx;
    }

    /// List a directory, returning `None` if it can't be listed OR the read exceeds
    /// the timeout.
    ///
    /// Every read is timed for the latency probe (when enabled), including the
    /// timed-out ones: an abandoned read still costs the serial walk its full
    /// `timeout`, so leaving it out would flatter the numbers.
    fn read(&mut self, path: &Path) -> FsChildrenResult {
        if self.latency.is_none() {
            return self.read_uninstrumented(path).0;
        }
        let started = Instant::now();
        let (result, timed_out) = self.read_uninstrumented(path);
        let now = Instant::now();
        if let Some(probe) = self.latency.as_mut() {
            probe.record(path, now.duration_since(started), timed_out, now);
        }
        result
    }

    /// The read itself. Returns the listing and whether it hit the timeout.
    fn read_uninstrumented(&mut self, path: &Path) -> (FsChildrenResult, bool) {
        if self.req_tx.send(path.to_path_buf()).is_err() {
            // Worker gone (a previous read is still parked, or it panicked): get a
            // fresh one and retry once.
            self.respawn();
            if self.req_tx.send(path.to_path_buf()).is_err() {
                return (None, false);
            }
        }
        match self.res_rx.recv_timeout(self.timeout) {
            Ok(result) => (result, false),
            Err(RecvTimeoutError::Timeout) => {
                log::warn!(
                    "local reconcile: read timed out after {:?}, abandoning {} (kept stale, heals later)",
                    self.timeout,
                    path.display()
                );
                self.respawn();
                (None, true)
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The worker exited without answering (e.g. `read_fn` panicked). Get a
                // fresh one; report this read as unlistable.
                self.respawn();
                (None, false)
            }
        }
    }
}

impl Drop for GuardedReader {
    fn drop(&mut self) {
        if let Some(probe) = self.latency.as_ref() {
            probe.finish(Instant::now());
        }
    }
}

/// Start a LOCAL full-tree reconcile on a background `std::thread`.
///
/// Mirrors [`scanner::scan_volume`]'s return shape so `manager::start_scan`'s
/// completion handler is reused unchanged: a [`ScanHandle`] for progress +
/// cancellation, and a `JoinHandle` the handler joins for the [`ScanSummary`].
pub(super) fn start_local_reconcile(
    root: PathBuf,
    space: IndexPathSpace,
    writer: &IndexWriter,
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = ScanHandle::new(Arc::clone(&progress), Arc::clone(&cancelled));

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-local-reconcile".into())
        .spawn(move || {
            // Yield CPU to the UI: reconcile walks the tree in the background.
            crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
            // Catch a panic INSIDE the walk and convert it to a typed
            // `ScanError::Panicked` so the `JoinHandle` resolves to
            // `Ok(Err(_))` (clean logged message + `ScanFailed` ⇒ Stale) rather
            // than a raw thread panic that surfaces as the handler's opaque
            // `Err(_)` "thread panicked" arm.
            run_catching_panics(|| run_local_reconcile(&root, &space, &writer, &progress, &cancelled))
        })
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Run a reconcile closure, converting any panic inside it into a typed
/// [`ScanError::Panicked`] (carrying the panic message) instead of unwinding the
/// scanner thread. See `start_local_reconcile` for why.
fn run_catching_panics(f: impl FnOnce() -> Result<ScanSummary, ScanError>) -> Result<ScanSummary, ScanError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(inner) => inner,
        Err(payload) => Err(ScanError::Panicked(panic_message(payload.as_ref()))),
    }
}

/// Best-effort human-readable text from a panic payload. `panic!` / `assert!`
/// produce either a `&'static str` or a formatted `String`; handle both, with a
/// fallback for anything else.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Normalize one directory's filesystem children into source-agnostic
/// [`LiveChild`]s, accumulating the live counters the progress bar reads (so a
/// multi-minute reconcile doesn't show a frozen bar) and the summary totals.
///
/// `seen_inodes` is the SINGLE set threaded across the whole BFS (NOT per-dir),
/// mirroring the fresh scan's one global `seen_inodes` in `run_scan`: it dedups hardlinks
/// so each inode's physical bytes land in `total_physical_bytes` / `bytes_scanned`
/// exactly ONCE, keeping the reconcile's `ScanSummary` byte totals identical to a
/// fresh scan of the same tree. Files with `nlink == 1` skip the set entirely.
///
/// ❌ Don't also zero the `LiveChild` snapshot's sizes here (the way `run_scan`
/// zeroes its `EntryRow`). The PERSISTED entries are deduped one layer down, by the
/// writer's `UpsertEntryV2` (`has_sized_entry_for_inode`); zeroing here would race
/// that dedup. The reconcile's first-seen-keeps choice is independent of which
/// occurrence the DB currently sizes, so on a mismatch the diff would send
/// `Upsert(old-null, S)` + `Upsert(old-sized, None)` and the writer could null BOTH,
/// UNDER-counting the inode. So the snapshot stays raw and only the totals dedup.
fn build_live_children(
    fs_children: &[(String, std::fs::Metadata, bool)],
    space: &IndexPathSpace,
    seen_inodes: &mut HashSet<u64>,
    total_entries: &mut u64,
    total_dirs: &mut u64,
    total_physical_bytes: &mut u64,
    progress: &ScanProgress,
) -> Vec<LiveChild> {
    // Pathological-directory census. This hook is the one that reads non-zero on
    // an established machine: a populated, previously-completed index reconciles
    // rather than running the guarded walker, so a walker-only census would stay
    // at zero on exactly the machines worth sampling.
    DEBUG_STATS.record_dir_listing(fs_children.len());

    let mut live = Vec::with_capacity(fs_children.len());
    for (name, meta, is_symlink) in fs_children {
        let is_dir = meta.is_dir();
        let mut snap = extract_metadata(meta, is_dir, *is_symlink);
        // Null the inode on FAT/exFAT (unstable derived inode): the stored value
        // must never let the live rename pre-pass false-match a reused inode. The
        // byte-total dedup below is inert on those formats (`nlink` is always 1).
        snap.inode = space.trust_inode(snap.inode);
        // Hardlink dedup for the byte totals, matching `run_scan`: count each inode's
        // physical bytes once. `insert` returns false on a repeat inode → contributes 0.
        let counts_physical = if !is_dir && !*is_symlink && matches!(snap.nlink, Some(n) if n > 1) {
            seen_inodes.insert(snap.inode.unwrap_or(0))
        } else {
            true
        };
        let entry_physical = if counts_physical {
            snap.physical_size.unwrap_or(0)
        } else {
            0
        };
        *total_physical_bytes += entry_physical;
        *total_entries += 1;
        progress.entries_scanned.fetch_add(1, Ordering::Relaxed);
        progress.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);
        if is_dir {
            *total_dirs += 1;
            progress.dirs_found.fetch_add(1, Ordering::Relaxed);
        }
        live.push(LiveChild {
            name: name.clone(),
            is_directory: is_dir,
            is_symlink: *is_symlink,
            snap,
        });
    }
    live
}

fn summary(entries: u64, dirs: u64, physical_bytes: u64, start: Instant, cancelled: bool) -> ScanSummary {
    ScanSummary {
        total_entries: entries,
        total_dirs: dirs,
        total_physical_bytes: physical_bytes,
        duration_ms: start.elapsed().as_millis() as u64,
        was_cancelled: cancelled,
    }
}

/// The synchronous LOCAL reconcile walk. Runs on the scanner thread.
///
/// Serial BFS from the volume root over `std::fs::read_dir`, diffing each dir
/// against the DB ([`reconciler::diff_dir_against_db`]) and writing only changes,
/// then the shared finish (marks before the single `ComputeAllAggregates`). Honors
/// the cancel flag, the empty-root guard, the read-only connection, the
/// `(parent_id, name)` new-dir resolution + shared id counter, and the
/// recurse-into-every-matched-child-dir rule. Keeps the read connection in
/// autocommit (no long-lived `BEGIN` read txn) so post-flush new-dir resolves see
/// fresh rows.
fn run_local_reconcile(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    progress: &ScanProgress,
    cancelled: &AtomicBool,
) -> Result<ScanSummary, ScanError> {
    let start = Instant::now();
    let db_path = writer.db_path();

    // A READ connection. A write-mode connection's pragmas can `SQLITE_BUSY`
    // and silently kill live indexing.
    let conn = IndexStore::open_read_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?;
    // `start_scan` already bumped + flushed `current_epoch` before spawning this
    // walk, so read the bumped value back and stamp every re-listed dir with it.
    let epoch = IndexStore::read_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?;

    // The volume root maps to its DB id (`ROOT_ID`). For the boot disk
    // `resolve_path("/")` is `ROOT_ID`; for a mount-rooted drive the mount root
    // (`/Volumes/X`) strips to `/` and resolves to `ROOT_ID` the same way — the
    // strip is applied at `resolve_abs`, while `root_str` stays absolute for the FS
    // walk below. Resolving it (rather than hardcoding `ROOT_ID`) also lets the
    // walker be exercised from any root in tests.
    let root_str = space.absolute(&root.to_string_lossy());
    let root_id = match space
        .resolve_abs(&conn, &root_str)
        .map_err(|e| ScanError::WriterSend(e.to_string()))?
    {
        Some(id) => id,
        None => {
            return Err(ScanError::Io(std::io::Error::other(
                "local reconcile: root is not in the index",
            )));
        }
    };

    let mut listed_ids: Vec<i64> = Vec::new();
    // ONE set for the whole walk (NOT per-dir), exactly like the fresh scan's single
    // `seen_inodes` in `run_scan`: dedups hardlinks across the entire tree so an
    // inode's bytes hit the summary totals once. See `build_live_children`.
    let mut seen_inodes: HashSet<u64> = HashSet::new();
    let mut total_entries = 0u64;
    let mut total_dirs = 0u64;
    let mut total_physical_bytes = 0u64;
    let (mut added, mut removed, mut updated) = (0u64, 0u64, 0u64);

    // BFS by (absolute dir path, its DB id). New dirs discovered this pass are
    // resolved to ids after a writer flush before we recurse into them.
    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));
    // (parent dir path, parent DB id, child name): resolved by `(parent_id, name)`
    // after a level's flush, never by absolute path.
    let mut new_dirs: Vec<(PathBuf, i64, String)> = Vec::new();

    // Suppress per-entry ancestor propagation for the bulk walk; the guard restores
    // it on EVERY exit (clean finish, cancel, empty-root, error). The shared finish
    // recomputes all dir_stats via one `ComputeAllAggregates`, so the per-entry walk
    // would be redundant O(entries × depth) work that wedges the writer on a large
    // delta. See `reconciler::BulkReconcileGuard`.
    let _bulk_guard = reconciler::BulkReconcileGuard::begin(writer);

    // Each directory read is capped at `LOCAL_LIST_TIMEOUT`: a hung File Provider
    // mount is abandoned and treated as unlistable instead of freezing the rescan.
    let mut reader = GuardedReader::for_fs(LOCAL_LIST_TIMEOUT, space.clone());

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            // Cancel: leave the prior index intact (no truncate ran) and send NO
            // marks/aggregate. Accepted drift window: the walk ran under the
            // `BulkReconcileGuard` (`SetDeltaPropagation(false)`), so entries
            // already diffed this pass got NO ancestor `dir_stats` propagation and
            // there's no final aggregate to reconcile them — those ancestors read
            // stale until the next COMPLETED rescan heals them (with no
            // `scan_completed_at`, the next launch re-reconciles).
            return Ok(summary(total_entries, total_dirs, total_physical_bytes, start, true));
        }

        let fs_children = match reader.read(&dir_path) {
            Some(c) => c,
            None => {
                if dir_path == *root {
                    // The ROOT itself is unlistable (its read errored or timed out):
                    // for a mount-rooted drive this means the mount VANISHED
                    // mid-reconcile (a yanked USB stick / SD card). Surface the typed
                    // `RootUnlistable` so the completion handler writes no
                    // `scan_completed_at` AND emits `index-scan-aborted` (clearing the
                    // stuck "scanning" row); the prior index is untouched and heals on
                    // a later pass. Distinct from `EmptyRoot` (a readable-but-empty
                    // root), which does NOT abort.
                    return Err(ScanError::RootUnlistable);
                }
                // A sub-directory we can't list: skip it. It keeps its old
                // `listed_epoch` (honest "stale/unknown") and heals on a later pass.
                continue;
            }
        };

        // Empty-root guard: if the VOLUME ROOT lists empty, bail BEFORE diffing
        // it — otherwise the diff sees an empty live listing and DELETES every
        // existing child, blanking the index. A reconcile only runs over an
        // already-populated index, so an empty root is a transient half-dead `/`,
        // not a real "everything deleted". A non-root dir that lists empty is a
        // genuine empty subdir and reconciles normally (its stale children are swept).
        if dir_path == *root && fs_children.is_empty() {
            log::warn!(
                "local reconcile: root listed empty for {} — treating as a failed rescan, keeping prior index",
                dir_path.display()
            );
            return Err(ScanError::EmptyRoot);
        }

        // This dir's listing succeeded (incl. empty) — stamp it after the walk.
        listed_ids.push(dir_id);

        let db_children =
            IndexStore::list_children_on(dir_id, &conn).map_err(|e| ScanError::WriterSend(e.to_string()))?;
        let live_children = build_live_children(
            &fs_children,
            space,
            &mut seen_inodes,
            &mut total_entries,
            &mut total_dirs,
            &mut total_physical_bytes,
            progress,
        );

        let diff = reconciler::diff_dir_against_db(dir_id, &live_children, &db_children, writer);
        added += diff.added;
        removed += diff.removed;
        updated += diff.updated;
        // Recurse into EVERY matched child dir (changed or not).
        for (child_id, child_name) in diff.matched_child_dirs {
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            new_dirs.push((dir_path.clone(), dir_id, child_name));
        }

        // Level drained + new dirs created: flush so the read connection sees their
        // freshly-assigned ids, then queue them for recursion. ❌ Don't wrap the walk
        // in one `BEGIN` read txn — autocommit per-dir reads keep the snapshot fresh
        // so these post-flush resolves see the new rows (and avoid freelist pinning).
        if !new_dirs.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("local reconcile: flush before resolving new dirs failed: {e}");
            }
            for (parent_path, parent_id, child_name) in new_dirs.drain(..) {
                let child_path = parent_path.join(&child_name);
                // Resolve by `(parent_id, name)`: single-component lookup under
                // the id we already hold, robust to any root.
                match IndexStore::resolve_component(&conn, parent_id, &child_name) {
                    Ok(Some(id)) => queue.push_back((child_path, id)),
                    Ok(None) => log::debug!(
                        "local reconcile: couldn't resolve new dir after flush: {}",
                        child_path.display()
                    ),
                    Err(e) => log::warn!(
                        "local reconcile: resolve_component failed for {}: {e}",
                        child_path.display()
                    ),
                }
            }
        }
    }

    // Clean finish: stamp every re-listed dir (marks before the single aggregate), then ONE `ComputeAllAggregates`
    // (never per-dir propagation), then trim the post-rescan WAL spike.
    reconciler::finish_reconcile(&listed_ids, epoch, writer).map_err(|e| ScanError::WriterSend(e.to_string()))?;
    writer
        .send(WriteMessage::WalCheckpoint)
        .map_err(|e| ScanError::WriterSend(e.to_string()))?;

    log::info!(
        "local reconcile: complete for {}: +{added} -{removed} ~{updated} ({} re-listed) in {}ms",
        root.display(),
        crate::pluralize::pluralize(total_dirs, "dir"),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start, false))
}

#[cfg(test)]
mod tests;
