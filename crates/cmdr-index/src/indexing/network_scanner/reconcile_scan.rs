//! The non-destructive reconcile: diff the live share against an
//! already-populated index and write only the changes.
//!
//! Same BFS and same round-trip disciplines as `full_scan.rs`, but no
//! `TruncateData` precedes it, so the last-good index stays visible (stale)
//! throughout and a mid-rescan disconnect leaves the prior data intact. The
//! per-dir diff itself is `reconciler::diff_dir_against_db`, shared with the
//! local reconcile walk. Shared helpers live in `mod.rs`.

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
    CONSECUTIVE_FAILURE_ABORT, VolumeScanError, is_typed_disconnect, list_one_directory, log_scan_progress, summary,
};
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner::{ScanProgress, ScanSummary};
use crate::indexing::store::IndexStore;
use crate::indexing::writer::{IndexWriter, WriteMessage};

/// Non-destructively RECONCILE a network volume against an already-populated
/// index, instead of truncating and rebuilding.
///
/// Walks the same BFS over `Volume::list_directory` as [`scan_volume_via_trait`](super::full_scan::scan_volume_via_trait),
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
    cancel: CancellationToken,
    pacer: ScanPacer,
) -> Result<ScanSummary, VolumeScanError> {
    use crate::indexing::reconcile::reconciler::{self, LiveChild};
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
        if cancel.is_cancelled() {
            // User cancel: stop, but leave the prior index intact (no truncate ran).
            // Mirror the fresh-scan cancel contract (no marks/aggregate on cancel).
            // In-flight listings are dropped (backends tolerate a dropped waiter).
            return Err(VolumeScanError::Cancelled(summary(
                total_entries,
                total_dirs,
                total_physical_bytes,
                start,
            )));
        }

        // Keep up to the current budget of listings in flight — matched (existing) child
        // dirs whose ids we already hold. Same overlap-the-latency-bound-I/O win as the
        // fresh scan, and the same throttle while the user browses this share;
        // processing (diff, writes) stays serial on this task and the DB read conn.
        while inflight.len() < pacer.listing_budget() {
            let Some((dir, id)) = queue.pop_front() else { break };
            let vol = Arc::clone(&volume);
            let cancel = cancel.clone();
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
                        "network_scanner: reconcile couldn't resolve new dir after flush: {}",
                        child_path.display()
                    ),
                    Err(e) => log::warn!(
                        "network_scanner: reconcile resolve_component failed for {}: {e}",
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
                    "network_scanner: device disconnected reconciling {}: {e}; \
                     keeping prior index ({} re-listed, {} queued/in-flight unreached)",
                    dir_path.display(),
                    cmdr_fs::pluralize::pluralize(total_dirs, "dir"),
                    cmdr_fs::pluralize::pluralize((queue.len() + inflight.len() + new_dirs.len()) as u64, "dir"),
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
                    "network_scanner: skipping unlistable dir {} during reconcile (consecutive_failures={consecutive_failures}): {err}",
                    dir_path.display(),
                );
                if consecutive_failures >= CONSECUTIVE_FAILURE_ABORT {
                    log::warn!(
                        "network_scanner: {consecutive_failures} consecutive listing failures during reconcile \
                         (looks like a disconnect); aborting and keeping prior index \
                         ({} re-listed, {} queued/in-flight unreached)",
                        cmdr_fs::pluralize::pluralize(total_dirs, "dir"),
                        cmdr_fs::pluralize::pluralize((queue.len() + inflight.len() + new_dirs.len()) as u64, "dir"),
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
                "network_scanner: reconcile root listed empty for {} ({}ms) — treating as a failed rescan, keeping prior index",
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
            if is_recursion_excluded_dir(&child_name) {
                log::debug!(
                    "network_scanner: not descending into NAS system dir {}",
                    dir_path.join(&child_name).display()
                );
                continue;
            }
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            if is_recursion_excluded_dir(&child_name) {
                log::debug!(
                    "network_scanner: not descending into NAS system dir {}",
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

    let dirs_listed = cmdr_fs::pluralize::pluralize(total_dirs, "dir");
    log::info!(
        "network_scanner: reconcile complete for {}: +{added} -{removed} ~{updated} ({dirs_listed} re-listed) in {}ms",
        root.display(),
        start.elapsed().as_millis()
    );

    Ok(summary(total_entries, total_dirs, total_physical_bytes, start))
}

/// Network-path adapter over the shared [`reconciler::finish_reconcile`] (stamp
/// every listed dir, then ONE `ComputeAllAggregates`), mapping its writer-send
/// error into `VolumeScanError`. The finish logic — and the marks-before-aggregate
/// ordering invariant — lives once in `reconciler`, shared with the local
/// reconcile walk, so the two paths can't drift.
fn finish_reconcile(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), VolumeScanError> {
    reconciler::finish_reconcile(listed_ids, epoch, writer).map_err(|e| VolumeScanError::WriterSend(e.to_string()))
}
