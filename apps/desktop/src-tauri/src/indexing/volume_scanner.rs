//! A `Volume`-trait recursive scanner for indexing network/USB volumes.
//!
//! The local guarded walker (the [`scanner`](super::scanner) module) is local-FS-only (`getattrlistbulk`)
//! and its `should_exclude` deliberately blocks `/Volumes/`. SMB (and, later,
//! MTP) shares are walked here instead, over the SAME `Volume::list_directory`
//! API the live pane uses, pulling sizes from the backend's stat. EVERYTHING
//! downstream of [`EntryRow`](super::store::EntryRow) is reused unchanged: the
//! shared `Arc<AtomicI64>` id counter via `ScanContext`, the single writer
//! thread (`InsertEntriesV2` batches), the aggregator (`ComputeAllAggregates`),
//! and `dir_stats`. Only the *front* of the pipeline (how entries are
//! discovered and stat'd) differs.
//!
//! ## Discipline for network round trips (plan rabbit hole #3)
//!
//! Every `list_directory` is a network syscall that can block 30–120 s on a
//! slow or hung mount, so the walk:
//!
//! - **is cancelable at every round trip**: the cancel flag is checked before
//!   each directory listing and the BFS bails immediately when set;
//! - **wraps each listing in a timeout** (`LIST_TIMEOUT`): a wedged mount yields
//!   a typed `VolumeScanError::Timeout` rather than parking forever;
//! - **wraps each round trip in `objc2::rc::autoreleasepool` on macOS**: the SMB
//!   listing path touches NSURL/`NSString`-adjacent code, and unpooled ObjC
//!   autoreleases leak multi-GB over a long walk (the same rule the index writer
//!   thread follows — see `indexing/CLAUDE.md`).
//!
//! ## Terminal disconnect ⇒ keep an honest partial; cancel ⇒ discard
//!
//! A mid-walk **disconnect** (the typed `DeviceDisconnected`/`Disconnected`, or
//! the consecutive-failure backstop for a disconnect-shaped untyped error) is
//! TERMINAL: the walk stops immediately rather than churning the still-queued
//! dirs into silently-empty rows (the reported prod bug). Before returning the
//! typed error, it runs the partial-preserving write sequence
//! ([`finish_partial_scan`]: flush + `MarkDirsListed` + `ComputeAllAggregates`)
//! so the kept partial is self-describing — scanned subtrees roll up to
//! `min_subtree_epoch > 0` (exact, stale once the epoch is bumped), unscanned
//! ones stay `0` (`—`/`≥`). The completion handler (`manager.rs`) then keeps the
//! instance + DB and marks the volume Stale.
//!
//! A **user cancel** still discards: `cancelled` returns `was_cancelled` with no
//! marks/aggregate, and the completion handler resets the volume to gray.
//!
//! This scanner NEVER writes the `scan_completed_at` meta marker (on any path);
//! the caller's completion handler does, only on a clean finish — the same
//! `scan_completed_at`-absent ⇒ no-Fresh / heal-to-rescan mechanism the local
//! scanner relies on (see `manager.rs`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use crate::file_system::volume::Volume;
use crate::indexing::store::{EntryRow, IndexStore, ScanContext};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

use super::scanner::{ScanProgress, ScanSummary};

/// Per-directory listing timeout. Network/USB `list_directory` blocks 30–120 s
/// on a hung mount; we cap a single round trip so a wedged share fails the walk
/// instead of parking it. Generous enough for a slow-but-alive NAS directory.
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

/// Batch size for `InsertEntriesV2` sends — matches the local guarded walker's default.
const BATCH_SIZE: usize = 2000;

/// How many `list_directory` round trips run concurrently during a walk. Directory
/// listing is latency-bound (each dir is open+query+close network round trips over an
/// otherwise-idle link), so keeping many in flight is a near-linear speedup until the
/// server's SMB credits saturate. Only the network I/O is concurrent; results are
/// processed serially on the walk task, so `ScanContext` id allocation and the writer
/// stay single-owner. 64 is a deliberate balance: it captures essentially all the
/// concurrency win, while staying gentle on a NAS that's also serving other load. Past
/// it there's little to gain — on a real raidz1-HDD QNAP a fresh scan became bound by
/// the single SQLite writer (its queue spiked into the thousands during big-directory
/// bursts), NOT by listing parallelism: the disks sat ~15% busy (ZFS ARC served most
/// metadata) and the NAS was never the ceiling. See DETAILS § "Bounded-concurrency walk".
const SCAN_CONCURRENCY: usize = 64;

/// Consecutive-failure backstop. A whole-volume disconnect that doesn't map to
/// the typed `DeviceDisconnected`/`Disconnected` variant (e.g. a generic
/// `IoError` "connection reset") would otherwise make every remaining queued
/// listing fail instantly — the exact prod bug, where ~6,475 dirs churned into
/// empty rows in ~1 s. So after this many CONSECUTIVE listing failures we abort
/// the walk (terminal), keeping the honest partial, rather than fabricating
/// empties. The counter resets on every success, so an isolated bad dir is still
/// skip-and-continue. 32 is generous enough that a sparse cluster of genuinely
/// unlistable dirs (a permission-walled tree) doesn't trip it, but small enough
/// that a real disconnect aborts in milliseconds.
const CONSECUTIVE_FAILURE_ABORT: usize = 32;

