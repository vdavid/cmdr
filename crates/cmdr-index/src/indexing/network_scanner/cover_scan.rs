//! The scoped walk: cover ONE coverage-frontier node over the `Volume` trait.
//!
//! The search-driven counterpart to `full_scan.rs`, and the reason a search over
//! a share or a phone can answer for ground the index has never seen. It is the
//! same BFS with the same round-trip disciplines (`mod.rs`), and differs in three
//! ways, each of them a consequence of who asked:
//!
//! - **It is SCOPED.** The root is a frontier node, resolved to its own entry id,
//!   not `ROOT_ID`. Someone searching one folder on a 10 TB NAS asked for that
//!   folder.
//! - **A cancel KEEPS what it read.** The full scan discards its partial (the
//!   caller resets the volume); a cover walk runs the mark-and-aggregate finish on
//!   every exit, because convergence is the whole point: the next search over the
//!   same ground has to start where this one stopped.
//! - **It only ever ADDS.** A name the index already holds under a directory keeps
//!   its row and its id; the walk writes the names that aren't there yet and
//!   descends into the ones that are. That's what makes it safe over ground an
//!   interrupted walk or a live event already touched, without the local walker's
//!   virgin-root refusal — over the `Volume` trait a per-directory name check costs
//!   one indexed query against a listing that costs a network round trip.
//!
//! ❌ It never deletes, and it never writes `scan_completed_at`: covering one
//! folder says nothing about the volume.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use tokio_util::sync::CancellationToken;

use cmdr_fs::volume::Volume;

use super::scan_pace::ScanPacer;
use super::system_dirs::is_recursion_excluded_dir;
use super::{
    BATCH_SIZE, CONSECUTIVE_FAILURE_ABORT, SCAN_COMMIT_INTERVAL, VolumeScanError, begin_scan_tx, commit_scan_tx,
    flush_batch, is_typed_disconnect, list_one_directory, log_scan_progress, summary,
};
use crate::indexing::IndexPathSpace;
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner::{CoveredEntry, EmitPacer, EntrySender, ScanSummary, WalkHeartbeat};
use crate::indexing::store::{EntryRow, IndexStore, UnreadableCause, normalize_for_comparison, resolve_scan_root};
use crate::indexing::writer::{IndexWriter, WriteMessage};

