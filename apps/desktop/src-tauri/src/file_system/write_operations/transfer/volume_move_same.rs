//! Same-volume move: the native-`rename` path (a single Arc on both sides).
//!
//! A same-volume move transfers zero bytes — each top-level item is renamed
//! into the destination (MTP MoveObject is one USB command), directory clashes
//! rename-merge child by child. The cross-volume path and the move dispatcher
//! live in `volume_move`; this module holds the rename body, its background
//! task wrapper, and the per-item operation-log journaling.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use super::super::conflict::ApplyToAll;
use super::super::journal;
use super::super::manager;
use super::super::state::WriteOperationState;
use super::super::types::{
    OperationEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, TransferContext, TransferOutcome,
    build_pre_skip_set, drive_transfer_serial_async,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_move::{FetchFut, ResolveFut, TransferFut};
use super::volume_preflight::{SourceHint, top_level_move_hints};
use super::volume_rename_merge::{RenameMergeCtx, rename_merge_directory};
use super::volume_transfer_error::map_volume_error;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::OpKind;

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

/// Moves files within the same volume using native `Volume::rename`.
///
/// For MTP, this uses MTP MoveObject: a single USB command per file.
/// Runs as a background task with operation registration, progress events, and cancellation.
pub(super) async fn move_within_same_volume(
    events: Arc<dyn OperationEventSink>,
    volume_id: String,
    volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
    initiator: crate::operation_log::types::Initiator,
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

    // A same-volume move journals under the one volume id as both source and dest;
    // the per-item record point in `_with_progress` reads it off the state.
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(progress_interval_ms))
            .with_journal_volumes(volume_id.clone(), volume_id.clone()),
    );
    let journal_volume_id = volume_id.clone();

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

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Move,
                Some(volume_name),
            );

            // Journal the same-volume move under the REAL volume id. The top-level
            // rename rows + search leaves land inside `_with_progress`.
            journal::open_volume_op(
                &op_id,
                OpKind::Move,
                initiator,
                &journal_volume_id,
                Some(&journal_volume_id),
                source_paths.len() as u64,
            );

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

            journal::finalize_op(
                &op_id,
                OpKind::Move,
                journal::execution_status_from_error(result.as_ref().err()),
            );

            match result {
                Ok(()) => {}
                Err(ref e) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                    log::info!("move_within_same_volume: operation {} cancelled", op_id);
                }
                Err(e) => {
                    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, e);
                    events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, e));
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

