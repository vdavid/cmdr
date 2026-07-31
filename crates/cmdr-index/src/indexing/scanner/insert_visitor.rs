//! The fresh scan's `DirVisitor`: turn each directory read into index rows.
//!
//! Split out of `mod.rs`, which keeps the scan's types, its public entry points,
//! and `run_scan`. This is the per-directory half: what a walker worker does with
//! the children it just read.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::exclusions::{ExclusionScope, is_canonicalization_alias, should_exclude};
use super::walker::{DirTask, DirVisitor, RawDirEntry, RawFileType, WalkReadError};
use super::{ScanError, ScanProgress};
use crate::indexing::events::{Diagnostic, IndexErrorReport, IndexEvent};
use crate::indexing::metadata;
use crate::indexing::store::EntryRow;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::firmlinks;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Fresh-scan [`DirVisitor`]: inserts every discovered entry as a new row,
/// attributing children to the directory being read via the carried `parent_id`.
///
/// A directory whose read SUCCEEDS is recorded in `listed_ids` (marked listed at
/// the current epoch after the walk); a timed-out or errored dir is never
/// recorded, so it stays `listed_epoch = 0` (honest "unknown"). Runs concurrently
/// on the walker's worker threads, so shared state is behind mutexes / atomics.
pub(super) struct InsertVisitor {
    writer: IndexWriter,
    /// Shared id counter from `IndexWriter` (the single allocation source).
    next_id: Arc<AtomicI64>,
    is_volume_root: bool,
    /// Exclusion scope for the per-child gate (see `ScanConfig::scope`).
    scope: ExclusionScope,
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
    /// Accumulating insert batch, flushed at `batch_size`.
    batch: Mutex<Vec<EntryRow>>,
    /// Inodes seen with nlink > 1, so each hardlink's size counts once.
    seen_inodes: Mutex<HashSet<u64>>,
    /// Ids of directories whose read succeeded (marked listed after the walk).
    listed_ids: Mutex<Vec<i64>>,
    /// First writer-send error, surfaced as the scan result.
    send_error: Mutex<Option<String>>,
}

impl InsertVisitor {
    pub(super) fn new(
        writer: IndexWriter,
        is_volume_root: bool,
        scope: ExclusionScope,
        inodes_trustworthy: bool,
        batch_size: usize,
        progress: &ScanProgress,
        walk_cancel: CancellationToken,
    ) -> Self {
        let next_id = Arc::clone(writer.next_id());
        Self {
            writer,
            next_id,
            is_volume_root,
            scope,
            inodes_trustworthy,
            batch_size,
            entries_scanned: Arc::clone(&progress.entries_scanned),
            dirs_found: Arc::clone(&progress.dirs_found),
            bytes_scanned: Arc::clone(&progress.bytes_scanned),
            walk_cancel,
            batch: Mutex::new(Vec::with_capacity(batch_size)),
            seen_inodes: Mutex::new(HashSet::new()),
            listed_ids: Mutex::new(Vec::new()),
            send_error: Mutex::new(None),
        }
    }

    fn send_entries(&self, entries: Vec<EntryRow>) {
        if entries.is_empty() {
            return;
        }
        if let Err(e) = self.writer.send(WriteMessage::InsertEntriesV2(entries)) {
            // A send failure means the writer is gone — abort the walk and keep the
            // first error to return from the scan.
            self.walk_cancel.cancel();
            let mut slot = self.send_error.lock_ignore_poison();
            if slot.is_none() {
                *slot = Some(e.to_string());
            }
        }
    }

    fn push_row(&self, row: EntryRow) {
        let full = {
            let mut batch = self.batch.lock_ignore_poison();
            batch.push(row);
            if batch.len() >= self.batch_size {
                std::mem::take(&mut *batch)
            } else {
                Vec::new()
            }
        };
        self.send_entries(full);
    }

    /// Flush the final partial batch and surface any captured send error.
    pub(super) fn finish(&self) -> Result<(), ScanError> {
        let remaining = std::mem::take(&mut *self.batch.lock_ignore_poison());
        self.send_entries(remaining);
        match self.send_error.lock_ignore_poison().take() {
            Some(msg) => Err(ScanError::WriterSend(msg)),
            None => Ok(()),
        }
    }

    pub(super) fn take_listed_ids(&self) -> Vec<i64> {
        std::mem::take(&mut *self.listed_ids.lock_ignore_poison())
    }
}

impl DirVisitor for InsertVisitor {
    fn visit_dir(&self, dir: &DirTask, children: Vec<RawDirEntry>) -> Vec<DirTask> {
        // This directory's read succeeded → mark it listed at scan end.
        self.listed_ids.lock_ignore_poison().push(dir.id);

        let mut subdirs = Vec::new();
        for child in children {
            let path_str = child.path.to_string_lossy();

            // Volume-root scans apply the exclusion policy; subtree scans were
            // explicitly chosen, so global exclusions don't apply. The scope comes
            // from the volume kind: `BootDisk` for the `/`-rooted boot scan,
            // `MountRooted` for an external drive rooted at `/Volumes/X` (which must
            // index its own subtree, skipping only junk basenames).
            if self.is_volume_root && should_exclude(&path_str, &self.scope) {
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

            self.push_row(EntryRow {
                id,
                parent_id: dir.id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
            });
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

    fn visit_read_error(&self, dir: &DirTask, err: &WalkReadError) {
        match err {
            WalkReadError::Io(e) => {
                // Surface TCC-restricted paths so the sidebar can show the "limited
                // by macOS" styling. The host filters to known TCC prefixes.
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    self.writer
                        .events()
                        .emit(IndexEvent::PathAccessDenied { path: dir.path.clone() });
                }
                log::debug!("Scanner: skipping errored dir {}: {e}", dir.path.display());
            }
            // Timeouts are already logged by the walker watchdog; left unmarked.
            WalkReadError::TimedOut => {}
        }
    }
}
