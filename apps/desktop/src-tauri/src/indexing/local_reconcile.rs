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
    fn read(&mut self, path: &Path) -> FsChildrenResult {
        if self.req_tx.send(path.to_path_buf()).is_err() {
            // Worker gone (a previous read is still parked, or it panicked): get a
            // fresh one and retry once.
            self.respawn();
            if self.req_tx.send(path.to_path_buf()).is_err() {
                return None;
            }
        }
        match self.res_rx.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                log::warn!(
                    "local reconcile: read timed out after {:?}, abandoning {} (kept stale, heals later)",
                    self.timeout,
                    path.display()
                );
                self.respawn();
                None
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The worker exited without answering (e.g. `read_fn` panicked). Get a
                // fresh one; report this read as unlistable.
                self.respawn();
                None
            }
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
            // Cancel: leave the prior index intact (no truncate ran) and send
            // NO marks/aggregate. Any partial entry writes already applied stay
            // size-consistent (`UpsertEntryV2`/`Delete*` auto-propagate dir_stats);
            // with no `scan_completed_at`, the next launch re-reconciles.
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use rusqlite::Connection;

    use super::*;
    use crate::indexing::firmlinks;
    use crate::indexing::store::{self, DirStatsById, IndexStore, ROOT_ID};
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    #[test]
    fn guarded_reader_returns_a_quick_result() {
        let read_fn: ReadFn = Arc::new(|_p| Some(vec![]));
        let mut reader = GuardedReader::with_read_fn(Duration::from_secs(5), read_fn);
        assert!(reader.read(Path::new("/x")).is_some(), "a fast read returns its result");
    }

    #[test]
    fn guarded_reader_abandons_a_hung_read_and_recovers() {
        use std::sync::atomic::AtomicUsize;
        // Only the FIRST read hangs; later reads are fast. This proves both that the
        // hung read is abandoned near the timeout (not waited out) AND that the reader
        // recovers — respawns a worker — for the next read.
        let calls = Arc::new(AtomicUsize::new(0));
        let read_fn: ReadFn = {
            let calls = Arc::clone(&calls);
            Arc::new(move |_p| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::thread::sleep(Duration::from_secs(2));
                }
                Some(vec![])
            })
        };
        let mut reader = GuardedReader::with_read_fn(Duration::from_millis(50), read_fn);

        let start = Instant::now();
        assert!(reader.read(Path::new("/hang")).is_none(), "a hung read returns None");
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "must abandon near the timeout, not wait out the ~2s hang (elapsed {:?})",
            start.elapsed()
        );
        assert!(
            reader.read(Path::new("/ok")).is_some(),
            "the reader recovers after a timeout and serves the next read",
        );
    }

    struct Harness {
        writer: IndexWriter,
        db_path: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn setup() -> Harness {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("local-reconcile.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        // Seed current_epoch = 1 so the first reconcile stamps a real value.
        writer
            .send(WriteMessage::UpdateMeta {
                key: store::CURRENT_EPOCH_KEY.to_string(),
                value: "1".to_string(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        Harness {
            writer,
            db_path,
            _dir: dir,
        }
    }

    /// A tree root under CWD (not /tmp — excluded on Linux, an alias on macOS).
    fn tree_root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("cmdr_local_reconcile_")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("tempdir in cwd")
    }

    fn norm(p: &Path) -> String {
        firmlinks::normalize_path(&p.to_string_lossy())
    }

    fn conn(h: &Harness) -> Connection {
        IndexStore::open_read_connection(&h.db_path).expect("read conn")
    }

    /// Seed every component of `abs_path` as a directory entry (mirrors what a full
    /// scan does before live events). Syncs the writer's next_id afterward.
    fn ensure_path_in_db(h: &Harness, abs_path: &str) {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        let mut current_id = ROOT_ID;
        for component in abs_path.strip_prefix('/').unwrap_or(abs_path).split('/') {
            if component.is_empty() {
                continue;
            }
            match IndexStore::resolve_component(&wconn, current_id, component).unwrap() {
                Some(id) => current_id = id,
                None => {
                    current_id =
                        IndexStore::insert_entry_v2(&wconn, current_id, component, true, false, None, None, None, None)
                            .unwrap();
                }
            }
        }
        let db_next_id = IndexStore::get_next_id(&wconn).unwrap();
        h.writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    /// Insert a single child entry directly (simulating a prior populated index),
    /// then sync the writer's next_id.
    fn insert_child(h: &Harness, parent_id: i64, name: &str, is_dir: bool, size: Option<u64>) {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        IndexStore::insert_entry_v2(&wconn, parent_id, name, is_dir, false, size, size, None, None).unwrap();
        let db_next_id = IndexStore::get_next_id(&wconn).unwrap();
        h.writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    fn resolve(h: &Harness, p: &Path) -> Option<i64> {
        store::resolve_path(&conn(h), &norm(p)).unwrap()
    }

    fn listed_epoch(h: &Harness, id: i64) -> Option<u64> {
        IndexStore::get_listed_epoch_by_id(&conn(h), id).unwrap()
    }

    fn dir_stats(h: &Harness, id: i64) -> Option<DirStatsById> {
        IndexStore::get_dir_stats_by_id(&conn(h), id).unwrap()
    }

    fn bump_epoch(h: &Harness) -> u64 {
        let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    }

    // ── catch_unwind wrapping ─────────────────────────────────────────────

    #[test]
    fn panic_in_walk_becomes_typed_scan_error() {
        // A panicking walk is converted to the typed `Panicked` variant rather than
        // a raw thread panic, for both panic-payload shapes. The variant is what
        // the completion handler matches on; the message round-trip is checked
        // separately in `panic_message_extracts_str_and_string_payloads` (asserting
        // a panic message's content here would substring-match an error value,
        // which the `error-string-match` check forbids).
        let str_payload = run_catching_panics(|| panic!("boom in the walk"));
        assert!(
            matches!(str_payload, Err(ScanError::Panicked(_))),
            "got: {str_payload:?}"
        );

        let string_payload = run_catching_panics(|| panic!("{}", String::from("formatted boom")));
        assert!(
            matches!(string_payload, Err(ScanError::Panicked(_))),
            "got: {string_payload:?}"
        );
    }

    /// The panic-payload downcast preserves the message for both the `&'static str`
    /// (`panic!("literal")`) and `String` (`panic!("{}", s)`) payload shapes. Tests
    /// the helper directly so the assertion is on a plain returned `String`, not on
    /// a destructured error value.
    #[test]
    fn panic_message_extracts_str_and_string_payloads() {
        let from_str = panic_message(&"literal payload" as &(dyn std::any::Any + Send));
        assert_eq!(from_str, "literal payload");

        let from_string = panic_message(&String::from("string payload") as &(dyn std::any::Any + Send));
        assert_eq!(from_string, "string payload");
    }

    #[test]
    fn no_panic_passes_the_result_through() {
        let summary = ScanSummary {
            total_entries: 7,
            total_dirs: 2,
            total_physical_bytes: 42,
            duration_ms: 1,
            was_cancelled: false,
        };
        let passed = run_catching_panics(|| Ok(summary.clone()));
        assert!(matches!(passed, Ok(s) if s.total_entries == 7 && s.total_physical_bytes == 42));

        let errored = run_catching_panics(|| Err(ScanError::EmptyRoot));
        assert!(matches!(errored, Err(ScanError::EmptyRoot)));
    }

    /// Run the reconcile walk synchronously and flush. `cancel` pre-trips the cancel
    /// flag (deterministic interruption).
    fn run_reconcile(h: &Harness, root: &Path, cancel: bool) -> Result<ScanSummary, ScanError> {
        let progress = ScanProgress::new();
        let flag = AtomicBool::new(cancel);
        // The existing tests seed the FULL absolute path chain in the DB, so the
        // `root` space (absolute == index-relative) is what round-trips them; the
        // mount-rooted variant is exercised by `reconcile_resolves_mount_rooted_root`.
        let result = run_local_reconcile(root, &IndexPathSpace::root(), &h.writer, &progress, &flag);
        h.writer.flush_blocking().unwrap();
        result
    }

    /// A reconcile must descend into existing child dirs whose own metadata
    /// did NOT change — recursion is decoupled from the write decision. Pinned by
    /// re-listing every dir to the new epoch on a no-change rescan.
    #[test]
    fn reconcile_descends_into_existing_unchanged_child_dirs() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir_all(rp.join("a/deep")).unwrap();
        std::fs::write(rp.join("a/deep/leaf.txt"), b"x").unwrap();

        // Epoch 1: build the index from disk.
        run_reconcile(&h, rp, false).expect("first reconcile");
        let a = resolve(&h, &rp.join("a")).expect("a indexed");
        let deep = resolve(&h, &rp.join("a/deep")).expect("deep indexed");
        assert_eq!(listed_epoch(&h, deep), Some(1), "deep listed at epoch 1");

        // Epoch 2 reconcile with NO disk changes: every dir must be re-listed.
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");
        assert_eq!(listed_epoch(&h, a), Some(2), "/a re-listed");
        assert_eq!(
            listed_epoch(&h, deep),
            Some(2),
            "unchanged child dir must still be re-listed"
        );
    }

    /// Deletion sweep (data-safety): an entry removed on disk is removed from the
    /// index after a reconcile, including one nested under a subdir (proving the
    /// walk recurses to reach it).
    #[test]
    fn reconcile_removes_entry_deleted_on_disk() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/keep.txt"), b"keep").unwrap();
        std::fs::write(rp.join("sub/gone.txt"), b"gone-soon").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let sub = resolve(&h, &rp.join("sub")).expect("sub indexed");
        assert_eq!(dir_stats(&h, sub).expect("sub stats").recursive_file_count, 2);

        std::fs::remove_file(rp.join("sub/gone.txt")).unwrap();
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");

        assert!(
            resolve(&h, &rp.join("sub/gone.txt")).is_none(),
            "deleted file gone from index"
        );
        assert!(
            resolve(&h, &rp.join("sub/keep.txt")).is_some(),
            "kept file still present"
        );
        assert_eq!(
            dir_stats(&h, sub).expect("sub stats after").recursive_file_count,
            1,
            "file count drops by one"
        );
    }

    /// A modified file's new size propagates up into ancestor dir_stats.
    #[test]
    fn reconcile_modified_file_propagates_to_ancestor_dir_stats() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/f.txt"), b"orig").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let root_id = resolve(&h, rp).expect("root in db");
        let size0 = dir_stats(&h, root_id).expect("root stats").recursive_logical_size;

        let bigger = b"a much longer body than before";
        std::fs::write(rp.join("sub/f.txt"), bigger).unwrap();
        bump_epoch(&h);
        run_reconcile(&h, rp, false).expect("second reconcile");

        let size1 = dir_stats(&h, root_id).expect("root stats after").recursive_logical_size;
        assert!(
            size1 > size0,
            "ancestor size grew after the file grew ({size0} -> {size1})"
        );
        assert_eq!(size1, bigger.len() as u64, "ancestor size equals the new file size");
    }

    /// Empty-root guard (data-safety): when the volume root lists empty, the
    /// reconcile returns `EmptyRoot` and does NOT blank the prior index. The
    /// completion handler maps `Err` to "no scan_completed_at" (unchanged manager
    /// logic), so the index heals on the next launch.
    #[test]
    fn reconcile_empty_root_keeps_prior_index_and_signals_empty_root() {
        let h = setup();
        let root = tree_root();
        let rp = root.path(); // empty on disk
        ensure_path_in_db(&h, &norm(rp));
        let rp_id = resolve(&h, rp).expect("root seeded");
        // Simulate a prior populated index: a child the live (empty) root no longer shows.
        insert_child(&h, rp_id, "ghost.txt", false, Some(10));
        h.writer.flush_blocking().unwrap();
        assert!(
            resolve(&h, &rp.join("ghost.txt")).is_some(),
            "precondition: ghost present"
        );

        let result = run_reconcile(&h, rp, false);
        assert!(
            matches!(result, Err(ScanError::EmptyRoot)),
            "empty root must surface EmptyRoot, got {result:?}"
        );
        assert!(
            resolve(&h, &rp.join("ghost.txt")).is_some(),
            "the prior index must NOT be blanked when the root lists empty"
        );
    }

    /// A full entries-table snapshot (all columns that a no-op must preserve;
    /// `listed_epoch` is deliberately excluded because a reconcile re-stamps it).
    /// Equal snapshots before/after prove NO entry was added, removed, re-id'd, or
    /// had its size/metadata rewritten.
    type EntryRowSnap = (
        i64,
        i64,
        String,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    fn entries_snapshot(h: &Harness) -> Vec<EntryRowSnap> {
        let c = conn(h);
        let mut stmt = c
            .prepare(
                "SELECT id, parent_id, name, is_directory, is_symlink, logical_size, physical_size, modified_at, inode \
                 FROM entries ORDER BY id",
            )
            .expect("prepare entries snapshot");

        stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .expect("query entries snapshot")
        .map(|r| r.expect("entries row"))
        .collect()
    }

    /// A `dir_stats` snapshot of the recursive aggregates a no-op must preserve
    /// (`min_subtree_epoch` is excluded — it advances with the epoch by design).
    type DirStatsSnap = (i64, i64, i64, i64, i64, i64);
    fn dir_stats_snapshot(h: &Harness) -> Vec<DirStatsSnap> {
        let c = conn(h);
        let mut stmt = c
            .prepare(
                "SELECT entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, \
                 recursive_dir_count, recursive_has_symlinks FROM dir_stats ORDER BY entry_id",
            )
            .expect("prepare dir_stats snapshot");

        stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .expect("query dir_stats snapshot")
        .map(|r| r.expect("dir_stats row"))
        .collect()
    }

    /// Convergence guard (the headline cross-check): a LOCAL reconcile run
    /// immediately after a REAL fresh scan of the SAME unchanged on-disk tree is a
    /// NO-OP. This pins that the two independent code paths — the fresh scan's
    /// parallel `run_scan` (metadata via `extract_metadata`, exclusion +
    /// canonicalization-alias gating in the `InsertVisitor`) and the serial reconcile
    /// walk (its own `read_fs_children` + `build_live_children` + `diff_dir_against_db`)
    /// — AGREE on the exact same tree. A divergence (reconcile re-adds, deletes, or
    /// rewrites an entry the scan didn't, the `/tmp`,`/var`,`/etc` alias / exclusion
    /// class of bug) breaks one of the assertions below.
    ///
    /// The fresh scan is driven via `scan_subtree`, NOT `scan_volume`: both run the
    /// identical `run_scan` core, but `scan_volume` hardcodes the scan root to
    /// `ROOT_ID` (it treats it as `/`), so only a literal `/` round-trips with the
    /// reconcile's `resolve_path`-based root lookup. `scan_subtree` builds the tree
    /// under the temp dir's resolved id — exactly the id the reconcile then walks —
    /// while exercising the same real fresh scan + metadata + exclusion logic. The
    /// tree contents come entirely from the fresh scan; `ensure_path_in_db` only seeds
    /// the ancestor chain so the subtree root resolves.
    #[test]
    fn reconcile_after_real_fresh_scan_of_unchanged_tree_is_a_no_op() {
        use crate::indexing::scanner::scan_subtree;

        let h = setup();
        let root = tree_root();
        let rp = root.path();
        // Seed only the ancestor chain (so the subtree root resolves); the tree's
        // CONTENTS are built solely by the real fresh scan below.
        ensure_path_in_db(&h, &norm(rp));

        // A real nested tree on disk with known file sizes.
        std::fs::create_dir_all(rp.join("a/deep")).unwrap();
        std::fs::create_dir(rp.join("b")).unwrap();
        std::fs::write(rp.join("a/a1.txt"), b"hello").unwrap(); // 5
        std::fs::write(rp.join("a/deep/leaf.txt"), b"leaf").unwrap(); // 4
        std::fs::write(rp.join("b/b1.txt"), b"bbb").unwrap(); // 3
        std::fs::write(rp.join("top.txt"), b"topfil").unwrap(); // 6

        // Build the index with the REAL fresh scanner (epoch 1).
        let cancelled = AtomicBool::new(false);
        let scan_summary = scan_subtree(rp, &h.writer, &cancelled).expect("fresh scan");
        h.writer.flush_blocking().unwrap();
        // 4 dirs (a, a/deep, b — the subtree root itself isn't counted by run_scan's
        // child walk) + 4 files. run_scan counts entries it WALKED (children), so the
        // subtree root is excluded from total_entries.
        assert_eq!(
            scan_summary.total_entries, 7,
            "the fresh scan saw 3 child dirs + 4 files"
        );

        // Resolve the dirs and capture the post-scan ground truth.
        let rp_id = resolve(&h, rp).expect("root resolved");
        let a = resolve(&h, &rp.join("a")).expect("a resolved");
        let deep = resolve(&h, &rp.join("a/deep")).expect("deep resolved");
        let b = resolve(&h, &rp.join("b")).expect("b resolved");
        assert_eq!(
            listed_epoch(&h, deep),
            Some(1),
            "the fresh scan stamped the deepest dir at epoch 1"
        );

        // Scope the dir_stats comparison to the SUBTREE dirs. The finish aggregate
        // commands differ in breadth by design (the fresh scan's `ComputeSubtreeAggregates`
        // vs the reconcile's `ComputeAllAggregates`), and the reconcile additionally
        // writes zero-stat rows for the seeded ANCESTOR chain above the temp dir —
        // an artifact of this test's deep root (in prod the reconcile root is `/` =
        // `ROOT_ID`, no ancestors). The convergence claim is about the dirs BOTH
        // paths actually walked: the subtree.
        let subtree_dirs: HashSet<i64> = [rp_id, a, deep, b].into_iter().collect();
        let scoped_dir_stats = |h: &Harness| -> Vec<DirStatsSnap> {
            dir_stats_snapshot(h)
                .into_iter()
                .filter(|row| subtree_dirs.contains(&row.0))
                .collect()
        };

        let entries_before = entries_snapshot(&h);
        let dir_stats_before = scoped_dir_stats(&h);
        let count_before = entries_before.len();
        assert_eq!(
            dir_stats_before.len(),
            4,
            "the fresh scan produced stats for all 4 subtree dirs"
        );

        // Bump the epoch (a continuity break would do this) and reconcile the SAME,
        // UNCHANGED tree.
        let new_epoch = bump_epoch(&h);
        assert_eq!(new_epoch, 2, "epoch advanced for the reconcile pass");
        run_reconcile(&h, rp, false).expect("reconcile after fresh scan");

        // 1. No spurious adds/deletes: the entry SET is byte-identical (same rows,
        //    same ids, same sizes/metadata) — the divergence catcher.
        let entries_after = entries_snapshot(&h);
        assert_eq!(
            entries_after.len(),
            count_before,
            "reconcile-after-fresh-scan must not change the entry count (no spurious add/delete)"
        );
        assert_eq!(
            entries_after, entries_before,
            "reconcile-after-fresh-scan must leave every entry row identical (no entry added, removed, re-id'd, or rewritten)"
        );

        // 2. Recursive dir_stats (sizes + counts) unchanged: the two paths agree on
        //    every aggregate.
        assert_eq!(
            scoped_dir_stats(&h),
            dir_stats_before,
            "reconcile-after-fresh-scan must leave every subtree dir's recursive aggregates unchanged"
        );

        // 3. Full coverage / recursion: every dir from the root down advanced to
        //    the new epoch, proving the reconcile re-listed the whole tree.
        for (label, id) in [("root", rp_id), ("a", a), ("a/deep", deep), ("b", b)] {
            assert_eq!(
                listed_epoch(&h, id),
                Some(new_epoch),
                "{label} must be re-listed at the reconcile epoch (full coverage)"
            );
        }
    }

    /// Hardlink dedup convergence (data-safety): a reconcile of an unchanged tree
    /// containing a hardlinked inode must agree with the fresh scan on the deduped
    /// byte totals.
    ///
    /// The fresh scan's `run_scan` counts each inode's physical bytes ONCE (zeroes the
    /// 2nd+ occurrence). The reconcile's `build_live_children` must accumulate
    /// `total_physical_bytes` the same way, else `ScanSummary.total_physical_bytes`
    /// (and the live `bytes_scanned` progress) inflates by every hardlink's size — a
    /// fresh scan and a reconcile of the SAME tree would report different totals.
    ///
    /// We assert the order-independent TOTAL (the summary's `total_physical_bytes`),
    /// which is invariant to which of the two hardlink names the fresh scan vs the
    /// reconcile happens to size. (The PERSISTED entries are kept one-per-inode by the
    /// writer's own `UpsertEntryV2` dedup, asserted as a guard via the root's recursive
    /// sizes.)
    #[test]
    fn reconcile_after_fresh_scan_does_not_double_count_hardlinks() {
        use crate::indexing::scanner::scan_subtree;

        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        // A real tree with a HARDLINK: `link.txt` and `orig.txt` share one inode.
        std::fs::write(rp.join("orig.txt"), b"hardlinked-body").unwrap(); // 15 bytes
        std::fs::hard_link(rp.join("orig.txt"), rp.join("link.txt")).expect("hard_link");
        std::fs::write(rp.join("plain.txt"), b"plain").unwrap(); // 5 bytes

        // Verify the hardlink actually shares an inode (sandboxes occasionally fall
        // back to a copy). If not, STOP rather than test nothing.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let i1 = std::fs::metadata(rp.join("orig.txt")).unwrap().ino();
            let i2 = std::fs::metadata(rp.join("link.txt")).unwrap().ino();
            assert_eq!(i1, i2, "hard_link must share an inode for this test to mean anything");
            assert!(
                std::fs::metadata(rp.join("orig.txt")).unwrap().nlink() > 1,
                "hardlinked inode must report nlink > 1"
            );
        }

        // Build the index with the REAL fresh scanner (which dedups hardlinks).
        let cancelled = AtomicBool::new(false);
        let fresh_summary = scan_subtree(rp, &h.writer, &cancelled).expect("fresh scan");
        h.writer.flush_blocking().unwrap();

        let rp_id = resolve(&h, rp).expect("root resolved");
        let physical_before = dir_stats(&h, rp_id)
            .expect("root stats after fresh scan")
            .recursive_physical_size;

        // Reconcile the SAME, UNCHANGED tree.
        bump_epoch(&h);
        let reconcile_summary = run_reconcile(&h, rp, false).expect("reconcile after fresh scan");

        // Primary claim: the reconcile's deduped total matches the fresh scan's. RED
        // before the fix (the reconcile counts the hardlink's bytes twice), GREEN after.
        assert_eq!(
            reconcile_summary.total_physical_bytes, fresh_summary.total_physical_bytes,
            "reconcile must dedup hardlinks in total_physical_bytes exactly like the fresh scan \
             (fresh={}, reconcile={})",
            fresh_summary.total_physical_bytes, reconcile_summary.total_physical_bytes
        );

        // Guard: the persisted aggregate is one-per-inode (the writer's UpsertEntryV2
        // dedup keeps this true regardless), so the root's recursive size is unchanged.
        let physical_after = dir_stats(&h, rp_id)
            .expect("root stats after reconcile")
            .recursive_physical_size;
        assert_eq!(
            physical_after, physical_before,
            "reconcile must not change the root's recursive physical size \
             ({physical_before} -> {physical_after})"
        );
    }

    /// End-to-end correctness on a large ADD delta through the real
    /// `run_local_reconcile` path (which now brackets its walk with
    /// `SetDeltaPropagation(false/true)`): reconciling a populated on-disk tree into
    /// an index that holds ONLY the root (every entry is new) must complete and
    /// produce correct recursive `dir_stats`.
    ///
    /// This proves the bulk-mode suppression is invisible to the final result: the
    /// per-entry ancestor propagation is skipped for the whole walk, yet the single
    /// `ComputeAllAggregates` in `finish_reconcile` recomputes every dir's stats
    /// exactly. (The `_no_op` convergence test pins the unchanged-tree case; this
    /// pins the add-everything case the wedge actually hit.)
    #[test]
    fn reconcile_from_empty_index_builds_correct_aggregates() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        // Seed ONLY the root dir (no contents): the reconcile adds the whole tree.
        ensure_path_in_db(&h, &norm(rp));

        // Tree with known totals: 5 files (150 bytes), 3 dirs (a, a/deep, b).
        std::fs::create_dir_all(rp.join("a/deep")).unwrap();
        std::fs::create_dir(rp.join("b")).unwrap();
        std::fs::write(rp.join("a/f1.txt"), vec![b'x'; 10]).unwrap();
        std::fs::write(rp.join("a/f2.txt"), vec![b'x'; 20]).unwrap();
        std::fs::write(rp.join("a/deep/f3.txt"), vec![b'x'; 30]).unwrap();
        std::fs::write(rp.join("b/f4.txt"), vec![b'x'; 40]).unwrap();
        std::fs::write(rp.join("top.txt"), vec![b'x'; 50]).unwrap();

        let summary = run_reconcile(&h, rp, false).expect("reconcile from empty");
        assert!(!summary.was_cancelled, "the reconcile must complete, not cancel");

        // Root aggregates: every file and dir, deduped sizes summed.
        let root_id = resolve(&h, rp).expect("root resolved");
        let root_stats = dir_stats(&h, root_id).expect("root stats");
        assert_eq!(root_stats.recursive_file_count, 5, "root must count all 5 files");
        assert_eq!(root_stats.recursive_dir_count, 3, "root must count a, a/deep, b");
        assert_eq!(root_stats.recursive_logical_size, 150, "root must sum all file sizes");

        // A sub-aggregate (a holds f1, f2, and a/deep/f3 = 60 bytes, 3 files, 1 dir).
        let a = resolve(&h, &rp.join("a")).expect("a resolved");
        let a_stats = dir_stats(&h, a).expect("a stats");
        assert_eq!(a_stats.recursive_file_count, 3, "a must count f1, f2, deep/f3");
        assert_eq!(a_stats.recursive_dir_count, 1, "a must count deep");
        assert_eq!(a_stats.recursive_logical_size, 60, "a must sum 10 + 20 + 30");
    }

    /// A MOUNT-ROOTED reconcile resolves its root only after the mount-relative
    /// strip. With `root` space (the pre-strip behavior) the absolute mount path is
    /// walked from `ROOT_ID` and misses, failing with `local reconcile: root is not
    /// in the index`; with the drive's `mount_rooted` space the mount root strips to
    /// `/` → `ROOT_ID` and the reconcile builds the tree in place. Pins that the
    /// strip at the reconcile's root resolve is load-bearing.
    #[test]
    fn reconcile_resolves_mount_rooted_root_via_strip() {
        let h = setup();
        // A tempdir stands in for the drive's mount root (`/Volumes/X`). Unlike a
        // `/`-rooted index, the absolute mount-path chain is NOT seeded — `ROOT_ID`
        // (the sentinel from `setup`) IS the mount.
        let mount = tree_root();
        let rp = mount.path();
        let mount_root = rp.to_string_lossy().to_string();

        std::fs::create_dir_all(rp.join("a/deep")).unwrap();
        std::fs::write(rp.join("a/f1.txt"), vec![b'x'; 10]).unwrap();
        std::fs::write(rp.join("a/deep/f2.txt"), vec![b'x'; 20]).unwrap();
        std::fs::write(rp.join("top.txt"), vec![b'x'; 5]).unwrap();

        // `root` space walks the absolute mount path from `ROOT_ID` and misses → the
        // pre-fix `root is not in the index` failure. (Typed variant match, no string
        // match per the `no-string-matching` rule.)
        let progress = ScanProgress::new();
        let flag = AtomicBool::new(false);
        let red = run_local_reconcile(rp, &IndexPathSpace::root(), &h.writer, &progress, &flag);
        assert!(
            matches!(red, Err(ScanError::Io(_))),
            "root space can't resolve the mount root from ROOT_ID: {red:?}"
        );

        // `mount_rooted` space strips the mount root to `/` → `ROOT_ID` → reconciles.
        let progress2 = ScanProgress::new();
        let flag2 = AtomicBool::new(false);
        let green = run_local_reconcile(
            rp,
            &IndexPathSpace::mount_rooted(mount_root),
            &h.writer,
            &progress2,
            &flag2,
        );
        h.writer.flush_blocking().unwrap();
        assert!(
            green.is_ok(),
            "mount-rooted space resolves the root and reconciles: {green:?}"
        );

        // The whole tree is indexed under `ROOT_ID`, by mount-relative name.
        let root_stats = dir_stats(&h, ROOT_ID).expect("root stats after reconcile");
        assert_eq!(
            root_stats.recursive_file_count, 3,
            "all 3 files indexed under the mount"
        );
        assert_eq!(root_stats.recursive_dir_count, 2, "a + a/deep indexed");
        assert_eq!(root_stats.recursive_logical_size, 35, "10 + 20 + 5");

        // Children hang off ROOT_ID by mount-relative name (not the absolute chain).
        let a_id = IndexStore::resolve_component(&conn(&h), ROOT_ID, "a")
            .unwrap()
            .expect("a resolves under ROOT_ID");
        assert!(a_id > ROOT_ID, "a is a real child entry of the mount root");
    }

    /// Vanished-volume (abort): a mount-rooted reconcile whose root can't be read
    /// (the drive was unplugged) surfaces the typed `RootUnlistable`, distinct from
    /// `EmptyRoot` (a readable-but-empty root). The completion handler maps
    /// `RootUnlistable` to an aborted scan (no `scan_completed_at`, an
    /// `index-scan-aborted` emit), while `EmptyRoot` keeps the prior index without
    /// an abort. This pins the distinguisher for the yanked-drive case.
    #[test]
    fn reconcile_vanished_root_surfaces_root_unlistable_not_empty_root() {
        let h = setup();
        // A mount root that does not exist on disk (a vanished / unplugged drive).
        // `mount_rooted` space resolves the root to `ROOT_ID` (the sentinel from
        // `setup`), so the walk reaches the FS read — which fails because the path is
        // gone — surfacing `RootUnlistable`. An `EmptyRoot` would instead require a
        // readable-but-empty root, so the two cases are genuinely distinct.
        let missing = std::env::temp_dir().join("cmdr-reconcile-vanished-root-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(!missing.exists(), "precondition: the mount root must be absent");

        let progress = ScanProgress::new();
        let flag = AtomicBool::new(false);
        let result = run_local_reconcile(
            &missing,
            &IndexPathSpace::mount_rooted(missing.to_string_lossy().to_string()),
            &h.writer,
            &progress,
            &flag,
        );
        assert!(
            matches!(result, Err(ScanError::RootUnlistable)),
            "a vanished mount root must surface RootUnlistable, got {result:?}"
        );
    }

    /// Cancel (data-safety): a cancelled reconcile returns `was_cancelled`
    /// (so the completion handler writes no scan_completed_at) and sends no
    /// marks/aggregate, leaving the prior index intact and un-restamped.
    #[test]
    fn cancelled_reconcile_leaves_prior_index_and_writes_no_marks() {
        let h = setup();
        let root = tree_root();
        let rp = root.path();
        ensure_path_in_db(&h, &norm(rp));

        std::fs::create_dir(rp.join("sub")).unwrap();
        std::fs::write(rp.join("sub/f.txt"), b"body").unwrap();

        run_reconcile(&h, rp, false).expect("first reconcile");
        let sub = resolve(&h, &rp.join("sub")).expect("sub indexed");
        assert_eq!(listed_epoch(&h, sub), Some(1), "sub listed at epoch 1");

        bump_epoch(&h); // -> 2
        let summary = run_reconcile(&h, rp, true).expect("cancelled reconcile returns Ok");
        assert!(summary.was_cancelled, "cancel must report was_cancelled");

        assert!(
            resolve(&h, &rp.join("sub/f.txt")).is_some(),
            "index not blanked by cancel"
        );
        assert_eq!(
            listed_epoch(&h, sub),
            Some(1),
            "no marks sent on cancel: listed_epoch stays at the old value"
        );
    }
}
