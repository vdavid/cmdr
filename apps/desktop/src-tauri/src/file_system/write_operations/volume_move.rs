//! Volume move operations.
//!
//! Move operations across different volume types:
//! - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
//! - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
//! - Cross-volume: copy to destination then delete sources

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, is_cancelled, register_operation_status, unregister_operation_status,
};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, TransferContext, TransferOutcome,
    build_pre_skip_set, drive_transfer_serial_async,
};
use super::types::{
    ConflictResolution, OperationEventSink, TauriEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent,
    WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult,
    WriteOperationType, WriteProgressEvent,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_copy::{WriteFailure, delete_volume_path_recursive, map_volume_error, write_error_event_from};
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::{Volume, VolumeError};

/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Pre-stats the subset of `source_paths` whose filenames appear in
/// `pre_known_conflicts` and returns those that are directories on
/// `source_volume`. Returns an empty set when not relevant (non-Skip
/// resolution, empty pre-known list, or no source-name matches).
///
/// Move has no scan phase, so we can't reuse a `SourceHint` map like
/// `volume_copy` does. We stat each candidate up front to keep the
/// bulk-skip prelude file-only (directories must fall through to per-iter
/// conflict resolution so their non-conflicting children still move). This
/// is one extra `get_metadata` round-trip per name-matching candidate
/// (typically few), only when the user picked Skip with a non-empty
/// pre-known conflict list. Acceptable cost for correctness.
async fn collect_known_directory_paths(
    source_volume: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    config_resolution: ConflictResolution,
    config_pre_known_conflicts: &[String],
) -> std::collections::HashSet<PathBuf> {
    use std::collections::HashSet;
    if config_resolution != ConflictResolution::Skip || config_pre_known_conflicts.is_empty() {
        return HashSet::new();
    }
    let names: HashSet<&str> = config_pre_known_conflicts.iter().map(String::as_str).collect();
    let mut out = HashSet::new();
    for p in source_paths {
        let name_matches = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| names.contains(n))
            .unwrap_or(false);
        if !name_matches {
            continue;
        }
        if let Ok(meta) = source_volume.get_metadata(p).await
            && meta.is_directory
        {
            out.insert(p.clone());
        }
    }
    out
}

