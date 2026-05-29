//! Delete implementation for write operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::helpers::spawn_async_sync;
use super::super::scan::{SourceItemTracker, scan_sources, take_cached_scan_result};
use super::super::state::{WriteOperationState, update_operation_status};
use super::super::transfer::volume_copy::map_volume_error;
use super::super::types::{
    DryRunResult, IoResultExt, OperationEventSink, TauriEventSink, WriteCancelledEvent, WriteCompleteEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::file_system::volume::{Volume, VolumeError};

// ============================================================================
// Delete implementation
// ============================================================================

pub(in crate::file_system::write_operations) fn delete_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    let events = TauriEventSink::new(app.clone());
    delete_files_with_progress_inner(&events, operation_id, state, sources, config)
}

pub(super) fn delete_files_with_progress_inner(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file delete loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        if let Some(cached) = take_cached_scan_result(preview_id).filter(|c| !c.files.is_empty()) {
            log::debug!(
                "delete_files_with_progress: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
            log::warn!(
                "preview_id={} cache miss despite frontend coordination, starting fresh scan for operation_id={}",
                preview_id,
                operation_id
            );
            scan_sources(
                sources,
                state,
                events,
                operation_id,
                WriteOperationType::Delete,
                config.sort_column,
                config.sort_order,
            )?
        }
    } else {
        scan_sources(
            sources,
            state,
            events,
            operation_id,
            WriteOperationType::Delete,
            config.sort_column,
            config.sort_order,
        )?
    };

    // Handle dry-run mode (delete has no conflicts)
    if config.dry_run {
        events.emit_dry_run_complete(DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: scan_result.file_count,
            // Delete frees the `du`-equivalent source footprint (a hardlinked
            // inode survives until its last link is removed), so report
            // `dedup_bytes`, not the write footprint.
            bytes_total: scan_result.dedup_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        });
        return Ok(());
    }

    // Phase 2: Delete files first (deepest first)
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Emit initial Deleting-phase event with totals. Important when reusing a
    // cached preview: no scanning events were emitted by the BE, so the FE
    // still has scan-phase tallies on screen. This event flips the FE to the
    // active-phase UI with the correct denominator on file/byte progress.
    // The byte denominator is `dedup_bytes`: delete frees each inode once, and
    // the per-file numerator below sums `progress_bytes` (also dedup'd).
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Deleting,
            None,
            0,
            scan_result.file_count,
            0,
            scan_result.dedup_bytes,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Deleting,
        None,
        0,
        scan_result.file_count,
        0,
        scan_result.dedup_bytes,
    );

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    // Delete files
    for file_info in &scan_result.files {
        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rolled_back: false, // Delete operations can't be rolled back
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use `progress_bytes` (dedup'd per file) so the numerator stays in
        // lockstep with the `dedup_bytes` denominator: a hardlinked inode is
        // freed once, on its last unlink. See `state.rs::FileInfo` and
        // `ScanResult::dedup_bytes`.
        let progress_bytes = file_info.progress_bytes;

        fs::remove_file(&file_info.path).with_path(&file_info.path)?;

        files_done += 1;
        bytes_done += progress_bytes;

        if let Some(source_path) = tracker.record(file_info) {
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source_path.display().to_string(),
            });
        }

        // Emit progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = file_info.path.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Deleting,
                    current_file.clone(),
                    files_done,
                    scan_result.file_count,
                    bytes_done,
                    scan_result.dedup_bytes,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                scan_result.file_count,
                bytes_done,
                scan_result.dedup_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (in reverse order - deepest first)
    for dir in scan_result.dirs.iter().rev() {
        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rolled_back: false, // Delete operations can't be rolled back
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Only remove if empty (files should already be deleted)
        let _ = fs::remove_dir(dir);
    }

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: files_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
    });

    Ok(())
}

// ============================================================================
// Volume-aware delete implementation (for MTP and other non-local volumes)
// ============================================================================

/// Entry collected during the volume scan phase. We don't carry the raw
/// `size` because the delete pipeline only ever needs the progress
/// contribution: equals `size` for the first occurrence of an inode in the
/// scan, `0` for hardlink dupes. Mirrors `FileInfo::progress_bytes` on the
/// local-FS walker. Volume backends without inode info (MTP, SMB) populate
/// this as `size` for every entry — they never produce hardlinks, so no
/// dedup happens and the count is correct.
struct VolumeDeleteEntry {
    path: PathBuf,
    progress_bytes: u64,
    is_dir: bool,
}

