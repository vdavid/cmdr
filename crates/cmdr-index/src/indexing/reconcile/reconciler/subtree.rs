//! The small-scope subtree reconcile: walk a subtree, diff each directory against
//! the DB, and write only the differences.
//!
//! The LIVE fill path (per-navigation verifier, `MustScanSubDirs`, SMB-overflow
//! `FullRefresh`) and the cover walk's repair path. Safe to interrupt at any point:
//! the DB is never left in a partially-deleted state.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cmdr_fs::log_private::LogPrivate;
use rusqlite::Connection;

use crate::indexing::IndexPathSpace;
use crate::indexing::deletes;
use crate::indexing::hold::VolumeWork;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::paths::path_prefix::compute_parent_path;
use crate::indexing::scanner::{self, LiveWalk};
use crate::indexing::store::IndexStore;
use crate::indexing::writer::{IndexWriter, WriteMessage};

use super::diff::{LiveChild, MissingRows, diff_dir_against_db};
use super::dir_read::read_fs_children;
use super::escalation::resolve_escalation_anchor;

/// Summary of a subtree reconciliation.
pub(crate) struct ReconcileSummary {
    pub added: u64,
    /// Directories among [`added`](Self::added). A caller reporting a walk's work
    /// needs the split; nothing else does, so ❌ don't grow a second counter per
    /// kind here.
    pub added_dirs: u64,
    pub removed: u64,
    pub updated: u64,
    /// Directories the walk reached but couldn't list, almost always because
    /// they were deleted between the event and the read (a compiler emptying its
    /// target dir). One line each was ~750 an hour on a build machine and none of
    /// them was a diagnosis, so the walk counts them and the summary line
    /// reports the number; the paths stay one `RUST_LOG=…reconcile=trace` away.
    pub unreadable_dirs: u64,
    pub duration: Duration,
    /// How much of `duration` was spent parked on the writer queue (a full channel,
    /// or a `flush_blocking` waiting for the writer to catch up), from
    /// `writer::wait_probe`. Without it a walk that was mostly WAITING is
    /// indistinguishable from a walk that was slow, and the log line blames the
    /// wrong subsystem.
    pub writer_wait: Duration,
    /// Set when the reconcile couldn't anchor because the subtree root's chain is
    /// still (partly) missing from the index (the skip branch, or a parent that
    /// resolves to a file). Carries the escalation anchor — a rescan root strictly
    /// closer to the volume root — for the caller to re-queue. `None` on success.
    pub escalation: Option<PathBuf>,
    /// Whether the walk stopped early because its token fired. A reconcile is
    /// safe to interrupt — every directory it listed is still marked — but a
    /// caller deciding whether a scope is now covered has to be able to tell a
    /// finished walk from a stopped one.
    pub cancelled: bool,
}

impl ReconcileSummary {
    /// What this anchor's own walk cost: the duration minus the time parked on the
    /// writer queue. This is what the per-subtree rescan throttle scales its window
    /// by, so charging the wait would let one saturated writer (an initial scan,
    /// say) inflate every anchor's measured cost at once and back a whole volume
    /// off for half an hour. Attribute only the walk.
    pub(crate) fn walk_cost(&self) -> Duration {
        self.duration.saturating_sub(self.writer_wait)
    }
}

/// This walk's accumulated wait on the writer queue, and a rearm for the next one.
/// `reconcile_subtree` arms the probe at its start, so every read covers exactly
/// one walk. See `writer::wait_probe`.
fn writer_wait() -> Duration {
    crate::indexing::writer::wait_probe::take()
}

