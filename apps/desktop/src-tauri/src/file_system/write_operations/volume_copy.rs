//! Unified volume copy operations.
//!
//! This module provides copy operations that work across different volume types.
//! It abstracts the differences between local and MTP volumes, providing a unified
//! interface for file copying regardless of source or destination type.
//!
//! Copy operation flow:
//! 1. Scan source files for count and total bytes
//! 2. Check destination space availability
//! 3. Scan for conflicts at destination
//! 4. Execute copy with progress reporting
//!
//! For cross-volume copies:
//! - Local → Local: Uses existing efficient file copy
//! - Local → MTP: Uses volume.import_from_local()
//! - MTP → Local: Uses volume.export_to_local()

use std::collections::HashMap;
use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::scan::take_cached_scan_result;

/// Per-source hints collected during the scan phase, so the copy loop can
/// skip re-probing the source type/size per file. `size` is only meaningful
/// when `is_directory == false`; it's the top-level file's size and feeds
/// the SMB compound fast-path.
#[derive(Clone, Copy, Default)]
struct SourceHint {
    is_directory: bool,
    size: u64,
}
use super::state::{
    OperationIntent, WRITE_OPERATION_STATE, WriteOperationState, is_cancelled, load_intent, register_operation_status,
    unregister_operation_status, update_operation_status,
};
use super::types::{
    ConflictResolution, OperationEventSink, TauriEventSink, VolumeCopyConfig, VolumeCopyScanResult,
    WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::{SourceItemInfo, Volume, VolumeError};

/// Starts a copy operation between two volumes.
///
/// This is the unified entry point for all copy operations:
/// - Local → Local
/// - Local → MTP
/// - MTP → Local
///
/// The function determines the appropriate copy strategy based on volume types
/// and handles progress reporting, conflict detection, and cancellation.
///
/// # Arguments
///
/// * `app` - Tauri app handle for event emission
/// * `source_volume` - The source volume to copy from
/// * `source_paths` - Paths of files/directories to copy (relative to source volume root)
/// * `dest_volume` - The destination volume to copy to
/// * `dest_path` - Destination directory path (relative to dest volume root)
/// * `config` - Copy operation configuration
///
/// # Events emitted
///
/// * `write-progress` - Every progress_interval_ms with WriteProgressEvent
/// * `write-complete` - On success with WriteCompleteEvent
/// * `write-error` - On error with WriteErrorEvent
/// * `write-cancelled` - If cancelled with WriteCancelledEvent
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate that volumes support the required operations
    if !source_volume.supports_export() {
        return Err(WriteOperationError::IoError {
            path: String::new(),
            message: format!("Source volume '{}' does not support export", source_volume.name()),
        });
    }

    // Optimization: If both volumes are local filesystem paths, use the battle-tested
    // copy.rs implementation which has proper cancellation support via macOS copyfile API.
    if let (Some(src_root), Some(dest_root)) = (source_volume.local_path(), dest_volume.local_path()) {
        log::debug!(
            "copy_between_volumes: both volumes are local, delegating to native copy (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        // Convert relative paths to absolute paths
        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = dest_root.join(dest_path.strip_prefix("/").unwrap_or(&dest_path));

        // Convert VolumeCopyConfig to WriteOperationConfig, preserving preview_id
        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            ..Default::default()
        };

        // Delegate to the existing copy implementation with full cancellation support
        return super::copy_files_start(app, absolute_sources, absolute_dest, write_config).await;
    }

    let operation_id = Uuid::new_v4().to_string();
    log::info!(
        "copy_between_volumes: operation_id={}, source_volume={}, dest_volume={}, {} sources, dest={}",
        operation_id,
        source_volume.name(),
        dest_volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(
        config.progress_interval_ms,
    )));

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Copy);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let events = TauriEventSink::new(app);
        let result: Result<(), WriteFailure> = copy_volumes_with_progress(
            &events,
            &operation_id_for_spawn,
            &state,
            source_volume,
            &source_paths,
            dest_volume,
            &dest_path,
            &config,
        )
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle result
        use tauri::Emitter;
        match result {
            Ok(()) => {
                // Success - write-complete event already emitted by copy_volumes_with_progress
            }
            Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                // write-cancelled was already emitted by copy_volumes_with_progress,
                // so don't also emit write-error — it would make the frontend log
                // a user-initiated cancel as an error.
                log::info!("copy_between_volumes: operation {} cancelled", operation_id_for_cleanup,);
            }
            Err(failure) => {
                // Toast-visible failure for cross-volume copy (Local↔SMB↔MTP).
                // Routed through `log_error!` so opt-in users get an auto report.
                crate::log_error!(
                    "copy_between_volumes: operation {} failed: {:?}",
                    operation_id_for_cleanup,
                    failure.error,
                );
                let _ = app_for_error.emit(
                    "write-error",
                    write_error_event_from(operation_id_for_cleanup, WriteOperationType::Copy, failure),
                );
            }
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Copy,
    })
}