/// Per-call future shape for the driver's `conflict_resolver` closure.
type ResolveFut<'a> = Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for the driver's `transfer_one` closure.
type TransferFut<'a> = Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

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

        let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
        let result: Result<(), WriteFailure> = move_volumes_with_progress(
            Arc::clone(&events),
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
///
/// Takes `Arc<dyn OperationEventSink>` (not `&dyn`) so closures passed to
/// `drive_transfer_serial_async` can `Arc::clone(&events)` into their
/// environment without borrowing outer-fn refs (the driver's
/// `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>` bound rejects
/// those — see `copy_volumes_with_progress` for the full rationale).
#[allow(
    clippy::too_many_arguments,
    reason = "Volume move requires passing multiple context parameters"
)]
pub(super) async fn move_volumes_with_progress(
    events: Arc<dyn OperationEventSink>,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteFailure> {
    let total_files = source_paths.len();

    // Move has no scan phase, so it tracks no per-source byte hints. The
    // driver's `bulk_skip_bytes` stays at 0; pre-skipped sources only bump
    // the file counter.
    //
    // Copy+delete per file: on partial failure, already-moved files exist
    // at dest, remaining files stay at source. No data loss, but the move
    // is partial.
    //
    // Bulk-skip is **file-only** (a top-level directory matching a pre-known
    // conflict means only SOME of its children collide — dropping the whole
    // subtree would lose non-conflicting files). Pre-stat the name-matching
    // candidates against `source_volume` so we can exclude directories.
    let known_directory_paths = collect_known_directory_paths(
        &source_volume,
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
    )
    .await;
    let pre_skip_paths = build_pre_skip_set(
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
        &known_directory_paths,
    );
    let bulk_skip_files = pre_skip_paths.len();

    let driver_config = DriverConfig {
        operation_type: WriteOperationType::Move,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: config.conflict_resolution,
        pre_known_conflicts: config.pre_known_conflicts.clone(),
    };

    // Per-source state shared with the driver's closures via interior
    // mutability. The transfer closure captures the originating
    // `(VolumeError, path)` on error so the post-loop branch can rebuild a
    // provider-enriched `FriendlyError`. The conflict resolver's
    // apply-to-all latch lives in a cell so the closure stays `Fn`-shaped
    // (the driver's `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send +
    // 'a>>` bound rejects `&mut` captures of outer-fn locals).
    let failure_ctx_cell: Arc<std::sync::Mutex<Option<(VolumeError, PathBuf)>>> = Arc::new(std::sync::Mutex::new(None));
    let apply_to_all_cell: Arc<std::sync::Mutex<Option<ConflictResolution>>> = Arc::new(std::sync::Mutex::new(None));

    // Closure captures: `config` and `operation_id` clone cheaply; `events`
    // is already an `Arc<dyn OperationEventSink>` on entry, so each closure
    // `Arc::clone(&events)`s into its environment. See
    // `volume_copy::copy_volumes_with_progress` for the full rationale.
    let config_owned: VolumeCopyConfig = config.clone();
    let operation_id_owned: String = operation_id.to_string();

    let outcome = drive_transfer_serial_async(
        &*events,
        state,
        operation_id,
        source_paths,
        dest_path,
        total_files,
        // Move tracks no byte totals: every progress event sets
        // `bytes_total = 0`. The driver respects whatever we pass.
        0,
        bulk_skip_files,
        0,
        &pre_skip_paths,
        &driver_config,
        {
            let dest_volume = Arc::clone(&dest_volume);
            move |p: &Path| -> FetchFut<'_> {
                let dest_volume = Arc::clone(&dest_volume);
                let p_owned = p.to_path_buf();
                Box::pin(async move {
                    dest_volume
                        .get_metadata(&p_owned)
                        .await
                        .ok()
                        .map(|m| m.size.unwrap_or(0))
                })
            }
        },
        {
            let source_volume = Arc::clone(&source_volume);
            let dest_volume = Arc::clone(&dest_volume);
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let apply_to_all = Arc::clone(&apply_to_all_cell);
            let config = config_owned.clone();
            let operation_id = operation_id_owned.clone();
            move |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let config = config.clone();
                let operation_id = operation_id.clone();
                let source_path_owned = input.source_path.to_path_buf();
                let initial_dest_owned = input.initial_dest_path.to_path_buf();
                let dest_size_hint = input.dest_size_hint;
                Box::pin(async move {
                    log::debug!(
                        "move_between_volumes: conflict detected at {}",
                        initial_dest_owned.display()
                    );
                    // Move has no scan phase, so pass `None` source-size
                    // hints; the resolver falls back to `scan_for_copy`
                    // (one MTP listing per conflicting source). The copy
                    // path is the one that supplies real hints from the
                    // cached scan.
                    let mut latched = apply_to_all.lock().unwrap().take();
                    let resolved = resolve_volume_conflict(
                        &source_volume,
                        &source_path_owned,
                        &dest_volume,
                        &initial_dest_owned,
                        &config,
                        &*events,
                        &operation_id,
                        &state,
                        &mut latched,
                        None,
                        dest_size_hint,
                    )
                    .await;
                    *apply_to_all.lock().unwrap() = latched;
                    let resolved = resolved?;
                    Ok(match resolved {
                        None => {
                            log::debug!(
                                "move_between_volumes: skipping {} due to conflict resolution",
                                source_path_owned.display()
                            );
                            ConflictDecision::Skip
                        }
                        Some(dest_path) => ConflictDecision::Proceed { dest_path },
                    })
                })
            }
        },
        {
            let source_volume = Arc::clone(&source_volume);
            let dest_volume = Arc::clone(&dest_volume);
            let progress_interval = state.progress_interval;
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let failure_ctx_cell = Arc::clone(&failure_ctx_cell);
            let operation_id = operation_id_owned.clone();
            let last_progress_time: Arc<std::sync::Mutex<Instant>> = Arc::new(std::sync::Mutex::new(Instant::now()));
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let failure_ctx_cell = Arc::clone(&failure_ctx_cell);
                let operation_id = operation_id.clone();
                let last_progress_time = Arc::clone(&last_progress_time);
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                let bytes_done_so_far = ctx.bytes_done_so_far;
                let files_done_so_far = ctx.files_done_so_far;
                Box::pin(async move {
                    // Probe is-directory once: needed both to pick the
                    // delete shape (recursive vs single) and as a hint for
                    // `copy_single_path`. Volume-move has no scan phase to
                    // cache this, so the stat is unavoidable.
                    let source_is_dir = source_volume.is_directory(&source_path).await.unwrap_or(false);

                    let no_progress = |_: u64, _: u64| -> ControlFlow<()> {
                        if is_cancelled(&state.intent) {
                            return ControlFlow::Break(());
                        }
                        ControlFlow::Continue(())
                    };
                    let bytes = match copy_single_path(
                        &source_volume,
                        &source_path,
                        source_is_dir,
                        None,
                        &dest_volume,
                        &dest_item_path,
                        &state,
                        &no_progress,
                        &|| {},
                    )
                    .await
                    {
                        Ok(b) => b,
                        Err(e) => {
                            log::warn!(
                                target: "move",
                                "move_between_volumes: copy phase failed for {} -> {}: {}",
                                source_path.display(),
                                dest_item_path.display(),
                                e
                            );
                            let mapped = map_volume_error(&source_path.display().to_string(), e.clone());
                            *failure_ctx_cell.lock().unwrap() = Some((e, source_path));
                            return Err(mapped);
                        }
                    };

                    // Delete source. `Volume::delete` is contractually for
                    // files or *empty* directories (LocalPosix uses
                    // `std::fs::remove_dir`, which fails ENOTEMPTY), so
                    // directory sources need a recursive sweep. Cross-volume
                    // copy doesn't touch the source, so its tree is intact.
                    let delete_result = if source_is_dir {
                        delete_volume_path_recursive(&source_volume, &source_path).await
                    } else {
                        source_volume.delete(&source_path).await
                    };
                    if let Err(e) = delete_result {
                        log::warn!(
                            target: "move",
                            "move_between_volumes: source delete failed for {} after successful copy: {}",
                            source_path.display(),
                            e
                        );
                        let mapped = map_volume_error(&source_path.display().to_string(), e.clone());
                        *failure_ctx_cell.lock().unwrap() = Some((e, source_path));
                        return Err(mapped);
                    }

                    // Throttled per-source progress emit. The driver's
                    // `Transferred` arm only updates counters; for the move
                    // path, no per-byte progress fires from
                    // `copy_single_path`, so without an emit here cancel-mid-batch
                    // sinks listening to `emit_progress` would never observe
                    // file-1's completion and never trip their cancel hook.
                    let mut last = last_progress_time.lock().unwrap();
                    if last.elapsed() >= progress_interval {
                        *last = Instant::now();
                        drop(last);
                        let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                        let new_files = files_done_so_far + 1;
                        let new_bytes = bytes_done_so_far + bytes;
                        state.emit_progress_via_sink(
                            &*events,
                            WriteProgressEvent::new(
                                operation_id.clone(),
                                WriteOperationType::Move,
                                WriteOperationPhase::Copying,
                                file_name,
                                new_files,
                                total_files,
                                new_bytes,
                                0,
                            ),
                        );
                    }

                    Ok(TransferOutcome::Transferred { bytes })
                })
            }
        },
    )
    .await;

    let copy_failure_ctx: Option<(VolumeError, PathBuf)> = failure_ctx_cell.lock().unwrap().take();
    let files_done = outcome.files_done;
    let bytes_done = outcome.bytes_done;

    match outcome.intent {
        PostLoopIntent::Completed => {
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
        PostLoopIntent::Cancelled => {
            // Move has no rollback (it's copy+delete-source per file);
            // cancelling leaves whatever's already at dest alone and stops
            // further work. Emit `write-cancelled` here so the FE sees the
            // cancel for the move path too (mirrors
            // `copy_volumes_with_progress`); without it the outer wrapper
            // would only log the cancel and the dialog would never close.
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_done,
                rolled_back: false,
            });
            Err(WriteFailure::synthetic(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            }))
        }
        PostLoopIntent::Failed(err) => {
            // Rebuild a `WriteFailure` with volume context if the transfer
            // closure populated it (so the FE gets a provider-enriched
            // `FriendlyError`); otherwise fall back to synthetic
            // (conflict-resolution errors and other non-`VolumeError`
            // paths).
            Err(match copy_failure_ctx {
                Some((volume_err, path)) => WriteFailure {
                    error: err,
                    volume_ctx: Some((volume_err, path)),
                },
                None => WriteFailure::synthetic(err),
            })
        }
    }
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

        let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
        let result: Result<(), WriteOperationError> = move_within_same_volume_with_progress(
            Arc::clone(&events),
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
///
/// Takes `Arc<dyn OperationEventSink>` (not `&dyn`) so closures passed to
/// `drive_transfer_serial_async` can `Arc::clone(&events)` into their
/// environment without borrowing outer-fn refs (the driver's
/// `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>` bound rejects
/// those — see `copy_volumes_with_progress` for the full rationale).
#[allow(
    clippy::too_many_arguments,
    reason = "Same-volume move requires passing multiple context parameters"
)]
pub(super) async fn move_within_same_volume_with_progress(
    events: Arc<dyn OperationEventSink>,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteOperationError> {
    let total_files = source_paths.len();

    // Bulk-skip is file-only. Same-volume move has no scan phase, so we
    // pre-stat name-matching candidates against the volume to identify
    // directories. See `move_volumes_with_progress` for the full reasoning.
    let known_directory_paths = collect_known_directory_paths(
        &volume,
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
    )
    .await;
    let pre_skip_paths = build_pre_skip_set(
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
        &known_directory_paths,
    );
    let bulk_skip_files = pre_skip_paths.len();

    let driver_config = DriverConfig {
        operation_type: WriteOperationType::Move,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: config.conflict_resolution,
        pre_known_conflicts: config.pre_known_conflicts.clone(),
    };

    // Interior mutability shapes the apply-to-all latch into a Mutex cell so
    // the conflict resolver stays `Fn`-shaped (the driver's
    // `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>` bound rejects
    // `&mut` captures of outer-fn locals).
    let apply_to_all_cell: Arc<std::sync::Mutex<Option<ConflictResolution>>> = Arc::new(std::sync::Mutex::new(None));

    let config_owned: VolumeCopyConfig = config.clone();
    let operation_id_owned: String = operation_id.to_string();

    let outcome = drive_transfer_serial_async(
        &*events,
        state,
        operation_id,
        source_paths,
        dest_path,
        total_files,
        // Same-volume move tracks no per-byte total: `bytes_total = 0` on
        // every progress event. The driver respects what we pass.
        0,
        bulk_skip_files,
        0,
        &pre_skip_paths,
        &driver_config,
        {
            let volume = Arc::clone(&volume);
            move |p: &Path| -> FetchFut<'_> {
                let volume = Arc::clone(&volume);
                let p_owned = p.to_path_buf();
                Box::pin(async move { volume.get_metadata(&p_owned).await.ok().map(|m| m.size.unwrap_or(0)) })
            }
        },
        {
            let volume = Arc::clone(&volume);
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let apply_to_all = Arc::clone(&apply_to_all_cell);
            let config = config_owned.clone();
            let operation_id = operation_id_owned.clone();
            move |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
                let volume = Arc::clone(&volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let config = config.clone();
                let operation_id = operation_id.clone();
                let source_path_owned = input.source_path.to_path_buf();
                let initial_dest_owned = input.initial_dest_path.to_path_buf();
                let dest_size_hint = input.dest_size_hint;
                Box::pin(async move {
                    log::debug!(
                        "move_within_same_volume: conflict detected at {}",
                        initial_dest_owned.display()
                    );
                    let mut latched = apply_to_all.lock().unwrap().take();
                    // Same volume on both sides; pass `&volume` twice.
                    // `None` source-size hint matches the legacy shape
                    // (same-volume move has no scan phase).
                    let resolved = resolve_volume_conflict(
                        &volume,
                        &source_path_owned,
                        &volume,
                        &initial_dest_owned,
                        &config,
                        &*events,
                        &operation_id,
                        &state,
                        &mut latched,
                        None,
                        dest_size_hint,
                    )
                    .await;
                    *apply_to_all.lock().unwrap() = latched;
                    let resolved = resolved?;
                    Ok(match resolved {
                        None => {
                            log::debug!(
                                "move_within_same_volume: skipping {} due to conflict resolution",
                                source_path_owned.display()
                            );
                            ConflictDecision::Skip
                        }
                        Some(dest_path) => ConflictDecision::Proceed { dest_path },
                    })
                })
            }
        },
        {
            let volume = Arc::clone(&volume);
            let progress_interval = state.progress_interval;
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let operation_id = operation_id_owned.clone();
            let last_progress_time: Arc<std::sync::Mutex<Instant>> = Arc::new(std::sync::Mutex::new(Instant::now()));
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let volume = Arc::clone(&volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let operation_id = operation_id.clone();
                let last_progress_time = Arc::clone(&last_progress_time);
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                let bytes_done_so_far = ctx.bytes_done_so_far;
                let files_done_so_far = ctx.files_done_so_far;
                Box::pin(async move {
                    let size = volume
                        .get_metadata(&source_path)
                        .await
                        .ok()
                        .and_then(|m| m.size)
                        .unwrap_or(0);

                    volume
                        .rename(&source_path, &dest_item_path, false)
                        .await
                        .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

                    // Throttled per-source progress emit. The driver's
                    // `Transferred` arm only updates counters; rename
                    // itself fires no per-byte progress, so without this
                    // emit cancel-mid-batch sinks listening to
                    // `emit_progress` would never observe a successful
                    // rename and never trip their cancel hook.
                    let mut last = last_progress_time.lock().unwrap();
                    if last.elapsed() >= progress_interval {
                        *last = Instant::now();
                        drop(last);
                        let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                        let new_files = files_done_so_far + 1;
                        let new_bytes = bytes_done_so_far + size;
                        state.emit_progress_via_sink(
                            &*events,
                            WriteProgressEvent::new(
                                operation_id.clone(),
                                WriteOperationType::Move,
                                WriteOperationPhase::Copying,
                                file_name,
                                new_files,
                                total_files,
                                new_bytes,
                                0,
                            ),
                        );
                    }

                    Ok(TransferOutcome::Transferred { bytes: size })
                })
            }
        },
    )
    .await;

    let files_moved = outcome.files_done;
    let bytes_moved = outcome.bytes_done;

    match outcome.intent {
        PostLoopIntent::Completed => {
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
        PostLoopIntent::Cancelled => {
            // Same-volume rename has no rollback; emit `write-cancelled`
            // so the FE closes the dialog. The outer wrapper only logs
            // the typed `Cancelled` error otherwise.
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_moved,
                rolled_back: false,
            });
            Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            })
        }
        PostLoopIntent::Failed(err) => Err(err),
    }
}

#[cfg(test)]
#[path = "volume_move_tests.rs"]
mod tests;