/// Why a `Volume`-trait scan ended other than cleanly.
#[derive(Debug)]
pub(crate) enum VolumeScanError {
    /// A directory listing exceeded `LIST_TIMEOUT` (wedged/hung mount).
    Timeout(PathBuf),
    /// The backend returned an error (disconnect mid-walk, permission, etc.).
    /// A `DeviceDisconnected`/`Disconnected` value here is a TERMINAL disconnect
    /// (see [`VolumeScanError::is_terminal_disconnect`]); other variants are the
    /// root-fatal case (failing to list the root itself).
    Volume(crate::file_system::volume::VolumeError),
    /// The consecutive-failure backstop tripped: `count` listings in a row
    /// failed with a non-typed (disconnect-shaped) error, so the walk aborted
    /// rather than churning every queued dir into a silently-empty row. `last`
    /// is the most recent failing error's display. Treated as a terminal
    /// disconnect by the completion handler — see `is_terminal_disconnect`.
    ConsecutiveFailures { count: usize, last: String },
    /// A writer send failed (the writer thread is gone).
    WriterSend(String),
    /// Setting up the scan context (root sentinel, id counter) failed.
    Context(String),
    /// The ROOT listing SUCCEEDED but returned zero children, so the walk
    /// produced an empty index. Distinct from a root listing that FAILED
    /// (`Volume`) — here the backend answered, it just answered "nothing". For a
    /// NAS share that's almost always a transient glitch or a wrong scan root,
    /// not a genuinely empty share, so we treat it as a failed scan: surfacing
    /// this makes the completion handler NOT persist `scan_completed_at`, which
    /// would otherwise strand the index as falsely "complete" and refuse all
    /// future rescans (the real-hardware bug). A genuinely empty share is
    /// vanishingly rare and self-heals on the next rescan, so the safe rule
    /// wins. See `indexing/DETAILS.md` § "No completion marker on an empty root".
    EmptyRoot,
}

impl VolumeScanError {
    /// Whether this error means the volume went away mid-walk (a continuity
    /// break), so the completion handler should KEEP the honest partial and mark
    /// the volume Stale rather than discard it. True for a typed
    /// `DeviceDisconnected`/`Disconnected` and for the consecutive-failure
    /// backstop; false for a timeout / context / writer-send failure (those are
    /// genuine aborts with no honest partial to keep).
    ///
    /// Classifies by the TYPED variant, never a message substring
    /// (`.claude/rules/no-string-matching.md`).
    pub(crate) fn is_terminal_disconnect(&self) -> bool {
        use crate::file_system::volume::VolumeError;
        matches!(
            self,
            Self::Volume(VolumeError::DeviceDisconnected(_)) | Self::ConsecutiveFailures { .. }
        )
    }
}

impl std::fmt::Display for VolumeScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(p) => write!(f, "listing timed out: {}", p.display()),
            Self::Volume(e) => write!(f, "volume error: {e}"),
            Self::ConsecutiveFailures { count, last } => {
                write!(f, "{count} consecutive listing failures (last: {last})")
            }
            Self::WriterSend(m) => write!(f, "writer send failed: {m}"),
            Self::Context(m) => write!(f, "scan context setup failed: {m}"),
            Self::EmptyRoot => write!(f, "root listing returned no children (treating as a failed scan)"),
        }
    }
}

impl std::error::Error for VolumeScanError {}

