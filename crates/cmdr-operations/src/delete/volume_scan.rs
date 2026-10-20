//! The volume delete's scan phase: the walk that lists what a delete will
//! remove, deepest first, with what each entry is (a link is a leaf, #505) and
//! what a re-run after an outage must still find (`outage.rs`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::event_sinks::OperationEventSink;
use super::super::scan_preview::entry_in_parent_listing;
use super::super::state::WriteOperationState;
use super::super::types::{WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::outage::{Listed, read_through_outages};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{EntryKind, Volume, VolumeError};

/// Entry collected during the volume scan phase. `progress_bytes` is the
/// progress contribution: equals the size for the first occurrence of an inode
/// in the scan, `0` for hardlink dupes. Mirrors `FileInfo::progress_bytes` on
/// the local-FS walker. Volume backends without inode info (MTP, SMB) populate
/// this as the size for every entry — they never produce hardlinks, so no
/// dedup happens and the count is correct. `listed` is what a delete re-run
/// after an outage must still find at `path` (`outage.rs`).
pub(super) struct VolumeDeleteEntry {
    pub(super) path: PathBuf,
    pub(super) progress_bytes: u64,
    pub(super) is_dir: bool,
    pub(super) listed: Listed,
}

/// Tracks the running tally across the whole recursive scan so the per-entry
/// `list_directory` callback (which can fire while a single dir is still
/// streaming entries from a slow MTP USB roundtrip) reads a coherent total.
pub(super) struct VolumeScanTracker {
    /// Last emit timestamp for the global throttle (shared across recursion levels).
    last_emit: Mutex<Instant>,
    progress_interval: Duration,
    /// Files committed to `entries` so far. The per-entry callback adds the
    /// in-flight `loaded_count` on top to form the displayed tally.
    pub(super) files_so_far: std::sync::atomic::AtomicUsize,
    pub(super) dirs_so_far: std::sync::atomic::AtomicUsize,
    pub(super) bytes_so_far: std::sync::atomic::AtomicU64,
}

impl VolumeScanTracker {
    pub(super) fn new(progress_interval: Duration) -> Self {
        Self {
            last_emit: Mutex::new(Instant::now()),
            progress_interval,
            files_so_far: std::sync::atomic::AtomicUsize::new(0),
            dirs_so_far: std::sync::atomic::AtomicUsize::new(0),
            bytes_so_far: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Returns true if enough time has passed since the last emit AND atomically
    /// resets the timer. Single-call gate so callers don't accidentally double-emit.
    pub(super) fn should_emit(&self) -> bool {
        // A throttle timestamp: a poisoned one is still a timestamp.
        let mut last = self.last_emit.lock_ignore_poison();
        if last.elapsed() < self.progress_interval {
            return false;
        }
        *last = Instant::now();
        true
    }
}

/// Recursively enumerates a directory tree via `volume.list_directory()`, collecting
/// files and directories. Directories are appended after their children, so the
/// resulting list is already in deepest-first order for safe deletion.
///
/// **A symlink is a leaf: the link goes, ❌ never its target's contents.** Only
/// an [`EntryKind::Directory`] is listed and descended. A listing sets
/// `is_directory` on a link to a folder (panes want it to navigate like one), and
/// some backends' `is_directory` follows the link, so neither may decide a
/// descent: `hint` carries an [`EntryKind`], read off an entry with
/// [`EntryKind::of`] or asked of [`Volume::entry_kind`], plus the size a
/// listing already told (the outage re-check reads it, so nothing stats again).
///
/// Emits scan progress two ways:
/// 1. **Per-entry** via `list_directory`'s `on_progress` callback, so the FE sees the tally climb
///    mid-listing — important for MTP, where one `list_directory` call on `/DCIM/Camera` with 1k+
///    photos can take ~17 s of USB roundtrips.
/// 2. **After each subtree** (this function's tail), as a final snapshot for that branch. Throttled
///    by the shared `VolumeScanTracker` so it doesn't race with the per-entry callback.
///
/// **Cancel contract**: this function does NOT emit `write-cancelled` before returning
/// `Err(Cancelled)`. The cancel check fires at every recursion level, so emitting here would
/// multi-fire. Top-level callers must call `emit_cancelled_if_aborted` (or otherwise emit
/// `write-cancelled`) on the returned result before propagating it, so the FE sees the proper
/// terminal event for the cancel flow.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(super) async fn scan_volume_recursive(
    volume: &Arc<dyn Volume>,
    volume_id: &str,
    path: &Path,
    hint: Option<Listed>,
    entries: &mut Vec<VolumeDeleteEntry>,
    total_bytes: &mut u64,
    seen_inodes: &mut std::collections::HashSet<u64>,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &Arc<VolumeScanTracker>,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    if state.intent().is_cancelled() {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".into(),
        });
    }

    // Resolve what `path` is. Prefer the caller's hint (a pane's listing of the
    // parent at the top level, the parent's `FileEntry` in recursion). Without
    // one, probe (`top_level_entry`, a read, so safe to re-run after an
    // outage), which on MTP lists the parent dir, so we avoid it whenever the
    // oracle can answer.
    let probed = match hint {
        Some(listed) => listed,
        None => read_through_outages(volume, state, || top_level_entry(volume.as_ref(), path))
            .await
            .map_err(|stop| stop.into_error(path))?,
    };
    let kind = probed.kind;

    if kind == EntryKind::Directory {
        super::super::mount_boundary::refuse_a_volume_mount_point(&**volume, path, state).await?;

        // Snapshot the cumulative tallies BEFORE the (potentially slow) listing
        // call. The per-entry callback shows `files_before + loaded_count` so the
        // FE sees a climbing number even when one `list_directory` takes seconds.
        // We don't know mid-stream whether each entry is a file or dir, so
        // optimistically attribute them to `files`; the post-listing loop below
        // corrects the split via the `dirs_so_far` counter.
        let files_before = tracker.files_so_far.load(Ordering::Relaxed);
        let dirs_before = tracker.dirs_so_far.load(Ordering::Relaxed);
        let bytes_before = tracker.bytes_so_far.load(Ordering::Relaxed);
        let current_dir_str = path.display().to_string();
        let op_id_for_cb = operation_id.to_string();
        let tracker_for_cb = Arc::clone(tracker);
        let state_for_cb = Arc::clone(state);

        let on_progress = move |p: cmdr_fs::volume::ListingProgress| {
            if !tracker_for_cb.should_emit() {
                return;
            }
            // The delete scan tally tracks files + bytes from already-walked
            // subdirs; the volume's per-listing callback adds the local
            // (files, bytes) it's enumerating right now.
            let files_now = files_before + p.files;
            let bytes_now = bytes_before + p.bytes;
            state_for_cb.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    op_id_for_cb.clone(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Scanning,
                    None,
                    files_now,
                    0,
                    bytes_now,
                    0,
                )
                .with_scan_meta(Some(current_dir_str.clone()), dirs_before + p.dirs, None),
            );
            state.scope().status().update(
                &op_id_for_cb,
                WriteOperationPhase::Scanning,
                None,
                files_now,
                0,
                bytes_now,
                0,
            );
        };

        // Oracle-first: if this directory is watcher-backed in the listing cache,
        // reuse the cached entries and skip the volume round-trip entirely.
        // Falls through to `volume.list_directory` on miss, preserving the
        // per-entry progress callback for slow MTP listings.
        let children = match state.scope().host().listings().authoritative_listing(volume_id, path) {
            Some(cached) => {
                // The cached listing is already complete, so synthesize a
                // single end-of-listing progress tick (with the tally we'd
                // have built incrementally) to keep the FE counter climbing
                // during the cache-fed pass too.
                let mut tally = cmdr_fs::volume::ListingProgress::default();
                for e in &cached {
                    if e.is_directory {
                        tally.dirs += 1;
                    } else {
                        tally.files += 1;
                        tally.bytes += e.size.unwrap_or(0);
                    }
                }
                on_progress(tally);
                cached
            }
            None => read_through_outages(volume, state, || {
                volume.list_directory_with_cancel(path, Some(&on_progress), Some(&state.backend_cancel))
            })
            .await
            .map_err(|stop| stop.into_error(path))?,
        };

        // Recurse into children first. list_directory returns FileEntry with size,
        // so we use child.size directly instead of calling get_metadata (which
        // returns NotSupported on MTP). Pass the child's own kind as the hint so
        // the recursive call doesn't re-probe (avoids another parent listing on
        // MTP). `EntryKind::of`, ❌ never `child.is_directory`: that's true for a
        // link to a folder, and descending it deletes the folder it points at.
        for child in &children {
            let child_path = PathBuf::from(&child.path);
            let child_kind = EntryKind::of(child);
            if child_kind == EntryKind::Directory {
                Box::pin(scan_volume_recursive(
                    volume,
                    volume_id,
                    &child_path,
                    Some(Listed {
                        kind: EntryKind::Directory,
                        size: None,
                    }),
                    entries,
                    total_bytes,
                    seen_inodes,
                    state,
                    events,
                    operation_id,
                    tracker,
                ))
                .await?;
            } else {
                // A link frees only itself, whatever size the listing reports
                // for what it points at.
                let size = if child_kind == EntryKind::Symlink {
                    0
                } else {
                    child.size.unwrap_or(0)
                };
                // Hardlink dedup: when the backend reports an inode, only
                // count the first occurrence toward `total_bytes` and the
                // tracker's running byte total. Subsequent hardlinks
                // contribute `progress_bytes = 0` so the delete loop's
                // numerator stays aligned with the denominator. Backends
                // without inode info (MTP, SMB) leave `inode = None` and
                // every entry is treated as unique — they never produce
                // hardlinks anyway. Mirrors `FileInfo::progress_bytes` on
                // the local-FS walker.
                let counts = match child.inode {
                    Some(ino) => seen_inodes.insert(ino),
                    None => true,
                };
                let progress_bytes = if counts { size } else { 0 };
                if counts {
                    *total_bytes += size;
                    tracker.bytes_so_far.fetch_add(size, Ordering::Relaxed);
                }
                tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                entries.push(VolumeDeleteEntry {
                    path: child_path,
                    progress_bytes,
                    is_dir: false,
                    listed: Listed {
                        kind: EntryKind::of(child),
                        size: child.size,
                    },
                });
            }
        }

        // Add directory after its children (already deepest-first order)
        tracker.dirs_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: true,
            listed: Listed {
                kind: EntryKind::Directory,
                size: None,
            },
        });
    } else {
        // Top-level file or link without listing context: size unknown, use 0.
        // Progress still tracks file count accurately.
        tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: false,
            listed: listed_top_level(volume.as_ref(), path, probed).await,
        });
    }

    // Final snapshot for this subtree (throttled so it doesn't race the per-entry callback).
    if tracker.should_emit() {
        let files_done = tracker.files_so_far.load(Ordering::Relaxed);
        let dirs_done = tracker.dirs_so_far.load(Ordering::Relaxed);
        let bytes_done = tracker.bytes_so_far.load(Ordering::Relaxed);
        let current_dir_str = path.display().to_string();
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Delete,
                WriteOperationPhase::Scanning,
                None,
                files_done,
                0,
                bytes_done,
                0,
            )
            .with_scan_meta(Some(current_dir_str), dirs_done, None),
        );
        state.scope().status().update(
            operation_id,
            WriteOperationPhase::Scanning,
            None,
            files_done,
            0,
            bytes_done,
            0,
        );
    }

    Ok(())
}

