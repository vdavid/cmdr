//! Volume move operations.
//!
//! Move operations across different volume types:
//! - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
//! - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
//! - Cross-volume: copy to destination then delete sources

use std::collections::HashSet;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, is_cancelled, register_operation_status, unregister_operation_status,
};
use super::types::{
    ConflictResolution, OperationEventSink, TauriEventSink, VolumeCopyConfig, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
    WriteProgressEvent,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_copy::{WriteFailure, delete_volume_path_recursive, map_volume_error, write_error_event_from};
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::Volume;

/// Unified move across volume types.
///
/// Determines the best strategy based on volume relationship:
/// - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
/// - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
/// - Cross-volume: copy to destination then delete sources
///
/// Emits the standard write events (`write-progress`, `write-complete`, `write-error`).
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Same volume: use native rename/move (instant for MTP)
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        return move_within_same_volume(app, source_volume, source_paths, dest_path, config).await;
    }

    // Both local: delegate to the battle-tested move implementation
    if let (Some(src_root), Some(dest_root)) = (source_volume.local_path(), dest_volume.local_path()) {
        log::debug!(
            "move_between_volumes: both volumes are local, delegating to native move (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = dest_root.join(dest_path.strip_prefix("/").unwrap_or(&dest_path));

        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            pre_known_conflicts: config.pre_known_conflicts,
            ..Default::default()
        };

        return super::move_files_start(app, absolute_sources, absolute_dest, write_config).await;
    }

    // Cross-volume: copy each file to destination, then delete source
    log::info!(
        "move_between_volumes: cross-volume move, {} -> {}, {} sources",
        source_volume.name(),
        dest_volume.name(),
        source_paths.len()
    );

    let operation_id = Uuid::new_v4().to_string();
    let operation_id_for_spawn = operation_id.clone();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(
        config.progress_interval_ms,
    )));

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, WriteOperationType::Move);

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let events = TauriEventSink::new(app);
        let result: Result<(), WriteFailure> = move_volumes_with_progress(
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

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        use tauri::Emitter;
        match result {
            Ok(()) => {}
            // Cancellations already emit write-cancelled from inside the handler;
            // don't also emit write-error. The frontend would log a user-initiated
            // cancel as an error.
            Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                log::info!("move_between_volumes: operation {} cancelled", operation_id_for_cleanup);
            }
            Err(failure) => {
                log::warn!(
                    target: "move",
                    "move operation {} failed: {:?}",
                    operation_id_for_cleanup,
                    failure.error
                );
                let _ = app_for_error.emit(
                    "write-error",
                    write_error_event_from(operation_id_for_cleanup, WriteOperationType::Move, failure),
                );
            }
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

/// Internal cross-volume move body. Takes a sink for event emission so unit
/// tests can drive the full pipeline with a `CollectorEventSink` instead of
/// spinning up a Tauri app. The public `move_between_volumes` wraps this in
/// the `tokio::spawn` + state-cache lifecycle.
#[allow(
    clippy::too_many_arguments,
    reason = "Volume move requires passing multiple context parameters"
)]
pub(super) async fn move_volumes_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteFailure> {
    let total_files = source_paths.len();
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let progress_interval = state.progress_interval;

    // Copy+delete per file: on partial failure, already-moved files exist at dest,
    // remaining files stay at source. No data loss, but the move is partial.
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    // Bulk-skip pre-known conflicts when the user chose Skip upfront.
    // See `copy_volumes_with_progress` for the full rationale. Move-skipped
    // means "don't transfer, don't delete source" — safe for both sides.
    let pre_skip_paths: HashSet<PathBuf> =
        if config.conflict_resolution == ConflictResolution::Skip && !config.pre_known_conflicts.is_empty() {
            let names: HashSet<&str> = config.pre_known_conflicts.iter().map(String::as_str).collect();
            source_paths
                .iter()
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| names.contains(n))
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        } else {
            HashSet::new()
        };

    if !pre_skip_paths.is_empty() {
        files_done = pre_skip_paths.len();
        log::info!(
            "move_between_volumes: bulk-skipping {} pre-known conflicts before main iteration",
            files_done
        );
        // Move doesn't track bytes_total (always 0), so just bump the file counter
        // and emit one progress event so the bar jumps in one go.
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Move,
                WriteOperationPhase::Copying,
                None,
                files_done,
                total_files,
                bytes_done,
                0,
            ),
        );
    }

    for source_path in source_paths {
        if is_cancelled(&state.intent) {
            return Err(WriteFailure::synthetic(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            }));
        }

        // Pre-known conflict, already accounted upfront.
        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let file_name = source_path.file_name().ok_or_else(|| {
            WriteFailure::synthetic(WriteOperationError::IoError {
                path: source_path.display().to_string(),
                message: "Invalid source path".to_string(),
            })
        })?;

        let mut dest_item = dest_path.join(file_name);

        // Probe once and reuse for both conflict detection and copy_single_path.
        // Volume-move doesn't have a scan phase to cache this, so one stat per
        // source is unavoidable here. Still cheaper than the old code path that
        // re-statted inside `copy_single_path`.
        let source_is_dir = source_volume.is_directory(source_path).await.unwrap_or(false);

        // Check for conflict: does destination already exist? Route every conflict
        // (file-vs-file, dir-vs-dir, file-vs-dir, dir-vs-file) through
        // `resolve_volume_conflict` so the user's chosen `conflict_resolution`
        // applies. For dir-vs-dir, picking Overwrite merges (the existing
        // contents stay; same-named files inside get overwritten by the recursive
        // copy); Skip skips the whole tree; Rename lands the source under a unique
        // name; Stop emits a `write-conflict` event and awaits the user's pick.
        if let Ok(dest_meta) = dest_volume.get_metadata(&dest_item).await {
            log::debug!(
                "move_between_volumes: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                dest_item.display(),
                source_is_dir,
                dest_meta.is_directory,
            );

            // Move has no scan phase, so pass `None` size hints; the
            // conflict resolver falls back to `scan_for_copy` to populate
            // the dialog (one MTP listing per conflicting source). Copy
            // is the path that supplies real hints from the cached scan.
            let resolved = resolve_volume_conflict(
                &source_volume,
                source_path,
                &dest_volume,
                &dest_item,
                config,
                events,
                operation_id,
                state,
                &mut apply_to_all_resolution,
                None,
                dest_meta.size,
            )
            .await
            .map_err(WriteFailure::synthetic)?;

            match resolved {
                None => {
                    // Skip: don't copy and don't delete source. The file
                    // still counts as "processed" for progress purposes;
                    // without this bump the bar would stall through a
                    // run of conflicts the user chose to skip.
                    log::debug!(
                        "move_between_volumes: skipping {} due to conflict resolution",
                        source_path.display()
                    );
                    files_done += 1;
                    if last_progress_time.elapsed() >= progress_interval {
                        state.emit_progress_via_sink(
                            events,
                            WriteProgressEvent::new(
                                operation_id.to_string(),
                                WriteOperationType::Move,
                                WriteOperationPhase::Copying,
                                Some(file_name.to_string_lossy().to_string()),
                                files_done,
                                total_files,
                                bytes_done,
                                0,
                            ),
                        );
                        last_progress_time = Instant::now();
                    }
                    continue;
                }
                Some(resolved_path) => {
                    dest_item = resolved_path;
                }
            }
        }

        // Copy to destination (no per-file progress for moves: total_bytes is 0)
        let no_progress = |_: u64, _: u64| -> ControlFlow<()> {
            if is_cancelled(&state.intent) {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        };
        let bytes = copy_single_path(
            &source_volume,
            source_path,
            source_is_dir,
            // Move has no scan phase to cache a size hint. The SMB
            // compound fast-path falls through to streaming cleanly
            // when the hint is missing.
            None,
            &dest_volume,
            &dest_item,
            state,
            &no_progress,
            &|| {},
        )
        .await
        .map_err(|e| {
            log::warn!(
                target: "move",
                "move_between_volumes: copy phase failed for {} -> {}: {}",
                source_path.display(),
                dest_item.display(),
                e
            );
            WriteFailure::from_volume(source_path, e)
        })?;

        // Delete source. The Volume trait's `delete` is contractually for files
        // or *empty* directories (LocalPosix uses `std::fs::remove_dir`, which
        // fails ENOTEMPTY), so for a directory source we recurse: the cross-
        // volume copy doesn't touch the source, so its tree is intact and needs
        // a depth-first sweep. Files take the cheap one-shot path.
        //
        // Failures here leave a partial-move state (data at dest, sources still
        // at origin). Log loudly so the cause is visible in the file log;
        // without this the FE only sees a generic "io_error".
        let delete_result = if source_is_dir {
            delete_volume_path_recursive(&source_volume, source_path).await
        } else {
            source_volume.delete(source_path).await
        };
        delete_result.map_err(|e| {
            log::warn!(
                target: "move",
                "move_between_volumes: source delete failed for {} after successful copy: {}",
                source_path.display(),
                e
            );
            WriteFailure::from_volume(source_path, e)
        })?;

        files_done += 1;
        bytes_done += bytes;

        if last_progress_time.elapsed() >= progress_interval {
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Move,
                    WriteOperationPhase::Copying,
                    Some(file_name.to_string_lossy().to_string()),
                    files_done,
                    total_files,
                    bytes_done,
                    0,
                ),
            );
            last_progress_time = Instant::now();
        }
    }

    log::info!(
        "move_between_volumes: completed op={}, files={}, bytes={}",
        operation_id,
        files_done,
        bytes_done
    );

    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done,
        bytes_processed: bytes_done,
    });

    Ok(())
}