/// Recursively scan `volume` from its `root`, streaming `EntryRow`s into
/// `writer`. Async (the `Volume` API is async); the caller runs it on a tokio
/// task. On clean completion, fires `ComputeAllAggregates` so the existing
/// aggregator computes `dir_stats` exactly as for a local scan.
///
/// Cancelable via `cancelled`; cancellation flushes the current batch and
/// returns `was_cancelled: true`. A timeout / backend error returns `Err`; the
/// caller discards the partial (D-interrupted).
pub(crate) async fn scan_volume_via_trait(
    volume: Arc<dyn Volume>,
    root: PathBuf,
    writer: IndexWriter,
    progress: Arc<ScanProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<ScanSummary, VolumeScanError> {
    let start = Instant::now();

    // Set up the scan context against a write connection (it creates the root
    // sentinel), mapping the scan root to ROOT_ID — identical to the local guarded
    // walker's volume-root setup, so all downstream id/parent logic is shared.
    let db_path = writer.db_path();
    // The scan reads `current_epoch` once at start (seeding meta to "1" if
    // absent) and stamps every successfully-listed dir with it. The caller
    // (`start_volume_scan`) has already bumped + flushed `current_epoch` before
    // spawning this walk, so the seed here is a no-op fallback and we read back
    // the bumped value on this same connection.
    let (mut scan_ctx, epoch) = {
        let conn = IndexStore::open_write_connection(&db_path).map_err(|e| VolumeScanError::Context(e.to_string()))?;
        let epoch = IndexStore::seed_current_epoch(&conn).map_err(|e| VolumeScanError::Context(e.to_string()))?;
        let ctx = ScanContext::new(&conn, &root, true, Arc::clone(writer.next_id()))
            .map_err(|e| VolumeScanError::Context(e.to_string()))?;
        (ctx, epoch)
    };

    // Ids of every directory whose listing SUCCEEDED (including empty results).
    // Emitted as `MarkDirsListed` once after the final `flush_batch` and before
    // `ComputeAllAggregates`, so each row is committed-in-order when stamped and
    // the ordering invariant (marks precede the final aggregate) holds for free.
    let mut listed_ids: Vec<i64> = Vec::new();

    let mut batch: Vec<EntryRow> = Vec::with_capacity(BATCH_SIZE);
    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut total_physical_bytes: u64 = 0;
    // Run of consecutive listing failures (any error, typed or not). Reset to 0
    // on every successful listing; the backstop trips at `CONSECUTIVE_FAILURE_ABORT`.
    let mut consecutive_failures: usize = 0;

    // Breadth-first, with up to SCAN_CONCURRENCY listings in flight at once. A dir's id
    // is registered in `ScanContext` when its PARENT's listing is processed (serially,
    // on this task), BEFORE the child is enqueued — so the "parent id registered before
    // we list the child" invariant holds even though the network listings overlap. Only
    // the I/O overlaps; result processing (id alloc, batching) stays single-owner, so no
    // locking. Each queue item is an absolute directory path; the root maps to ROOT_ID.
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.clone());
    let mut last_progress_log = Instant::now();
    let mut inflight = FuturesUnordered::new();

    loop {
        if cancelled.load(Ordering::Relaxed) {
            // In-flight listings are dropped here; the smb2/MTP backends tolerate a
            // dropped request waiter. Flush what we batched and report the cancel.
            flush_batch(&mut batch, &writer)?;
            return Ok(summary(total_entries, total_dirs, total_physical_bytes, start, true));
        }

        // Keep the pipe full: launch listings until the concurrency cap or the queue
        // drains. Each future owns its clones (self-contained) and returns the dir path
        // alongside the result so the processor can resolve its parent id. Goes through
        // `list_directory_for_scan` (inside `list_one_directory`) so a backend sharing a
        // serialized resource with foreground work (MTP's single USB pipe) yields it
        // between bounded units rather than pinning it for the whole directory.
        while inflight.len() < SCAN_CONCURRENCY {
            let Some(dir) = queue.pop_front() else { break };
            let vol = Arc::clone(&volume);
            let cancel = Arc::clone(&cancelled);
            inflight.push(async move {
                let r = list_one_directory(vol, dir.clone(), cancel).await;
                (dir, r)
            });
        }

        // Nothing queued and nothing in flight ⇒ the walk is done.
        let Some((dir_path, result)) = inflight.next().await else {
            break;
        };

        let entries = match result {
            Ok(e) => {
                consecutive_failures = 0;
                e
            }
            // TERMINAL disconnect: the whole volume went away mid-walk. Matched
            // by the TYPED variant (never a message substring,
            // `.claude/rules/no-string-matching.md`). Stop topping up and drop the
            // in-flight listings rather than churning the still-queued dirs into
            // silently-empty rows (the reported prod bug). Write the partial-preserving
            // sequence in ONE place (flush + marks + aggregate, NO scan_completed_at) so
            // the kept partial is honest, then surface the typed error to the completion
            // handler.
            Err(VolumeScanError::Volume(e)) if is_typed_disconnect(&e) => {
                log::warn!(
                    "volume_scanner: device disconnected listing {}: {e}; \
                     keeping honest partial ({} listed, {} queued/in-flight unscanned)",
                    dir_path.display(),
                    crate::pluralize::pluralize(total_dirs, "dir"),
                    crate::pluralize::pluralize((queue.len() + inflight.len()) as u64, "dir"),
                );
                finish_partial_scan(&mut batch, &listed_ids, epoch, &writer)?;
                return Err(VolumeScanError::Volume(e));
            }
            Err(VolumeScanError::Volume(ref e)) if dir_path == root => {
                // Failing to list the root itself with a non-disconnect error is
                // fatal — there's nothing to index. Surface it so the caller
                // discards and resets to gray (no honest partial to keep).
                return Err(VolumeScanError::Volume(e.clone()));
            }
            Err(err) => {
                // A sub-directory we can't list (permission, transient, timeout),
                // or a disconnect-shaped error that didn't map to the typed
                // variant. Skip it and keep walking the rest, like the local guarded
                // walker skips errored entries — BUT count consecutive failures.
                // A vanished volume that surfaces as an untyped error makes EVERY
                // listing fail, so the backstop aborts the walk (terminal) instead of
                // fabricating empties. Concurrency loosens "consecutive" (up to
                // SCAN_CONCURRENCY failures can be in flight at once), but a real
                // disconnect piles failures with no successes to reset the counter, so
                // it still trips; an isolated bad dir is reset by its many healthy peers.
                consecutive_failures += 1;
                log::debug!(
                    "volume_scanner: skipping unlistable dir {} (consecutive_failures={consecutive_failures}): {err}",
                    dir_path.display(),
                );
                if consecutive_failures >= CONSECUTIVE_FAILURE_ABORT {
                    log::warn!(
                        "volume_scanner: {consecutive_failures} consecutive listing failures \
                         (looks like a disconnect); aborting walk and keeping honest partial \
                         ({} listed, {} queued/in-flight unscanned)",
                        crate::pluralize::pluralize(total_dirs, "dir"),
                        crate::pluralize::pluralize((queue.len() + inflight.len()) as u64, "dir"),
                    );
                    finish_partial_scan(&mut batch, &listed_ids, epoch, &writer)?;
                    return Err(VolumeScanError::ConsecutiveFailures {
                        count: consecutive_failures,
                        last: err.to_string(),
                    });
                }
                continue;
            }
        };

        // The parent's id was registered when it was discovered (or is ROOT_ID
        // for the scan root). If it's somehow absent, skip the whole subtree.
        let parent_id = match scan_ctx.lookup_parent(&dir_path) {
            Some(id) => id,
            None => {
                log::debug!("volume_scanner: parent id missing for {}, skipping", dir_path.display());
                continue;
            }
        };

        // This directory's listing succeeded — record its id so it gets stamped
        // `listed_epoch`, even when empty (empty-but-listed → `0 bytes`, distinct
        // from never-listed → `—`). Done here, outside the per-entry loop below,
        // so an empty result still marks. A listing that ERRORED hit `continue`
        // above and never reaches this point, so it stays `listed_epoch=0`.
        listed_ids.push(parent_id);
        log_scan_progress(&mut last_progress_log, "scanning", &dir_path, total_dirs, total_entries);

        for entry in entries {
            let is_dir = entry.is_directory;
            let is_symlink = entry.is_symlink;
            let child_path = PathBuf::from(&entry.path);
            let id = scan_ctx.alloc_id();

            if is_dir {
                total_dirs += 1;
                progress.dirs_found.fetch_add(1, Ordering::Relaxed);
                // Skip recursion into NAS snapshot/system dirs (@eaDir,
                // @Recently-Snapshot, …): hardlinked/huge, and recursively sizing them
                // stalled a real first-scan. The row is still indexed (visible,
                // navigable); we just don't walk its subtree, so its size stays
                // honestly unknown rather than a misleading roll-up. See `system_dirs`.
                if crate::indexing::system_dirs::is_recursion_excluded_dir(&entry.name) {
                    log::debug!(
                        "volume_scanner: not descending into NAS system dir {}",
                        child_path.display()
                    );
                } else {
                    scan_ctx.register_dir(child_path.clone(), id);
                    queue.push_back(child_path);
                }
            }

            // SMB/MTP have no inode and no separate physical size; mirror the
            // logical size into physical so dir_stats' physical totals are
            // populated (the backend reports one size). Symlinks contribute no
            // size, matching the local scanner's `du`-style omission.
            let (logical_size, physical_size) = if is_symlink {
                (None, None)
            } else {
                let s = entry.size;
                (s, entry.physical_size.or(s))
            };

            let entry_physical = physical_size.unwrap_or(0);
            total_physical_bytes += entry_physical;
            progress.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);
            total_entries += 1;
            progress.entries_scanned.fetch_add(1, Ordering::Relaxed);

            batch.push(EntryRow {
                id,
                parent_id,
                name: entry.name,
                is_directory: is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at: entry.modified_at,
                inode: entry.inode,
            });

            if batch.len() >= BATCH_SIZE {
                flush_batch(&mut batch, &writer)?;
            }
        }
    }

    // The whole walk produced zero entries, which can only mean the ROOT itself
    // listed empty (a non-empty root queues children and pushes rows). A NAS
    // share that lists fine in a live pane but scans to nothing is the
    // wrong-root / transient-glitch case, not a genuinely empty share — so treat
    // it as a failed scan and refuse to mark completion (the completion handler
    // maps `Err` to "discard, reset to gray", leaving no stranding marker). We
    // bail BEFORE `finish_partial_scan` so no marks/aggregate touch the empty DB.
    if total_entries == 0 {
        log::warn!(
            "volume_scanner: root listed empty for {} ({}ms) — treating as a failed scan, not marking complete",
            root.display(),
            start.elapsed().as_millis()
        );
        return Err(VolumeScanError::EmptyRoot);
    }

    // Clean finish: the same partial-preserving sequence the terminal-abort
    // branches run (flush + marks + aggregate), then trim the WAL. Sharing it
    // keeps the ordering invariant (marks precede the final aggregate) in ONE
    // place, so a clean scan and an aborted partial roll up identically.
    finish_partial_scan(&mut batch, &listed_ids, epoch, &writer)?;
    writer
        .send(WriteMessage::WalCheckpoint)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;

    log::info!(
        "volume_scanner: walk complete for {}: entries={total_entries}, dirs={total_dirs} in {}ms",
        root.display(),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start, false))
}