/// Reconcile a subtree by diffing the filesystem against the DB directory-by-directory.
///
/// Unlike `scanner::scan_subtree` which deletes all descendants then re-inserts,
/// this function walks each directory, compares children by name, and only writes
/// the differences. Safe to interrupt at any point: the DB is never in a
/// partially-deleted state.
///
/// This is the LIVE small-scope fill path (per-navigation verifier,
/// `MustScanSubDirs`, SMB-overflow `FullRefresh`): it propagates coverage per
/// listed dir. The full-rescan path does NOT use this — the network rescan walks
/// via `network_scanner::reconcile_volume_via_trait`, which reuses the shared
/// [`diff_dir_against_db`] but stamps + runs ONE `ComputeAllAggregates` (the
/// single-aggregate constraint the perf bench measured), never per-dir propagation.
///
/// `live` is who is watching this pass happen; pass `None` to fill the index and
/// nothing else. It matters because this is also the cover walk's repair path
/// (`lifecycle/cover`): the search that asked for that walk answers with the
/// index's covered half plus what the walk hands back, and the covered half was
/// read from an arena that PREDATES this pass. So the rows a repair creates reach
/// the search through [`LiveWalk::emit`] or through nothing, and a silent repair
/// leaves that search short while its walk still ends `Completed` — a wrong answer
/// calling itself exhaustive. ⚠️ Only the created rows: a row the index already
/// held is the covered half's to report, and sending it too would double it.
/// `work` is the walk's own: its stop signal, and the share of the volume's hold it
/// reads under. It also answers the delete gate below — whether this generation's
/// drive is still listed — which is why the walk takes the whole value and ❌ never a
/// bare token.
pub(in crate::indexing) fn reconcile_subtree(
    root: &Path,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    work: &VolumeWork,
    live: Option<LiveWalk<'_>>,
) -> Result<ReconcileSummary, String> {
    // Split so the two halves can end at different times: a consumer that goes
    // away stops being FED, and the pulse keeps moving for whoever else is reading
    // it (a second run judging whether this walk is still working).
    let heartbeat = live.as_ref().map(|live| live.heartbeat);
    let mut emit = live.map(|live| live.emit);
    let start = Instant::now();
    // Arm the writer-wait probe, discarding whatever ran on this thread before.
    let _ = writer_wait();
    let mut added: u64 = 0;
    let mut added_dirs: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;

    // The epoch every dir we successfully list this pass is stamped with. A
    // reconcile *stamps* with the current epoch; it never bumps it. Read once.
    let epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
    // Every dir whose direct contents we successfully list (incl. empty), so we
    // can `MarkDirsListed` them after the walk and lift ancestor coverage.
    // Without this, a reconcile-discovered subtree stays `listed_epoch = 0`
    // forever and drags every ancestor to incomplete — the exact local-live-path
    // regression this milestone guards against.
    let mut listed_dir_ids: Vec<i64> = Vec::new();

    // The absolute path in this volume's world (firmlink-normalized for the boot
    // disk, raw for a mount-rooted drive); the mount-relative strip is applied only
    // at the `resolve_abs` argument, so `root_str` stays absolute for the FS reads.
    let root_str = space.absolute(&root.to_string_lossy());
    let root_id = match space.resolve_abs(conn, &root_str) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Root not in DB. This happens when must_scan_sub_dirs fires for a
            // newly created/copied directory. Try to create it: resolve the parent,
            // stat the root, and upsert it via the writer.
            let parent_path = compute_parent_path(&root_str);
            let parent_id = match space.resolve_abs(conn, &parent_path) {
                Ok(Some(id)) => {
                    // Harden against the type-change orphan class: the parent must be
                    // a DIRECTORY row before we parent new entries under it. A parent
                    // that resolves to a FILE (a stale file→dir type change) means the
                    // chain is broken; escalate to a rescan that re-lists the deepest
                    // existing dir, healing it — never upsert under a file id.
                    let parent_is_dir = matches!(
                        IndexStore::get_entry_by_id(conn, id),
                        Ok(Some(e)) if e.is_directory
                    );
                    if parent_is_dir {
                        id
                    } else {
                        log::debug!(
                            "reconcile_subtree: parent of {} is not a directory row, escalating",
                            LogPrivate::dir_path(&root_str)
                        );
                        return Ok(ReconcileSummary {
                            added: 0,
                            added_dirs: 0,
                            removed: 0,
                            updated: 0,
                            unreadable_dirs: 0,
                            duration: start.elapsed(),
                            writer_wait: writer_wait(),
                            escalation: resolve_escalation_anchor(space, conn, &root_str),
                            cancelled: work.cancel.is_cancelled(),
                        });
                    }
                }
                Ok(None) => {
                    // Neither root nor parent in DB: the chain above is (partly)
                    // missing (Leak B, subtree-scan variant). Escalate to a rescan
                    // anchored at the highest missing dir (strictly closer to the
                    // volume root, so the caller's re-queue converges by depth)
                    // rather than dropping the whole subtree's credit.
                    log::debug!(
                        "reconcile_subtree: neither root nor parent in DB, escalating: {}",
                        LogPrivate::dir_path(&root_str)
                    );
                    return Ok(ReconcileSummary {
                        added: 0,
                        added_dirs: 0,
                        removed: 0,
                        updated: 0,
                        unreadable_dirs: 0,
                        duration: start.elapsed(),
                        writer_wait: writer_wait(),
                        escalation: resolve_escalation_anchor(space, conn, &root_str),
                        cancelled: work.cancel.is_cancelled(),
                    });
                }
                Err(e) => return Err(format!("resolve_path for parent: {e}")),
            };

            // Stat the root directory and upsert it
            let metadata = match std::fs::symlink_metadata(root) {
                Ok(m) => m,
                Err(e) => {
                    // The root vanished between the event and now: nothing to index
                    // and no chain to heal (a real delete will arrive as its own
                    // event), so no escalation.
                    log::debug!(
                        "reconcile_subtree: can't stat root {}: {e}",
                        LogPrivate::dir_path(&root_str)
                    );
                    return Ok(ReconcileSummary {
                        added: 0,
                        added_dirs: 0,
                        removed: 0,
                        updated: 0,
                        unreadable_dirs: 0,
                        duration: start.elapsed(),
                        writer_wait: writer_wait(),
                        escalation: None,
                        cancelled: work.cancel.is_cancelled(),
                    });
                }
            };

            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let snap = extract_metadata(&metadata, metadata.is_dir(), metadata.is_symlink());
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                // Null the inode on FAT/exFAT (unstable derived inode).
                inode: space.trust_inode(snap.inode),
                nlink: snap.nlink,
            });

            // Flush so the read connection can see the new entry
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush after root upsert failed: {e}");
            }
            added += 1;
            // ❌ Counted, never handed to `emit`, and that can't cost a consumer a
            // row: a cover walk materializes its root before the repair runs
            // (`cover/bootstrap`) and reports it there, so this branch is only ever
            // reached with no consumer attached.
            if metadata.is_dir() {
                added_dirs += 1;
            }

            match space.resolve_abs(conn, &root_str) {
                Ok(Some(id)) => id,
                Ok(None) => {
                    // We just upserted and flushed, yet the row is absent: a write
                    // race or a concurrent delete. Re-queuing the same root would
                    // spin, so don't escalate; the next real event heals it.
                    log::warn!(
                        "reconcile_subtree: root still not in DB after upsert, skipping: {}",
                        LogPrivate::dir_path(&root_str)
                    );
                    return Ok(ReconcileSummary {
                        added,
                        added_dirs,
                        removed: 0,
                        updated: 0,
                        unreadable_dirs: 0,
                        duration: start.elapsed(),
                        writer_wait: writer_wait(),
                        escalation: None,
                        cancelled: work.cancel.is_cancelled(),
                    });
                }
                Err(e) => return Err(format!("resolve_path for root after upsert: {e}")),
            }
        }
        Err(e) => return Err(format!("resolve_path for root: {e}")),
    };

    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));

    // Collect newly-created directories so we can flush the writer, resolve their IDs,
    // and then queue them for recursive processing.
    let mut new_dir_paths: Vec<PathBuf> = Vec::new();
    // Dirs this walk reached but couldn't list. Counted, not logged per path:
    // see the field's doc on `ReconcileSummary`.
    let mut unreadable_dirs: u64 = 0;

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if work.cancel.is_cancelled() {
            break;
        }

        // Before the read, not after: a walk parked on a directory that hangs has
        // to show as working, which is the whole reason the pulse counts STARTS.
        if let Some(heartbeat) = heartbeat {
            heartbeat.entering(&dir_path);
        }
        // ⚠️ **The rows BEFORE the listing, ❌ never after.** A row the live path
        // adds after a listing was taken isn't in it, so diffing that listing
        // against newer rows reaps a file that's right there on disk: gone from
        // the index, its folder reading 0 bytes, with no event left to heal it.
        let db_children =
            IndexStore::list_children_on(dir_id, conn).map_err(|e| format!("list_children_on({dir_id}): {e}"))?;
        let listing = match read_fs_children(&dir_path, space) {
            Some(listing) => listing,
            None => {
                unreadable_dirs += 1;
                continue;
            }
        };
        // A test's gate (`rescan/gate.rs`): the listing is taken, and the diff isn't.
        #[cfg(test)]
        super::rescan::gate::pass_after_listing(&dir_path);
        // We successfully listed this dir's direct contents (an empty listing
        // still counts). Stamp it at the current epoch after the walk.
        listed_dir_ids.push(dir_id);
        if space.absolute(&dir_path.to_string_lossy()) == space.volume_root_string() {
            // The volume ROOT listed, which is half of what clears the delete
            // generation; the presence read below is the other half.
            deletes::root_listed(work.volume_id());
        }

        // ⚠️ **Presence AFTER the read, ❌ never before.** An unmounted `/Volumes/X`
        // whose mount-point folder survives lists as EMPTY and complete, so a read
        // taken first would pass the gate and let every top-level child be deleted.
        // One read per directory, ❌ never per entry.
        let drive_listed = work.drive_is_listed();
        if drive_listed {
            // Also the presence half of the delete generation's reset: paired with a
            // root listing that came after the last batch, it says the drive really
            // was there all along.
            deletes::drive_seen(work.volume_id());
        } else {
            // And the batches this walk already sent are the ones nothing can take
            // back, so the index says so on disk.
            deletes::note_the_drive_left(work.volume_id(), writer);
        }
        let missing = if listing.complete && drive_listed {
            MissingRows::Delete
        } else {
            MissingRows::Keep
        };

        // Normalize the local listing into source-agnostic `LiveChild`s and run
        // the shared per-dir diff (same logic the network walk uses).
        let live_children: Vec<LiveChild> = listing
            .children
            .into_iter()
            .map(|child| {
                let mut snap = child.snap;
                // Null the inode on FAT/exFAT so the value `diff_dir_against_db`
                // stores can never feed a false rename match.
                snap.inode = space.trust_inode(snap.inode);
                LiveChild {
                    name: child.name,
                    is_directory: child.is_dir,
                    is_symlink: child.is_symlink,
                    snap,
                }
            })
            .collect();

        let diff = diff_dir_against_db(dir_id, &live_children, &db_children, missing, writer);
        if diff.removed > 0 {
            deletes::batch_sent(work.volume_id());
        }
        added += diff.added;
        added_dirs += diff.added_children.iter().filter(|child| child.is_directory).count() as u64;
        removed += diff.removed;
        updated += diff.updated;
        hand_rows_over(&mut emit, &dir_path, &diff.added_children);
        for (child_id, child_name) in diff.matched_child_dirs {
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            new_dir_paths.push(dir_path.join(child_name));
        }

        // If we found new directories and the queue is empty (current level done),
        // flush the writer so the read connection can resolve the new IDs.
        if !new_dir_paths.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush failed: {e}");
            }
            for new_dir in new_dir_paths.drain(..) {
                let path_str = space.absolute(&new_dir.to_string_lossy());
                if let Ok(Some(id)) = space.resolve_abs(conn, &path_str) {
                    queue.push_back((new_dir, id));
                }
            }
        }
    }

    // Stamp every dir we listed at the current epoch, then lift ancestor
    // coverage. The walk collected ids shallow→deep (BFS), so recompute
    // deepest-first: a parent's `min_subtree_epoch` reads its children's stored
    // values, which must already reflect this pass. `propagate_min_subtree_epoch`
    // short-circuits once a value stabilizes, so the repeated up-walks are cheap.
    // (This is the SMALL-SCOPE live path; a full rescan uses the single-aggregate
    // path in `network_scanner`, not per-dir propagation — see the fn doc.)
    if !listed_dir_ids.is_empty() {
        // Chunk under SQLite's bound-parameter ceiling (+1 for the epoch param).
        const MARK_CHUNK: usize = 900;
        for chunk in listed_dir_ids.chunks(MARK_CHUNK) {
            if let Err(e) = writer.send(WriteMessage::MarkDirsListed {
                ids: chunk.to_vec(),
                epoch,
            }) {
                log::warn!("reconcile_subtree: failed to send MarkDirsListed: {e}");
            }
        }
        for dir_id in listed_dir_ids.iter().rev() {
            let _ = writer.send(WriteMessage::PropagateMinSubtreeEpoch(*dir_id));
        }
    }

    Ok(ReconcileSummary {
        added,
        added_dirs,
        removed,
        updated,
        unreadable_dirs,
        duration: start.elapsed(),
        writer_wait: writer_wait(),
        escalation: None,
        cancelled: work.cancel.is_cancelled(),
    })
}

