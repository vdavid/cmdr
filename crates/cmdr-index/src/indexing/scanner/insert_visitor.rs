//! The fresh scan's `DirVisitor`: turn each directory read into index rows.
//!
//! Split out of `mod.rs`, which keeps the scan's types, its public entry points,
//! and `run_scan`. This is the per-directory half: what a walker worker does with
//! the children it just read.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::exclusions::is_canonicalization_alias;
use super::walker::{DirTask, DirVisitor, RawDirEntry, RawFileType, WalkReadError};
use super::{CoveredEntry, EmitPacer, EntrySender, MARK_CHUNK, ScanError, ScanProgress, WalkPolicy};
use crate::indexing::events::{Diagnostic, IndexErrorReport, IndexEvent};
use crate::indexing::metadata;
use crate::indexing::store::EntryRow;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::firmlinks;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Everything one flush hands to the writer, accumulated under ONE lock.
///
/// The three live together because their ORDER is the whole point. A directory's
/// own row is written by its PARENT's `visit_dir`, so it can still be sitting in
/// `rows` when the directory's own read succeeds; `mark_dirs_listed` is a
/// PK `UPDATE` that silently updates zero rows, so a mark that overtakes its row
/// leaves that directory at `listed_epoch = 0` forever. Accumulating both under
/// the same lock and sending rows-then-marks inside that critical section makes
/// the overtake unrepresentable: every id in `listed` was appended AFTER its own
/// row went into `rows`, so it rides the same flush or a later one.
///
/// ❌ Don't split these into separate mutexes, and ❌ don't send outside the
/// lock: two workers flushing concurrently could then hand the writer batch 2
/// before batch 1, and batch 2's marks would land ahead of batch 1's rows.
#[derive(Default)]
struct Pending {
    /// Rows waiting to be inserted.
    rows: Vec<EntryRow>,
    /// Directories whose read succeeded since the last flush.
    listed: Vec<i64>,
    /// The same entries in the shape a live search consumes. Populated only when
    /// somebody is listening.
    discovered: Vec<CoveredEntry>,
    /// When [`discovered`](Self::discovered) has waited long enough to be handed
    /// over part-full. It lives in here so asking costs no second lock; the ROWS
    /// keep their own schedule, since a batch of rows nobody is waiting on is
    /// throughput and a batch of entries somebody is watching is latency.
    emit_pacer: EmitPacer,
}

impl Pending {
    /// Take the entries waiting for a live consumer, stopping their clock.
    fn take_discovered(&mut self) -> Vec<CoveredEntry> {
        self.emit_pacer.sent();
        std::mem::take(&mut self.discovered)
    }
}

/// Fresh-scan [`DirVisitor`]: inserts every discovered entry as a new row,
/// attributing children to the directory being read via the carried `parent_id`.
///
/// A directory whose read SUCCEEDS is marked listed at the current epoch, right
/// after the flush that carries its own row; a timed-out or errored dir is never
/// marked, so it stays `listed_epoch = 0` (honest "unknown"). Runs concurrently
/// on the walker's worker threads, so shared state is behind mutexes / atomics.
pub(super) struct InsertVisitor {
    writer: IndexWriter,
    /// Shared id counter from `IndexWriter` (the single allocation source).
    next_id: Arc<AtomicI64>,
    /// What this walk refuses to descend into: the structural exclusion policy
    /// (with its on/off), and the device it must not leave.
    policy: WalkPolicy,
    /// Whether the scanned volume's inode is a trustworthy identity (see
    /// `ScanConfig::inodes_trustworthy`). `false` on FAT/exFAT ⇒ every stored
    /// `inode` is nulled and hardlink dedup is skipped.
    inodes_trustworthy: bool,
    batch_size: usize,
    /// Live progress counters (shared with the manager-facing `ScanHandle`); the
    /// scan summary reads their final values.
    entries_scanned: Arc<AtomicU64>,
    dirs_found: Arc<AtomicU64>,
    bytes_scanned: Arc<AtomicU64>,
    /// A child of the scan's token, cancelled when a writer send fails so the
    /// walk stops promptly. Cancelling it does NOT mark the scan user-cancelled.
    walk_cancel: CancellationToken,
    /// The epoch every successfully-listed directory is stamped with.
    epoch: u64,
    /// Where a live consumer (a search walking its frontier) receives the
    /// entries as they're discovered, or `None` for a plain indexing scan.
    /// Dropped by the visitor once the consumer goes away.
    emit: Mutex<Option<EntrySender>>,
    /// Everything the next flush will hand the writer, in order.
    pending: Mutex<Pending>,
    /// Inodes seen with nlink > 1, so each hardlink's size counts once.
    seen_inodes: Mutex<HashSet<u64>>,
    /// First writer-send error, surfaced as the scan result.
    send_error: Mutex<Option<String>>,
    /// Directories this walk couldn't read, by cause, so the frontier stops
    /// offering them on every later search.
    unreadable_ids: Mutex<UnreadableIds>,
}

