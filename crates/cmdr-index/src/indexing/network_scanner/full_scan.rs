//! The destructive full scan: walk the whole share and rebuild the index from
//! what's there.
//!
//! Preceded by a `TruncateData`, so the volume shows nothing until the walk
//! repopulates it. The non-destructive counterpart, which diffs against a
//! populated index and keeps the last-good data visible throughout, is
//! `reconcile_scan.rs`. Both share the round-trip disciplines (cancel, timeout,
//! autoreleasepool, terminal-disconnect + consecutive-failure backstop) and the
//! helpers that implement them, in `mod.rs`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use cmdr_fs::volume::Volume;

use super::scan_pace::ScanPacer;
use super::system_dirs::is_recursion_excluded_dir;
use super::{
    BATCH_SIZE, CONSECUTIVE_FAILURE_ABORT, SCAN_COMMIT_INTERVAL, VolumeScanError, begin_scan_tx, commit_scan_tx,
    flush_batch, is_typed_disconnect, list_one_directory, log_scan_progress, summary,
};
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner::{ScanProgress, ScanSummary};
use crate::indexing::store::{EntryRow, IndexStore, ScanContext};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

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
    reconciler::send_marks(listed_ids, epoch, writer).map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
    // (c) Aggregate over what's present.
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
    Ok(())
}