/// Moves files within the same volume using native `Volume::rename`.
///
/// For MTP, this uses MTP MoveObject: a single USB command per file.
/// Runs as a background task with operation registration, progress events, and cancellation.
async fn move_within_same_volume(
    app: tauri::AppHandle,
    volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let operation_id_for_spawn = operation_id.clone();

    log::info!(
        "move_within_same_volume: operation_id={}, volume={}, {} sources, dest={}",
        operation_id,
        volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    let progress_interval_ms = config.progress_interval_ms;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, WriteOperationType::Move);

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let events = TauriEventSink::new(app);
        let result: Result<(), WriteOperationError> = move_within_same_volume_with_progress(
            &events,
            &operation_id_for_spawn,
            &state,
            volume,
            &source_paths,
            &dest_path,
            &config,
        )
        .await;

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        use tauri::Emitter;
        match result {
            Ok(()) => {}
            // Cancellations already emit write-cancelled from inside the handler;
            // don't also emit write-error. The frontend would log a user-initiated
            // cancel as an error.
            Err(ref e) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                log::info!("move_between_volumes: operation {} cancelled", operation_id_for_cleanup);
            }
            Err(e) => {
                log::warn!(
                    target: "move",
                    "move operation {} failed: {:?}",
                    operation_id_for_cleanup,
                    e
                );
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent::new(operation_id_for_cleanup, WriteOperationType::Move, e),
                );
            }
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