/// Performs a pre-flight scan for volume copy without executing.
///
/// This scans the source files and checks destination for conflicts and space.
/// Use this to show the user what will happen before starting the copy.
///
/// # Arguments
///
/// * `source_volume` - The source volume to scan
/// * `source_paths` - Paths of files/directories to copy
/// * `dest_volume` - The destination volume
/// * `dest_path` - Destination directory path
/// * `max_conflicts` - Maximum number of conflicts to return
pub async fn scan_for_volume_copy(
    source_volume: &dyn Volume,
    source_paths: &[PathBuf],
    dest_volume: &dyn Volume,
    dest_path: &Path,
    max_conflicts: usize,
) -> Result<VolumeCopyScanResult, VolumeError> {
    // Scan source for total bytes and file count
    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_bytes = 0u64;
    let mut source_items: Vec<SourceItemInfo> = Vec::new();

    for source_path in source_paths {
        let scan = source_volume.scan_for_copy(source_path).await?;
        total_files += scan.file_count;
        total_dirs += scan.dir_count;
        total_bytes += scan.total_bytes;

        // Collect source item info for conflict detection
        // For now, we just use the top-level item name
        if let Some(name) = source_path.file_name() {
            let metadata = source_volume.get_metadata(source_path).await.ok();
            source_items.push(SourceItemInfo {
                name: name.to_string_lossy().to_string(),
                size: metadata.as_ref().and_then(|m| m.size).unwrap_or(0),
                modified: metadata
                    .as_ref()
                    .and_then(|m| m.modified_at.map(|ms| (ms / 1000) as i64)),
            });
        }
    }

    // Get destination space info
    let dest_space = dest_volume.get_space_info().await?;

    // Check if there's enough space
    if dest_space.available_bytes < total_bytes {
        return Err(VolumeError::IoError {
            message: format!(
                "Not enough space: need {} bytes, only {} available",
                total_bytes, dest_space.available_bytes
            ),
            raw_os_error: None,
        });
    }

    // Scan for conflicts at destination
    let all_conflicts = dest_volume.scan_for_conflicts(&source_items, dest_path).await?;

    // Limit the number of conflicts returned
    let conflicts = if all_conflicts.len() > max_conflicts {
        all_conflicts.into_iter().take(max_conflicts).collect()
    } else {
        all_conflicts
    };

    Ok(VolumeCopyScanResult {
        file_count: total_files,
        dir_count: total_dirs,
        total_bytes,
        dest_space,
        conflicts,
    })
}

