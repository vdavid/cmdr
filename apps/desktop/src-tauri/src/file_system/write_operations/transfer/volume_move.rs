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
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::super::conflict::ApplyToAll;
use super::super::manager;
use super::super::state::WriteOperationState;
use super::super::types::{
    OperationEventSink, TauriEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, SerialLeafProgress, TransferContext,
    TransferOutcome, build_pre_skip_set, drive_transfer_serial_async,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_copy::{WriteFailure, delete_volume_path_recursive, map_volume_error, write_error_event_from};
use super::volume_preflight::{SourceHint, scan_volume_sources, top_level_move_hints};
use super::volume_rename_merge::{RenameMergeCtx, rename_merge_directory};
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
#[allow(
    clippy::too_many_arguments,
    reason = "each volume travels with its ID (for the busy set) plus its Arc; bundling them would just shuffle the same fields into a struct at every call site"
)]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Same volume: use native rename/move (instant for MTP)
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        return move_within_same_volume(app, source_volume_id, source_volume, source_paths, dest_path, config).await;
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

        // Pass both volume IDs so a local→USB / DMG move still marks the
        // ejectable destination busy while it runs, plus the real
        // `Volume::lane_key()`s so the manager serializes against the mount.
        let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
        return super::super::move_files_start(
            app,
            absolute_sources,
            absolute_dest,
            write_config,
            vec![source_volume_id, dest_volume_id],
            Some(lanes),
        )
        .await;
    }

    // Cross-volume: copy each file to destination, then delete source
    log::info!(
        "move_between_volumes: cross-volume move, {} -> {}, {} sources",
        source_volume.name(),
        dest_volume.name(),
        source_paths.len()
    );

    let operation_id = Uuid::new_v4().to_string();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(
        config.progress_interval_ms,
    )));

    // Occupies both volumes' lanes (source AND destination). Both volume IDs go
    // in `volume_ids` for the eject guard.
    let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
    let source_volume_name = source_volume.name().to_string();
    let summary = manager::OperationSummaryText {
        source: Some(source_volume.name().to_string()),
        destination: Some(dest_volume.name().to_string()),
    };
    let descriptor = manager::OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Move,
        lanes,
        volume_ids: vec![source_volume_id, dest_volume_id],
        summary,
    };

    let app_for_op = app.clone();
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let app = app_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            let app_for_error = app.clone();
            // Settle guard: emits `write-settled` at end of scope, after the
            // terminal event and after `on_settled`'s cache cleanup.
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                app_for_error.clone(),
                op_id.clone(),
                WriteOperationType::Move,
                Some(source_volume_name),
            );

            let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
            let result: Result<(), WriteFailure> = move_volumes_with_progress(
                Arc::clone(&events),
                &op_id,
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
                Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                    log::info!("move_between_volumes: operation {} cancelled", op_id);
                }
                Err(failure) => {
                    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, failure.error);
                    let _ = app_for_error.emit(
                        "write-error",
                        write_error_event_from(op_id.clone(), WriteOperationType::Move, failure),
                    );
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

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
        // Streaming path: `SerialLeafProgress` owns leaf-granular milestones.
        emit_per_source_milestone: false,
    };

    // Per-source state shared with the driver's closures via interior
    // mutability. The conflict resolver's apply-to-all latch lives in a cell so
    // the closure stays `Fn`-shaped (the driver's `for<'a> FnMut(...) ->
    // Pin<Box<dyn Future + Send + 'a>>` bound rejects `&mut` captures of
    // outer-fn locals).
    let apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>> = Arc::new(std::sync::Mutex::new(ApplyToAll::default()));

    // Closure captures: `config` and `operation_id` clone cheaply; `events`
    // is already an `Arc<dyn OperationEventSink>` on entry, so each closure
    // `Arc::clone(&events)`s into its environment. See
    // `volume_copy::copy_volumes_with_progress` for the full rationale.
    let config_owned: VolumeCopyConfig = config.clone();
    let operation_id_owned: String = operation_id.to_string();

    // Operation-wide leaf-file counter for the File progress bar. The driver's
    // own `files_done` counts TOP-LEVEL sources (one folder = 1), but the bar's
    // denominator is the preflight LEAF count, so the bar reads from this shared
    // counter, which `SerialLeafProgress::on_leaf_complete` bumps once per inner
    // file. Seeded with the bulk-skipped leaves the driver credits up front.
    let leaf_files_done = Arc::new(AtomicUsize::new(bulk_skip_files));

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
            let source_hints = Arc::clone(&source_hints);
            let operation_id = operation_id_owned.clone();
            let config_for_merge = config_owned.clone();
            let merge_apply_to_all = Arc::clone(&apply_to_all_cell);
            let last_progress_time: Arc<std::sync::Mutex<Instant>> = Arc::new(std::sync::Mutex::new(Instant::now()));
            let leaf_files_done = Arc::clone(&leaf_files_done);
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let source_hints = Arc::clone(&source_hints);
                let operation_id = operation_id.clone();
                let config_for_merge = config_for_merge.clone();
                let merge_apply_to_all = Arc::clone(&merge_apply_to_all);
                let last_progress_time = Arc::clone(&last_progress_time);
                let leaf_files_done = Arc::clone(&leaf_files_done);
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
                    let leaf_progress = SerialLeafProgress::new(
                        Arc::clone(&events),
                        Arc::clone(&state),
                        operation_id.clone(),
                        WriteOperationType::Move,
                        file_name.clone(),
                        bytes_done_so_far,
                        Arc::clone(&leaf_files_done),
                        total_files,
                        total_bytes,
                        Arc::clone(&last_progress_time),
                        progress_interval,
                    );
                    let on_file_progress = {
                        let leaf_progress = Arc::clone(&leaf_progress);
                        move |file_bytes_done: u64, _file_bytes_total: u64| leaf_progress.on_chunk(file_bytes_done)
                    };
                    let on_file_complete = {
                        let leaf_progress = Arc::clone(&leaf_progress);
                        move |leaf_bytes: u64| leaf_progress.on_leaf_complete(leaf_bytes)
                    };
                    // Cross-volume move's copy phase doesn't use the granular
                    // rollback ledger (move rollback reverses renames / cleans
                    // staging separately), so a throwaway ledger is fine here.
                    let created = super::volume_strategy::CreatedPaths::default();
                    // Merge context: when a source folder lands on a same-named
                    // dest folder, deep file clashes inside honor the file policy
                    // (Stop-wait, latch, conditional reduce, type mismatches) —
                    // the same granularity the top-level move already has.
                    let merge_ctx = super::volume_strategy::MergeCtx {
                        events: &*events,
                        operation_id: &operation_id,
                        config: &config_for_merge,
                        state: &state,
                        apply_to_all: &merge_apply_to_all,
                        source_hints: &source_hints,
                    };
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
                        &on_file_complete,
                        Some(&merge_ctx),
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
                            return Err(map_volume_error(&source_path.display().to_string(), e));
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
                            return Err(map_volume_error(&source_path.display().to_string(), e));
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
                        return Err(map_volume_error(&source_path.display().to_string(), e));
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
            // `err` is already the typed `WriteOperationError` the FE renders from.
            Err(WriteFailure::synthetic(err))
        }
    }
}