/// Non-destructively RECONCILE a network volume against an already-populated
/// index, instead of truncating and rebuilding.
///
/// Walks the same BFS over `Volume::list_directory` as [`scan_volume_via_trait`],
/// with the same round-trip disciplines (cancel, timeout, autoreleasepool,
/// terminal-disconnect + consecutive-failure backstop). But per listed dir it
/// DIFFS the live listing against the DB rows ([`reconciler::diff_dir_against_db`],
/// shared with the local reconcile walk) and writes only the changes — so the
/// last-good index stays visible (stale) throughout and a mid-rescan disconnect
/// leaves the prior data intact. No `TruncateData` precedes it (the manager skips
/// the truncate for the reconcile path).
///
/// Coverage: stamps every successfully-listed dir, then runs ONE
/// `ComputeAllAggregates` (NOT per-dir propagation — the perf bench measured that
/// ~2× slower at full scale; `docs/notes/m3-reconcile-rescan-gate.md`). After a
/// reconcile the writer's accumulator maps are empty (no `InsertEntriesV2`), so
/// `ComputeAllAggregates` takes the O(dirs) bulk-SQL bottom-up path.
///
/// A no-op reconcile (nothing changed on disk) writes ZERO entry rows — unchanged
/// rows are diffed and skipped, never re-UPSERTed — so it never touches the
/// catastrophic `INSERT OR REPLACE`/`platform_case` path.
pub(crate) async fn reconcile_volume_via_trait(
    volume: Arc<dyn Volume>,
    root: PathBuf,
    writer: IndexWriter,
    progress: Arc<ScanProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<ScanSummary, VolumeScanError> {
    use crate::indexing::reconciler::{self, LiveChild};
    use crate::indexing::store::ROOT_ID;

    let start = Instant::now();
    let db_path = writer.db_path();

    // A READ connection for path/child resolution (the reconcile path holds a read
    // connection, never a write one — write-mode pragmas can `SQLITE_BUSY` and
    // silently kill live indexing; see `indexing/CLAUDE.md`). The caller has
    // already bumped + flushed `current_epoch` before spawning this walk; read it
    // back here and stamp listed dirs with it.
    let conn = IndexStore::open_read_connection(&db_path).map_err(|e| VolumeScanError::Context(e.to_string()))?;
    let epoch = IndexStore::read_current_epoch(&conn).map_err(|e| VolumeScanError::Context(e.to_string()))?;

    // Ids of every directory whose listing SUCCEEDED (including empty results),
    // stamped after the walk and before the single aggregate.
    let mut listed_ids: Vec<i64> = Vec::new();
    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut total_physical_bytes: u64 = 0;
    let mut consecutive_failures: usize = 0;
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;

    // BFS by (absolute dir path, its DB id). The scan root maps to ROOT_ID in this
    // index (same as the fresh scan). New dirs discovered this pass are resolved to
    // ids after a writer flush before we recurse into them.
    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.clone(), ROOT_ID));
    let mut last_progress_log = Instant::now();
    let mut inflight = FuturesUnordered::new();
    // New child dirs discovered this pass, drained after a WAVE's flush (when nothing
    // is queued or in flight). Each is (parent dir path, parent DB id, child name): we
    // resolve the freshly-written child by `(parent_id, name)` rather than by absolute
    // path, because the index root is the VOLUME root (mapped to ROOT_ID), not `/`. An
    // absolute-path walk from ROOT_ID would fail for any non-`/` root (e.g.
    // `/Volumes/naspi`), which is exactly the SMB/MTP case — that bug left a post-Forget
    // enable resolving zero new dirs, so the reconcile stopped at the root and falsely
    // completed. See `indexing/DETAILS.md` § "Non-destructive rescan".
    let mut new_dirs: Vec<(PathBuf, i64, String)> = Vec::new();

    // Suppress per-entry ancestor propagation for the bulk walk; the guard restores
    // it on EVERY exit (clean finish, cancel, empty-root, disconnect, error). The
    // shared finish recomputes all dir_stats via one `ComputeAllAggregates`, so the
    // per-entry walk would be redundant O(entries × depth) work. See
    // `reconciler::BulkReconcileGuard`.
    let _bulk_guard = reconciler::BulkReconcileGuard::begin(&writer);

    loop {
        if cancelled.load(Ordering::Relaxed) {
            // User cancel: stop, but leave the prior index intact (no truncate ran).
            // Mirror the fresh-scan cancel contract (no marks/aggregate on cancel).
            // In-flight listings are dropped (backends tolerate a dropped waiter).
            return Ok(summary(total_entries, total_dirs, total_physical_bytes, start, true));
        }

        // Keep up to SCAN_CONCURRENCY listings in flight — matched (existing) child dirs
        // whose ids we already hold. Same overlap-the-latency-bound-I/O win as the fresh
        // scan; processing (diff, writes) stays serial on this task and the DB read conn.
        while inflight.len() < SCAN_CONCURRENCY {
            let Some((dir, id)) = queue.pop_front() else { break };
            let vol = Arc::clone(&volume);
            let cancel = Arc::clone(&cancelled);
            inflight.push(async move {
                let r = list_one_directory(vol, dir.clone(), cancel).await;
                ((dir, id), r)
            });
        }

        // Wave boundary: nothing queued and nothing in flight. If new dirs were
        // discovered this wave, flush so the read connection can resolve their
        // freshly-written ids, then queue them for the next wave. Otherwise we're done.
        if inflight.is_empty() {
            if new_dirs.is_empty() {
                break;
            }
            writer
                .flush()
                .await
                .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
            for (parent_path, parent_id, child_name) in new_dirs.drain(..) {
                let child_path = parent_path.join(&child_name);
                // Resolve by (parent_id, name), NOT by absolute path: the index root is
                // the volume root (ROOT_ID), so an absolute-path walk from ROOT_ID only
                // works when the root is `/`. We hold the parent's DB id, so a
                // single-component lookup is both correct for any root AND cheaper.
                match IndexStore::resolve_component(&conn, parent_id, &child_name) {
                    Ok(Some(id)) => queue.push_back((child_path, id)),
                    Ok(None) => log::debug!(
                        "volume_scanner: reconcile couldn't resolve new dir after flush: {}",
                        child_path.display()
                    ),
                    Err(e) => log::warn!(
                        "volume_scanner: reconcile resolve_component failed for {}: {e}",
                        child_path.display()
                    ),
                }
            }
            continue;
        }

        let ((dir_path, dir_id), result) = match inflight.next().await {
            Some(v) => v,
            None => break,
        };

        let entries = match result {
            Ok(e) => {
                consecutive_failures = 0;
                e
            }
            // TERMINAL disconnect: stop topping up and keep the prior index intact.
            // There's no partial to roll up (we never truncated), but we still stamp the
            // dirs we DID re-list this pass and run the aggregate, so reconciled subtrees
            // flip fresh and the rest stays as it was (stale). Then surface the typed error.
            Err(VolumeScanError::Volume(e)) if is_typed_disconnect(&e) => {
                log::warn!(
                    "volume_scanner: device disconnected reconciling {}: {e}; \
                     keeping prior index ({} re-listed, {} queued/in-flight unreached)",
                    dir_path.display(),
                    crate::pluralize::pluralize(total_dirs, "dir"),
                    crate::pluralize::pluralize((queue.len() + inflight.len() + new_dirs.len()) as u64, "dir"),
                );
                finish_reconcile(&listed_ids, epoch, &writer)?;
                return Err(VolumeScanError::Volume(e));
            }
            Err(VolumeScanError::Volume(ref e)) if dir_path == root => {
                // Failing to list the root with a non-disconnect error: nothing to
                // reconcile from. Surface it; the prior index is untouched.
                return Err(VolumeScanError::Volume(e.clone()));
            }
            Err(err) => {
                consecutive_failures += 1;
                log::debug!(
                    "volume_scanner: skipping unlistable dir {} during reconcile (consecutive_failures={consecutive_failures}): {err}",
                    dir_path.display(),
                );
                if consecutive_failures >= CONSECUTIVE_FAILURE_ABORT {
                    log::warn!(
                        "volume_scanner: {consecutive_failures} consecutive listing failures during reconcile \
                         (looks like a disconnect); aborting and keeping prior index \
                         ({} re-listed, {} queued/in-flight unreached)",
                        crate::pluralize::pluralize(total_dirs, "dir"),
                        crate::pluralize::pluralize((queue.len() + inflight.len() + new_dirs.len()) as u64, "dir"),
                    );
                    finish_reconcile(&listed_ids, epoch, &writer)?;
                    return Err(VolumeScanError::ConsecutiveFailures {
                        count: consecutive_failures,
                        last: err.to_string(),
                    });
                }
                continue;
            }
        };

        // The ROOT listed EMPTY: bail BEFORE diffing it, so we don't write
        // removals for every prior child (which would blank the index). A
        // reconcile only runs over an already-populated index, so an empty root
        // here is the share glitching/half-dead, not a real "everything was
        // deleted" — refuse to mark completion and keep the prior stale-but-real
        // index. Matched on the typed root path, not a message. (A non-root dir
        // that lists empty is a genuine empty subdir and reconciles normally.)
        if dir_path == root && entries.is_empty() {
            log::warn!(
                "volume_scanner: reconcile root listed empty for {} ({}ms) — treating as a failed rescan, keeping prior index",
                root.display(),
                start.elapsed().as_millis()
            );
            return Err(VolumeScanError::EmptyRoot);
        }

        // This dir's listing succeeded — stamp it (incl. empty).
        listed_ids.push(dir_id);
        log_scan_progress(
            &mut last_progress_log,
            "reconciling",
            &dir_path,
            total_dirs,
            total_entries,
        );

        // Normalize the live listing into source-agnostic `LiveChild`s.
        let mut live_children: Vec<LiveChild> = Vec::with_capacity(entries.len());
        for entry in &entries {
            let is_dir = entry.is_directory;
            let is_symlink = entry.is_symlink;
            // SMB/MTP: no inode, no separate physical size; mirror logical into
            // physical, symlinks contribute none (matching the fresh-scan path).
            let (logical_size, physical_size) = if is_symlink {
                (None, None)
            } else {
                (entry.size, entry.physical_size.or(entry.size))
            };
            let entry_physical = physical_size.unwrap_or(0);
            total_physical_bytes += entry_physical;
            progress.bytes_scanned.fetch_add(entry_physical, Ordering::Relaxed);
            total_entries += 1;
            progress.entries_scanned.fetch_add(1, Ordering::Relaxed);
            if is_dir {
                total_dirs += 1;
                progress.dirs_found.fetch_add(1, Ordering::Relaxed);
            }
            live_children.push(LiveChild {
                name: entry.name.clone(),
                is_directory: is_dir,
                is_symlink,
                snap: crate::indexing::metadata::MetadataSnapshot {
                    logical_size,
                    physical_size,
                    modified_at: entry.modified_at,
                    inode: None,
                    nlink: None,
                },
            });
        }

        let db_children =
            IndexStore::list_children_on(dir_id, &conn).map_err(|e| VolumeScanError::Context(e.to_string()))?;

        let diff = reconciler::diff_dir_against_db(dir_id, &live_children, &db_children, &writer);
        added += diff.added;
        removed += diff.removed;
        updated += diff.updated;
        // Same NAS snapshot/system-dir exclusion as the fresh scan: keep the row
        // (it's diffed in like any child) but don't recurse into its subtree. Logged
        // (like the fresh-scan branch) so an error report visibly confirms the skip.
        for (child_id, child_name) in diff.matched_child_dirs {
            if crate::indexing::system_dirs::is_recursion_excluded_dir(&child_name) {
                log::debug!(
                    "volume_scanner: not descending into NAS system dir {}",
                    dir_path.join(&child_name).display()
                );
                continue;
            }
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            if crate::indexing::system_dirs::is_recursion_excluded_dir(&child_name) {
                log::debug!(
                    "volume_scanner: not descending into NAS system dir {}",
                    dir_path.join(&child_name).display()
                );
                continue;
            }
            new_dirs.push((dir_path.clone(), dir_id, child_name));
        }
    }

    // Clean finish: stamp listed dirs, run ONE aggregate, trim the WAL.
    finish_reconcile(&listed_ids, epoch, &writer)?;
    writer
        .send(WriteMessage::WalCheckpoint)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;

    let dirs_listed = crate::pluralize::pluralize(total_dirs, "dir");
    log::info!(
        "volume_scanner: reconcile complete for {}: +{added} -{removed} ~{updated} ({dirs_listed} re-listed) in {}ms",
        root.display(),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start, false))
}