/// The ids one walk condemned, split by the sentence they turn into.
///
/// `pub(super)` so the scan driver can send one mark message per cause.
///
/// Two lists rather than a `Vec<(i64, UnreadableCause)>` because the marks go out
/// as one message per cause over a batch of ids, and because the split is the
/// decision worth being able to read at a glance: a refusal is the user's to fix
/// and permanent until they do, while abandoned ground is Cmdr's to retry.
#[derive(Default)]
pub(super) struct UnreadableIds {
    /// Reads the OS refused (permission denied).
    pub(super) denied: Vec<i64>,
    /// Reads that timed out, failed with any other errno, or were pruned unread by
    /// the walker's consecutive-failure budget.
    pub(super) abandoned: Vec<i64>,
}

impl InsertVisitor {
    #[allow(
        clippy::too_many_arguments,
        reason = "one visitor per scan, built in one place; a config struct would only move the list"
    )]
    pub(super) fn new(
        writer: IndexWriter,
        policy: WalkPolicy,
        inodes_trustworthy: bool,
        batch_size: usize,
        progress: &ScanProgress,
        walk_cancel: CancellationToken,
        epoch: u64,
        emit: Option<EntrySender>,
    ) -> Self {
        let next_id = Arc::clone(writer.next_id());
        Self {
            writer,
            next_id,
            policy,
            inodes_trustworthy,
            batch_size,
            entries_scanned: Arc::clone(&progress.entries_scanned),
            dirs_found: Arc::clone(&progress.dirs_found),
            bytes_scanned: Arc::clone(&progress.bytes_scanned),
            walk_cancel,
            epoch,
            emit: Mutex::new(emit),
            pending: Mutex::new(Pending {
                rows: Vec::with_capacity(batch_size),
                ..Pending::default()
            }),
            seen_inodes: Mutex::new(HashSet::new()),
            send_error: Mutex::new(None),
            unreadable_ids: Mutex::new(UnreadableIds::default()),
        }
    }

    /// Hand one flush's worth of work to the writer, ROWS FIRST and marks second,
    /// with the caller still holding `pending`'s lock. See [`Pending`] for why the
    /// order and the lock are both load-bearing.
    fn flush(&self, pending: &mut Pending) {
        let rows = std::mem::take(&mut pending.rows);
        let listed = std::mem::take(&mut pending.listed);
        let discovered = pending.take_discovered();

        if !rows.is_empty()
            && let Err(e) = self.writer.send(WriteMessage::InsertEntriesV2(rows))
        {
            // A send failure means the writer is gone — abort the walk and keep the
            // first error to return from the scan.
            self.walk_cancel.cancel();
            let mut slot = self.send_error.lock_ignore_poison();
            if slot.is_none() {
                *slot = Some(e.to_string());
            }
            return;
        }
        for chunk in listed.chunks(MARK_CHUNK) {
            if let Err(e) = self.writer.send(WriteMessage::MarkDirsListed {
                ids: chunk.to_vec(),
                epoch: self.epoch,
            }) {
                log::warn!("Scanner: failed to send MarkDirsListed: {e}");
            }
        }
        self.emit(discovered);
    }

    /// Hand one batch to whoever is consuming the walk live. A consumer that has
    /// gone away (a closed search dialog) just stops being fed: the walk keeps
    /// running, because walking is coverage work and its rows are already in the
    /// index for the next query to find.
    fn emit(&self, discovered: Vec<CoveredEntry>) {
        if discovered.is_empty() {
            return;
        }
        let mut slot = self.emit.lock_ignore_poison();
        if let Some(sender) = slot.as_ref()
            && sender.send(discovered).is_err()
        {
            *slot = None;
        }
    }

    fn push_row(&self, row: EntryRow, discovered: Option<CoveredEntry>) {
        let mut pending = self.pending.lock_ignore_poison();
        pending.rows.push(row);
        if let Some(entry) = discovered {
            pending.discovered.push(entry);
            pending.emit_pacer.waiting();
        }
        if pending.rows.len() >= self.batch_size {
            self.flush(&mut pending);
        } else if pending.emit_pacer.is_due() {
            // The rows aren't ready to go and the entries have waited: hand over
            // what a live consumer is watching without waiting for the writer's
            // batch to fill. Rows and marks keep their pairing untouched — this
            // sends neither.
            let discovered = pending.take_discovered();
            self.emit(discovered);
        }
    }

    /// Flush the final partial batch — rows, marks, and entries alike — and
    /// surface any captured send error.
    ///
    /// Runs on the CANCEL path too: dropping the queued marks would throw away
    /// coverage the walk genuinely earned, and then no amount of searching would
    /// ever shrink the frontier.
    pub(super) fn finish(&self) -> Result<(), ScanError> {
        {
            let mut pending = self.pending.lock_ignore_poison();
            self.flush(&mut pending);
        }
        // Let the consumer see the end of the walk rather than waiting on a
        // channel nothing will ever write to again.
        *self.emit.lock_ignore_poison() = None;
        match self.send_error.lock_ignore_poison().take() {
            Some(msg) => Err(ScanError::WriterSend(msg)),
            None => Ok(()),
        }
    }

    /// The directories this walk found it can't read, by cause, for the caller to
    /// stamp once the walk is over.
    ///
    /// ⚠️ The caller stamps them AFTER [`finish`](Self::finish), so every
    /// `MarkDirsListed` this walk earned is already ahead of them on the writer
    /// channel. ❌ Never derive the list from "what is still unlisted" instead:
    /// that condemns everything the walk read but hasn't stamped yet, which reads
    /// as a speed-up and shows up only as a quietly short entry count
    /// (`marking_abandoned_ground_costs_no_coverage`).
    pub(super) fn take_unreadable_ids(&self) -> UnreadableIds {
        std::mem::take(&mut self.unreadable_ids.lock_ignore_poison())
    }
}

