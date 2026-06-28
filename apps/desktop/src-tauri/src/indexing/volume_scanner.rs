//! A `Volume`-trait recursive scanner for indexing network/USB volumes.
//!
//! The jwalk [`scanner`](super::scanner) is local-FS-only (`getattrlistbulk`)
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
use crate::indexing::writer::{IndexWriter, WriteMessage};

use super::scanner::{ScanProgress, ScanSummary};

/// Per-directory listing timeout. Network/USB `list_directory` blocks 30–120 s
/// on a hung mount; we cap a single round trip so a wedged share fails the walk
/// instead of parking it. Generous enough for a slow-but-alive NAS directory.
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

/// Batch size for `InsertEntriesV2` sends — matches the jwalk scanner's default.
const BATCH_SIZE: usize = 2000;

/// How many `list_directory` round trips run concurrently during a walk. Directory
/// listing is latency-bound (each dir is open+query+close network round trips over an
/// otherwise-idle link), so keeping many in flight is a near-linear speedup until the
/// server's SMB credits saturate. Only the network I/O is concurrent; results are
/// processed serially on the walk task, so `ScanContext` id allocation and the writer
/// stay single-owner. 32 keeps a NAS busy without flooding it.
const SCAN_CONCURRENCY: usize = 32;

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
    // sentinel), mapping the scan root to ROOT_ID — identical to the jwalk
    // scanner's volume-root setup, so all downstream id/parent logic is shared.
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
                let r = list_one_directory(vol.as_ref(), &dir, &cancel).await;
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
                // variant. Skip it and keep walking the rest, like the jwalk
                // scanner skips errored entries — BUT count consecutive failures.
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
                let r = list_one_directory(vol.as_ref(), &dir, &cancel).await;
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