/// Internal same-volume rename body. Takes a sink for event emission so unit
/// tests can drive the full pipeline with a `CollectorEventSink` instead of
/// spinning up a Tauri app.
#[allow(
    clippy::too_many_arguments,
    reason = "Same-volume move requires passing multiple context parameters"
)]
pub(super) async fn move_within_same_volume_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteOperationError> {
    let total_files = source_paths.len();
    let mut files_moved = 0usize;
    let mut bytes_moved = 0u64;
    let mut last_progress_time = Instant::now();
    let progress_interval = state.progress_interval;
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    // Bulk-skip pre-known conflicts upfront (Skip mode only). See
    // `copy_volumes_with_progress` for rationale.
    let pre_skip_paths: HashSet<PathBuf> =
        if config.conflict_resolution == ConflictResolution::Skip && !config.pre_known_conflicts.is_empty() {
            let names: HashSet<&str> = config.pre_known_conflicts.iter().map(String::as_str).collect();
            source_paths
                .iter()
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| names.contains(n))
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        } else {
            HashSet::new()
        };

    if !pre_skip_paths.is_empty() {
        files_moved = pre_skip_paths.len();
        log::info!(
            "move_within_same_volume: bulk-skipping {} pre-known conflicts before main iteration",
            files_moved
        );
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Move,
                WriteOperationPhase::Copying,
                None,
                files_moved,
                total_files,
                bytes_moved,
                0,
            ),
        );
    }

    for source_path in source_paths {
        if is_cancelled(&state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Pre-known conflict, already accounted upfront.
        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let file_name = source_path.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: source_path.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;

        let mut dest_item = dest_path.join(file_name);

        // Check for conflict: does destination already exist?
        if let Ok(dest_meta) = volume.get_metadata(&dest_item).await {
            let source_is_dir = volume.is_directory(source_path).await.unwrap_or(false);
            log::debug!(
                "move_within_same_volume: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                dest_item.display(),
                source_is_dir,
                dest_meta.is_directory,
            );

            let resolved = resolve_volume_conflict(
                &volume,
                source_path,
                &volume,
                &dest_item,
                config,
                events,
                operation_id,
                state,
                &mut apply_to_all_resolution,
                None,
                dest_meta.size,
            )
            .await?;

            match resolved {
                None => {
                    // Skip: don't move this file. Still counts toward
                    // progress so the bar doesn't stall through a run
                    // of conflicts.
                    log::debug!(
                        "move_within_same_volume: skipping {} due to conflict resolution",
                        source_path.display()
                    );
                    files_moved += 1;
                    if last_progress_time.elapsed() >= progress_interval {
                        state.emit_progress_via_sink(
                            events,
                            WriteProgressEvent::new(
                                operation_id.to_string(),
                                WriteOperationType::Move,
                                WriteOperationPhase::Copying,
                                Some(file_name.to_string_lossy().to_string()),
                                files_moved,
                                total_files,
                                bytes_moved,
                                0,
                            ),
                        );
                        last_progress_time = Instant::now();
                    }
                    continue;
                }
                Some(resolved_path) => {
                    dest_item = resolved_path;
                }
            }
        }

        let size = volume
            .get_metadata(source_path)
            .await
            .ok()
            .and_then(|m| m.size)
            .unwrap_or(0);

        volume
            .rename(source_path, &dest_item, false)
            .await
            .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

        files_moved += 1;
        bytes_moved += size;

        if last_progress_time.elapsed() >= progress_interval {
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Move,
                    WriteOperationPhase::Copying,
                    Some(file_name.to_string_lossy().to_string()),
                    files_moved,
                    total_files,
                    bytes_moved,
                    0,
                ),
            );
            last_progress_time = Instant::now();
        }
    }

    log::info!(
        "move_within_same_volume: completed op={}, files={}, bytes={}",
        operation_id,
        files_moved,
        bytes_moved
    );

    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_moved,
        bytes_processed: bytes_moved,
    });

    Ok(())
}