/// What `source` is per a pane's listing of its parent (`entry_in_parent_listing`):
/// its kind, link-aware, and a file's size, with no I/O.
pub(super) fn listed_in_parent_listing(state: &WriteOperationState, volume_id: &str, source: &Path) -> Option<Listed> {
    entry_in_parent_listing(state.scope().host().listings(), volume_id, source).map(|entry| {
        let kind = EntryKind::of(&entry);
        Listed {
            kind,
            size: if kind == EntryKind::File { entry.size } else { None },
        }
    })
}

/// What a top-level entry is, with a link reported AS a link, in ONE request:
/// `entry_kind` on a backend that can hold links ([`BackendKind::has_symlinks`]),
/// where a following stat would call a link to a folder a folder (#505).
/// Elsewhere a plain `get_metadata` (what `entry_kind` asks there anyway), whose
/// size also serves a re-run's re-check (`outage.rs`), so an object store spends
/// one HEAD per selected file, the one its cost estimate counts.
///
/// [`BackendKind::has_symlinks`]: cmdr_fs::volume::BackendKind::has_symlinks
async fn top_level_entry(volume: &dyn Volume, path: &Path) -> Result<Listed, VolumeError> {
    if volume.backend_kind().has_symlinks() {
        return volume.entry_kind(path).await.map(|kind| Listed { kind, size: None });
    }
    volume.get_metadata(path).await.map(|entry| Listed {
        kind: EntryKind::of(&entry),
        size: if entry.is_directory { None } else { entry.size },
    })
}

/// What a top-level file or link is, for a delete re-run after an outage to
/// check against: what the walk learned of it (`probed`), plus a file's size
/// when the walk didn't learn it, by one `get_metadata`, and only on a volume
/// with a session: no other volume is ever waited for, so nothing would read
/// it. Best effort: an unanswered stat leaves the size unknown, and a re-run
/// then leaves the file rather than delete it unproven.
async fn listed_top_level(volume: &dyn Volume, path: &Path, probed: Listed) -> Listed {
    if probed.kind != EntryKind::File || probed.size.is_some() || volume.connection_state().is_none() {
        return probed;
    }
    match volume.get_metadata(path).await {
        Ok(entry) => Listed {
            kind: EntryKind::of(&entry),
            size: entry.size,
        },
        Err(_) => probed,
    }
}

#[cfg(test)]
#[path = "volume_scan_tests.rs"]
mod tests;