/// Walk one coverage-frontier node on a volume the index reaches only through the
/// `Volume` trait, filling its index and feeding whoever is listening.
///
/// `root` is an absolute path in the space the coverage answer used; it must
/// already have an `entries` row (`lifecycle/cover/bootstrap.rs` materializes the
/// chain when it doesn't). `emit` is where a live consumer receives entries as
/// they're found; pass `None` to fill the index and nothing else. `heartbeat` is
/// how that consumer sees the walk moving between batches, and what it gave up on.
///
/// `Ok` means the walk reached the end of this subtree. A cancel arrives as
/// `Cancelled` and a mid-walk disconnect as its own typed error — both AFTER the
/// coverage the walk earned is durable.
#[allow(
    clippy::too_many_arguments,
    reason = "the walk's whole context: what to read, where to write, who's listening, and how to stop. A param struct would only rename the same eight things"
)]
pub(crate) async fn cover_volume_subtree(
    volume: Arc<dyn Volume>,
    root: PathBuf,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    emit: Option<EntrySender>,
    cancel: &CancellationToken,
    pacer: &ScanPacer,
    heartbeat: &WalkHeartbeat,
) -> Result<ScanSummary, VolumeScanError> {
    let start = Instant::now();

    // A READ connection, like the reconcile walk: the writer owns the write side,
    // and a second write-mode connection can `SQLITE_BUSY` live indexing. The epoch
    // is read, never seeded — a walk stamps the value the volume is on, and the
    // cold-drive seeding happened when the writer was stood up.
    let db_path = writer.db_path();
    let conn = IndexStore::open_read_connection(&db_path).map_err(|e| VolumeScanError::Context(e.to_string()))?;
    let epoch = IndexStore::read_current_epoch(&conn).map_err(|e| VolumeScanError::Context(e.to_string()))?;
    let root_id = resolve_root_id(&conn, space, &root)?;

    let mut writes = CoverWrites::new(writer, epoch, emit);
    // A frontier rooted AT a NAS system directory: mark it and stop, without one
    // round trip. Both whole-volume walks index such a directory's own row and
    // refuse its subtree, so it sits at `listed_epoch = 0` — which the descent rule
    // reads as frontier and would hand to every later search. Walking it is the
    // stall `system_dirs.rs` exists to prevent (44 TB of hardlinked snapshots on a
    // 10 TB volume), so the walk says "won't read" instead of "haven't read".
    if root
        .file_name()
        .is_some_and(|name| is_recursion_excluded_dir(&name.to_string_lossy()))
    {
        log::debug!(
            "network_scanner: not covering NAS system dir {}; marking it as ground we won't read",
            root.display()
        );
        writes.mark_unreadable(root_id);
        writes.finish(root_id)?;
        return Ok(summary(0, 0, 0, start));
    }

    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut total_physical_bytes: u64 = 0;
    let mut consecutive_failures: usize = 0;

    // BFS by (absolute dir path, its DB id), exactly like the reconcile walk: the
    // scan root is a frontier node rather than the volume root, so ids are carried
    // rather than looked up by path. Only the network I/O overlaps — results are
    // processed serially on this task, so id allocation stays single-owner.
    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.clone(), root_id));
    let mut inflight = FuturesUnordered::new();
    let mut last_progress_log = Instant::now();

    let mut tx_open = false;
    begin_scan_tx(writer, &mut tx_open)?;
    let mut last_commit = Instant::now();

    loop {
        if cancel.is_cancelled() {
            // ❌ NOT the full scan's discard. Everything this walk listed stays
            // marked, so the frontier is genuinely smaller than it was — that is
            // the convergence property the whole coverage concept rests on.
            commit_scan_tx(writer, &mut tx_open)?;
            writes.finish(root_id)?;
            return Err(VolumeScanError::Cancelled(summary(
                total_entries,
                total_dirs,
                total_physical_bytes,
                start,
            )));
        }

        if tx_open && last_commit.elapsed() >= SCAN_COMMIT_INTERVAL {
            commit_scan_tx(writer, &mut tx_open)?;
            begin_scan_tx(writer, &mut tx_open)?;
            last_commit = Instant::now();
        }

        // Keep the pipe full, re-reading the budget at every top-up so browsing the
        // share drops the walk to one listing in flight (`scan_pace.rs`). A search
        // is foreground work, but it's the same shared session the pane navigates
        // through, so the same yield applies.
        while inflight.len() < pacer.listing_budget() {
            let Some((dir, id)) = queue.pop_front() else { break };
            // Counted (and named) as the listing goes OUT, not as it comes back: a
            // round trip that never returns is exactly the one a watcher wants to
            // see named.
            heartbeat.entering(&dir);
            let vol = Arc::clone(&volume);
            let cancel = cancel.clone();
            inflight.push(async move {
                let r = list_one_directory(vol, dir.clone(), cancel).await;
                ((dir, id), r)
            });
        }

        let Some(((dir_path, dir_id), result)) = inflight.next().await else {
            break;
        };

        let entries = match result {
            Ok(entries) => {
                consecutive_failures = 0;
                entries
            }
            // The volume went away mid-walk. Stop rather than churning the queued
            // dirs into silently-empty rows, and keep what we have — same typed
            // branch as the full scan, same reason.
            Err(VolumeScanError::Volume(e)) if is_typed_disconnect(&e) => {
                log::warn!(
                    "network_scanner: device disconnected covering {}: {e}; keeping what the walk covered",
                    dir_path.display()
                );
                commit_scan_tx(writer, &mut tx_open)?;
                writes.finish(root_id)?;
                return Err(VolumeScanError::Volume(e));
            }
            Err(err) if dir_path == root => {
                // The frontier node itself can't be listed — refused, timed out, or
                // anything else. There is nothing to cover, so this is a FAILED root
                // rather than a covered one: it stays frontier and the next search
                // asks again. ⚠️ Matched on the root path for EVERY error, not just
                // the backend's typed ones, because falling through to the skip
                // branch below would drain the queue and report a clean walk over
                // ground nothing ever read.
                commit_scan_tx(writer, &mut tx_open)?;
                writes.finish(root_id)?;
                return Err(err);
            }
            Err(err) => {
                consecutive_failures += 1;
                // Ground this walk started and won't finish. THIS run's answer is a
                // lower bound and has to say so.
                //
                // ⚠️ The dir also stays unlisted with NO `unreadable_cause`, so the
                // frontier offers it again and every later search re-pays the same
                // failing listing. That is a known bug, and the local walker's fixed
                // twin (`UnreadableCause::Abandoned`). ❌ Don't port that fix
                // mechanically: a whole-share disconnect reaches this same arm, and
                // marking on it would condemn every directory the walk touched on
                // the way down. `DETAILS.md` § "A failed listing leaves no cause".
                heartbeat.abandoned(1);
                log::debug!(
                    "network_scanner: skipping unlistable dir {} while covering (consecutive_failures={consecutive_failures}): {err}",
                    dir_path.display(),
                );
                if consecutive_failures >= CONSECUTIVE_FAILURE_ABORT {
                    log::warn!(
                        "network_scanner: {consecutive_failures} consecutive listing failures while covering \
                         (looks like a disconnect); stopping and keeping what the walk covered"
                    );
                    commit_scan_tx(writer, &mut tx_open)?;
                    writes.finish(root_id)?;
                    return Err(VolumeScanError::ConsecutiveFailures {
                        count: consecutive_failures,
                        last: err.to_string(),
                    });
                }
                continue;
            }
        };

        // This directory's listing succeeded, so it gets stamped — including when
        // it came back empty. ❌ No empty-root refusal here, unlike the two
        // whole-volume walks: a share that lists empty is a glitch worth refusing,
        // an empty FOLDER is an ordinary thing to search, and refusing to mark it
        // would hand it back to every later search forever.
        writes.mark_listed(dir_id);
        log_scan_progress(&mut last_progress_log, "covering", &dir_path, total_dirs, total_entries);

        // Add-only, per directory: whatever the index already holds under this
        // directory keeps its row and its id. Two things need it, and the second is
        // the sharp one:
        //
        // - ground an interrupted walk (or a live event) already wrote, where fresh
        //   ids for the same names would collide, `INSERT OR IGNORE` would drop one,
        //   and everything below the dropped id would be orphaned;
        // - MTP's same-name siblings, which are two DIFFERENT objects with one name
        //   in one folder. The index can only hold one (`idx_parent_name_folded`),
        //   so the walk takes the first and skips the rest EXPLICITLY, rather than
        //   allocating an id whose rows silently vanish.
        let mut taken = existing_children(&conn, dir_id)?;

        for entry in entries {
            let folded = normalize_for_comparison(&entry.name);
            match taken.get(&folded).copied() {
                // A row the index already holds. A directory among them is descended
                // into with the id it already has, so the ground below it converges
                // too; nothing is rewritten either way.
                Some(Slot::InTheIndex(child)) => {
                    if let Some(child_id) = child {
                        if is_recursion_excluded_dir(&entry.name) {
                            writes.mark_unreadable(child_id);
                        } else {
                            queue.push_back((PathBuf::from(&entry.path), child_id));
                        }
                    }
                    taken.insert(folded, Slot::TakenHere);
                    continue;
                }
                // A second object with the same name in one folder, which only MTP
                // produces. The index can hold one, so the walk keeps the first and
                // says so, rather than allocating an id whose rows silently vanish.
                Some(Slot::TakenHere) => {
                    log::debug!(
                        "network_scanner: {} has a same-name sibling this index can't hold; keeping the first",
                        entry.path
                    );
                    continue;
                }
                None => {
                    taken.insert(folded, Slot::TakenHere);
                }
            }

            let is_dir = entry.is_directory;
            let is_symlink = entry.is_symlink;
            let child_path = PathBuf::from(&entry.path);
            let id = writer.next_id().fetch_add(1, Ordering::Relaxed);

            if is_dir {
                total_dirs += 1;
                // The dir's own row is indexed either way; its SUBTREE is what a NAS
                // system dir doesn't get walked (`system_dirs.rs`). Marked as ground
                // we won't read, so the frontier stops offering it — see the root
                // case above for why that matters more here than on a full scan.
                if is_recursion_excluded_dir(&entry.name) {
                    log::debug!(
                        "network_scanner: not descending into NAS system dir {}",
                        child_path.display()
                    );
                    writes.mark_unreadable(id);
                } else {
                    queue.push_back((child_path.clone(), id));
                }
            }

            // SMB/MTP have no inode and no separate physical size; mirror the
            // logical size into physical so `dir_stats`' physical totals are
            // populated. Symlinks contribute no size, matching every other walk.
            let (logical_size, physical_size) = if is_symlink {
                (None, None)
            } else {
                let s = entry.size;
                (s, entry.physical_size.or(s))
            };
            total_physical_bytes += physical_size.unwrap_or(0);
            total_entries += 1;

            writes.push(
                EntryRow {
                    id,
                    parent_id: dir_id,
                    name: entry.name,
                    is_directory: is_dir,
                    is_symlink,
                    logical_size,
                    physical_size,
                    modified_at: entry.modified_at,
                    inode: space.trust_inode(entry.inode),
                },
                CoveredEntry {
                    path: child_path,
                    is_directory: is_dir,
                    is_symlink,
                    logical_size,
                    physical_size,
                    modified_at: entry.modified_at,
                },
            )?;
        }
    }

    commit_scan_tx(writer, &mut tx_open)?;
    writes.finish(root_id)?;

    log::debug!(
        "network_scanner: covered {} ({} entries, {} dirs) in {}ms",
        root.display(),
        total_entries,
        total_dirs,
        start.elapsed().as_millis()
    );
    Ok(summary(total_entries, total_dirs, total_physical_bytes, start))
}