/// Internal function that performs the actual copy with progress reporting.
///
/// Exposed as `pub(crate)` under `cfg(test)` so integration tests in sibling
/// modules (for example the SMB concurrent-copy cross-contamination test in
/// `volume/smb.rs`) can drive the real copy pipeline with a
/// `CollectorEventSink` instead of spinning up a full Tauri app. In
/// production, the only caller is `copy_between_volumes` in this file.
#[allow(
    clippy::too_many_arguments,
    reason = "Volume copy requires passing multiple context parameters"
)]
pub(crate) async fn copy_volumes_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteFailure> {
    log::debug!(
        "copy_volumes_with_progress: starting operation_id={}, {} sources",
        operation_id,
        source_paths.len()
    );

    // Phase 1: Scan sources (or reuse cached scan from preview)
    let total_files;
    let total_bytes;

    // Per-source hint collected during the scan: whether the top-level path
    // is a directory and, for top-level files, the file size. The copy loop
    // reuses these to skip an `is_directory` probe per file and, for SMB, to
    // pick the 1-RTT compound fast-path when the file fits in one READ.
    let mut source_hints: HashMap<PathBuf, SourceHint> = HashMap::with_capacity(source_paths.len());

    if let Some(cached) = config.preview_id.as_deref().and_then(take_cached_scan_result) {
        total_files = cached.file_count;
        total_bytes = cached.total_bytes;
        log::debug!(
            "copy_volumes_with_progress: reused cached scan for operation_id={}, files={}, bytes={}",
            operation_id,
            total_files,
            total_bytes
        );
        // TODO: extend the preview cache to carry per-source type + size so this
        // branch doesn't need to re-stat. For now, the preview path already saved
        // one full scan per source — this extra stat is bounded by source count
        // and the compound fast-path falls back cleanly when size is unknown.
        for source_path in source_paths {
            let is_dir = source_volume.is_directory(source_path).await.unwrap_or(false);
            source_hints.insert(
                source_path.clone(),
                SourceHint {
                    is_directory: is_dir,
                    size: 0,
                },
            );
        }
    } else {
        log::debug!(
            "copy_volumes_with_progress: scanning sources for operation_id={}",
            operation_id
        );

        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::Scanning,
                None,
                0,
                0,
                0,
                0,
            ),
        );

        if is_cancelled(&state.intent) {
            return Err(WriteFailure::synthetic(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            }));
        }

        // Single pipelined batch scan. For SMB this fires N stat requests
        // over one session in parallel instead of N sequential RTTs (Fix 4).
        // Default impl loops per-path for backends where per-path I/O is
        // cheap (local FS, in-memory). MTP overrides to group by parent dir.
        let batch = source_volume.scan_for_copy_batch(source_paths).await.map_err(|e| {
            // No specific source path here — pick the first one or fall back to the dest.
            let path = source_paths.first().cloned().unwrap_or_else(|| dest_path.to_path_buf());
            WriteFailure::from_volume(&path, e)
        })?;

        total_files = batch.aggregate.file_count;
        let total_dirs = batch.aggregate.dir_count;
        total_bytes = batch.aggregate.total_bytes;

        for (source_path, scan) in &batch.per_path {
            // For top-level files, `scan.total_bytes` == the file size.
            // For directories, we leave `size = 0` (unused downstream).
            let size = if scan.top_level_is_directory {
                0
            } else {
                scan.total_bytes
            };
            source_hints.insert(
                source_path.clone(),
                SourceHint {
                    is_directory: scan.top_level_is_directory,
                    size,
                },
            );
        }

        log::debug!(
            "copy_volumes_with_progress: scan complete for operation_id={}, files={}, dirs={}, bytes={}",
            operation_id,
            total_files,
            total_dirs,
            total_bytes
        );
    }

    // Phase 2: Check destination space
    let dest_space = dest_volume
        .get_space_info()
        .await
        .map_err(|e| WriteFailure::from_volume(dest_path, e))?;
    if dest_space.available_bytes < total_bytes {
        return Err(WriteFailure::synthetic(WriteOperationError::InsufficientSpace {
            required: total_bytes,
            available: dest_space.available_bytes,
            volume_name: Some(dest_volume.name().to_string()),
        }));
    }

    // Phase 3: Copy files with progress
    // Shared atomics — updated by in-flight tasks (under concurrency) or
    // the sequential closure below. The driver reads them after each file to
    // keep `files_done` / `bytes_done` in sync for post-loop bookkeeping.
    let files_done_atomic = Arc::new(AtomicUsize::new(0));
    let atomic_bytes_done = Arc::new(AtomicU64::new(0));
    let last_progress_mutex = Arc::new(std::sync::Mutex::new(Instant::now()));
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let progress_interval = Duration::from_millis(config.progress_interval_ms);

    // Determine concurrency for this batch.
    // Clamped to 32 per F6 (matches smb2's MAX_PIPELINE_WINDOW). The sequential
    // fallback (F7) handles 1-2 file batches where spawning tasks isn't worth
    // it, and backends that return 1 from max_concurrent_ops.
    let concurrency = source_volume
        .max_concurrent_ops()
        .min(dest_volume.max_concurrent_ops())
        .min(32);
    let use_concurrent_path = source_paths.len() >= 3 && concurrency > 1;
    log::debug!(
        "copy_volumes_with_progress: {} sources, concurrency={} (src={}, dst={}), path={}",
        source_paths.len(),
        concurrency,
        source_volume.max_concurrent_ops(),
        dest_volume.max_concurrent_ops(),
        if use_concurrent_path {
            "concurrent"
        } else {
            "sequential"
        },
    );

    // Emit initial copying phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::Copying,
            None,
            0,
            total_files,
            0,
            total_bytes,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        0,
        total_files,
        0,
        total_bytes,
    );

    // Track "apply to all" resolution for conflicts
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    // Track successfully copied destination paths for rollback/cleanup.
    // Wrapped in Arc<Mutex> so concurrent tasks can push independently. The
    // sequential path uses the same container for a uniform post-loop flow.
    let copied_paths: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    // In concurrent mode, in-flight tasks each pin down their own partial
    // destination path so a cancel/error can delete all of them. Sequential
    // mode keeps the legacy single-slot behavior via a 1-element vec.
    let in_flight_partials: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut last_dest_path: Option<PathBuf> = None;
    let mut copy_error: Option<WriteFailure> = None;

    if use_concurrent_path {
        // Concurrent path: FuturesUnordered-driven sliding window sized by
        // `concurrency`. Each task streams one top-level source item end-to-end.
        // Conflict resolution runs synchronously on this driver before the task
        // is spawned (F14) so the whole batch blocks on a single Stop prompt
        // instead of racing per-task prompts.
        type CopyTaskFuture<'a> =
            std::pin::Pin<Box<dyn Future<Output = Result<(PathBuf, u64), (PathBuf, VolumeError)>> + Send + 'a>>;
        let mut in_flight: FuturesUnordered<CopyTaskFuture<'_>> = FuturesUnordered::new();

        // Inline helper: drains ONE future from `in_flight`, updates tracking.
        // Returns Err on the first task failure (caller breaks + stores copy_error).
        // `in_flight` is threaded through as a mutable borrow so the helper is
        // just a local lambda in shape, but we inline below for borrow clarity.

        let mut iter = source_paths.iter();
        loop {
            // Keep pushing new tasks until either sources run out or the window is full.
            while in_flight.len() < concurrency {
                if is_cancelled(&state.intent) {
                    break;
                }
                let Some(source_path) = iter.next() else {
                    break;
                };

                // Resolve destination path + conflict synchronously.
                let mut dest_item_path = if let Some(name) = source_path.file_name() {
                    dest_path.join(name)
                } else {
                    dest_path.to_path_buf()
                };
                if let Ok(dest_meta) = dest_volume.get_metadata(&dest_item_path).await {
                    // Reuse the per-source hint from the scan instead of re-statting.
                    let source_is_dir = source_hints.get(source_path).map(|h| h.is_directory).unwrap_or(false);
                    log::debug!(
                        "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                        dest_item_path.display(),
                        source_is_dir,
                        dest_meta.is_directory,
                    );
                    let resolved = resolve_volume_conflict(
                        &source_volume,
                        source_path,
                        &dest_volume,
                        &dest_item_path,
                        config,
                        events,
                        operation_id,
                        state,
                        &mut apply_to_all_resolution,
                    )
                    .await
                    .map_err(WriteFailure::synthetic)?;
                    match resolved {
                        None => {
                            log::debug!(
                                "copy_volumes_with_progress: skipping {} due to conflict resolution",
                                source_path.display()
                            );
                            continue;
                        }
                        Some(p) => dest_item_path = p,
                    }
                }

                let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                log::debug!(
                    "copy_volumes_with_progress: spawning copy {} -> {}",
                    source_path.display(),
                    dest_item_path.display()
                );

                // Mark this destination as in-flight so cancel/error can clean it up.
                in_flight_partials.lock().unwrap().push(dest_item_path.clone());

                let src_vol = Arc::clone(&source_volume);
                let dst_vol = Arc::clone(&dest_volume);
                let state_clone = Arc::clone(state);
                let events_ref: &dyn OperationEventSink = events;
                let op_id = operation_id;
                let files_done_a = Arc::clone(&files_done_atomic);
                let bytes_done_a = Arc::clone(&atomic_bytes_done);
                let last_prog_a = Arc::clone(&last_progress_mutex);
                let source_owned = source_path.clone();
                let dest_owned = dest_item_path.clone();
                let file_name_owned = file_name.clone();
                let hint = source_hints.get(source_path).copied().unwrap_or_default();
                let source_is_dir_hint = hint.is_directory;
                let source_size_hint = if hint.is_directory { None } else { Some(hint.size) };

                in_flight.push(Box::pin(async move {
                    // Per-task `last_file_bytes` tracks bytes reported for the
                    // file this task is copying; deltas roll up into the
                    // shared `bytes_done_a` so the throttle emits an aggregate.
                    let last_file_bytes = AtomicU64::new(0);
                    let on_file_progress = |file_bytes_done: u64, _total: u64| -> ControlFlow<()> {
                        if is_cancelled(&state_clone.intent) {
                            return ControlFlow::Break(());
                        }
                        let prev = last_file_bytes.swap(file_bytes_done, Ordering::Relaxed);
                        let delta = file_bytes_done.saturating_sub(prev);
                        let current_total = bytes_done_a.fetch_add(delta, Ordering::Relaxed) + delta;
                        let current_files_done = files_done_a.load(Ordering::Relaxed);
                        let last = *last_prog_a.lock().unwrap();
                        if last.elapsed() >= progress_interval {
                            *last_prog_a.lock().unwrap() = Instant::now();
                            state_clone.emit_progress_via_sink(
                                events_ref,
                                WriteProgressEvent::new(
                                    op_id.to_string(),
                                    WriteOperationType::Copy,
                                    WriteOperationPhase::Copying,
                                    file_name_owned.clone(),
                                    current_files_done,
                                    total_files,
                                    current_total,
                                    total_bytes,
                                ),
                            );
                            update_operation_status(
                                op_id,
                                WriteOperationPhase::Copying,
                                file_name_owned.clone(),
                                current_files_done,
                                total_files,
                                current_total,
                                total_bytes,
                            );
                        }
                        ControlFlow::Continue(())
                    };
                    let on_file_complete = || {
                        files_done_a.fetch_add(1, Ordering::Relaxed);
                    };
                    let result = copy_single_path(
                        &src_vol,
                        &source_owned,
                        source_is_dir_hint,
                        source_size_hint,
                        &dst_vol,
                        &dest_owned,
                        &state_clone,
                        &on_file_progress,
                        &on_file_complete,
                    )
                    .await;
                    match result {
                        Ok(bytes) => {
                            // If the volume didn't call the progress callback,
                            // add bytes_copied to the aggregate so the total is
                            // right. Same compensation the sequential path does.
                            if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes > 0 {
                                bytes_done_a.fetch_add(bytes, Ordering::Relaxed);
                            }
                            Ok((dest_owned, bytes))
                        }
                        Err(e) => Err((dest_owned, e)),
                    }
                }));
            }

            if in_flight.is_empty() {
                break;
            }

            match in_flight.next().await {
                Some(Ok((completed_dest, _bytes))) => {
                    // Remove from in-flight partials and record as completed.
                    let mut partials = in_flight_partials.lock().unwrap();
                    if let Some(pos) = partials.iter().position(|p| p == &completed_dest) {
                        partials.swap_remove(pos);
                    }
                    drop(partials);
                    copied_paths.lock().unwrap().push(completed_dest);
                }
                Some(Err((failed_dest, e))) => {
                    // Remove from in-flight partials — this one's its own
                    // partial cleanup the post-loop logic will do.
                    let mut partials = in_flight_partials.lock().unwrap();
                    if let Some(pos) = partials.iter().position(|p| p == &failed_dest) {
                        partials.swap_remove(pos);
                    }
                    drop(partials);
                    last_dest_path = Some(failed_dest.clone());
                    copy_error = Some(WriteFailure::from_volume(&failed_dest, e));
                    // Drop remaining in-flight tasks — their streams close,
                    // temp files get cleaned up by the per-backend write
                    // abort + delete path. Partial cleanup is done below.
                    break;
                }
                None => break,
            }
        }

        // Drain whatever's left on cancel/error. On success, `in_flight` is
        // already empty. On abort, drop cancels the remaining futures (F10).
        drop(in_flight);
        // Sync counters for post-loop reporting.
        files_done = files_done_atomic.load(Ordering::Relaxed);
        bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
    } else {
        // Sequential path (unchanged semantics). Kept behavior-equivalent to
        // pre-P4.2 for small batches and for backends that don't parallelize.
        for source_path in source_paths {
            if is_cancelled(&state.intent) {
                break;
            }

            let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
            let mut dest_item_path = if let Some(name) = source_path.file_name() {
                dest_path.join(name)
            } else {
                dest_path.to_path_buf()
            };

            if let Ok(dest_meta) = dest_volume.get_metadata(&dest_item_path).await {
                let source_is_dir = source_hints.get(source_path).map(|h| h.is_directory).unwrap_or(false);
                log::debug!(
                    "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                    dest_item_path.display(),
                    source_is_dir,
                    dest_meta.is_directory,
                );
                let resolved = resolve_volume_conflict(
                    &source_volume,
                    source_path,
                    &dest_volume,
                    &dest_item_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )
                .await
                .map_err(WriteFailure::synthetic)?;
                match resolved {
                    None => {
                        log::debug!(
                            "copy_volumes_with_progress: skipping {} due to conflict resolution",
                            source_path.display()
                        );
                        continue;
                    }
                    Some(resolved_path) => {
                        dest_item_path = resolved_path;
                    }
                }
            }

            log::debug!(
                "copy_volumes_with_progress: copying {} -> {}",
                source_path.display(),
                dest_item_path.display()
            );

            let last_file_bytes = AtomicU64::new(0);
            let file_name_for_cb = file_name.clone();
            let bytes_done_a = Arc::clone(&atomic_bytes_done);
            let files_done_a = Arc::clone(&files_done_atomic);
            let last_prog_a = Arc::clone(&last_progress_mutex);

            let on_file_progress = |file_bytes_done: u64, _file_bytes_total: u64| -> ControlFlow<()> {
                if is_cancelled(&state.intent) {
                    return ControlFlow::Break(());
                }
                let prev = last_file_bytes.swap(file_bytes_done, Ordering::Relaxed);
                let delta = file_bytes_done.saturating_sub(prev);
                let current_total = bytes_done_a.fetch_add(delta, Ordering::Relaxed) + delta;
                let current_files_done = files_done_a.load(Ordering::Relaxed);
                let last = *last_prog_a.lock().unwrap();
                if last.elapsed() >= progress_interval {
                    *last_prog_a.lock().unwrap() = Instant::now();
                    state.emit_progress_via_sink(
                        events,
                        WriteProgressEvent::new(
                            operation_id.to_string(),
                            WriteOperationType::Copy,
                            WriteOperationPhase::Copying,
                            file_name_for_cb.clone(),
                            current_files_done,
                            total_files,
                            current_total,
                            total_bytes,
                        ),
                    );
                    update_operation_status(
                        operation_id,
                        WriteOperationPhase::Copying,
                        file_name_for_cb.clone(),
                        current_files_done,
                        total_files,
                        current_total,
                        total_bytes,
                    );
                }
                ControlFlow::Continue(())
            };

            let on_file_complete = || {
                files_done_atomic.fetch_add(1, Ordering::Relaxed);
            };

            last_dest_path = Some(dest_item_path.clone());

            let hint = source_hints.get(source_path).copied().unwrap_or_default();
            let source_is_dir_hint = hint.is_directory;
            let source_size_hint = if hint.is_directory { None } else { Some(hint.size) };
            match copy_single_path(
                &source_volume,
                source_path,
                source_is_dir_hint,
                source_size_hint,
                &dest_volume,
                &dest_item_path,
                state,
                &on_file_progress,
                &on_file_complete,
            )
            .await
            {
                Ok(bytes_copied) => {
                    copied_paths.lock().unwrap().push(dest_item_path);
                    last_dest_path = None;
                    files_done = files_done_atomic.load(Ordering::Relaxed);
                    bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
                    if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes_copied > 0 {
                        bytes_done += bytes_copied;
                        atomic_bytes_done.store(bytes_done, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
                    copy_error = Some(WriteFailure::from_volume(source_path, e));
                    break;
                }
            }
        }
    }

    // Unwrap shared containers for post-loop logic.
    let mut copied_paths: Vec<PathBuf> = Arc::try_unwrap(copied_paths)
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_else(|arc| arc.lock().unwrap().clone());
    let in_flight_partials: Vec<PathBuf> = Arc::try_unwrap(in_flight_partials)
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_else(|arc| arc.lock().unwrap().clone());

    // Post-loop: handle success, cancellation, or error
    let intent = load_intent(&state.intent);

    if copy_error.is_none() && !is_cancelled(&state.intent) {
        // All files copied successfully
        log::info!(
            "copy_volumes_with_progress: completed op={} files={} bytes={}",
            operation_id,
            files_done,
            bytes_done
        );

        events.emit_complete(WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            files_processed: files_done,
            bytes_processed: bytes_done,
        });

        return Ok(());
    }

    // Cancelled or errored — decide between rollback and cancel
    if intent == OperationIntent::RollingBack {
        // Include the last in-progress item in rollback (it was partially created)
        if let Some(partial_path) = last_dest_path.take() {
            copied_paths.push(partial_path);
        }
        // Under concurrency there can be multiple partials — the tasks we
        // dropped on abort each left a .cmdr-tmp-<uuid> that the backend's
        // writer.abort() cleaned up, but the destination path itself may have
        // an already-renamed file. Roll those back too.
        for partial in in_flight_partials.iter() {
            if !copied_paths.contains(partial) {
                copied_paths.push(partial.clone());
            }
        }

        // User requested rollback — delete all copied files in reverse order with progress
        log::info!(
            "copy_volumes_with_progress: rolling back op={}, {} paths to delete",
            operation_id,
            copied_paths.len()
        );

        let rollback_completed = volume_rollback_with_progress(
            &dest_volume,
            &copied_paths,
            events,
            operation_id,
            state,
            files_done,
            bytes_done,
            total_files,
            total_bytes,
        )
        .await;

        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            files_processed: files_done,
            rolled_back: rollback_completed,
        });
    } else {
        // Stopped or error — keep completed files, clean up partial files.
        // Sequential path leaves at most one partial in `last_dest_path`.
        // Concurrent path leaves one-per-in-flight-task in `in_flight_partials`
        // (already net of anything that finished before the abort).
        let mut partials_to_clean: Vec<PathBuf> = Vec::new();
        if let Some(partial_path) = last_dest_path.take() {
            partials_to_clean.push(partial_path);
        }
        for partial in &in_flight_partials {
            if !partials_to_clean.contains(partial) {
                partials_to_clean.push(partial.clone());
            }
        }
        for partial_path in &partials_to_clean {
            log::debug!(
                "copy_volumes_with_progress: cleaning up partial file {} for op={}",
                partial_path.display(),
                operation_id,
            );
            if let Err(e) = delete_volume_path_recursive(&dest_volume, partial_path).await {
                log::warn!(
                    "copy_volumes_with_progress: failed to clean up partial file {}: {:?}",
                    partial_path.display(),
                    e
                );
            }
        }

        if copy_error.is_none() {
            // Pure cancellation (Stopped)
            log::info!(
                "copy_volumes_with_progress: cancelled op={}, keeping {} copied files",
                operation_id,
                copied_paths.len()
            );
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                files_processed: files_done,
                rolled_back: false,
            });
        }
    }

    if let Some(err) = copy_error {
        return Err(err);
    }

    Err(WriteFailure::synthetic(WriteOperationError::Cancelled {
        message: "Operation cancelled by user".to_string(),
    }))
}