/// Recursively scan `volume` from its `root`, streaming `EntryRow`s into
/// `writer`. Async (the `Volume` API is async); the caller runs it on a tokio
/// task. On clean completion, fires `ComputeAllAggregates` so the existing
/// aggregator computes `dir_stats` exactly as for a local scan.
///
/// Cancelable via `cancel`; cancellation flushes the current batch and returns
/// `Err(VolumeScanError::Cancelled)` carrying the partial totals. A timeout /
/// backend error returns its own `Err`; the caller discards the partial in both
/// cases (D-interrupted).
///
/// `pacer` decides how many listings may be in flight at each top-up, so the walk
/// gets out of the way while the user browses this share ([`ScanPacer`]).
pub async fn scan_volume_via_trait(
    volume: Arc<dyn Volume>,
    root: PathBuf,
    writer: IndexWriter,
    progress: Arc<ScanProgress>,
    cancel: CancellationToken,
    pacer: ScanPacer,
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

    // Breadth-first, with up to FULL_LISTING_BUDGET listings in flight at once. A dir's id
    // is registered in `ScanContext` when its PARENT's listing is processed (serially,
    // on this task), BEFORE the child is enqueued — so the "parent id registered before
    // we list the child" invariant holds even though the network listings overlap. Only
    // the I/O overlaps; result processing (id alloc, batching) stays single-owner, so no
    // locking. Each queue item is an absolute directory path; the root maps to ROOT_ID.
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.clone());
    let mut last_progress_log = Instant::now();
    let mut inflight = FuturesUnordered::new();

    // Wrap the insert stream in ONE explicit transaction, committed on an interval,
    // so the writer fsyncs per interval instead of per batch (`SCAN_COMMIT_INTERVAL`).
    // `commit_scan_tx` closes it before EVERY exit below, so the connection never
    // returns mid-transaction.
    let mut tx_open = false;
    begin_scan_tx(&writer, &mut tx_open)?;
    let mut last_commit = Instant::now();

    loop {
        if cancel.is_cancelled() {
            // In-flight listings are dropped here; the smb2/MTP backends tolerate a
            // dropped request waiter. Flush what we batched and report the cancel.
            flush_batch(&mut batch, &writer)?;
            commit_scan_tx(&writer, &mut tx_open)?;
            return Err(VolumeScanError::Cancelled(summary(
                total_entries,
                total_dirs,
                total_physical_bytes,
                start,
            )));
        }

        // Commit the insert transaction on the interval and reopen, so accumulated
        // inserts (and the partial `dir_stats` written inside it) become durable and
        // reader-visible without an fsync per batch.
        if tx_open && last_commit.elapsed() >= SCAN_COMMIT_INTERVAL {
            commit_scan_tx(&writer, &mut tx_open)?;
            begin_scan_tx(&writer, &mut tx_open)?;
            last_commit = Instant::now();
        }

        // Keep the pipe full: launch listings until the current budget or the queue
        // drains. Each future owns its clones (self-contained) and returns the dir path
        // alongside the result so the processor can resolve its parent id. Goes through
        // `list_directory_for_scan` (inside `list_one_directory`) so a backend sharing a
        // serialized resource with foreground work (MTP's single USB pipe) yields it
        // between bounded units rather than pinning it for the whole directory.
        //
        // The budget is re-read here, not hoisted: it drops to 1 the moment the user
        // navigates this share, so the in-flight backlog drains and a navigation
        // queues behind one listing instead of 64. In-flight listings are never
        // cancelled (that would waste a completed round trip), so the yield takes
        // effect within one drain.
        while inflight.len() < pacer.listing_budget() {
            let Some(dir) = queue.pop_front() else { break };
            let vol = Arc::clone(&volume);
            let cancel = cancel.clone();
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
            // by the TYPED variant (never a message substring). Stop topping up
            // and drop the
            // in-flight listings rather than churning the still-queued dirs into
            // silently-empty rows (the reported prod bug). Write the partial-preserving
            // sequence in ONE place (flush + marks + aggregate, NO scan_completed_at) so
            // the kept partial is honest, then surface the typed error to the completion
            // handler.
            Err(VolumeScanError::Volume(e)) if is_typed_disconnect(&e) => {
                log::warn!(
                    "network_scanner: device disconnected listing {}: {e}; \
                     keeping honest partial ({} listed, {} queued/in-flight unscanned)",
                    dir_path.display(),
                    cmdr_fs::pluralize::pluralize(total_dirs, "dir"),
                    cmdr_fs::pluralize::pluralize((queue.len() + inflight.len()) as u64, "dir"),
                );
                commit_scan_tx(&writer, &mut tx_open)?;
                finish_partial_scan(&mut batch, &listed_ids, epoch, &writer)?;
                return Err(VolumeScanError::Volume(e));
            }
            Err(VolumeScanError::Volume(ref e)) if dir_path == root => {
                // Failing to list the root itself with a non-disconnect error is
                // fatal — there's nothing to index. Surface it so the caller
                // discards and resets to gray (no honest partial to keep).
                commit_scan_tx(&writer, &mut tx_open)?;
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
                // FULL_LISTING_BUDGET failures can be in flight at once), but a real
                // disconnect piles failures with no successes to reset the counter, so
                // it still trips; an isolated bad dir is reset by its many healthy peers.
                consecutive_failures += 1;
                log::debug!(
                    "network_scanner: skipping unlistable dir {} (consecutive_failures={consecutive_failures}): {err}",
                    dir_path.display(),
                );
                if consecutive_failures >= CONSECUTIVE_FAILURE_ABORT {
                    log::warn!(
                        "network_scanner: {consecutive_failures} consecutive listing failures \
                         (looks like a disconnect); aborting walk and keeping honest partial \
                         ({} listed, {} queued/in-flight unscanned)",
                        cmdr_fs::pluralize::pluralize(total_dirs, "dir"),
                        cmdr_fs::pluralize::pluralize((queue.len() + inflight.len()) as u64, "dir"),
                    );
                    commit_scan_tx(&writer, &mut tx_open)?;
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
                log::debug!(
                    "network_scanner: parent id missing for {}, skipping",
                    dir_path.display()
                );
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
                if is_recursion_excluded_dir(&entry.name) {
                    log::debug!(
                        "network_scanner: not descending into NAS system dir {}",
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
            "network_scanner: root listed empty for {} ({}ms) — treating as a failed scan, not marking complete",
            root.display(),
            start.elapsed().as_millis()
        );
        // Close the (empty) transaction before bailing BEFORE finish, so the
        // connection doesn't return mid-transaction over an untouched DB.
        commit_scan_tx(&writer, &mut tx_open)?;
        return Err(VolumeScanError::EmptyRoot);
    }

    // Clean finish: commit the insert transaction, then the same partial-preserving
    // sequence the terminal-abort branches run (flush + marks + aggregate), then
    // trim the WAL. Committing FIRST keeps the marks/aggregate in autocommit and the
    // ordering invariant (marks precede the final aggregate) in ONE place, so a
    // clean scan and an aborted partial roll up identically.
    commit_scan_tx(&writer, &mut tx_open)?;
    finish_partial_scan(&mut batch, &listed_ids, epoch, &writer)?;
    writer
        .send(WriteMessage::WalCheckpoint)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;

    log::info!(
        "network_scanner: walk complete for {}: entries={total_entries}, dirs={total_dirs} in {}ms",
        root.display(),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start))
}