/// The frontier node's own entry id.
///
/// The absolute path crosses into the volume's index path space first (identity on
/// the boot disk, mount-root-stripped on every trait-scanned volume, whose index
/// `ROOT_ID` is the mount): an absolute walk from `ROOT_ID` misses at the very
/// first component on a share.
fn resolve_root_id(conn: &rusqlite::Connection, space: &IndexPathSpace, root: &Path) -> Result<i64, VolumeScanError> {
    let absolute = space.absolute(&root.to_string_lossy());
    let index_relative = space
        .index_relative(&absolute)
        .ok_or_else(|| VolumeScanError::Context(format!("{} is outside the volume's index", root.display())))?;
    resolve_scan_root(conn, Path::new(&index_relative), false).map_err(|e| VolumeScanError::Context(e.to_string()))
}

/// Directory ids per `MarkDirsUnreadable` message, mirroring the local walker's.
const MARK_CHUNK: usize = 10_000;

/// What a name under the directory being listed already resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// The index holds a row for it, carrying its id when it's a directory (the
    /// only case the walk can descend into).
    InTheIndex(Option<i64>),
    /// This listing has already claimed the name, either by writing a row for it or
    /// by descending into the row that was there.
    TakenHere,
}

/// Every name the index already holds under `dir_id`, folded the way the store's
/// uniqueness index folds it. One indexed query per directory, against a listing
/// that cost a network round trip.
fn existing_children(conn: &rusqlite::Connection, dir_id: i64) -> Result<HashMap<String, Slot>, VolumeScanError> {
    let rows = IndexStore::list_children_on(dir_id, conn).map_err(|e| VolumeScanError::Context(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                normalize_for_comparison(&row.name),
                Slot::InTheIndex(row.is_directory.then_some(row.id)),
            )
        })
        .collect())
}