// ============================================================================
// Volume rollback helpers
// ============================================================================

/// Rolls back copied files on a volume with progress events, matching the local copy's
/// `rollback_with_progress` pattern. Deletes paths in reverse order so that files inside
/// directories are removed before the directories themselves.
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
async fn volume_rollback_with_progress(
    volume: &Arc<dyn Volume>,
    copied_paths: &[PathBuf],
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    let paths_to_delete = copied_paths.len();
    let mut paths_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::RollingBack,
            None,
            files_at_cancel,
            files_total,
            bytes_at_cancel,
            bytes_total,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::RollingBack,
        None,
        files_at_cancel,
        files_total,
        bytes_at_cancel,
        bytes_total,
    );

    // Delete in reverse order (newest first)
    for path in copied_paths.iter().rev() {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "volume_rollback_with_progress: rollback cancelled at {}/{} paths, keeping remaining",
                paths_deleted,
                paths_to_delete,
            );
            return false;
        }

        // Each copied path may be a file or a directory tree — delete recursively
        if let Err(e) = delete_volume_path_recursive(volume, path).await {
            log::warn!(
                "volume_rollback_with_progress: failed to delete {}: {:?}",
                path.display(),
                e
            );
        }
        paths_deleted += 1;

        // Throttled progress events with decreasing values
        if last_progress_time.elapsed() >= state.progress_interval {
            let remaining_files = files_at_cancel.saturating_sub(paths_deleted);
            let remaining_bytes = if paths_to_delete > 0 {
                bytes_at_cancel - (bytes_at_cancel as f64 * paths_deleted as f64 / paths_to_delete as f64) as u64
            } else {
                0
            };

            let current_file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Copy,
                    WriteOperationPhase::RollingBack,
                    Some(current_file_name.clone()),
                    remaining_files,
                    files_total,
                    remaining_bytes,
                    bytes_total,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::RollingBack,
                Some(current_file_name),
                remaining_files,
                files_total,
                remaining_bytes,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    true
}

