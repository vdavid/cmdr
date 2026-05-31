//! Volume move operations.
//!
//! Move operations across different volume types:
//! - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
//! - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
//! - Cross-volume: copy to destination then delete sources

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::super::helpers::ApplyToAll;
use super::super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, register_operation_status, unregister_operation_status,
};
use super::super::types::{
    OperationEventSink, TauriEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, TransferContext, TransferOutcome,
    build_pre_skip_set, drive_transfer_serial_async, make_serial_per_file_progress,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_copy::{WriteFailure, delete_volume_path_recursive, map_volume_error, write_error_event_from};
use super::volume_preflight::{SourceHint, scan_volume_sources};
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Resolve `path` against `volume.local_path()` and register it with the
/// downloads watcher's ignore set. Skips silently when `volume` isn't
/// local-FS-backed: those paths can't trigger the watcher anyway.
fn note_pending_for_local_volume(volume: &Arc<dyn Volume>, path: &Path) {
    let Some(root) = volume.local_path() else {
        return;
    };
    let absolute = if path.as_os_str().is_empty() || path == Path::new(".") {
        root
    } else if path.is_absolute() {
        if path.starts_with(&root) || root == Path::new("/") {
            path.to_path_buf()
        } else {
            root.join(path.strip_prefix("/").unwrap_or(path))
        }
    } else {
        root.join(path)
    };
    crate::downloads::note_pending_write_for_cmdr(&absolute);
}

/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

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

        return super::super::move_files_start(app, absolute_sources, absolute_dest, write_config).await;
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

    let source_volume_name = source_volume.name().to_string();
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();
        // RAII settle guard: emits `write-settled` after the spawned task
        // returns. Drop runs at end of scope; the FE waits on this event
        // before closing the "Cancelling…" dialog.
        let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
            app_for_error.clone(),
            operation_id_for_cleanup.clone(),
            WriteOperationType::Move,
            Some(source_volume_name),
        );

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
                    write_error_event_from(operation_id_for_cleanup.clone(), WriteOperationType::Move, failure),
                );
            }
        }

        // Cleanup happens AFTER terminal events emit, BEFORE the settle
        // guard's Drop. See `volume_copy.rs` for the full ordering rationale.
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);
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
    // Phase 1: Preflight scan. Same helper the copy pipeline uses; we need
    // it for `total_bytes` (so the FE's Size progress bar isn't pinned at
    // zero) and for per-source `is_directory` / `size` hints (which save an
    // `is_directory` probe per source inside the copy+delete loop on MTP).
    //
    // Copy+delete per file: on partial failure, already-moved files exist at
    // dest, remaining files stay at source. No data loss, but the move is
    // partial.
    let preflight = scan_volume_sources(
        &source_volume,
        source_paths,
        config,
        operation_id,
        WriteOperationType::Move,
        state,
        &*events,
    )
    .await?;
    let total_files = preflight.total_files;
    let total_bytes = preflight.total_bytes;
    let known_directory_paths = preflight.known_directory_paths();
    let source_hints: Arc<HashMap<PathBuf, SourceHint>> = Arc::new(preflight.source_hints);

    // Bulk-skip is **file-only** (a top-level directory matching a pre-known
    // conflict means only SOME of its children collide — dropping the whole
    // subtree would lose non-conflicting files).
    let pre_skip_paths = build_pre_skip_set(
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
        &known_directory_paths,
    );
    let mut bulk_skip_files = 0usize;
    let mut bulk_skip_bytes = 0u64;
    for path in &pre_skip_paths {
        let size = source_hints
            .get(path)
            .map(|h| if h.is_directory { 0 } else { h.size })
            .unwrap_or(0);
        bulk_skip_files += 1;
        bulk_skip_bytes += size;
    }

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
    let apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>> = Arc::new(std::sync::Mutex::new(ApplyToAll::default()));

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
        total_bytes,
        bulk_skip_files,
        bulk_skip_bytes,
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
            let source_hints = Arc::clone(&source_hints);
            let config = config_owned.clone();
            let operation_id = operation_id_owned.clone();
            move |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let source_hints = Arc::clone(&source_hints);
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
                    // Reuse the cached scan hint so the conflict dialog
                    // doesn't re-list the parent dir per conflict on MTP.
                    let source_hint = source_hints.get(&source_path_owned).copied();
                    let source_size_hint = source_hint.and_then(|h| (!h.is_directory).then_some(h.size));
                    // `Some` only when the preflight produced a hint, so the
                    // resolver keeps its trait-call fallback for the no-hint case.
                    let source_is_directory_hint = source_hint.map(|h| h.is_directory);
                    let mut latched = *apply_to_all.lock_ignore_poison();
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
                        source_size_hint,
                        dest_size_hint,
                        source_is_directory_hint,
                    )
                    .await;
                    *apply_to_all.lock_ignore_poison() = latched;
                    let resolved = resolved?;
                    Ok(match resolved {
                        None => {
                            log::debug!(
                                "move_between_volumes: skipping {} due to conflict resolution",
                                source_path_owned.display()
                            );
                            // Credit the source's byte size so the Size bar
                            // matches the file counter when every source is
                            // skipped. Dirs report 0 in `source_hints` (the
                            // recursive total isn't tracked there).
                            let bytes_accounted = source_hint.map(|h| h.size).unwrap_or(0);
                            ConflictDecision::Skip { bytes_accounted }
                        }
                        Some(rc) => ConflictDecision::Proceed {
                            dest_path: rc.write_path,
                            replace_after_write: rc.replace_after_write,
                        },
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
            let source_hints = Arc::clone(&source_hints);
            let operation_id = operation_id_owned.clone();
            let last_progress_time: Arc<std::sync::Mutex<Instant>> = Arc::new(std::sync::Mutex::new(Instant::now()));
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let failure_ctx_cell = Arc::clone(&failure_ctx_cell);
                let source_hints = Arc::clone(&source_hints);
                let operation_id = operation_id.clone();
                let last_progress_time = Arc::clone(&last_progress_time);
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                // `Some(orig)` ⇒ `dest_item_path` is a temp sibling on the dest
                // volume; after the copy lands, swap it over `orig` BEFORE
                // deleting the source (a move must never delete the source if
                // the destination isn't fully in place).
                let replace_after_write = ctx.replace_after_write.map(Path::to_path_buf);
                let bytes_done_so_far = ctx.bytes_done_so_far;
                let files_done_so_far = ctx.files_done_so_far;
                Box::pin(async move {
                    // Use the cached scan hint for type + size. Falls back to
                    // a per-source `is_directory` probe if the hint is missing
                    // (cached preview without per-path data, or future
                    // backends that don't populate it).
                    let hint = source_hints.get(&source_path).copied();
                    let source_is_dir = match hint {
                        Some(h) => h.is_directory,
                        None => source_volume.is_directory(&source_path).await.unwrap_or(false),
                    };
                    let source_size_hint = hint.and_then(|h| (!h.is_directory).then_some(h.size));

                    let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                    let on_file_progress = make_serial_per_file_progress(
                        Arc::clone(&events),
                        Arc::clone(&state),
                        operation_id.clone(),
                        WriteOperationType::Move,
                        file_name.clone(),
                        files_done_so_far,
                        bytes_done_so_far,
                        total_files,
                        total_bytes,
                        Arc::clone(&last_progress_time),
                        progress_interval,
                    );
                    // Cross-volume move's copy phase doesn't use the granular
                    // rollback ledger (move rollback reverses renames / cleans
                    // staging separately), so a throwaway ledger is fine here.
                    let created = super::volume_strategy::CreatedPaths::default();
                    let bytes = match copy_single_path(
                        &source_volume,
                        &source_path,
                        source_is_dir,
                        source_size_hint,
                        &dest_volume,
                        &dest_item_path,
                        &state,
                        &created,
                        &on_file_progress,
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
                            *failure_ctx_cell.lock_ignore_poison() = Some((e, source_path));
                            return Err(mapped);
                        }
                    };

                    // Safe-replace finalize (file→file Overwrite): the temp on
                    // the dest volume now holds the complete new data. Delete
                    // the original dest and rename the temp into place. This
                    // MUST happen before deleting the source: if the finalize
                    // fails, the source is untouched, the original dest is
                    // intact, and the new data survives in the temp.
                    if let Some(orig) = replace_after_write
                        && let Err(e) =
                            super::volume_conflict::finalize_safe_replace(&dest_volume, &dest_item_path, &orig).await
                        {
                            log::warn!(
                                target: "move",
                                "move_between_volumes: safe-replace finalize failed for {} (temp {} preserved, source {} untouched): {}",
                                orig.display(),
                                dest_item_path.display(),
                                source_path.display(),
                                e
                            );
                            let mapped = map_volume_error(&source_path.display().to_string(), e.clone());
                            *failure_ctx_cell.lock_ignore_poison() = Some((e, source_path));
                            return Err(mapped);
                        }

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
                        *failure_ctx_cell.lock_ignore_poison() = Some((e, source_path));
                        return Err(mapped);
                    }

                    // Per-file milestone emit (bumped `files_done = N`,
                    // bumped `bytes_done`) now lives in the driver's
                    // `Transferred` arm — fires uniformly across copy + move
                    // and pins the FE's files-axis even when the chunked
                    // emits absorbed the throttle window. See
                    // `transfer_driver.rs::drive_transfer_serial_async`.
                    Ok(TransferOutcome::Transferred { bytes })
                })
            }
        },
    )
    .await;

    let copy_failure_ctx: Option<(VolumeError, PathBuf)> = failure_ctx_cell.lock_ignore_poison().take();
    let files_done = outcome.files_done;
    let bytes_done = outcome.bytes_done;
    let files_skipped = outcome.files_skipped;

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
                files_skipped,
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

    let volume_name = volume.name().to_string();
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();
        // Settle guard: emits `write-settled` when the task exits. See
        // `volume_copy.rs` for the ordering rationale.
        let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
            app_for_error.clone(),
            operation_id_for_cleanup.clone(),
            WriteOperationType::Move,
            Some(volume_name),
        );

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
                    WriteErrorEvent::new(operation_id_for_cleanup.clone(), WriteOperationType::Move, e),
                );
            }
        }

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);
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
    // Phase 1: Preflight scan. Same-volume rename doesn't transfer bytes
    // (MTP MoveObject is one USB call per file), but we still want to show
    // the user a Size progress bar that tracks alongside the Files bar, and
    // we want the bulk-skip prelude to know which sources are directories
    // without a per-source `is_directory` probe. The preflight call hits the
    // cached preview from TransferDialog for free in the common case;
    // otherwise it's one batch scan up front.
    let preflight = scan_volume_sources(
        &volume,
        source_paths,
        config,
        operation_id,
        WriteOperationType::Move,
        state,
        &*events,
    )
    .await
    .map_err(|f| f.error)?;
    let total_files = preflight.total_files;
    let total_bytes = preflight.total_bytes;
    let known_directory_paths = preflight.known_directory_paths();
    let source_hints: Arc<HashMap<PathBuf, SourceHint>> = Arc::new(preflight.source_hints);

    // Bulk-skip is file-only. Top-level directory matches are excluded so
    // their non-conflicting children still move.
    let pre_skip_paths = build_pre_skip_set(
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
        &known_directory_paths,
    );
    let mut bulk_skip_files = 0usize;
    let mut bulk_skip_bytes = 0u64;
    for path in &pre_skip_paths {
        let size = source_hints
            .get(path)
            .map(|h| if h.is_directory { 0 } else { h.size })
            .unwrap_or(0);
        bulk_skip_files += 1;
        bulk_skip_bytes += size;
    }

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
    let apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>> = Arc::new(std::sync::Mutex::new(ApplyToAll::default()));

    let config_owned: VolumeCopyConfig = config.clone();
    let operation_id_owned: String = operation_id.to_string();

    let outcome = drive_transfer_serial_async(
        &*events,
        state,
        operation_id,
        source_paths,
        dest_path,
        total_files,
        total_bytes,
        bulk_skip_files,
        bulk_skip_bytes,
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
            let source_hints = Arc::clone(&source_hints);
            let config = config_owned.clone();
            let operation_id = operation_id_owned.clone();
            move |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
                let volume = Arc::clone(&volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let source_hints = Arc::clone(&source_hints);
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
                    let source_hint = source_hints.get(&source_path_owned).copied();
                    let source_size_hint = source_hint.and_then(|h| (!h.is_directory).then_some(h.size));
                    // `Some` only when the preflight produced a hint, so the
                    // resolver keeps its trait-call fallback for the no-hint case.
                    let source_is_directory_hint = source_hint.map(|h| h.is_directory);
                    let mut latched = *apply_to_all.lock_ignore_poison();
                    // Same volume on both sides; pass `&volume` twice.
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
                        source_size_hint,
                        dest_size_hint,
                        source_is_directory_hint,
                    )
                    .await;
                    *apply_to_all.lock_ignore_poison() = latched;
                    let resolved = resolved?;
                    Ok(match resolved {
                        None => {
                            log::debug!(
                                "move_within_same_volume: skipping {} due to conflict resolution",
                                source_path_owned.display()
                            );
                            // Credit the source's byte size so the Size bar
                            // matches the file counter when every source is
                            // skipped.
                            let bytes_accounted = source_hint.map(|h| h.size).unwrap_or(0);
                            ConflictDecision::Skip { bytes_accounted }
                        }
                        Some(rc) => {
                            // Same-volume move uses `volume.rename` (atomic-ish,
                            // no streaming), so it keeps the legacy delete-first
                            // overwrite shape — NOT the cross-volume safe-replace
                            // temp dance. When the resolver hands back a temp +
                            // `replace_after_write` (file→file Overwrite), delete
                            // the original here and rename straight onto it. (We
                            // can't rely on `rename(force=true)`: MTP's variant
                            // doesn't delete an existing dest.) For dir-merge /
                            // Rename, `replace_after_write` is `None` and we use
                            // the resolved path as-is.
                            match rc.replace_after_write {
                                Some(orig) => {
                                    match volume.delete(&orig).await {
                                        Ok(()) => {}
                                        Err(VolumeError::NotFound(_)) => {}
                                        Err(e) => return Err(map_volume_error(&orig.display().to_string(), e)),
                                    }
                                    ConflictDecision::Proceed {
                                        dest_path: orig,
                                        replace_after_write: None,
                                    }
                                }
                                None => ConflictDecision::Proceed {
                                    dest_path: rc.write_path,
                                    replace_after_write: None,
                                },
                            }
                        }
                    })
                })
            }
        },
        {
            let volume = Arc::clone(&volume);
            let source_hints = Arc::clone(&source_hints);
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let volume = Arc::clone(&volume);
                let source_hints = Arc::clone(&source_hints);
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                Box::pin(async move {
                    // Use the cached scan hint for size. Falls back to a
                    // per-source stat if the hint is missing (cached preview
                    // without per-path data, or future backends that don't
                    // populate it).
                    let size = match source_hints.get(&source_path).copied() {
                        Some(h) if !h.is_directory => h.size,
                        Some(_) => 0,
                        None => volume
                            .get_metadata(&source_path)
                            .await
                            .ok()
                            .and_then(|m| m.size)
                            .unwrap_or(0),
                    };

                    // Register both halves with the downloads watcher's
                    // ignore set when the volume is local-FS-backed.
                    // No-ops otherwise. Same rationale as `commands/rename.rs`:
                    // suppress both the arrival and the move-out.
                    note_pending_for_local_volume(&volume, &source_path);
                    note_pending_for_local_volume(&volume, &dest_item_path);

                    volume
                        .rename(&source_path, &dest_item_path, false)
                        .await
                        .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

                    // Per-file milestone emit (bumped `files_done` /
                    // `bytes_done`) now lives in the driver's `Transferred`
                    // arm — fires uniformly across copy + move. See
                    // `transfer_driver.rs::drive_transfer_serial_async`.
                    Ok(TransferOutcome::Transferred { bytes: size })
                })
            }
        },
    )
    .await;

    let files_moved = outcome.files_done;
    let bytes_moved = outcome.bytes_done;
    let files_skipped = outcome.files_skipped;

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
                files_skipped,
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