/// Network-path adapter over the shared [`reconciler::finish_reconcile`] (stamp
/// every listed dir, then ONE `ComputeAllAggregates`), mapping its writer-send
/// error into `VolumeScanError`. The finish logic — and the marks-before-aggregate
/// ordering invariant — lives once in `reconciler`, shared with the local
/// reconcile walk, so the two paths can't drift.
fn finish_reconcile(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), VolumeScanError> {
    crate::indexing::reconciler::finish_reconcile(listed_ids, epoch, writer)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))
}

/// List one directory over the `Volume` trait, giving up on it after
/// [`LIST_TIMEOUT`] and (macOS) draining the autoreleasepool. The pool is drained
/// per round trip so autoreleased ObjC objects from the SMB listing path don't
/// accumulate across a long walk.
///
/// Uses `list_directory_for_scan` so a foreground-priority backend (MTP) walks
/// the folder in yielding units; `cancelled` threads in so an in-flight listing
/// bails within one round trip (the MTP path checks it at each unit and per
/// `GetObjectInfo`), not just between directories.
///
/// ❌ The listing runs in its OWN task and the timeout races that task's join
/// handle, never the listing future itself. Timing out drops the handle, which
/// DETACHES the task; it does not cancel it. That distinction is load-bearing:
/// wrapping the listing future directly would drop it mid-round-trip, and on MTP
/// that abandons an in-flight PTP transaction and wedges the phone
/// (`mtp/connection/CLAUDE.md`). The walk gives up on the directory either way;
/// the difference is whether the device survives it. A background MTP scan hits
/// this routinely: it parks at `background_yield_point` while the user is
/// active, so a big folder easily outlives `LIST_TIMEOUT`.
async fn list_one_directory(
    volume: Arc<dyn Volume>,
    dir_path: PathBuf,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<crate::file_system::listing::FileEntry>, VolumeScanError> {
    let listing_path = dir_path.clone();
    let listing = tokio::spawn(async move {
        let result = volume.list_directory_for_scan(&listing_path, Some(&cancelled)).await;
        // Drain the autoreleased ObjC objects this listing created before the
        // future resolves. Cheap no-op on non-macOS.
        drain_autorelease_pool();
        result
    });

    match tokio::time::timeout(LIST_TIMEOUT, listing).await {
        Ok(Ok(Ok(entries))) => Ok(entries),
        Ok(Ok(Err(e))) => Err(VolumeScanError::Volume(e)),
        Ok(Err(join_err)) => Err(VolumeScanError::Volume(
            crate::file_system::volume::VolumeError::IoError {
                message: format!("Directory listing task failed: {join_err}"),
                raw_os_error: None,
            },
        )),
        Err(_elapsed) => Err(VolumeScanError::Timeout(dir_path)),
    }
}

/// Drain the current thread's ObjC autorelease pool. On macOS this wraps a
/// no-op closure in `objc2::rc::autoreleasepool`, which drains on scope exit; on
/// other platforms it's a no-op. We can't hold an `autoreleasepool` guard across
/// an `.await` (it isn't `Send`), so we drain after the await resolves instead.
#[inline]
fn drain_autorelease_pool() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {});
}