/// Recursively deletes a file or directory on a volume.
///
/// For files: calls `volume.delete()` directly.
/// For directories: lists contents, deletes children (files first, then subdirs),
/// then deletes the directory itself. Best-effort — logs errors but continues.
pub(super) async fn delete_volume_path_recursive(volume: &Arc<dyn Volume>, path: &Path) -> Result<(), VolumeError> {
    let is_dir = match volume.is_directory(path).await {
        Ok(true) => true,
        Ok(false) => false,
        Err(_) => {
            // Path may not exist (already deleted or never fully created) — nothing to do
            return Ok(());
        }
    };

    if !is_dir {
        return volume.delete(path).await;
    }

    // List directory contents and delete children first
    let children = volume.list_directory(path, None).await?;

    // Delete files first, then recurse into subdirectories
    for child in &children {
        let child_path = PathBuf::from(&child.path);
        if child.is_directory {
            if let Err(e) = Box::pin(delete_volume_path_recursive(volume, &child_path)).await {
                log::warn!(
                    "delete_volume_path_recursive: failed to delete subdirectory {}: {:?}",
                    child_path.display(),
                    e
                );
            }
        } else if let Err(e) = volume.delete(&child_path).await {
            log::warn!(
                "delete_volume_path_recursive: failed to delete file {}: {:?}",
                child_path.display(),
                e
            );
        }
    }

    // Delete the now-empty directory
    volume.delete(path).await
}