/// Everything one cover walk hands the writer, in the order that keeps a mark
/// from overtaking the row it stamps.
///
/// Rows go out in batches as the walk finds them; the marks for every listed
/// directory go out ONCE, in [`finish`](Self::finish), after the last batch. A
/// directory's own row is written by its parent's listing, so a mark sent that
/// late can never overtake it — `mark_dirs_listed` is a PK `UPDATE` that silently
/// updates zero rows, and an overtaken mark would leave the directory at
/// `listed_epoch = 0` forever.
struct CoverWrites<'a> {
    writer: &'a IndexWriter,
    epoch: u64,
    batch: Vec<EntryRow>,
    /// The same entries in the shape a live search consumes, held only while
    /// somebody is listening.
    discovered: Vec<CoveredEntry>,
    /// When [`discovered`](Self::discovered) has waited long enough to go over
    /// part-full, so a sparse tree doesn't look like a walk that found nothing.
    emit_pacer: EmitPacer,
    emit: Option<EntrySender>,
    listed: Vec<i64>,
    /// Directories the walk deliberately won't read into. Stamped so the coverage
    /// frontier stops offering them to every later search.
    unreadable: Vec<i64>,
}

impl<'a> CoverWrites<'a> {
    fn new(writer: &'a IndexWriter, epoch: u64, emit: Option<EntrySender>) -> Self {
        Self {
            writer,
            epoch,
            batch: Vec::with_capacity(BATCH_SIZE),
            discovered: Vec::new(),
            emit_pacer: EmitPacer::new(),
            emit,
            listed: Vec::new(),
            unreadable: Vec::new(),
        }
    }