/// List one directory over the `Volume` trait, wrapped in a timeout and (macOS)
/// an autoreleasepool. The pool is drained per round trip so autoreleased ObjC
/// objects from the SMB listing path don't accumulate across a long walk.
///
/// Uses `list_directory_for_scan` so a foreground-priority backend (MTP) walks
/// the folder in yielding units; `cancelled` threads in so an in-flight listing
/// bails within one round trip (the MTP path checks it at each unit and per
/// `GetObjectInfo`), not just between directories.
async fn list_one_directory(
    volume: &dyn Volume,
    dir_path: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<crate::file_system::listing::FileEntry>, VolumeScanError> {
    let fut = async {
        let result = volume.list_directory_for_scan(dir_path, Some(cancelled)).await;
        // Drain the autoreleased ObjC objects this listing created before the
        // future resolves. Cheap no-op on non-macOS.
        drain_autorelease_pool();
        result
    };

    match tokio::time::timeout(LIST_TIMEOUT, fut).await {
        Ok(Ok(entries)) => Ok(entries),
        Ok(Err(e)) => Err(VolumeScanError::Volume(e)),
        Err(_elapsed) => Err(VolumeScanError::Timeout(dir_path.to_path_buf())),
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
        .send(WriteMessage::ComputeAllAggregates)
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
mod tests {
    use std::sync::atomic::AtomicU64;

    use std::future::Future;
    use std::pin::Pin;

    use super::*;
    use crate::file_system::listing::FileEntry;
    use crate::file_system::volume::{InMemoryVolume, ListingProgress, VolumeError};
    use crate::indexing::store::{ROOT_ID, resolve_path};

    fn progress() -> Arc<ScanProgress> {
        // `ScanProgress::new` is private; build the public-fielded struct directly.
        Arc::new(ScanProgress {
            entries_scanned: Arc::new(AtomicU64::new(0)),
            dirs_found: Arc::new(AtomicU64::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
        })
    }

    fn entry(name: &str, path: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
        FileEntry {
            size,
            ..FileEntry::new(name.to_string(), path.to_string(), is_dir, false)
        }
    }

    /// Walk a small in-memory tree over the `Volume` trait and assert the index
    /// reflects its contents: the writer/aggregator reuse is exercised end to
    /// end (entries land under ROOT_ID, sizes flow into dir_stats). This is the
    /// backend-agnostic half of the SMB-fixture integration test; the live SMB
    /// scan is pinned by `smb_integration_volume_scan_indexes_share` (Docker).
    #[tokio::test]
    async fn scans_in_memory_tree_into_index() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Build an in-memory volume with a known tree:
        //   /sub/         (dir)
        //   /sub/leaf.txt (11 bytes)
        //   /top.txt      (5 bytes)
        let vol = InMemoryVolume::with_entries(
            "Test",
            vec![
                entry("sub", "/sub", true, None),
                entry("leaf.txt", "/sub/leaf.txt", false, Some(11)),
                entry("top.txt", "/top.txt", false, Some(5)),
            ],
        );
        let vol: Arc<dyn Volume> = Arc::new(vol);

        let cancelled = Arc::new(AtomicBool::new(false));
        let summary = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("scan should complete");

        assert!(!summary.was_cancelled);
        assert_eq!(summary.total_entries, 3, "2 files + 1 dir");
        assert_eq!(summary.total_dirs, 1);

        // Async test: await the flush rather than `flush_blocking` (which would
        // `block_on` the current runtime thread and panic).
        writer.flush().await.expect("flush");
        writer.shutdown();

        let store = IndexStore::open(&db_path).expect("reopen");
        let children = store.list_children(ROOT_ID).expect("list root");
        assert_eq!(children.len(), 2, "root has sub/ and top.txt");
        let sub = children.iter().find(|e| e.name == "sub").expect("sub dir present");
        assert!(sub.is_directory);
        let sub_children = store.list_children(sub.id).expect("list sub");
        assert_eq!(sub_children.len(), 1);
        assert_eq!(sub_children[0].name, "leaf.txt");
        assert_eq!(sub_children[0].logical_size, Some(11));
    }

    /// The recursive size scan must NOT descend into NAS snapshot/system dirs
    /// (`@eaDir`, `@Recently-Snapshot`, …): they're hardlinked/huge and recursively
    /// sizing them stalled a real first-scan (`@Recently-Snapshot` alone reported 44 TB
    /// on a 10 TB volume). The dir's OWN row stays indexed (listed + navigable), but its
    /// subtree is never walked — at the share root AND nested inside a normal dir.
    #[tokio::test]
    async fn skips_recursion_into_nas_system_dirs() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-skip.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        let vol = InMemoryVolume::with_entries(
            "Test",
            vec![
                entry("photos", "/photos", true, None),
                // Synology thumbnail sidecar nested inside a normal dir → skip recursion.
                entry("@eaDir", "/photos/@eaDir", true, None),
                entry("thumb.jpg", "/photos/@eaDir/thumb.jpg", false, Some(999)),
                // Snapshot root at the share root → skip recursion.
                entry("@Recently-Snapshot", "/@Recently-Snapshot", true, None),
                entry(
                    "full-copy.bin",
                    "/@Recently-Snapshot/full-copy.bin",
                    false,
                    Some(44_000),
                ),
                entry("keep.txt", "/keep.txt", false, Some(5)),
            ],
        );
        let vol: Arc<dyn Volume> = Arc::new(vol);

        let cancelled = Arc::new(AtomicBool::new(false));
        scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("scan should complete");

        writer.flush().await.expect("flush");
        writer.shutdown();

        let store = IndexStore::open(&db_path).expect("reopen");

        // The system dirs themselves ARE indexed (visible + navigable).
        let root_children = store.list_children(ROOT_ID).expect("list root");
        let snap = root_children
            .iter()
            .find(|e| e.name == "@Recently-Snapshot")
            .expect("@Recently-Snapshot row present (visible, navigable)");
        let photos = root_children
            .iter()
            .find(|e| e.name == "photos")
            .expect("photos present");

        // …but their subtrees are NOT walked.
        assert_eq!(
            store.list_children(snap.id).expect("list snapshot").len(),
            0,
            "snapshot subtree must not be indexed (no recursive descent)",
        );
        let photos_children = store.list_children(photos.id).expect("list photos");
        let eadir = photos_children
            .iter()
            .find(|e| e.name == "@eaDir")
            .expect("@eaDir row present under photos");
        assert_eq!(
            store.list_children(eadir.id).expect("list eaDir").len(),
            0,
            "@eaDir subtree must not be indexed even nested under a normal dir",
        );
    }

    /// A test `Volume` that delegates to an inner `InMemoryVolume` but returns a
    /// TRANSIENT (`PermissionDenied`) error when listing one specific path. Lets
    /// the scanner exercise the "a listing that errors is NOT marked, but the
    /// walk continues" branch — a single transient/permission failure is
    /// skip-and-continue, distinct from a typed `DeviceDisconnected` (terminal).
    struct FailingListVolume {
        inner: InMemoryVolume,
        fail_path: PathBuf,
    }

    type ListFut<'a, T> = Pin<Box<dyn Future<Output = Result<T, VolumeError>> + Send + 'a>>;

    impl Volume for FailingListVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> ListFut<'a, Vec<FileEntry>> {
            if path == self.fail_path {
                return Box::pin(async { Err(VolumeError::PermissionDenied("test: subdir listing failed".into())) });
            }
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
            self.inner.is_directory(path)
        }
    }

    /// A subdir whose listing errors is NOT stamped (`listed_epoch` stays 0),
    /// while its successfully-listed siblings (including an empty-but-listed dir)
    /// and the root get the current epoch. The unit-level disconnect anchor.
    #[tokio::test]
    async fn errored_listing_is_not_marked() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-mark.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Tree:
        //   /good/        (dir, lists fine, has one file)
        //   /good/a.txt
        //   /empty/       (dir, lists fine but empty → empty-but-listed)
        //   /bad/         (dir, listing ERRORS transiently → must stay listed_epoch=0)
        //   /bad/hidden   (file under bad; never discovered because bad won't list)
        let inner = InMemoryVolume::with_entries(
            "Test",
            vec![
                entry("good", "/good", true, None),
                entry("a.txt", "/good/a.txt", false, Some(7)),
                entry("empty", "/empty", true, None),
                entry("bad", "/bad", true, None),
                entry("hidden", "/bad/hidden", false, Some(3)),
            ],
        );
        let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
            inner,
            fail_path: PathBuf::from("/bad"),
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        let summary = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("scan should complete (a single bad subdir is skipped)");
        assert!(!summary.was_cancelled);

        writer.flush().await.expect("flush");
        writer.shutdown();

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let epoch = IndexStore::read_current_epoch(&conn).expect("epoch");
        assert_eq!(epoch, 1, "first scan stamps epoch 1");

        let id_of = |p: &str| -> i64 { resolve_path(&conn, p).expect("resolve").expect("present") };

        // Root and the dirs that listed successfully (incl. empty) are stamped.
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, ROOT_ID).expect("root epoch"),
            Some(1),
            "root listed",
        );
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, id_of("/good")).expect("good epoch"),
            Some(1),
            "good listed",
        );
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, id_of("/empty")).expect("empty epoch"),
            Some(1),
            "empty-but-listed dir is stamped",
        );

        // The errored subdir's row exists (parent listed it) but stays unlisted.
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, id_of("/bad")).expect("bad epoch"),
            Some(0),
            "a dir whose own listing errored stays listed_epoch=0 (honest unknown)",
        );
    }

    /// A test `Volume` that counts `list_directory` calls and returns a
    /// `DeviceDisconnected` error once the count reaches `fail_after_calls`. Lets
    /// a test assert the walk STOPS at the disconnect (no further round trips
    /// against a dead session) by reading the call counter back afterwards.
    struct CountingDisconnectVolume {
        inner: InMemoryVolume,
        fail_after_calls: usize,
        /// Total `list_directory` attempts so far (incremented on every call).
        calls: Arc<AtomicU64>,
        /// When true, the failure is a plain `IoError` (a disconnect-SHAPED error
        /// that does NOT map to the typed `DeviceDisconnected`/`Disconnected`
        /// variant), to exercise the consecutive-failure backstop instead of the
        /// typed terminal branch. When false, it's `DeviceDisconnected` (typed).
        untyped_failure: bool,
    }

    impl Volume for CountingDisconnectVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> ListFut<'a, Vec<FileEntry>> {
            let n = (self.calls.fetch_add(1, Ordering::Relaxed) + 1) as usize;
            if n >= self.fail_after_calls {
                let untyped = self.untyped_failure;
                return Box::pin(async move {
                    if untyped {
                        Err(VolumeError::IoError {
                            message: "test: connection reset".into(),
                            raw_os_error: None,
                        })
                    } else {
                        Err(VolumeError::DeviceDisconnected("test: session dropped mid-walk".into()))
                    }
                });
            }
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
            self.inner.is_directory(path)
        }
    }

    /// Build a wide tree: a root with `n_subdirs` empty subdirs. The BFS lists
    /// the root first (call 1), then each subdir in turn (calls 2..=n_subdirs+1).
    fn wide_tree(n_subdirs: usize) -> InMemoryVolume {
        let mut entries = Vec::new();
        for i in 0..n_subdirs {
            entries.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
        }
        InMemoryVolume::with_entries("Test", entries)
    }

    /// THE regression test for the reported prod bug. A volume disconnects after
    /// listing K of N dirs: the walk must STOP promptly (not churn the remaining
    /// N−K queued dirs into empty rows), return the typed `DeviceDisconnected`
    /// error, and — crucially — the caller must write NO `scan_completed_at`
    /// (asserted at the manager level; here we assert the typed error + prompt
    /// stop, which is what the completion handler routes on).
    #[tokio::test]
    async fn disconnect_mid_walk_stops_promptly_and_returns_typed_error() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-disconnect.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Root + 200 empty subdirs (≫ SCAN_CONCURRENCY). BFS: list root (call 1)
        // discovers 200 dirs, then lists them concurrently (up to SCAN_CONCURRENCY in
        // flight). The 4th list call returns a typed disconnect. The walk must stop
        // topping up and drop the in-flight listings rather than churning all 200.
        let n_subdirs = 200;
        let fail_after_calls = 4;
        let calls = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
            inner: wide_tree(n_subdirs),
            fail_after_calls,
            calls: Arc::clone(&calls),
            untyped_failure: false,
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        // The typed terminal error, NOT a clean Ok (which is today's bug: a clean
        // finish over silently-empty rows). Matched by the TYPED variant.
        match result {
            Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
            other => panic!("expected typed DeviceDisconnected terminal error, got {other:?}"),
        }

        // Prompt stop: the walk bailed within ~one concurrency window of the disconnect
        // and did NOT churn the remaining queued dirs. With concurrency the count is no
        // longer exactly `fail_after_calls` (up to SCAN_CONCURRENCY listings were already
        // in flight), but it's bounded well below the full `n_subdirs`.
        let made = calls.load(Ordering::Relaxed) as usize;
        assert!(
            made < n_subdirs,
            "walk must stop at the disconnect, not churn all {n_subdirs} queued dirs (made {made})",
        );
        assert!(
            made <= 1 + SCAN_CONCURRENCY + fail_after_calls,
            "walk must stop within ~one concurrency window of the disconnect (made {made})",
        );

        writer.flush().await.expect("flush");
        writer.shutdown();
    }

    /// The consecutive-failure backstop: a disconnect-shaped error that does NOT
    /// map to the typed variant (here `IoError`) must still abort the walk after
    /// `CONSECUTIVE_FAILURE_ABORT` consecutive failures, rather than churning
    /// every queued dir into an empty row.
    #[tokio::test]
    async fn consecutive_untyped_failures_trip_the_backstop() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-backstop.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Enough subdirs that the backstop (N consecutive) trips well before the
        // queue drains, even with up to SCAN_CONCURRENCY listings in flight. Root lists
        // fine (call 1), then every subdir listing fails with an untyped IoError.
        let n_subdirs = CONSECUTIVE_FAILURE_ABORT * 6;
        let calls = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
            inner: wide_tree(n_subdirs),
            fail_after_calls: 2, // root ok, then every child fails
            calls: Arc::clone(&calls),
            untyped_failure: true,
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        match result {
            Err(VolumeScanError::ConsecutiveFailures { count, .. }) => {
                assert_eq!(count, CONSECUTIVE_FAILURE_ABORT, "aborts at exactly the threshold");
            }
            other => panic!("expected ConsecutiveFailures backstop abort, got {other:?}"),
        }

        // Bounded stop: the backstop aborts after ~root + one concurrency window +
        // N failures (concurrency means some listings were already in flight), and the
        // remaining dirs were never attempted — well short of the full queue.
        let made = calls.load(Ordering::Relaxed) as usize;
        assert!(
            made < n_subdirs,
            "backstop must stop well short of churning the whole {n_subdirs}-dir queue (made {made})",
        );
        assert!(
            made <= 1 + SCAN_CONCURRENCY + CONSECUTIVE_FAILURE_ABORT,
            "backstop stops within ~one concurrency window of the threshold (made {made})",
        );

        writer.flush().await.expect("flush");
        writer.shutdown();
    }

    /// A single transient failure followed by successes does NOT trip the
    /// backstop: the consecutive counter resets on every success, so an isolated
    /// bad dir is still skip-and-continue (the existing behavior we keep).
    #[tokio::test]
    async fn isolated_transient_failure_does_not_trip_backstop() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-transient.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // One subdir fails (untyped), the rest list fine. The scan completes
        // cleanly (the bad dir is skipped, stays listed_epoch=0).
        let inner = InMemoryVolume::with_entries(
            "Test",
            vec![
                entry("good", "/good", true, None),
                entry("a.txt", "/good/a.txt", false, Some(7)),
                entry("bad", "/bad", true, None),
                entry("alsogood", "/alsogood", true, None),
            ],
        );
        let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
            inner,
            fail_path: PathBuf::from("/bad"),
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        let summary = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("an isolated transient failure is skipped, scan completes");
        assert!(!summary.was_cancelled);

        writer.flush().await.expect("flush");
        writer.shutdown();
    }

    /// A `Volume` wrapper that records the maximum number of `list_directory` calls in
    /// flight at once. The `yield_now` lets sibling listings launched in the same
    /// `FuturesUnordered` batch coexist before any resolves, so the recorded max
    /// reflects real concurrency rather than instantly-ready mock timing.
    struct ConcurrencyTrackingVolume {
        inner: InMemoryVolume,
        in_flight: Arc<AtomicU64>,
        max_in_flight: Arc<AtomicU64>,
    }

    impl Volume for ConcurrencyTrackingVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> ListFut<'a, Vec<FileEntry>> {
            Box::pin(async move {
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(now, Ordering::SeqCst);
                tokio::task::yield_now().await;
                let r = self.inner.list_directory(path, on_progress).await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                r
            })
        }
        fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
            self.inner.is_directory(path)
        }
    }

    /// THE speedup regression guard: the walk lists directories CONCURRENTLY, capped at
    /// `SCAN_CONCURRENCY`. With many sibling dirs queued, multiple `list_directory` round
    /// trips are in flight at once — a revert to a serial walk would record a max of 1.
    #[tokio::test]
    async fn walk_lists_directories_concurrently() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-concurrency.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Root with many empty subdirs (≫ SCAN_CONCURRENCY): the root listing discovers
        // them all, then they list concurrently up to the cap.
        let in_flight = Arc::new(AtomicU64::new(0));
        let max_in_flight = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
            inner: wide_tree(SCAN_CONCURRENCY * 2),
            in_flight: Arc::clone(&in_flight),
            max_in_flight: Arc::clone(&max_in_flight),
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("scan completes");
        writer.flush().await.expect("flush");
        writer.shutdown();

        let max = max_in_flight.load(Ordering::SeqCst) as usize;
        assert!(
            max > 1,
            "the walk must list concurrently, not serially (max in flight = {max})"
        );
        assert!(
            max <= SCAN_CONCURRENCY,
            "concurrency must stay capped at SCAN_CONCURRENCY (max in flight = {max})",
        );
    }

    /// `is_terminal_disconnect` routes the completion handler: true for a typed
    /// `DeviceDisconnected` and the consecutive-failure backstop (keep honest
    /// partial + Stale), false for a timeout / context / writer-send (discard).
    #[test]
    fn terminal_disconnect_classification() {
        assert!(
            VolumeScanError::Volume(VolumeError::DeviceDisconnected("x".into())).is_terminal_disconnect(),
            "typed DeviceDisconnected is a terminal disconnect"
        );
        assert!(
            VolumeScanError::ConsecutiveFailures {
                count: CONSECUTIVE_FAILURE_ABORT,
                last: "io".into()
            }
            .is_terminal_disconnect(),
            "the consecutive-failure backstop is a terminal disconnect"
        );
        // Non-disconnect terminations are NOT kept as honest partials.
        assert!(
            !VolumeScanError::Timeout(PathBuf::from("/wedged")).is_terminal_disconnect(),
            "a timeout is discarded, not kept"
        );
        assert!(
            !VolumeScanError::Volume(VolumeError::PermissionDenied("root".into())).is_terminal_disconnect(),
            "a non-disconnect volume error (root-fatal) is discarded"
        );
        assert!(!VolumeScanError::WriterSend("gone".into()).is_terminal_disconnect());
        assert!(!VolumeScanError::Context("ctx".into()).is_terminal_disconnect());
    }

    /// A `Volume` whose ROOT listing FAILS with a non-disconnect, non-typed
    /// error (here `PermissionDenied`). Lets a test exercise the root-fatal
    /// branch: the scanner must surface the error so the caller doesn't mark
    /// completion over a never-built index.
    struct RootFailsVolume {
        inner: InMemoryVolume,
    }

    impl Volume for RootFailsVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> ListFut<'a, Vec<FileEntry>> {
            if path == Path::new("/") {
                return Box::pin(async { Err(VolumeError::PermissionDenied("test: root listing denied".into())) });
            }
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
            self.inner.is_directory(path)
        }
    }

    /// A fresh scan whose ROOT listing SUCCEEDS but returns ZERO children must
    /// NOT report a clean completion: it returns the typed `EmptyRoot` error so
    /// the completion handler leaves `scan_completed_at` unwritten. This is the
    /// guard against the real-hardware bug where a NAS scan that walked nothing
    /// stamped a false "complete" marker and stranded the index forever. (The
    /// completion handler's persistence of the marker is asserted at the manager
    /// level; here we pin the typed error the handler routes on.)
    #[tokio::test]
    async fn empty_root_fresh_scan_does_not_complete() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-empty-root.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        // Root lists fine but has no children at all.
        let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        match result {
            Err(VolumeScanError::EmptyRoot) => {}
            other => panic!("expected EmptyRoot (no completion), got {other:?}"),
        }
        // EmptyRoot is NOT a terminal disconnect: the completion handler discards
        // and resets to gray rather than keeping a "stale" empty partial.
        assert!(
            !VolumeScanError::EmptyRoot.is_terminal_disconnect(),
            "an empty root is a failed scan to discard, not an honest partial to keep",
        );

        writer.flush().await.expect("flush");
        writer.shutdown();
    }

    /// The root-fatal case stays fatal: a ROOT listing that ERRORS (not empty,
    /// not a disconnect) surfaces the error so no completion marker is written.
    /// Distinguishes "root listing FAILED" (`Volume`) from "root listed EMPTY"
    /// (`EmptyRoot`) — both refuse completion, via different typed variants.
    #[tokio::test]
    async fn failed_root_listing_does_not_complete() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-root-fail.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        let vol: Arc<dyn Volume> = Arc::new(RootFailsVolume {
            inner: InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]),
        });

        let cancelled = Arc::new(AtomicBool::new(false));
        let result = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        match result {
            Err(VolumeScanError::Volume(VolumeError::PermissionDenied(_))) => {}
            other => panic!("expected the root-fatal Volume error (no completion), got {other:?}"),
        }

        writer.flush().await.expect("flush");
        writer.shutdown();
    }

    /// A pre-set cancel flag stops the walk immediately and reports
    /// `was_cancelled` (the caller then discards the partial — D-interrupted).
    #[tokio::test]
    async fn honors_cancellation_before_first_listing() {
        use crate::indexing::writer::IndexWriter;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("vol-scan-cancel.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

        let vol = InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]);
        let vol: Arc<dyn Volume> = Arc::new(vol);

        let cancelled = Arc::new(AtomicBool::new(true));
        let summary = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("cancelled scan still returns Ok");
        assert!(summary.was_cancelled);
        assert_eq!(summary.total_entries, 0, "nothing scanned after immediate cancel");

        writer.shutdown();
    }

    // ── Non-destructive reconcile rescan (network path) ────────

    use crate::indexing::writer::IndexWriter;
    use rusqlite::Connection;

    fn entry_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .expect("count entries")
    }

    /// Recursive logical size of a dir by absolute path, from `dir_stats`.
    fn dir_size(conn: &Connection, path: &str) -> u64 {
        let id = resolve_path(conn, path).expect("resolve").expect("present");
        IndexStore::get_dir_stats_by_id(conn, id)
            .expect("stats")
            .map(|s| s.recursive_logical_size)
            .unwrap_or(0)
    }

    fn min_epoch(conn: &Connection, path: &str) -> u64 {
        let id = resolve_path(conn, path).expect("resolve").expect("present");
        IndexStore::get_dir_stats_by_id(conn, id)
            .expect("stats")
            .map(|s| s.min_subtree_epoch)
            .unwrap_or(0)
    }

    /// Build a writer + DB pre-populated to an "already fully scanned" state by
    /// running a fresh `scan_volume_via_trait` over `vol`. Returns (writer, db_path,
    /// tempdir). Epoch is seeded to 1 by the fresh scan.
    async fn fresh_scan(vol: Arc<dyn Volume>) -> (IndexWriter, PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("reconcile.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        let cancelled = Arc::new(AtomicBool::new(false));
        scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("fresh scan");
        writer.flush().await.expect("flush");
        (writer, db_path, dir)
    }

    /// A small known tree:
    ///   /sub/         (dir)
    ///   /sub/keep.txt (4 bytes)
    ///   /sub/mod.txt  (4 bytes)
    ///   /top.txt      (5 bytes)
    fn base_tree() -> Vec<FileEntry> {
        vec![
            entry("sub", "/sub", true, None),
            entry("keep.txt", "/sub/keep.txt", false, Some(4)),
            entry("mod.txt", "/sub/mod.txt", false, Some(4)),
            entry("top.txt", "/top.txt", false, Some(5)),
        ]
    }

    /// A reconcile rescan over an UNCHANGED tree writes ZERO entry rows (the
    /// no-op-cheap property the perf bench relied on): unchanged rows are diffed and
    /// skipped, never re-UPSERTed, so the catastrophic INSERT OR REPLACE path is
    /// never touched. Coverage still re-stamps to the new epoch.
    #[tokio::test]
    async fn reconcile_noop_writes_zero_entry_rows() {
        let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let rows_before = entry_count(&conn);
        let max_id_before: i64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
            .unwrap();

        // A continuity break would bump the epoch before a rescan; mirror that.
        let new_epoch = {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap()
        };

        // Reconcile the SAME tree (nothing changed on disk).
        let cancelled = Arc::new(AtomicBool::new(false));
        reconcile_volume_via_trait(
            Arc::clone(&vol),
            PathBuf::from("/"),
            writer.clone(),
            progress(),
            cancelled,
        )
        .await
        .expect("reconcile");
        writer.flush().await.expect("flush");

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            entry_count(&conn),
            rows_before,
            "no-op reconcile must not change the entry row count"
        );
        let max_id_after: i64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            max_id_after, max_id_before,
            "no-op reconcile must not allocate any new ids (zero rows written)"
        );
        // Coverage re-stamped to the new epoch (the single aggregate ran).
        assert_eq!(
            min_epoch(&conn, "/sub"),
            new_epoch,
            "no-op reconcile re-stamps coverage to the new epoch"
        );

        writer.shutdown();
    }

    /// A reconcile rescan with changes (add / remove / modify) refreshes sizes
    /// correctly AND ends byte-identical (entry set + dir sizes) to a
    /// fresh-from-scratch scan of the SAME final tree. The 1.83 TB-ghost guard.
    #[tokio::test]
    async fn reconcile_with_changes_matches_fresh_from_scratch() {
        let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

        // Final tree: remove keep.txt, modify mod.txt (4→20 bytes), add new.txt,
        // add a new subdir with a file.
        let final_tree = vec![
            entry("sub", "/sub", true, None),
            entry("mod.txt", "/sub/mod.txt", false, Some(20)),
            entry("new.txt", "/sub/new.txt", false, Some(7)),
            entry("deep", "/sub/deep", true, None),
            entry("d.txt", "/sub/deep/d.txt", false, Some(3)),
            entry("top.txt", "/top.txt", false, Some(5)),
        ];
        let vol_after: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree.clone()));

        // Bump epoch (continuity break) then reconcile to the final tree.
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        reconcile_volume_via_trait(
            Arc::clone(&vol_after),
            PathBuf::from("/"),
            writer.clone(),
            progress(),
            cancelled,
        )
        .await
        .expect("reconcile");
        writer.flush().await.expect("flush");

        // Fresh-from-scratch oracle: scan the final tree into a clean DB.
        let vol_oracle: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree));
        let (oracle_writer, oracle_db, _odir) = fresh_scan(vol_oracle).await;

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let oconn = IndexStore::open_read_connection(&oracle_db).expect("oracle read conn");

        // keep.txt gone; new.txt + deep/ present.
        assert!(
            resolve_path(&conn, "/sub/keep.txt").unwrap().is_none(),
            "removed file gone"
        );
        assert!(
            resolve_path(&conn, "/sub/new.txt").unwrap().is_some(),
            "added file present"
        );
        assert!(
            resolve_path(&conn, "/sub/deep/d.txt").unwrap().is_some(),
            "new subtree present"
        );

        // Same recursive sizes as a fresh build (no ghosts).
        assert_eq!(
            dir_size(&conn, "/sub"),
            dir_size(&oconn, "/sub"),
            "/sub size matches fresh"
        );
        assert_eq!(dir_size(&conn, "/"), dir_size(&oconn, "/"), "root size matches fresh");
        // mod.txt's new size is reflected: /sub = mod(20) + new(7) + deep/d(3) = 30.
        assert_eq!(dir_size(&conn, "/sub"), 30, "reconciled /sub reflects modify + adds");

        writer.shutdown();
        oracle_writer.shutdown();
    }

    /// A mid-rescan DISCONNECT leaves the PRIOR complete index intact (now possible
    /// — no truncate ran) and surfaces the typed terminal error. The re-listed dirs
    /// are stamped at the rescan epoch; unreached dirs keep their prior data. The
    /// completion handler (manager) then bumps past the epoch so everything reads
    /// stale — here we assert the prior data SURVIVES (the headline reconcile property).
    #[tokio::test]
    async fn mid_reconcile_disconnect_keeps_prior_index() {
        // Wide tree so the disconnect leaves real dirs unreached.
        let mut before = vec![entry("top.txt", "/top.txt", false, Some(5))];
        for i in 0..20 {
            before.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
            before.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
        }
        let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", before));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let rows_before = entry_count(&conn);
        assert!(rows_before > 20, "prior complete index has all dirs");
        let root_size_before = dir_size(&conn, "/");

        // A disconnecting volume: lists the root + a couple dirs, then drops.
        let calls = Arc::new(AtomicU64::new(0));
        let mut after = vec![entry("top.txt", "/top.txt", false, Some(5))];
        for i in 0..20 {
            after.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
            after.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
        }
        let vol_disc: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
            inner: InMemoryVolume::with_entries("Test", after),
            fail_after_calls: 4, // root + a few dirs, then disconnect
            calls: Arc::clone(&calls),
            untyped_failure: false,
        });

        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let result =
            reconcile_volume_via_trait(vol_disc, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        match result {
            Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
            other => panic!("expected typed terminal disconnect, got {other:?}"),
        }
        writer.flush().await.expect("flush");

        // The prior index is INTACT: no truncate ran, all rows still present, sizes
        // unchanged (the unreached dirs were never re-listed, so their data stands).
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            entry_count(&conn),
            rows_before,
            "mid-rescan disconnect must not lose any prior rows (no truncate)"
        );
        assert_eq!(
            dir_size(&conn, "/"),
            root_size_before,
            "prior root size survives a mid-rescan disconnect"
        );

        writer.shutdown();
    }

    /// First scan (empty DB) is a fresh truncate+build, NOT a reconcile: the manager
    /// chooses by entry-count, but at this layer we confirm `scan_volume_via_trait`
    /// builds correctly from empty (the precondition the reconcile path relies on:
    /// a populated DB). This pins that the two entry points produce the same index.
    #[tokio::test]
    async fn first_scan_builds_then_reconcile_is_a_no_op() {
        let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let built = entry_count(&conn);
        // 4 tree entries (sub, keep.txt, mod.txt, top.txt) + the ROOT_ID sentinel.
        assert_eq!(built, 5, "first scan built all 4 entries plus the root sentinel");

        // Immediately reconciling the same tree is a no-op (zero new rows).
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        reconcile_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("reconcile");
        writer.flush().await.expect("flush");

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(entry_count(&conn), built, "reconcile after first scan adds no rows");

        writer.shutdown();
    }

    /// Count entries stamped at exactly `epoch` (the dirs this reconcile pass
    /// successfully re-listed). A reconcile that descends the whole tree stamps
    /// every dir; one that stops at the root stamps only the root.
    fn dirs_listed_at_epoch(conn: &Connection, epoch: u64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch = ?1",
            [epoch],
            |r| r.get(0),
        )
        .expect("count listed dirs")
    }

    /// THE regression test for the reported prod bug: a reconcile over an
    /// already-partially-indexed share must DESCEND into every existing child
    /// dir, not stop at the root after matching its children by name.
    ///
    /// Setup mirrors prod (`naspi`): the DB knows the root + its top-level dirs
    /// from an earlier interrupted scan, but those dirs are EMPTY in the index —
    /// their real subtrees were never listed. The live volume has the full tree.
    /// A child dir being "unchanged" at the root's level (same mtime → no UPSERT)
    /// says NOTHING about whether its own subtree was ever scanned, so the
    /// reconcile must recurse into it regardless.
    ///
    /// Pre-fix (recursion gated on a change/upsert) this stamped only the root
    /// and left every deep file missing — a green badge over an unscanned share.
    #[tokio::test]
    async fn reconcile_descends_into_existing_unchanged_child_dirs() {
        // Prior index: root + 3 top-level dirs, each EMPTY (the interrupted-scan
        // state). A fresh scan stamps these at epoch 1 with stable mtimes.
        let shallow = vec![
            entry("a", "/a", true, None),
            entry("b", "/b", true, None),
            entry("c", "/c", true, None),
        ];
        let vol_prior: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", shallow));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_prior)).await;

        // The full live tree: the SAME 3 top dirs (unchanged → no UPSERT at the
        // root), now each holding a subdir with a deep file. 3 top dirs + 3
        // subdirs = 6 dirs total under the root, plus the root itself = 7 dirs.
        let full = vec![
            entry("a", "/a", true, None),
            entry("sub_a", "/a/sub_a", true, None),
            entry("deep_a.txt", "/a/sub_a/deep_a.txt", false, Some(11)),
            entry("b", "/b", true, None),
            entry("sub_b", "/b/sub_b", true, None),
            entry("deep_b.txt", "/b/sub_b/deep_b.txt", false, Some(22)),
            entry("c", "/c", true, None),
            entry("sub_c", "/c/sub_c", true, None),
            entry("deep_c.txt", "/c/sub_c/deep_c.txt", false, Some(33)),
        ];
        let vol_full: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", full));

        // A continuity break bumps the epoch before a rescan; mirror that so the
        // reconcile stamps re-listed dirs at the NEW epoch (distinct from epoch 1).
        let new_epoch = {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap()
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        reconcile_volume_via_trait(vol_full, PathBuf::from("/"), writer.clone(), progress(), cancelled)
            .await
            .expect("reconcile");
        writer.flush().await.expect("flush");

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

        // The walk descended into EVERY dir: root + 3 top + 3 sub = 7 dirs, all
        // stamped at the new epoch. Pre-fix only the root (1) was stamped.
        assert_eq!(
            dirs_listed_at_epoch(&conn, new_epoch),
            7,
            "reconcile must re-list every dir (root + 3 top + 3 sub), not stop at the root"
        );

        // The deep files the prior index never had are now present and sized —
        // proof the recursion actually listed the subtrees, not just stamped them.
        for (path, size) in [
            ("/a/sub_a/deep_a.txt", 11u64),
            ("/b/sub_b/deep_b.txt", 22),
            ("/c/sub_c/deep_c.txt", 33),
        ] {
            let id = resolve_path(&conn, path)
                .expect("resolve")
                .unwrap_or_else(|| panic!("{path} should be indexed after reconcile descends"));
            let row = IndexStore::get_entry_by_id(&conn, id).expect("entry").expect("present");
            assert_eq!(row.logical_size, Some(size), "{path} reconciled with its real size");
        }

        // Recursive sizes rolled up through the descended tree: root = 11+22+33.
        assert_eq!(
            dir_size(&conn, "/"),
            66,
            "root recursive size reflects the deep files the reconcile descended to find"
        );

        writer.shutdown();
    }

    /// A reconcile rescan whose ROOT suddenly lists EMPTY (the share glitched or
    /// the session is half-dead) must NOT report a clean completion: it returns
    /// the typed `EmptyRoot` error so the prior (stale-but-real) index is kept
    /// and never overwritten as falsely-complete-and-empty. Without this guard a
    /// transient empty root strands the index as "complete" with zero entries.
    #[tokio::test]
    async fn reconcile_empty_root_does_not_complete() {
        // Start from a real, fully-scanned tree so the reconcile path runs over a
        // populated index.
        let populated: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
        let (writer, db_path, _dir) = fresh_scan(Arc::clone(&populated)).await;

        let rows_before = {
            let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
            entry_count(&conn)
        };
        assert!(rows_before > 0, "precondition: the index has data to reconcile against");

        // A continuity break bumps the epoch before a rescan; mirror that.
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap();
        }

        // Now reconcile against a volume whose root lists EMPTY (the glitch).
        let empty: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));
        let cancelled = Arc::new(AtomicBool::new(false));
        let result = reconcile_volume_via_trait(empty, PathBuf::from("/"), writer.clone(), progress(), cancelled).await;

        match result {
            Err(VolumeScanError::EmptyRoot) => {}
            other => panic!("expected EmptyRoot from a reconcile whose root went empty, got {other:?}"),
        }
        writer.flush().await.expect("flush");

        // The prior index is untouched — reconcile wrote no changes and we bailed
        // before the diff/removal/marks, so the stale-but-real rows survive.
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            entry_count(&conn),
            rows_before,
            "a glitched empty-root reconcile must not blank the prior index",
        );

        writer.shutdown();
    }

    /// THE regression test for the post-Forget SMB enable bug: a reconcile over an
    /// EMPTY DB whose scan root is NOT `/` (the real case — an SMB share mounts at
    /// `/Volumes/<share>`) must still DESCEND into every newly-discovered child
    /// dir, fully indexing the multi-level tree.
    ///
    /// The enable path routes a no-completion-marker DB through the reconcile walk;
    /// post-Forget that DB is empty, so EVERY dir is "new". New dirs are resolved
    /// after a flush to get their freshly-assigned ids before recursing. Resolving
    /// by ABSOLUTE PATH (`/Volumes/naspi/_test`) walks component-by-component from
    /// ROOT_ID, but the index root IS `/Volumes/naspi` (mapped to ROOT_ID) — so the
    /// walk fails at the first component (`Volumes`) and resolves NOTHING. The
    /// reconcile then stops at the root and falsely "completes" with only the
    /// top-level entries (badge green, no real scan). Resolving by `(parent_id,
    /// name)` is correct for any root. Pre-fix this assertion fails: only the root
    /// and its immediate children are indexed, the subtrees are missing.
    #[tokio::test]
    async fn reconcile_from_empty_db_with_non_root_mount_indexes_full_tree() {
        // An SMB-shaped mount: root is `/Volumes/naspi`, with a multi-level tree.
        let root = PathBuf::from("/Volumes/naspi");
        let tree = vec![
            entry("top", "/Volumes/naspi/top", true, None),
            entry("sub", "/Volumes/naspi/top/sub", true, None),
            entry("deep.txt", "/Volumes/naspi/top/sub/deep.txt", false, Some(42)),
            entry("other", "/Volumes/naspi/other", true, None),
            entry("leaf.txt", "/Volumes/naspi/other/leaf.txt", false, Some(7)),
        ];
        let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("naspi", tree));

        // Empty DB + writer (the post-Forget state). The manager bumps the epoch at
        // the scan-start funnel before spawning the walk; mirror that so listed dirs
        // stamp the bumped epoch.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("reconcile-empty.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        let new_epoch = {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap()
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        let summary = reconcile_volume_via_trait(vol, root, writer.clone(), progress(), cancelled)
            .await
            .expect("reconcile from empty DB on a non-`/` mount");
        assert!(!summary.was_cancelled);
        writer.flush().await.expect("flush");

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

        // The walk descended into EVERY dir: root + top + top/sub + other = 4 dirs,
        // all stamped at the new epoch. Pre-fix only the root (1) was stamped.
        assert_eq!(
            dirs_listed_at_epoch(&conn, new_epoch),
            4,
            "reconcile must re-list every dir (root + top + top/sub + other), not stop at the root"
        );

        // The deep files prove recursion actually listed the subtrees rather than
        // just stamping the top level. Resolved by (parent_id, name) chains since
        // `resolve_path` from `/` can't reach a `/Volumes/naspi`-rooted index.
        let id_of = |parent: i64, name: &str| -> i64 {
            IndexStore::resolve_component(&conn, parent, name)
                .expect("resolve")
                .unwrap_or_else(|| panic!("{name} should be indexed after reconcile descends"))
        };
        let top = id_of(ROOT_ID, "top");
        let sub = id_of(top, "sub");
        let deep = id_of(sub, "deep.txt");
        let deep_row = IndexStore::get_entry_by_id(&conn, deep)
            .expect("entry")
            .expect("present");
        assert_eq!(
            deep_row.logical_size,
            Some(42),
            "deep.txt reconciled with its real size"
        );

        let other = id_of(ROOT_ID, "other");
        let leaf = id_of(other, "leaf.txt");
        let leaf_row = IndexStore::get_entry_by_id(&conn, leaf)
            .expect("entry")
            .expect("present");
        assert_eq!(leaf_row.logical_size, Some(7), "leaf.txt reconciled with its real size");

        writer.shutdown();
    }
}