/// A write-operation failure carrying the typed `WriteOperationError` for FE rendering plus,
/// when available, the originating `(VolumeError, path)` so the outer emit can build a
/// provider-enriched `FriendlyError` via `WriteErrorEvent::with_friendly`. `volume_ctx` is
/// `None` for failures that didn't start as a `VolumeError` (cancellation, validation,
/// synthetic IoError).
#[derive(Debug, Clone)]
pub(crate) struct WriteFailure {
    pub error: WriteOperationError,
    pub volume_ctx: Option<(VolumeError, PathBuf)>,
}

impl WriteFailure {
    /// Construct a `WriteFailure` from an originating `VolumeError + path`. Maps the error
    /// to a `WriteOperationError` and retains the volume context for friendly rendering.
    /// One spot to clone, one spot to map — replaces the per-call-site `e.clone()` boilerplate.
    pub(super) fn from_volume(path: &Path, e: VolumeError) -> Self {
        let error = map_volume_error(&path.display().to_string(), e.clone());
        Self {
            error,
            volume_ctx: Some((e, path.to_path_buf())),
        }
    }

    /// Construct a `WriteFailure` from a synthetic `WriteOperationError` (no volume
    /// context — used for cancellation, validation errors, etc.).
    pub(super) fn synthetic(error: WriteOperationError) -> Self {
        Self {
            error,
            volume_ctx: None,
        }
    }
}

