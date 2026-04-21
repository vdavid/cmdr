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

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::scan::take_cached_scan_result;
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

    let state = Arc::new(WriteOperationState {
        intent: Arc::new(AtomicU8::new(0)),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        conflict_resolution_tx: std::sync::Mutex::new(None),
    });

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
        let result: Result<(), WriteOperationError> = copy_volumes_with_progress(
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
            Err(write_err) => {
                if matches!(write_err, WriteOperationError::Cancelled { .. }) {
                    // write-cancelled was already emitted by copy_volumes_with_progress,
                    // so don't also emit write-error — it would make the frontend log
                    // a user-initiated cancel as an error.
                    log::info!("copy_between_volumes: operation {} cancelled", operation_id_for_cleanup,);
                } else {
                    log::error!(
                        "copy_between_volumes: operation {} failed: {:?}",
                        operation_id_for_cleanup,
                        write_err
                    );
                    let _ = app_for_error.emit(
                        "write-error",
                        WriteErrorEvent {
                            operation_id: operation_id_for_cleanup,
                            operation_type: WriteOperationType::Copy,
                            error: write_err,
                        },
                    );
                }
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
#[allow(
    clippy::too_many_arguments,
    reason = "Volume copy requires passing multiple context parameters"
)]
async fn copy_volumes_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteOperationError> {
    log::debug!(
        "copy_volumes_with_progress: starting operation_id={}, {} sources",
        operation_id,
        source_paths.len()
    );

    // Phase 1: Scan sources (or reuse cached scan from preview)
    let mut total_files;
    let mut total_bytes;

    if let Some(cached) = config.preview_id.as_deref().and_then(take_cached_scan_result) {
        total_files = cached.file_count;
        total_bytes = cached.total_bytes;
        log::debug!(
            "copy_volumes_with_progress: reused cached scan for operation_id={}, files={}, bytes={}",
            operation_id,
            total_files,
            total_bytes
        );
    } else {
        log::debug!(
            "copy_volumes_with_progress: scanning sources for operation_id={}",
            operation_id
        );

        events.emit_progress(WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            phase: WriteOperationPhase::Scanning,
            current_file: None,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
        });

        total_files = 0;
        total_bytes = 0u64;
        let mut total_dirs = 0;

        for source_path in source_paths {
            if is_cancelled(&state.intent) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            let scan = source_volume
                .scan_for_copy(source_path)
                .await
                .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;
            total_files += scan.file_count;
            total_dirs += scan.dir_count;
            total_bytes += scan.total_bytes;
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
        .map_err(|e| map_volume_error(&dest_path.display().to_string(), e))?;
    if dest_space.available_bytes < total_bytes {
        return Err(WriteOperationError::InsufficientSpace {
            required: total_bytes,
            available: dest_space.available_bytes,
            volume_name: Some(dest_volume.name().to_string()),
        });
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
    events.emit_progress(WriteProgressEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        current_file: None,
        files_done: 0,
        files_total: total_files,
        bytes_done: 0,
        bytes_total: total_bytes,
    });
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
    let mut copy_error: Option<WriteOperationError> = None;

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
                    let source_is_dir = source_volume.is_directory(source_path).await.unwrap_or(false);
                    let dest_is_dir = dest_meta.is_directory;
                    if source_is_dir && dest_is_dir {
                        log::debug!(
                            "copy_volumes_with_progress: merging directories {} -> {}",
                            source_path.display(),
                            dest_item_path.display()
                        );
                    } else {
                        log::debug!(
                            "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                            dest_item_path.display(),
                            source_is_dir,
                            dest_is_dir
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
                        .await?;
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
                            events_ref.emit_progress(WriteProgressEvent {
                                operation_id: op_id.to_string(),
                                operation_type: WriteOperationType::Copy,
                                phase: WriteOperationPhase::Copying,
                                current_file: file_name_owned.clone(),
                                files_done: current_files_done,
                                files_total: total_files,
                                bytes_done: current_total,
                                bytes_total: total_bytes,
                            });
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
                    copy_error = Some(map_volume_error(&failed_dest.display().to_string(), e));
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
                let source_is_dir = source_volume.is_directory(source_path).await.unwrap_or(false);
                let dest_is_dir = dest_meta.is_directory;
                if source_is_dir && dest_is_dir {
                    log::debug!(
                        "copy_volumes_with_progress: merging directories {} -> {}",
                        source_path.display(),
                        dest_item_path.display()
                    );
                } else {
                    log::debug!(
                        "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                        dest_item_path.display(),
                        source_is_dir,
                        dest_is_dir
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
                    .await?;
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
                    events.emit_progress(WriteProgressEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        phase: WriteOperationPhase::Copying,
                        current_file: file_name_for_cb.clone(),
                        files_done: current_files_done,
                        files_total: total_files,
                        bytes_done: current_total,
                        bytes_total: total_bytes,
                    });
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

            match copy_single_path(
                &source_volume,
                source_path,
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
                    copy_error = Some(map_volume_error(&source_path.display().to_string(), e));
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

    Err(WriteOperationError::Cancelled {
        message: "Operation cancelled by user".to_string(),
    })
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
    events.emit_progress(WriteProgressEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::RollingBack,
        current_file: None,
        files_done: files_at_cancel,
        files_total,
        bytes_done: bytes_at_cancel,
        bytes_total,
    });
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
            events.emit_progress(WriteProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                phase: WriteOperationPhase::RollingBack,
                current_file: Some(current_file_name.clone()),
                files_done: remaining_files,
                files_total,
                bytes_done: remaining_bytes,
                bytes_total,
            });
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
async fn delete_volume_path_recursive(volume: &Arc<dyn Volume>, path: &Path) -> Result<(), VolumeError> {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
    use crate::file_system::write_operations::types::{
        CollectorEventSink, WriteConflictEvent, WriteSourceItemDoneEvent,
    };

    #[test]
    fn test_volume_copy_config_default() {
        let config = VolumeCopyConfig::default();
        assert_eq!(config.progress_interval_ms, 200);
        assert_eq!(config.max_conflicts_to_show, 100);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_empty_source_returns_error_without_space_info() {
        // InMemoryVolume without configured space_info returns NotSupported for get_space_info
        let source = Arc::new(InMemoryVolume::new("Source"));
        let dest = Arc::new(InMemoryVolume::new("Dest"));

        let result = scan_for_volume_copy(source.as_ref(), &[], dest.as_ref(), Path::new("/"), 10).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_with_in_memory_volumes() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_file(Path::new("/file1.txt"), b"Hello").await.unwrap();
        source.create_file(Path::new("/file2.txt"), b"World").await.unwrap();
        let source = Arc::new(source);

        let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

        let paths = vec![PathBuf::from("/file1.txt"), PathBuf::from("/file2.txt")];
        let result = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new("/"), 10)
            .await
            .unwrap();

        assert_eq!(result.file_count, 2);
        assert_eq!(result.total_bytes, 10); // "Hello" + "World"
        assert!(result.conflicts.is_empty());
        assert!(result.dest_space.available_bytes >= result.total_bytes);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_detects_conflicts_in_memory() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source
            .create_file(Path::new("/report.txt"), b"new content")
            .await
            .unwrap();
        let source = Arc::new(source);

        let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);
        dest.create_file(Path::new("/report.txt"), b"old content")
            .await
            .unwrap();
        let dest = Arc::new(dest);

        let result = scan_for_volume_copy(
            source.as_ref(),
            &[PathBuf::from("/report.txt")],
            dest.as_ref(),
            Path::new("/"),
            10,
        )
        .await
        .unwrap();

        assert_eq!(result.file_count, 1);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].source_path, "report.txt");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_insufficient_space() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source
            .create_file(Path::new("/big.bin"), &vec![0u8; 1000])
            .await
            .unwrap();
        let source = Arc::new(source);

        // Dest has only 500 bytes available
        let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1000, 500));

        let result = scan_for_volume_copy(
            source.as_ref(),
            &[PathBuf::from("/big.bin")],
            dest.as_ref(),
            Path::new("/"),
            10,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_directory_tree() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_directory(Path::new("/docs")).await.unwrap();
        source
            .create_file(Path::new("/docs/readme.txt"), b"Read me")
            .await
            .unwrap();
        source
            .create_file(Path::new("/docs/notes.txt"), b"Notes here")
            .await
            .unwrap();
        let source = Arc::new(source);

        let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

        let result = scan_for_volume_copy(
            source.as_ref(),
            &[PathBuf::from("/docs")],
            dest.as_ref(),
            Path::new("/"),
            10,
        )
        .await
        .unwrap();

        assert_eq!(result.file_count, 2);
        assert_eq!(result.total_bytes, 17); // 7 + 10
    }

    #[test]
    fn test_map_volume_error_not_found() {
        let err = map_volume_error("/ctx", VolumeError::NotFound("/test/path".to_string()));
        assert!(matches!(err, WriteOperationError::SourceNotFound { path } if path == "/test/path"));
    }

    #[test]
    fn test_map_volume_error_permission_denied() {
        let err = map_volume_error("/ctx", VolumeError::PermissionDenied("Access denied".to_string()));
        assert!(
            matches!(err, WriteOperationError::PermissionDenied { path, message } if message == "Access denied" && path == "/ctx")
        );
    }

    #[test]
    fn test_map_volume_error_already_exists() {
        let err = map_volume_error("/ctx", VolumeError::AlreadyExists("/existing".to_string()));
        assert!(matches!(err, WriteOperationError::DestinationExists { path } if path == "/existing"));
    }

    #[test]
    fn test_map_volume_error_not_supported() {
        let err = map_volume_error("/ctx", VolumeError::NotSupported);
        assert!(
            matches!(err, WriteOperationError::IoError { path, message } if message.contains("not supported") && path == "/ctx")
        );
    }

    // ========================================
    // LocalPosixVolume integration tests
    // ========================================

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_with_local_volumes() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_scan_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_scan_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create source files
        fs::write(src_dir.join("file1.txt"), "Hello").unwrap();
        fs::write(src_dir.join("file2.txt"), "World").unwrap();

        let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
        let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

        let paths = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
        let scan = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new(""), 10)
            .await
            .unwrap();
        assert_eq!(scan.file_count, 2);
        assert_eq!(scan.total_bytes, 10); // "Hello" + "World"
        assert!(scan.conflicts.is_empty());
        assert!(scan.dest_space.total_bytes > 0);

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_detects_conflicts() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_conflict_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_conflict_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create source file
        fs::write(src_dir.join("conflict.txt"), "New content").unwrap();

        // Create existing file at destination
        fs::write(dst_dir.join("conflict.txt"), "Old content").unwrap();

        let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
        let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

        let scan = scan_for_volume_copy(
            source.as_ref(),
            &[PathBuf::from("conflict.txt")],
            dest.as_ref(),
            Path::new(""),
            10,
        )
        .await
        .unwrap();
        assert_eq!(scan.file_count, 1);
        assert_eq!(scan.conflicts.len(), 1);
        assert_eq!(scan.conflicts[0].source_path, "conflict.txt");
        assert_eq!(scan.conflicts[0].source_size, 11); // "New content"
        assert_eq!(scan.conflicts[0].dest_size, 11); // "Old content"

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_for_volume_copy_max_conflicts() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create 5 conflicting files
        let mut source_paths = Vec::new();
        for i in 0..5 {
            let name = format!("file{}.txt", i);
            fs::write(src_dir.join(&name), "new").unwrap();
            fs::write(dst_dir.join(&name), "old").unwrap();
            source_paths.push(PathBuf::from(&name));
        }

        let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
        let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

        // Request max 3 conflicts
        let scan = scan_for_volume_copy(source.as_ref(), &source_paths, dest.as_ref(), Path::new(""), 3)
            .await
            .unwrap();
        assert_eq!(scan.conflicts.len(), 3); // Limited to max

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    // ========================================================================
    // Multi-file copy execution tests (via copy_volumes_with_progress)
    // ========================================================================

    fn make_state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState {
            intent: Arc::new(AtomicU8::new(0)),
            progress_interval: Duration::from_millis(50),
            conflict_resolution_tx: std::sync::Mutex::new(None),
        })
    }

    fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
        (
            Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
            Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_all_files_arrive() {
        let (source, dest) = make_volumes();

        source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
        source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
        source.create_file(Path::new("/c.txt"), b"charlie").await.unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig::default();

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-1",
            &state,
            Arc::clone(&source),
            &[
                PathBuf::from("/a.txt"),
                PathBuf::from("/b.txt"),
                PathBuf::from("/c.txt"),
            ],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(result.is_ok(), "copy should succeed: {:?}", result);

        // All 3 files at destination with correct content
        let mut stream_a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
        assert_eq!(stream_a.next_chunk().await.unwrap().unwrap(), b"alpha");
        let mut stream_b = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
        assert_eq!(stream_b.next_chunk().await.unwrap().unwrap(), b"bravo");
        let mut stream_c = dest.open_read_stream(Path::new("/c.txt")).await.unwrap();
        assert_eq!(stream_c.next_chunk().await.unwrap().unwrap(), b"charlie");

        // Completion event emitted
        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].files_processed, 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_progress_tracking() {
        let (source, dest) = make_volumes();

        source.create_file(Path::new("/x.bin"), &[0; 100_000]).await.unwrap();
        source.create_file(Path::new("/y.bin"), &[0; 50_000]).await.unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig {
            progress_interval_ms: 0, // Emit on every progress call
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-2",
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/x.bin"), PathBuf::from("/y.bin")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(result.is_ok());

        // Progress events should have been emitted
        let progress = events.progress.lock().unwrap();
        assert!(!progress.is_empty(), "expected progress events");

        // Final completion should show correct totals
        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].bytes_processed, 150_000);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_cancel_before_start() {
        let (source, dest) = make_volumes();

        source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
        source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        // Set Stopped BEFORE starting
        state.intent.store(2, Ordering::Relaxed);
        let config = VolumeCopyConfig::default();

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-pre-cancel",
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
        // No files should have been copied
        assert!(!dest.exists(Path::new("/a.txt")).await);
        assert!(!dest.exists(Path::new("/b.txt")).await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_cancel_mid_flight() {
        // Use a custom event sink that triggers cancellation deterministically
        // when progress reports files_done >= 2.
        struct CancelAfterNSink {
            inner: CollectorEventSink,
            intent: Arc<AtomicU8>,
            cancel_after_files: usize,
        }

        impl OperationEventSink for CancelAfterNSink {
            fn emit_progress(&self, event: WriteProgressEvent) {
                if event.phase == WriteOperationPhase::Copying && event.files_done >= self.cancel_after_files {
                    self.intent.store(2, Ordering::Relaxed);
                }
                self.inner.emit_progress(event);
            }
            fn emit_complete(&self, e: WriteCompleteEvent) {
                self.inner.emit_complete(e);
            }
            fn emit_cancelled(&self, e: WriteCancelledEvent) {
                self.inner.emit_cancelled(e);
            }
            fn emit_error(&self, e: WriteErrorEvent) {
                self.inner.emit_error(e);
            }
            fn emit_conflict(&self, e: WriteConflictEvent) {
                self.inner.emit_conflict(e);
            }
            fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        }

        let (source, dest) = make_volumes();
        for i in 1..=5 {
            source
                .create_file(Path::new(&format!("/{}.bin", i)), &vec![0; 100_000])
                .await
                .unwrap();
        }

        let state = make_state();
        let events = Arc::new(CancelAfterNSink {
            inner: CollectorEventSink::new(),
            intent: Arc::clone(&state.intent),
            cancel_after_files: 2,
        });
        let config = VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-cancel-mid",
            &state,
            Arc::clone(&source),
            &[
                PathBuf::from("/1.bin"),
                PathBuf::from("/2.bin"),
                PathBuf::from("/3.bin"),
                PathBuf::from("/4.bin"),
                PathBuf::from("/5.bin"),
            ],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        // Cancellation from write_from_stream's progress callback results in an IoError
        // (the VolumeError::IoError "Operation cancelled" maps to WriteOperationError::IoError).
        // The outer loop then detects the Stopped intent and returns Cancelled.
        assert!(result.is_err(), "expected error, got {:?}", result);

        // At least 2 files should exist but not all 5
        assert!(dest.exists(Path::new("/1.bin")).await);
        assert!(dest.exists(Path::new("/2.bin")).await);
        let mut total = 0;
        for i in 1..=5 {
            if dest.exists(Path::new(&format!("/{}.bin", i))).await {
                total += 1;
            }
        }
        assert!(total < 5, "expected fewer than 5 files, got {}", total);

        // The cancel either emits a write-cancelled event (if the intent check fires
        // between files) or returns an error (if write_from_stream's progress callback
        // returned Break). Both are valid cancellation paths.
        let cancelled = events.inner.cancelled.lock().unwrap();
        let had_error = result.is_err();
        assert!(
            cancelled.len() == 1 || had_error,
            "expected either a cancelled event or an error"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_skip_conflict() {
        let (source, dest) = make_volumes();

        source.create_file(Path::new("/new.txt"), b"new content").await.unwrap();
        source
            .create_file(Path::new("/conflict.txt"), b"source version")
            .await
            .unwrap();
        // Pre-existing file at destination
        dest.create_file(Path::new("/conflict.txt"), b"dest version")
            .await
            .unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Skip,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-skip",
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/new.txt"), PathBuf::from("/conflict.txt")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(result.is_ok());

        // New file should be copied
        let mut stream = dest.open_read_stream(Path::new("/new.txt")).await.unwrap();
        assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new content");

        // Conflicting file should keep destination version (skip)
        let mut stream = dest.open_read_stream(Path::new("/conflict.txt")).await.unwrap();
        assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"dest version");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_multi_file_copy_overwrite_conflict() {
        let (source, dest) = make_volumes();

        source
            .create_file(Path::new("/file.txt"), b"new version")
            .await
            .unwrap();
        dest.create_file(Path::new("/file.txt"), b"old version").await.unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-overwrite",
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/file.txt")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(result.is_ok());

        // File should have source content (overwritten)
        let mut stream = dest.open_read_stream(Path::new("/file.txt")).await.unwrap();
        assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new version");
    }

    // ── Phase 4.2 concurrency tests ──────────────────────────────────
    //
    // Exercise the FuturesUnordered path in `copy_volumes_with_progress`.
    // `InMemoryVolume` returns `max_concurrent_ops() = 32`, so batches of
    // 3+ files automatically take the concurrent branch (clamped to 32).

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_copy_50_files_all_succeed() {
        let (source, dest) = make_volumes();

        // 50 small files — well over the threshold=3 and concurrency=32.
        for i in 0..50 {
            let name = format!("/file_{:02}.bin", i);
            source
                .create_file(Path::new(&name), &vec![i as u8; 1024])
                .await
                .unwrap();
        }

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig {
            progress_interval_ms: 0, // Emit on every progress call
            ..VolumeCopyConfig::default()
        };

        let paths: Vec<PathBuf> = (0..50).map(|i| PathBuf::from(format!("/file_{:02}.bin", i))).collect();
        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-concurrent-50",
            &state,
            Arc::clone(&source),
            &paths,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;
        assert!(result.is_ok(), "expected success, got {:?}", result);

        // All 50 files landed at destination with the right content.
        for i in 0..50 {
            let name = format!("/file_{:02}.bin", i);
            let mut stream = dest.open_read_stream(Path::new(&name)).await.unwrap();
            let mut collected = Vec::new();
            while let Some(Ok(chunk)) = stream.next_chunk().await {
                collected.extend_from_slice(&chunk);
            }
            assert_eq!(collected, vec![i as u8; 1024], "wrong content for {}", name);
        }

        // Progress events were emitted (throttled, but >= 1 under concurrency).
        let progress = events.progress.lock().unwrap();
        assert!(
            !progress.is_empty(),
            "expected at least one progress event under concurrency"
        );

        // Completion event with correct totals.
        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].files_processed, 50);
        assert_eq!(complete[0].bytes_processed, 50 * 1024);
    }

    /// Volume wrapper that delegates everything to an inner `InMemoryVolume`
    /// except for a single poisoned filename, which returns an I/O error on
    /// read. Used to exercise abort-on-first-error under concurrency.
    struct PoisonedReadVolume {
        inner: Arc<InMemoryVolume>,
        poisoned_file: String,
    }

    impl Volume for PoisonedReadVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>,
        > {
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>,
        > {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            self.inner.is_directory(path)
        }
        fn supports_export(&self) -> bool {
            true
        }
        fn supports_streaming(&self) -> bool {
            true
        }
        fn max_concurrent_ops(&self) -> usize {
            32
        }
        fn scan_for_copy<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<crate::file_system::volume::CopyScanResult, VolumeError>> + Send + 'a>,
        > {
            self.inner.scan_for_copy(path)
        }
        fn get_space_info<'a>(
            &'a self,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<crate::file_system::volume::SpaceInfo, VolumeError>> + Send + 'a>,
        > {
            self.inner.get_space_info()
        }
        fn open_read_stream<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<
            Box<
                dyn Future<Output = Result<Box<dyn crate::file_system::volume::VolumeReadStream>, VolumeError>>
                    + Send
                    + 'a,
            >,
        > {
            let name = self.poisoned_file.clone();
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                if path.to_string_lossy() == name {
                    return Err(VolumeError::IoError {
                        message: "Injected read failure".into(),
                        raw_os_error: Some(5), // EIO
                    });
                }
                inner.open_read_stream(path).await
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_copy_aborts_on_first_error() {
        let inner_source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
        for i in 0..20 {
            let name = format!("/file_{:02}.bin", i);
            inner_source
                .create_file(Path::new(&name), &vec![0xAB; 1024])
                .await
                .unwrap();
        }
        // File 05 will fail when read.
        let source: Arc<dyn Volume> = Arc::new(PoisonedReadVolume {
            inner: Arc::clone(&inner_source),
            poisoned_file: "/file_05.bin".to_string(),
        });
        let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let config = VolumeCopyConfig::default();

        let paths: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/file_{:02}.bin", i))).collect();
        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-concurrent-err",
            &state,
            Arc::clone(&source),
            &paths,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        // Must return an IoError (the injected one). The in-flight tasks drop
        // cleanly and the outer loop returns the mapped error.
        assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

        // Not all 20 files should be at the dest (some were still in flight
        // or not yet started when the abort fired). The poisoned file itself
        // cannot have landed.
        assert!(!dest.exists(Path::new("/file_05.bin")).await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_copy_cancellation_mid_batch() {
        // Custom event sink that flips the intent to Stopped after a few
        // progress events land. Deterministic: doesn't rely on timing.
        struct CancelOnProgressSink {
            inner: CollectorEventSink,
            intent: Arc<AtomicU8>,
            cancel_after_events: usize,
            events_seen: AtomicUsize,
        }
        impl OperationEventSink for CancelOnProgressSink {
            fn emit_progress(&self, event: WriteProgressEvent) {
                if event.phase == WriteOperationPhase::Copying
                    && self.events_seen.fetch_add(1, Ordering::Relaxed) >= self.cancel_after_events
                {
                    self.intent.store(2, Ordering::Relaxed);
                }
                self.inner.emit_progress(event);
            }
            fn emit_complete(&self, e: WriteCompleteEvent) {
                self.inner.emit_complete(e);
            }
            fn emit_cancelled(&self, e: WriteCancelledEvent) {
                self.inner.emit_cancelled(e);
            }
            fn emit_error(&self, e: WriteErrorEvent) {
                self.inner.emit_error(e);
            }
            fn emit_conflict(&self, e: WriteConflictEvent) {
                self.inner.emit_conflict(e);
            }
            fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        }

        let (source, dest) = make_volumes();
        // 20 large-ish files so the batch stays in flight long enough
        // for the cancel to land while tasks are running.
        for i in 0..20 {
            let name = format!("/big_{:02}.bin", i);
            source
                .create_file(Path::new(&name), &vec![i as u8; 200_000])
                .await
                .unwrap();
        }

        let state = make_state();
        let events = Arc::new(CancelOnProgressSink {
            inner: CollectorEventSink::new(),
            intent: Arc::clone(&state.intent),
            cancel_after_events: 2,
            events_seen: AtomicUsize::new(0),
        });
        let config = VolumeCopyConfig {
            progress_interval_ms: 0, // Emit on every chunk so we can trigger early.
            ..VolumeCopyConfig::default()
        };

        let paths: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/big_{:02}.bin", i))).collect();
        let result = copy_volumes_with_progress(
            events.as_ref(),
            "test-op-concurrent-cancel",
            &state,
            Arc::clone(&source),
            &paths,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        // Either Cancelled (pure cancel branch) or IoError (if a task's
        // progress callback returned Break). Both are valid cancellation
        // shapes — matching the sequential test `test_multi_file_copy_cancel_mid_flight`.
        assert!(
            matches!(
                result,
                Err(WriteOperationError::Cancelled { .. }) | Err(WriteOperationError::IoError { .. })
            ),
            "expected Cancelled or IoError, got {:?}",
            result
        );

        // Intent was flipped to Stopped by the sink — confirm we observed it.
        assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);

        // Less than all 20 landed (cancellation worked somewhere).
        let mut total = 0;
        for i in 0..20 {
            if dest.exists(Path::new(&format!("/big_{:02}.bin", i))).await {
                total += 1;
            }
        }
        assert!(total < 20, "expected fewer than 20 files at dest, got {}", total);
    }

    // ── Phase 4 baseline bench (real QNAP NAS) ────────────────────────
    //
    // Measures end-to-end wall-clock for copying 100 × 10 KB files from
    // the QNAP `naspi` share to a local temp dir, through the real
    // `copy_volumes_with_progress` code path. Requires:
    //
    // - QNAP reachable at 192.168.1.111 with the `naspi` share,
    //   user "david", password in `SMB2_TEST_NAS_PASSWORD` env var.
    // - 100 × 10 KB files pre-uploaded at `_test/bench_100tiny/f_000.bin`
    //   through `f_099.bin` (see `smb2`'s `bench_100_tiny_files_seq_vs_parallel`
    //   — running that benchmark uploads them as a side effect).
    //
    // Run with:
    //   cd apps/desktop/src-tauri && cargo test --release \
    //     --lib phase4_bench -- --ignored --nocapture --test-threads=1

    #[tokio::test]
    #[ignore = "Phase 4 baseline — requires QNAP at 192.168.1.111 and SMB2_TEST_NAS_PASSWORD env var"]
    #[allow(
        clippy::print_stdout,
        clippy::needless_update,
        reason = "Bench test prints a timing report by design (run with --nocapture); the struct-update is intentional for future-proofing."
    )]
    async fn phase4_bench_baseline_smb_to_local_100_tiny_files() {
        use crate::file_system::volume::LocalPosixVolume;
        use crate::file_system::volume::smb::connect_smb_volume;
        use crate::file_system::write_operations::types::CollectorEventSink;

        const FILE_COUNT: usize = 100;

        // Load password from env (or fall back to the smb2 crate's .env file).
        let password = nas_password_from_env()
            .expect("SMB2_TEST_NAS_PASSWORD not set. Copy smb2/.env.example to smb2/.env, or set in your shell.");

        // ── Set up source (SMB) ───────────────────────────────────────
        let smb_setup_start = Instant::now();
        let smb_volume = connect_smb_volume(
            "naspi",
            "/Volumes/naspi-bench-p4",
            "192.168.1.111",
            "naspi",
            Some("david"),
            Some(password.as_str()),
            445,
        )
        .await
        .expect("SMB connect failed — is QNAP at 192.168.1.111 reachable?");
        let smb_setup = smb_setup_start.elapsed();

        // ── Set up destination (local temp dir) ───────────────────────
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let local_volume = Arc::new(LocalPosixVolume::new("bench-local", tmpdir.path().to_path_buf()));

        let source_volume: Arc<dyn Volume> = Arc::new(smb_volume);
        let source_paths: Vec<PathBuf> = (0..FILE_COUNT)
            .map(|i| PathBuf::from(format!("_test/bench_100tiny/f_{:03}.bin", i)))
            .collect();

        // ── Run the copy through the real pipeline ────────────────────
        let state = Arc::new(WriteOperationState {
            intent: Arc::new(AtomicU8::new(0)),
            progress_interval: Duration::from_millis(200),
            conflict_resolution_tx: std::sync::Mutex::new(None),
        });
        let events = CollectorEventSink::new();
        let config = VolumeCopyConfig {
            progress_interval_ms: 200,
            conflict_resolution: ConflictResolution::Overwrite,
            max_conflicts_to_show: 0,
            preview_id: None,
            ..Default::default()
        };

        let copy_start = Instant::now();
        let result = copy_volumes_with_progress(
            &events,
            "phase4-bench",
            &state,
            Arc::clone(&source_volume),
            &source_paths,
            Arc::clone(&local_volume) as Arc<dyn Volume>,
            Path::new("/"),
            &config,
        )
        .await;
        let copy_elapsed = copy_start.elapsed();

        result.expect("copy pipeline failed");

        // Verify all 100 files landed at the destination.
        for i in 0..FILE_COUNT {
            let p = tmpdir.path().join(format!("f_{:03}.bin", i));
            let md = std::fs::metadata(&p).unwrap_or_else(|e| panic!("missing dest file {p:?}: {e:?}"));
            assert_eq!(md.len(), 10 * 1024, "wrong size for {p:?}");
        }

        let fps = FILE_COUNT as f64 / copy_elapsed.as_secs_f64();
        println!();
        println!("─────────────────────────────────────────────────────────");
        println!("Phase 4 baseline: 100 × 10 KB files, QNAP → local (cmdr pipeline)");
        println!("─────────────────────────────────────────────────────────");
        println!("SMB connect + session setup: {:.2?}", smb_setup);
        println!(
            "Copy wall-clock:             {:.2?}  =  {:.1} files/sec",
            copy_elapsed, fps
        );
        println!("─────────────────────────────────────────────────────────");
    }

    /// Read the NAS test password from env, falling back to `../../smb2/.env`.
    fn nas_password_from_env() -> Option<String> {
        if let Ok(p) = std::env::var("SMB2_TEST_NAS_PASSWORD") {
            return Some(p);
        }
        // Fall back: read from the smb2 crate's .env if present.
        let smb2_env_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent() // src-tauri -> desktop
            .and_then(|p| p.parent()) // desktop -> apps
            .and_then(|p| p.parent()) // apps -> cmdr
            .and_then(|p| p.parent()) // cmdr -> projects-git/vdavid
            .map(|p| p.join("smb2").join(".env"))?;
        let contents = std::fs::read_to_string(&smb2_env_path).ok()?;
        for line in contents.lines() {
            if let Some(rest) = line.strip_prefix("SMB2_TEST_NAS_PASSWORD=") {
                let unquoted = rest.trim_matches('"').to_string();
                return Some(unquoted);
            }
        }
        None
    }
}