/// Journal one moved top-level item of a same-volume move: the `rollback_unit`
/// row (one rename-back reverses the whole subtree) plus the buffered `search_only`
/// leaves. `overwrote` is the OR of the top-level file→file overwrite (recorded in
/// `overwritten_sources` by the resolver) and any deep-merge overwrite (via
/// `merge_overwrote`) — either makes the move `not_rollbackable`. No-ops without a
/// journal target.
#[allow(clippy::too_many_arguments, reason = "the fields of one journaled moved item")]
fn journal_same_volume_moved_item(
    op_id: &str,
    journal_volumes: Option<&(String, String)>,
    overwritten_sources: &std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    merge_overwrote: &std::sync::atomic::AtomicBool,
    entry_type: crate::operation_log::types::EntryType,
    source: &Path,
    dest: &Path,
    size: Option<i64>,
    buffered: Option<&super::super::journal_search::BufferedLeaves>,
) {
    let Some((src_vol, dst_vol)) = journal_volumes else {
        return;
    };
    let overwrote = merge_overwrote.load(std::sync::atomic::Ordering::Relaxed)
        || overwritten_sources.lock_ignore_poison().contains(source);
    journal::record_volume_leaf(
        op_id,
        entry_type,
        src_vol,
        source,
        Some((dst_vol, dest)),
        size,
        None,
        overwrote,
        crate::operation_log::types::ItemOutcome::Done,
    );
    if let Some(buf) = buffered {
        super::super::journal_search::persist_and_note(op_id, src_vol, source, dst_vol, Some(dest), buf);
    }
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

    // Ensure the destination directory exists before the rename. Each item is
    // renamed to `dest_path.join(name)`, so `dest_path` itself must be a real
    // directory; creating it (and any missing ancestors) recursively lets a
    // same-volume move into a brand-new folder just work, matching copy and the
    // local-FS path. A merge into an existing dest is a no-op create, so the
    // server-side-rename fast path is untouched when the dest already exists.
    volume
        .create_directory_all(dest_path)
        .await
        .map_err(|e| map_volume_error(&dest_path.display().to_string(), e))?;

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

    // Operation-log journaling for the same-volume move: the one volume id (as both
    // source and dest) plus the set of top-level sources whose conflict resolution
    // overwrote an existing dest file. The resolver runs BEFORE the transfer closure
    // (in a separate driver callback), so the overwrite verdict crosses to the
    // record point through this shared set. `None` journal target ⇒ no journaling.
    let journal_volumes = state.journal_volumes.clone();
    let overwritten_sources: Arc<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

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
            let overwritten_sources = Arc::clone(&overwritten_sources);
            move |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
                let volume = Arc::clone(&volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let source_hints = Arc::clone(&source_hints);
                let config = config.clone();
                let overwritten_sources = Arc::clone(&overwritten_sources);
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
                                    // A file→file overwrite: record it so the journal
                                    // finalizes this move `not_rollbackable` (the
                                    // replaced original is gone).
                                    overwritten_sources
                                        .lock_ignore_poison()
                                        .insert(source_path_owned.clone());
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
            let journal_volumes = journal_volumes.clone();
            let overwritten_sources = Arc::clone(&overwritten_sources);
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let volume = Arc::clone(&volume);
                let source_hints = Arc::clone(&source_hints);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let apply_to_all = Arc::clone(&apply_to_all);
                let config_for_merge = config_for_merge.clone();
                let operation_id = operation_id.clone();
                let journal_volumes = journal_volumes.clone();
                let overwritten_sources = Arc::clone(&overwritten_sources);
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

                    // Operation-log: a same-volume move is a same-FS-style move, so
                    // the top-level item is the `rollback_unit` row and the subtree's
                    // descendants are `search_only` leaves. Enumerate them from the
                    // drive index BEFORE the rename (the reconciler prunes the moved
                    // subtree on its FSEvent); persist only after the item succeeds.
                    // A volume with no index downgrades to `index_absent`. `None`
                    // journal target (tests) skips all of this.
                    let entry_type = if source_is_dir {
                        crate::operation_log::types::EntryType::Dir
                    } else {
                        crate::operation_log::types::EntryType::File
                    };
                    let row_size = hint.and_then(|h| (!h.is_directory).then_some(h.size as i64));
                    let buffered_leaves = journal_volumes.as_ref().filter(|_| source_is_dir).map(|(src_vol, _)| {
                        super::super::journal_search::enumerate_subtree_for_search(
                            src_vol,
                            &source_path,
                            super::super::journal_search::SEARCH_LEAF_CAP,
                        )
                    });
                    // Deep-merge overwrites cross to this record point through the
                    // merge ctx's flag; a top-level file→file overwrite through the
                    // resolver's `overwritten_sources` set.
                    let merge_overwrote = std::sync::atomic::AtomicBool::new(false);

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
                            overwrote: &merge_overwrote,
                        };
                        // Register both halves of every deep child rename with
                        // the downloads watcher's ignore set. No-ops on
                        // non-local volumes.
                        let note = |from: &Path, to: &Path| {
                            note_pending_for_local_volume(&volume, from);
                            note_pending_for_local_volume(&volume, to);
                        };
                        rename_merge_directory(&merge_ctx, &source_path, &dest_item_path, &note).await?;
                        journal_same_volume_moved_item(
                            &operation_id,
                            journal_volumes.as_ref(),
                            &overwritten_sources,
                            &merge_overwrote,
                            entry_type,
                            &source_path,
                            &dest_item_path,
                            row_size,
                            buffered_leaves.as_ref(),
                        );
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

                    journal_same_volume_moved_item(
                        &operation_id,
                        journal_volumes.as_ref(),
                        &overwritten_sources,
                        &merge_overwrote,
                        entry_type,
                        &source_path,
                        &dest_item_path,
                        row_size,
                        buffered_leaves.as_ref(),
                    );

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