impl DirVisitor for InsertVisitor {
    fn visit_dir(&self, dir: &DirTask, children: Vec<RawDirEntry>) -> Vec<DirTask> {
        // This directory's read succeeded → mark it listed, with the next flush.
        // Its own row was written by its PARENT's `visit_dir`, so it is already in
        // `rows` or in a batch the writer has: appending here can never overtake
        // it. See `Pending`.
        self.pending.lock_ignore_poison().listed.push(dir.id);
        let emitting = self.emit.lock_ignore_poison().is_some();

        let mut subdirs = Vec::new();
        for child in children {
            let path_str = child.path.to_string_lossy();

            // The structural exclusion policy, when this walk runs it: which rules
            // apply comes from the volume KIND (`BootDisk` for the `/`-rooted boot
            // scan, `MountRooted` for a drive at `/Volumes/X`, which must index its
            // own subtree), whether they run at all comes from what the walk is.
            if self.policy.excludes(&path_str) {
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

            // Something is mounted here, so this directory is another volume's
            // ground. Cut exactly as an exclusion does — no row, not just no
            // descent: a row nothing ever lists would sit in the frontier forever
            // and hand itself to every later search, and the bytes under it belong
            // in the other volume's `dir_stats`, not this one's. Only for a walk
            // that pins a device, and only for directories, so it costs one `lstat`
            // per discovered directory on the search walk and nothing at all on a
            // full scan.
            if is_dir && self.policy.leaves_the_volume(&child.path) {
                log::debug!(
                    "Scanner: not descending into {}, it's on another volume",
                    child.path.display()
                );
                continue;
            }

            // Prefer the reader's inline stat (macOS `getattrlistbulk` supplies it,
            // avoiding a per-entry `lstat` — the dominant local-walk cost). When the
            // reader didn't provide it (`std_read_dir`, or a bulk-read fallback),
            // stat the entry here. Both funnel through `metadata`'s rules.
            let snap = match child.stat {
                Some(s) => metadata::metadata_from_raw(
                    s.logical_size,
                    s.physical_size,
                    s.modified_at,
                    s.inode,
                    s.nlink,
                    is_dir,
                    is_symlink,
                ),
                None => match std::fs::symlink_metadata(&child.path) {
                    Ok(meta) => metadata::extract_metadata(&meta, is_dir, is_symlink),
                    Err(_) => metadata::MetadataSnapshot {
                        logical_size: None,
                        physical_size: None,
                        modified_at: None,
                        inode: None,
                        nlink: None,
                    },
                },
            };

            // Deduplicate hardlinks: if nlink > 1, count each inode's size once.
            //
            // On a volume without stable inodes (FAT/exFAT), never STORE the
            // derived inode: it's an unstable identity that would let the live
            // rename pre-pass false-match a reused inode. Hardlink dedup is skipped
            // there for the same reason (it keys off the inode) — and `nlink` is
            // always 1 on those formats anyway, so the branch would never fire.
            let (logical_size, physical_size, modified_at, inode) = if !self.inodes_trustworthy {
                (snap.logical_size, snap.physical_size, snap.modified_at, None)
            } else if !is_dir && !is_symlink && matches!(snap.nlink, Some(n) if n > 1) {
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

            // What a live consumer gets is the entry as a LISTING shows it, so
            // the sizes are the pre-dedup ones: hardlink dedup exists to keep
            // the stored recursive sums honest, and a search result showing a
            // hardlinked file as 0 bytes would just be wrong.
            let discovered = emitting.then(|| CoveredEntry {
                path: child.path.clone(),
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
            });

            self.push_row(
                EntryRow {
                    id,
                    parent_id: dir.id,
                    name,
                    is_directory: is_dir,
                    is_symlink,
                    logical_size,
                    physical_size,
                    modified_at,
                    inode,
                },
                discovered,
            );
        }
        subdirs
    }

    fn note_worker_spawn_failure(&self, error: &std::io::Error) {
        // Worth a report: the walk still finishes, but at reduced parallelism, and
        // a machine that can't spawn threads has a bigger problem than this scan.
        self.writer.events().emit(IndexEvent::Error {
            report: IndexErrorReport::WalkWorkerSpawnFailed {
                detail: Diagnostic(error.to_string()),
            },
        });
    }

    /// Hand over a batch that has been waiting, even though nothing new was
    /// found. Every other hook here fires on discovery, so a walk parked on one
    /// slow directory would otherwise hold everything it found before it parked
    /// until the walk ended.
    fn on_watchdog_tick(&self) {
        let discovered = {
            let mut pending = self.pending.lock_ignore_poison();
            if !pending.emit_pacer.is_due() {
                return;
            }
            pending.take_discovered()
        };
        self.emit(discovered);
    }

    /// Record every directory whose contents this walk couldn't get, with the cause
    /// that says whose problem it is.
    ///
    /// ⚠️ Every case gets a cause, and that is the point. Leaving the
    /// non-permission errno uncaused (as "it might be transient, so let it be
    /// retried") left the directory indistinguishable from ground nothing had
    /// reached yet, so the coverage frontier handed it to EVERY later search and
    /// each one re-paid the same failing read. On a machine with a disconnected
    /// mount that is a full stall timeout per directory per search, forever, with
    /// nothing ever converging. The retry lives in `writer/abandoned_retry.rs`
    /// instead, where it costs one attempt per backoff window rather than one per
    /// search.
    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError) {
        match err {
            WalkReadError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // Surface TCC-restricted paths so the sidebar can show the "limited
                // by macOS" styling. The host filters to known TCC prefixes.
                self.writer
                    .events()
                    .emit(IndexEvent::PathAccessDenied { path: dir.path.clone() });
                self.unreadable_ids.lock_ignore_poison().denied.push(dir.id);
                log::debug!("Scanner: skipping errored dir {}: {e}", dir.path.display());
            }
            WalkReadError::Io(e) => {
                self.unreadable_ids.lock_ignore_poison().abandoned.push(dir.id);
                log::debug!("Scanner: skipping errored dir {}: {e}", dir.path.display());
            }
            // Already logged by the walker watchdog, so no line here.
            WalkReadError::TimedOut => self.unreadable_ids.lock_ignore_poison().abandoned.push(dir.id),
        }
    }

    /// A task the give-up budget dropped unread. No log line (one give-up line
    /// covers the whole subtree), and the same cause as a read that failed: from a
    /// search's side these are identical, ground this walk didn't get.
    fn visit_pruned(&self, dir: &DirTask) {
        self.unreadable_ids.lock_ignore_poison().abandoned.push(dir.id);
    }
}
