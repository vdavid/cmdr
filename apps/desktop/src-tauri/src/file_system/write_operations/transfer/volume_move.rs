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
use super::super::journal;
use super::super::manager;
use super::super::state::WriteOperationState;
use super::super::types::{
    OperationEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, SerialLeafProgress, TransferContext,
    TransferOutcome, build_pre_skip_set, drive_transfer_serial_async,
};
use super::volume_cleanup::delete_volume_path_recursive;
use super::volume_conflict::resolve_volume_conflict;
// The same-volume rename path lives in `volume_move_same`; the dispatcher below
// routes to its entry point.
use super::volume_move_same::move_within_same_volume;
use super::volume_preflight::{SourceHint, scan_volume_sources};
use super::volume_strategy::copy_single_path;
use super::volume_transfer_error::{WriteFailure, map_volume_error, write_error_event_from};
use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::OpKind;

// The driver-closure future-shape aliases are shared with `volume_move_same`
// (which imports them from here), so they're `pub(super)` rather than private.
/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
pub(super) type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Per-call future shape for the driver's `conflict_resolver` closure.
pub(super) type ResolveFut<'a> =
    Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for the driver's `transfer_one` closure.
pub(super) type TransferFut<'a> =
    Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

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
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
    initiator: crate::operation_log::types::Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Same volume: use native rename/move (instant for MTP)
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        return move_within_same_volume(
            events,
            source_volume_id,
            source_volume,
            source_paths,
            dest_path,
            config,
            initiator,
        )
        .await;
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
            events,
            absolute_sources,
            absolute_dest,
            write_config,
            vec![source_volume_id, dest_volume_id],
            Some(lanes),
            initiator,
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

    // The per-leaf record points inside `move_volumes_with_progress` journal under
    // these REAL volume ids (carried on the op state so the test call sites stay
    // unchanged); the deferred's open/finalize bracket uses them directly.
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(config.progress_interval_ms))
            .with_journal_volumes(source_volume_id.clone(), dest_volume_id.clone()),
    );
    let journal_source_volume_id = source_volume_id.clone();
    let journal_dest_volume_id = dest_volume_id.clone();

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

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            // Settle guard: emits `write-settled` at end of scope, after the
            // terminal event and after `on_settled`'s cache cleanup.
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Move,
                Some(source_volume_name),
            );

            // Journal the cross-volume move under the REAL volume ids (per-leaf
            // rows land inside `move_volumes_with_progress`; this brackets the op).
            journal::open_volume_op(
                &op_id,
                OpKind::Move,
                initiator,
                &journal_source_volume_id,
                Some(&journal_dest_volume_id),
                source_paths.len() as u64,
            );

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

            journal::finalize_op(
                &op_id,
                OpKind::Move,
                journal::execution_status_from_error(result.as_ref().err().map(|f| &f.error)),
            );

            match result {
                Ok(()) => {}
                Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                    log::info!("move_between_volumes: operation {} cancelled", op_id);
                }
                Err(failure) => {
                    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, failure.error);
                    events.emit_error(write_error_event_from(op_id.clone(), WriteOperationType::Move, failure));
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
pub(crate) async fn move_volumes_with_progress(
    events: Arc<dyn OperationEventSink>,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteFailure> {
    // Phase 0: Ensure the destination directory exists, creating it and any
    // missing ancestors on the dest volume (local, SMB, MTP, in-memory), so a
    // cross-volume move into a not-yet-existing folder just works on every
    // backend (parity with the local-FS `ensure_destination_dir`). Source and
    // dest are different volumes here, so the dest-inside-source guard doesn't
    // apply. A move into an already-existing dest is a no-op create.
    dest_volume
        .create_directory_all(dest_path)
        .await
        .map_err(|e| WriteFailure::from_volume(dest_path, e))?;

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

    // The operation-log journal target (real source + dest volume ids), set by the
    // `move_between_volumes` deferred. `None` in tests / the both-local shortcut,
    // where the per-leaf record point below no-ops.
    let journal_volumes = state.journal_volumes.clone();

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
            let journal_volumes = journal_volumes.clone();
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
                let journal_volumes = journal_volumes.clone();
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
                    // The copy phase's per-file ledger. Cross-volume move's own
                    // rollback reverses renames / cleans staging separately, but
                    // the operation-log capture harvests it below for the per-leaf
                    // journal rows.
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
                    // Overwrote iff the top-level file→file safe-replace fires below
                    // OR a deep-merge child replaced a dest file. Captured before
                    // `replace_after_write` is consumed; feeds move eligibility.
                    let source_overwrote = replace_after_write.is_some() || created.any_overwrote();
                    // Where a FILE source actually lands: `orig` after a safe-replace
                    // (the temp `dest_item_path` gets renamed onto it below), else
                    // `dest_item_path`. A DIR source's dest root is `dest_item_path`.
                    let landed_dest = if source_is_dir {
                        dest_item_path.clone()
                    } else {
                        replace_after_write.clone().unwrap_or_else(|| dest_item_path.clone())
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

                    // Journal the moved leaves under the REAL volume ids: a file
                    // source → one leaf (source on the source volume, dest on the
                    // dest volume); a dir source → one leaf per copied file (source
                    // rebased from the dest tail) plus the created dest dirs after
                    // them. Per-source here (not post-loop) because the throwaway
                    // ledger is drained per iteration; the created dirs land right
                    // after this source's files, so their `seq` still follows the
                    // contents within the subtree.
                    if let Some((src_vol, dst_vol)) = journal_volumes.as_ref() {
                        let files = std::mem::take(&mut *created.files.lock_ignore_poison());
                        let dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                        journal::record_volume_transfer_source(
                            &operation_id,
                            src_vol,
                            &source_path,
                            dst_vol,
                            &landed_dest,
                            source_is_dir,
                            &files,
                            (!source_is_dir).then_some(bytes as i64),
                            source_overwrote,
                        );
                        journal::record_created_dirs_on(&operation_id, dst_vol, &dirs);
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

#[cfg(test)]
#[path = "volume_move_tests.rs"]
mod tests;