/// How many created rows cross to a live consumer at once.
///
/// One directory's worth would do on any ordinary tree, and a directory with a
/// million new children is not ordinary: the channel behind this is bounded at a
/// few batches (`cover::BATCH_QUEUE_DEPTH`) on the understanding that a batch is
/// about this size, and one unbounded batch would quietly undo that. Matches the
/// parallel walker's own batch size.
const EMIT_CHUNK: usize = 2_000;

/// Hand the rows one directory's diff created to whoever is consuming this pass
/// live, in bounded batches.
///
/// A consumer that has gone away (a closed search dialog) just stops being fed:
/// `emit` is dropped and the walk runs on, because its rows are already in the
/// index for the next query to find. Same contract as the parallel walker's own
/// `emit` (`scanner/insert_visitor.rs`).
fn hand_rows_over(emit: &mut Option<&scanner::EntrySender>, dir_path: &Path, created: &[&LiveChild]) {
    if emit.is_none() || created.is_empty() {
        return;
    }
    for chunk in created.chunks(EMIT_CHUNK) {
        let batch: Vec<scanner::CoveredEntry> = chunk
            .iter()
            .map(|child| scanner::CoveredEntry {
                path: dir_path.join(&child.name),
                is_directory: child.is_directory,
                is_symlink: child.is_symlink,
                // The sizes a LISTING shows, pre-dedup: the writer's hardlink dedup
                // keeps the stored recursive sums honest, and a search result
                // showing a hardlinked file as 0 bytes would just be wrong.
                logical_size: child.snap.logical_size,
                physical_size: child.snap.physical_size,
                modified_at: child.snap.modified_at,
            })
            .collect();
        if let Some(sender) = emit
            && sender.send(batch).is_err()
        {
            *emit = None;
            return;
        }
    }
}