/// Moves files within the same volume using native `Volume::rename`.
///
/// For MTP, this uses MTP MoveObject: a single USB command per file.
/// Runs as a background task with operation registration, progress events, and cancellation.
async fn move_within_same_volume(
    app: tauri::AppHandle,
    volume_id: String,
    volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();

    log::info!(
        "move_within_same_volume: operation_id={}, volume={}, {} sources, dest={}",
        operation_id,
        volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    let progress_interval_ms = config.progress_interval_ms;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    // Same-volume move: a single lane (the volume's own).
    let lane = volume.lane_key();
    let volume_name = volume.name().to_string();
    let summary = manager::OperationSummaryText {
        source: Some(volume.name().to_string()),
        destination: Some(volume.name().to_string()),
    };
    let descriptor = manager::OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Move,
        lanes: vec![lane],
        volume_ids: vec![volume_id],
        summary,
    };

    let app_for_op = app.clone();
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let app = app_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            let app_for_error = app.clone();
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                app_for_error.clone(),
                op_id.clone(),
                WriteOperationType::Move,
                Some(volume_name),
            );

            let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
            let result: Result<(), WriteOperationError> = move_within_same_volume_with_progress(
                Arc::clone(&events),
                &op_id,
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
                Err(ref e) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                    log::info!("move_within_same_volume: operation {} cancelled", op_id);
                }
                Err(e) => {
                    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, e);
                    let _ = app_for_error.emit(
                        "write-error",
                        WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, e),
                    );
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

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
pub(crate) async fn move_within_same_volume_with_progress(
    events: Arc<dyn OperationEventSink>,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteOperationError> {
    // Dest-inside-source guard: the rename-merge makes this path recursive for
    // the first time, so moving `/A` into `/A/sub` would re-discover and
    // re-rename the content it just moved. Reject `dest == source ||
    // dest.starts_with(source)` for a directory source, mirroring
    // `copy_volumes_with_progress`'s guard. Path-prefix only — no
    // `canonicalize`, since these aren't local paths (MTP/SMB/InMemory).
    for source in source_paths {
        if (dest_path == source.as_path() || dest_path.starts_with(source))
            && matches!(volume.is_directory(source).await, Ok(true))
        {
            return Err(WriteOperationError::DestinationInsideSource {
                source: source.display().to_string(),
                destination: dest_path.display().to_string(),
            });
        }
    }

    // Top-level hints, NOT a deep pre-flight scan. A same-volume move is a
    // rename — it transfers zero bytes, so there's no Size bar to feed (the FE
    // hides it on `bytes_total == 0`). We need only the per-source
    // `is_directory` / size hints (for the conflict resolver and
    // `known_directory_paths`) and a file count, both of which a single
    // pipelined batch stat of the TOP-LEVEL items supplies — O(top-level
    // items), never a subtree walk. A cached TransferDialog preview is consumed
    // for free when present; otherwise `scan_for_copy_batch` runs one batch
    // (SMB pipelines the stats; MTP groups by parent). `files_total` is the
    // count of selected top-level items (each counts 1 when its rename / merge
    // completes); `bytes_total` is 0.
    let top_level = top_level_move_hints(&volume, source_paths, config).await?;
    let total_files = source_paths.len();
    let total_bytes = 0u64;
    let known_directory_paths = top_level.known_directory_paths();
    let source_hints: Arc<HashMap<PathBuf, SourceHint>> = Arc::new(top_level.source_hints);

    // Bulk-skip is file-only. Top-level directory matches are excluded so
    // their non-conflicting children still move.
    let pre_skip_paths = build_pre_skip_set(
        source_paths,
        config.conflict_resolution,
        &config.pre_known_conflicts,
        &known_directory_paths,
    );
    // A rename moves no bytes (`total_bytes == 0`), so bulk-skipped sources
    // credit 0 bytes too — only the file counter tracks progress on this path.
    let bulk_skip_files = pre_skip_paths.len();
    let bulk_skip_bytes = 0u64;

    let driver_config = DriverConfig {
        operation_type: WriteOperationType::Move,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: config.conflict_resolution,
        pre_known_conflicts: config.pre_known_conflicts.clone(),
        // Rename-merge: no streaming, so the driver milestone is the only emit.
        emit_per_source_milestone: true,
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
                            // A rename moves no bytes (the byte axis stays at 0),
                            // so a skip credits 0 bytes — the file counter alone
                            // tracks progress on this path.
                            ConflictDecision::Skip { bytes_accounted: 0 }
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
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let apply_to_all = Arc::clone(&apply_to_all_cell);
            let config_for_merge = config_owned.clone();
            let operation_id = operation_id_owned.clone();
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let volume = Arc::clone(&volume);
                let source_hints = Arc::clone(&source_hints);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let config_for_merge = config_for_merge.clone();
                let operation_id = operation_id.clone();
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                Box::pin(async move {
                    // Source type from the top-level hint; falls back to a stat
                    // only when no hint reached us (cached preview without
                    // per-path data). A rename moves zero bytes, so the byte
                    // axis stays at 0 throughout — no size lookup needed.
                    let hint = source_hints.get(&source_path).copied();
                    let source_is_dir = match hint {
                        Some(h) => h.is_directory,
                        None => volume.is_directory(&source_path).await.unwrap_or(false),
                    };

                    // Dir-vs-dir collision: the resolver short-circuited to a
                    // merge (no prompt for the folder), handing back the existing
                    // dest dir as the target. A flat `rename` would fail
                    // `AlreadyExists`, so walk the source level by level and
                    // rename each child into the merged destination instead.
                    // Files / cross-type clashes inside follow the file policy.
                    if source_is_dir && volume.is_directory(&dest_item_path).await.unwrap_or(false) {
                        let merge_ctx = RenameMergeCtx {
                            volume: &volume,
                            events: &*events,
                            operation_id: &operation_id,
                            config: &config_for_merge,
                            state: &state,
                            apply_to_all: &apply_to_all,
                        };
                        // Register both halves of every deep child rename with
                        // the downloads watcher's ignore set. No-ops on
                        // non-local volumes.
                        let note = |from: &Path, to: &Path| {
                            note_pending_for_local_volume(&volume, from);
                            note_pending_for_local_volume(&volume, to);
                        };
                        rename_merge_directory(&merge_ctx, &source_path, &dest_item_path, &note).await?;
                        // A rename moves no bytes: the item counts 1 (driver's
                        // milestone), the byte axis stays at 0.
                        return Ok(TransferOutcome::Transferred { bytes: 0 });
                    }

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

                    // Per-file milestone emit (bumped `files_done`) lives in the
                    // driver's `Transferred` arm. A rename moves no bytes, so the
                    // byte axis stays at 0.
                    Ok(TransferOutcome::Transferred { bytes: 0 })
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