/// Whether a `VolumeError` means the whole volume went away mid-walk (terminal
/// disconnect), classified by the TYPED variant — never a message substring
/// (`.claude/rules/no-string-matching.md`).
///
/// `DeviceDisconnected` is the one `VolumeError` variant that means "the volume
/// is gone": a dropped MTP device AND a broken SMB smb2 session both surface as
/// `DeviceDisconnected` from `list_directory` (the SMB-connection-state
/// `Disconnected` is a separate enum used by the FE-facing `smb_connection_state`
/// probe, not returned from a listing call). A `ConnectionTimeout` is handled by
/// the `Timeout`/consecutive-failure path, not here.
fn is_typed_disconnect(e: &crate::file_system::volume::VolumeError) -> bool {
    use crate::file_system::volume::VolumeError;
    matches!(e, VolumeError::DeviceDisconnected(_))
}

/// The partial-preserving write sequence, in ONE place. Run on BOTH a clean finish and a terminal abort (disconnect /
/// consecutive-failure backstop):
///
/// (a) `flush_batch` the last in-flight `InsertEntriesV2` batch (else up to
///     `BATCH_SIZE` rows are dropped),
/// (b) emit the accumulated `MarkDirsListed` for every successfully-listed dir,
/// (c) emit `ComputeAllAggregates` so `dir_stats` (hence `min_subtree_epoch`)
///     exist for what's present — marked subtrees roll up to `epoch > 0` (exact,
///     and stale once the epoch is bumped), unmarked ones to `0` (`—`/`≥`).
///
/// It deliberately does NOT write `scan_completed_at` — that's the completion
/// handler's job, gated on a clean finish, so an interrupted partial heals to a
/// rescan on relaunch (the accepted session-scoped limitation) while staying honest and
/// browsable this session.
fn finish_partial_scan(
    batch: &mut Vec<EntryRow>,
    listed_ids: &[i64],
    epoch: u64,
    writer: &IndexWriter,
) -> Result<(), VolumeScanError> {
    // (a) Flush the last batch so every entry row is committed-in-order before
    // the marks' PK-keyed UPDATE and the aggregate run.
    flush_batch(batch, writer)?;
    // (b) Stamp every successfully-listed dir (ordering invariant: marks precede
    // the final aggregate; the single in-order writer guarantees it). Shared with
    // the reconcile finish so both paths stamp identically.
    crate::indexing::reconciler::send_marks(listed_ids, epoch, writer)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
    // (c) Aggregate over what's present.
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
    Ok(())
}

fn flush_batch(batch: &mut Vec<EntryRow>, writer: &IndexWriter) -> Result<(), VolumeScanError> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    writer
        .send(WriteMessage::InsertEntriesV2(entries))
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))
}

/// Minimum gap between scan-progress heartbeat log lines.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(1);

/// Throttled scan-progress heartbeat (~1/s) at DEBUG. The per-listing
/// `SmbVolume::list_directory` line is at TRACE (off by default), so on a long network
/// scan this is what tells a triager reading an error report that the walk is ALIVE,
/// WHERE it is, and how far along — without the per-directory flood. `phase` is
/// `"scanning"` (fresh) or `"reconciling"`.
fn log_scan_progress(last_log: &mut Instant, phase: &str, dir_path: &Path, total_dirs: u64, total_entries: u64) {
    if last_log.elapsed() < PROGRESS_LOG_INTERVAL {
        return;
    }
    *last_log = Instant::now();
    log::debug!(
        "volume_scanner: {phase}… {}, {}, current: {}",
        crate::pluralize::pluralize(total_dirs, "dir"),
        crate::pluralize::pluralize_with(total_entries, "entry", "entries"),
        dir_path.display()
    );
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

#[cfg(test)]
mod tests;