/// Convenience: take a captured `(VolumeError, PathBuf)` and build the `WriteFailure` from
/// it. Used inside loops where we cloned the path for logging and want to surface the
/// volume context out alongside the typed write error.
impl From<(VolumeError, PathBuf)> for WriteFailure {
    fn from(ctx: (VolumeError, PathBuf)) -> Self {
        let (volume_error, path) = ctx;
        let error = map_volume_error(&path.display().to_string(), volume_error.clone());
        Self {
            error,
            volume_ctx: Some((volume_error, path)),
        }
    }
}

/// Builds a `WriteErrorEvent` from a `WriteFailure`. When the failure carries the originating
/// `VolumeError + path`, uses `with_friendly` for provider-enriched suggestions; otherwise
/// falls back to the variant-derived `new`. Shared by `volume_move` and `volume_copy`.
pub(super) fn write_error_event_from(
    operation_id: String,
    operation_type: WriteOperationType,
    failure: WriteFailure,
) -> WriteErrorEvent {
    match failure.volume_ctx {
        Some((volume_error, path)) => {
            WriteErrorEvent::with_friendly(operation_id, operation_type, failure.error, &volume_error, &path)
        }
        None => WriteErrorEvent::new(operation_id, operation_type, failure.error),
    }
}

/// Maps VolumeError to WriteOperationError, attaching path context where the original error lacks one.
pub(super) fn map_volume_error(context_path: &str, e: VolumeError) -> WriteOperationError {
    match e {
        VolumeError::NotFound(path) => WriteOperationError::SourceNotFound { path },
        VolumeError::PermissionDenied(msg) => WriteOperationError::PermissionDenied {
            path: context_path.to_string(),
            message: msg,
        },
        VolumeError::AlreadyExists(path) => WriteOperationError::DestinationExists { path },
        VolumeError::NotSupported => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: "Operation not supported by this volume type".to_string(),
        },
        VolumeError::DeviceDisconnected(_) => WriteOperationError::DeviceDisconnected {
            path: context_path.to_string(),
        },
        VolumeError::ReadOnly(_) => WriteOperationError::ReadOnlyDevice {
            path: context_path.to_string(),
            device_name: None,
        },
        VolumeError::StorageFull { .. } => WriteOperationError::InsufficientSpace {
            required: 0,
            available: 0,
            volume_name: None,
        },
        VolumeError::ConnectionTimeout(_) => WriteOperationError::ConnectionInterrupted {
            path: context_path.to_string(),
        },
        VolumeError::Cancelled(_) => WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        VolumeError::IoError { message, .. } => WriteOperationError::IoError {
            path: context_path.to_string(),
            message,
        },
        VolumeError::FriendlyGit(git_err) => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: git_err.to_string(),
        },
        VolumeError::IsADirectory(path) => WriteOperationError::IoError {
            path,
            message: "Is a directory".to_string(),
        },
    }
}

#[cfg(test)]
#[path = "volume_copy_tests.rs"]
mod tests;