/// Tracks the running tally across the whole recursive scan so the per-entry
/// `list_directory` callback (which can fire while a single dir is still
/// streaming entries from a slow MTP USB roundtrip) reads a coherent total.
struct VolumeScanTracker {
    /// Last emit timestamp for the global throttle (shared across recursion levels).
    last_emit: Mutex<Instant>,
    progress_interval: Duration,
    /// Files committed to `entries` so far. The per-entry callback adds the
    /// in-flight `loaded_count` on top to form the displayed tally.
    files_so_far: std::sync::atomic::AtomicUsize,
    dirs_so_far: std::sync::atomic::AtomicUsize,
    bytes_so_far: std::sync::atomic::AtomicU64,
}

impl VolumeScanTracker {
    fn new(progress_interval: Duration) -> Self {
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
    fn should_emit(&self) -> bool {
        let Ok(mut last) = self.last_emit.lock() else {
            return false;
        };
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
async fn scan_volume_recursive(
    volume: &dyn Volume,
    volume_id: &str,
    path: &Path,
    is_dir_hint: Option<bool>,
    entries: &mut Vec<VolumeDeleteEntry>,
    total_bytes: &mut u64,
    seen_inodes: &mut std::collections::HashSet<u64>,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &Arc<VolumeScanTracker>,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    if super::super::state::is_cancelled(&state.intent) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Resolve whether `path` is a directory. Prefer the caller's hint (the
    // top-level cache-hit path supplies it from `CopyScanResult`, the
    // recursive call supplies it from the parent's `FileEntry`). Without a
    // hint, fall back to a real `is_directory` probe — which on MTP lists
    // the parent dir, so we avoid it whenever the oracle can answer.
    let is_dir = match is_dir_hint {
        Some(v) => v,
        None => volume
            .is_directory(path)
            .await
            .map_err(|e| map_volume_error(&path.display().to_string(), e))?,
    };

    if is_dir {
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

        let on_progress = move |p: crate::file_system::volume::ListingProgress| {
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
            update_operation_status(
                &op_id_for_cb,
                WriteOperationPhase::Scanning,
                None,
                files_now,
                0,
                bytes_now,
                0,
            );
        };

        // Oracle-first: if this directory is watcher-backed in `LISTING_CACHE`,
        // reuse the cached entries and skip the volume round-trip entirely.
        // Falls through to `volume.list_directory` on miss, preserving the
        // per-entry progress callback for slow MTP listings.
        let children = match try_get_watched_listing(volume_id, path) {
            Some(cached) => {
                // The cached listing is already complete, so synthesize a
                // single end-of-listing progress tick (with the tally we'd
                // have built incrementally) to keep the FE counter climbing
                // during the cache-fed pass too.
                let mut tally = crate::file_system::volume::ListingProgress::default();
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
            None => volume
                .list_directory_with_cancel(path, Some(&on_progress), Some(&state.backend_cancel))
                .await
                .map_err(|e| map_volume_error(&path.display().to_string(), e))?,
        };

        // Recurse into children first. list_directory returns FileEntry with size,
        // so we use child.size directly instead of calling get_metadata (which
        // returns NotSupported on MTP). Pass `is_dir_hint = Some(child.is_directory)`
        // so the recursive call doesn't re-probe (avoids another parent listing
        // on MTP).
        for child in &children {
            let child_path = PathBuf::from(&child.path);
            if child.is_directory {
                Box::pin(scan_volume_recursive(
                    volume,
                    volume_id,
                    &child_path,
                    Some(true),
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
                let size = child.size.unwrap_or(0);
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
                });
            }
        }

        // Add directory after its children (already deepest-first order)
        tracker.dirs_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: true,
        });
    } else {
        // Top-level file without listing context: size unknown, use 0.
        // Progress still tracks file count accurately.
        tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: false,
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
        update_operation_status(
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

/// Emits `write-cancelled` when `result` is `Err(Cancelled)`. Use this after
/// every top-level `scan_volume_recursive` call so the FE sees the proper
/// terminal event before the error propagates up. See `scan_volume_recursive`'s
/// "Cancel contract" doc for why this is the caller's responsibility.
fn emit_cancelled_if_aborted(
    result: &Result<(), WriteOperationError>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &VolumeScanTracker,
) {
    if matches!(result, Err(WriteOperationError::Cancelled { .. })) {
        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_processed: tracker.files_so_far.load(std::sync::atomic::Ordering::Relaxed),
            rolled_back: false,
        });
    }
}

/// Deletes files on a non-local volume (like MTP) with progress reporting.
///
/// Uses `volume.list_directory()` for scanning and `volume.delete()` per item.
/// Emits the same events as `delete_files_with_progress` so the frontend progress
/// dialog works unchanged.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(in crate::file_system::write_operations) async fn delete_volume_files_with_progress(
    volume: Arc<dyn Volume>,
    volume_id: &str,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    let events = TauriEventSink::new(app.clone());
    delete_volume_files_with_progress_inner(volume, volume_id, &events, operation_id, state, sources, config).await
}

#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(super) async fn delete_volume_files_with_progress_inner(
    volume: Arc<dyn Volume>,
    volume_id: &str,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    // Phase 1: Scan. Recursively enumerate the tree via volume.list_directory().
    // Fast path: if the scan preview already enumerated the top-level shape (file
    // vs dir + size per source), reuse it. The per-source `top_level_is_directory`
    // and `total_bytes` flags come from `CachedScanResult::per_path` populated by
    // `run_volume_scan_preview`. We still need to recurse into top-level dirs to
    // get the per-file paths delete needs — but the recursion's walker is now
    // oracle-aware, so a watched subtree skips the `list_directory` round-trip.
    let cached_scan = config.preview_id.as_deref().and_then(take_cached_scan_result);

    let mut entries: Vec<VolumeDeleteEntry> = Vec::new();
    let mut total_bytes = 0u64;
    // Operation-scoped hardlink dedup. Shared across all top-level sources
    // so a hardlink that spans two different sources still counts once
    // (matches the local-FS walker's contract — `seen_inodes` in
    // `scan.rs::walk_dir_recursive`). Empty for backends without inode info.
    let mut seen_inodes: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let tracker = Arc::new(VolumeScanTracker::new(state.progress_interval));
    let mut last_scan_emit = Instant::now();

    if let Some(scan) = cached_scan.as_ref() {
        log::debug!(
            "delete_volume_files_with_progress: reused cached scan for operation_id={}, files={}, bytes={}, per_path={}",
            operation_id,
            scan.file_count,
            scan.total_bytes,
            scan.per_path.len()
        );

        // Index cached per-path results by source path so we can look up
        // `top_level_is_directory` and `total_bytes` for each input source.
        let by_path: std::collections::HashMap<PathBuf, &crate::file_system::volume::CopyScanResult> =
            scan.per_path.iter().map(|(p, r)| (p.clone(), r)).collect();

        for source in sources {
            if super::super::state::is_cancelled(&state.intent) {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: 0,
                    rolled_back: false,
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            match by_path.get(source) {
                Some(per_path_scan) if !per_path_scan.top_level_is_directory => {
                    // Top-level file: size comes straight from the cache. No
                    // `is_directory` probe and no `list_directory` round-trip.
                    // No inode info on the cache-hit path (`CopyScanResult`
                    // doesn't carry it), so each top-level cached file is
                    // treated as a unique inode. Cross-source hardlinks at
                    // the top level aren't dedup'd; within-source recursion
                    // still dedupes via the walker's `seen_inodes`.
                    let size = per_path_scan.total_bytes;
                    tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                    tracker.bytes_so_far.fetch_add(size, Ordering::Relaxed);
                    total_bytes += size;
                    entries.push(VolumeDeleteEntry {
                        path: source.to_path_buf(),
                        progress_bytes: size,
                        is_dir: false,
                    });
                }
                Some(_) => {
                    // Top-level dir: recurse via the oracle-aware walker, with
                    // `is_dir_hint = Some(true)` so the recursion never re-probes
                    // the top-level. Any subtree that's open in another pane and
                    // watcher-fresh gets cache-fed inside the walker.
                    let result = scan_volume_recursive(
                        &*volume,
                        volume_id,
                        source,
                        Some(true),
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
                None => {
                    // Scan preview was non-volume (local-FS) or didn't include
                    // this source. Fall back to the no-preview shape for this
                    // path: oracle-aware walker resolves the type.
                    let result = scan_volume_recursive(
                        &*volume,
                        volume_id,
                        source,
                        None,
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
            }

            // Even on the all-files fast path, throttle a single scan-progress
            // emit per interval so the FE dialog shows movement during the
            // entry-list build. Without this the user sees "Scanning…" frozen
            // until the actual delete phase starts.
            if last_scan_emit.elapsed() >= state.progress_interval {
                let files_done = tracker.files_so_far.load(Ordering::Relaxed);
                let dirs_done = tracker.dirs_so_far.load(Ordering::Relaxed);
                let bytes_done = tracker.bytes_so_far.load(Ordering::Relaxed);
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
                    .with_scan_meta(Some(source.display().to_string()), dirs_done, None),
                );
                update_operation_status(
                    operation_id,
                    WriteOperationPhase::Scanning,
                    None,
                    files_done,
                    0,
                    bytes_done,
                    0,
                );
                last_scan_emit = Instant::now();
            }
        }
    } else {
        for source in sources {
            // Oracle-first parent lookup: if the parent's listing is watched
            // (open in some pane), pull `is_directory` from there instead of
            // calling `volume.is_directory`, which on MTP lists the parent.
            // Falls through to the trait probe when the oracle can't answer.
            let parent_hint = source.parent().and_then(|parent| {
                let cached_entries = try_get_watched_listing(volume_id, parent)?;
                let name = source.file_name()?.to_string_lossy().to_string();
                cached_entries
                    .iter()
                    .find(|e| {
                        PathBuf::from(&e.path)
                            .file_name()
                            .map(|n| n.to_string_lossy() == name)
                            .unwrap_or(false)
                    })
                    .map(|e| e.is_directory)
            });

            let is_dir = match parent_hint {
                Some(v) => v,
                None => volume.is_directory(source).await.unwrap_or(false),
            };

            if is_dir {
                let result = scan_volume_recursive(
                    &*volume,
                    volume_id,
                    source,
                    Some(true),
                    &mut entries,
                    &mut total_bytes,
                    &mut seen_inodes,
                    state,
                    events,
                    operation_id,
                    &tracker,
                )
                .await;
                emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                result?;
            } else {
                // Top-level file: size unknown without listing the parent, use 0.
                // Progress still tracks file count accurately, and individual file
                // deletes are near-instant on MTP.
                tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                entries.push(VolumeDeleteEntry {
                    path: source.to_path_buf(),
                    progress_bytes: 0,
                    is_dir: false,
                });
            }
        }
    }

    let file_count = entries.iter().filter(|e| !e.is_dir).count();
    let dirs_count = entries.iter().filter(|e| e.is_dir).count();

    // Emit final scan progress (scan complete: tallies == totals)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Scanning,
            None,
            file_count,
            file_count,
            total_bytes,
            total_bytes,
        )
        .with_scan_meta(None, dirs_count, None),
    );

    // Handle dry-run mode
    if config.dry_run {
        events.emit_dry_run_complete(DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: file_count,
            bytes_total: total_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        });
        return Ok(());
    }

    // Phase 2: Delete. Files first, then directories deepest-first.
    // entries are already in order: children before parents (due to recursive scan).
    // We process files first, then dirs in reverse order.
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Delete files
    for entry in entries.iter().filter(|e| !e.is_dir) {
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rolled_back: false,
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // E2E throttle so cancel-during-delete tests on fast virtual MTP have a
        // deterministic window in which to click Cancel before all files are
        // gone. Reuses the copy throttle knob (`set_test_throttle` exposes a
        // generic per-step throttle). Zero-cost outside E2E.
        if let Some(ms) = crate::test_mode::effective_copy_throttle_ms()
            && ms > 0
        {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }

        match volume
            .delete_with_cancel(&entry.path, Some(&state.backend_cancel))
            .await
        {
            Ok(()) => {}
            // Cancel landed mid-iteration (during the throttle sleep or inside
            // delete_with_cancel itself). The top-of-loop cancel check only
            // catches cancels that land between iterations; without this arm,
            // the `?` propagation would surface as a Cancelled error with no
            // `write-cancelled` event, breaking the settle contract (see the
            // module CLAUDE.md § "Settle contract").
            Err(VolumeError::Cancelled(_)) => {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rolled_back: false,
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }
            Err(e) => return Err(map_volume_error(&entry.path.display().to_string(), e)),
        }

        files_done += 1;
        // `progress_bytes` (not `size`) so the numerator stays in lockstep
        // with the dedup'd `total_bytes` denominator. See
        // `VolumeDeleteEntry::progress_bytes` for the contract.
        bytes_done += entry.progress_bytes;

        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = entry.path.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Deleting,
                    current_file.clone(),
                    files_done,
                    file_count,
                    bytes_done,
                    total_bytes,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                file_count,
                bytes_done,
                total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (already in deepest-first order from scan_volume_recursive)
    for entry in entries.iter().filter(|e| e.is_dir) {
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rolled_back: false,
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Best-effort directory removal (may fail if not empty due to partial delete)
        let _ = volume
            .delete_with_cancel(&entry.path, Some(&state.backend_cancel))
            .await;
    }

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: files_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
    });

    Ok(())
}