    /// Record one discovered entry: a row for the index, and the same entry for
    /// whoever is watching the walk.
    fn push(&mut self, row: EntryRow, discovered: CoveredEntry) -> Result<(), VolumeScanError> {
        self.batch.push(row);
        if self.emit.is_some() {
            self.discovered.push(discovered);
            self.emit_pacer.waiting();
        }
        if self.batch.len() >= BATCH_SIZE {
            self.flush()?;
        } else if self.emit_pacer.is_due() {
            // The writer's batch isn't full and the consumer's has waited: hand
            // over what somebody is watching, and leave the rows to fill.
            self.hand_over();
        }
        Ok(())
    }

    /// A directory whose listing succeeded, to be stamped at the finish.
    fn mark_listed(&mut self, dir_id: i64) {
        self.listed.push(dir_id);
    }

    /// A directory the walk won't read into, to be stamped at the finish. ⚠️ Its
    /// row still exists and stays navigable; what the mark says is that nothing is
    /// coming for its subtree, so the frontier must stop naming it.
    fn mark_unreadable(&mut self, dir_id: i64) {
        self.unreadable.push(dir_id);
    }

    fn flush(&mut self) -> Result<(), VolumeScanError> {
        flush_batch(&mut self.batch, self.writer)?;
        self.hand_over();
        Ok(())
    }

    /// Give the entries found so far to whoever is watching the walk.
    ///
    /// A consumer that has gone away (a closed search dialog) just stops being
    /// fed: the walk keeps running, because walking is coverage work and its rows
    /// are already in the index for the next query to find.
    fn hand_over(&mut self) {
        self.emit_pacer.sent();
        if self.discovered.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.discovered);
        if let Some(sender) = self.emit.as_ref()
            && sender.send(batch).is_err()
        {
            self.emit = None;
        }
    }

    /// Flush the last rows, stamp every directory the walk listed, and roll the
    /// subtree up. Runs on EVERY exit path, cancel included.
    fn finish(&mut self, root_id: i64) -> Result<(), VolumeScanError> {
        self.flush()?;
        // Let the consumer see the end of the walk rather than waiting on a channel
        // nothing will ever write to again.
        self.emit = None;
        for chunk in std::mem::take(&mut self.unreadable).chunks(MARK_CHUNK) {
            self.writer
                .send(WriteMessage::MarkDirsUnreadable {
                    ids: chunk.to_vec(),
                    // Always `Declined` here: the trait walk marks exactly one
                    // thing, a NAS system directory it won't descend into. A read
                    // the SHARE refuses fails the listing instead, which leaves
                    // the directory unlisted and retriable.
                    cause: UnreadableCause::Declined,
                })
                .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
        }
        if self.listed.is_empty() {
            return Ok(());
        }
        reconciler::send_marks(&self.listed, self.epoch, self.writer)
            .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
        self.listed.clear();
        // Scoped, not `ComputeAllAggregates`: this walk touched one subtree, and the
        // writer repairs the ancestor chain above it from there.
        self.writer
            .send(WriteMessage::ComputeSubtreeAggregates { root_id })
            .map_err(|e| VolumeScanError::WriterSend(e.to_string()))
    }
}
