//! The serial driver behind a volume-to-volume copy.
//!
//! One source at a time, taken when the batch is too small to be worth spawning
//! tasks (fewer than three sources) or when a backend reports
//! `max_concurrent_ops() == 1` (MTP always does). The window-driven counterpart
//! is `volume/copy_concurrent.rs`.
//!
//! The per-iteration scaffolding — cancellation check, pre-skip, conflict
//! detect/resolve, skip accounting, the paired progress emit — belongs to the
//! shared `drive_transfer_serial_async`, which the cross-volume move and the
//! local-FS copy also use. What lives here is only what a COPY does per source:
//! dispatch the conflict resolver, and stream the item via `copy_single_path`.
//!
//! Every closure below is bounded `for<'a> FnMut(…) -> Pin<Box<dyn Future + Send + 'a>>`,
//! so its future must be valid for ANY input lifetime including `'static`. That
//! is why nothing here borrows a caller reference: each closure clones an `Arc`
//! or an owned value into its environment instead.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::super::super::conflict::ApplyToAll;
use super::super::super::journal;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{
    OperationEventSink, VolumeCopyConfig, WriteOperationError, WriteOperationPhase, WriteOperationType,
};
use super::super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, SerialLeafProgress, TransferContext,
    TransferOutcome, drive_transfer_serial_async,
};
use super::super::transfer_probe::OperationProbe;
use super::conflict::resolve_volume_conflict;
use super::preflight::SourceHint;
use super::strategy::copy_single_path;
use super::transfer_error::{WriteFailure, map_volume_error};
use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;

/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Per-call future shape for the driver's `conflict_resolver` closure.
type ResolveFut<'a> = Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for the driver's `transfer_one` closure.
type TransferFut<'a> = Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

/// Everything the serial driver needs from its caller. The `Arc` fields are
/// clones of `copy_volumes_with_progress`'s own ledgers, so the caller keeps
/// reading them after the driver returns.
pub(super) struct SerialCopy<'a> {
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: &'a str,
    pub(super) state: &'a Arc<WriteOperationState>,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) source_paths: &'a [PathBuf],
    pub(super) dest_volume: Arc<dyn Volume>,
    pub(super) dest_path: &'a Path,
    pub(super) config: &'a VolumeCopyConfig,
    pub(super) total_files: usize,
    pub(super) total_bytes: u64,
    /// What the bulk pre-skip pass already credited, so the driver's prelude
    /// emits one progress event rather than replaying them one by one.
    pub(super) bulk_skip_files: usize,
    pub(super) bulk_skip_bytes: u64,
    pub(super) pre_skip_paths: &'a HashSet<PathBuf>,
    /// Taken by value: the driver hands it to its closures as an `Arc` and the
    /// caller never reads it again.
    pub(super) source_hints: HashMap<PathBuf, SourceHint>,
    pub(super) progress_interval: Duration,
    /// Real (source, dest) volume ids for the operation-log journal. `None` for
    /// the both-local shortcut and in tests that install no journal.
    pub(super) journal_volumes: &'a Option<(String, String)>,
    pub(super) op_probe: &'a Option<Arc<OperationProbe>>,
    pub(super) apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) copied_paths: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) deep_skipped_files: Arc<AtomicUsize>,
    pub(super) deep_skipped_bytes: Arc<AtomicU64>,
}

/// What the driver leaves for the caller's post-loop bookkeeping.
pub(super) struct SerialOutcome {
    pub(super) files_done: usize,
    pub(super) bytes_done: u64,
    pub(super) files_skipped: usize,
    pub(super) bytes_skipped: u64,
    /// The half-written partial to sweep, when one source failed mid-stream.
    pub(super) last_dest_path: Option<PathBuf>,
    /// The failure, if any. The caller's post-loop owns rollback and cleanup.
    pub(super) copy_error: Option<WriteFailure>,
}

/// Runs every source through the shared serial driver, one at a time.
pub(super) async fn drive_transfer_serial(ctx: SerialCopy<'_>) -> SerialOutcome {
    let SerialCopy {
        events,
        operation_id,
        state,
        source_volume,
        source_paths,
        dest_volume,
        dest_path,
        config,
        total_files,
        total_bytes,
        bulk_skip_files,
        bulk_skip_bytes,
        pre_skip_paths,
        mut source_hints,
        progress_interval,
        journal_volumes,
        op_probe,
        apply_to_all_cell,
        copied_paths,
        created_dirs,
        deep_skipped_files,
        deep_skipped_bytes,
    } = ctx;

    let mut last_dest_path: Option<PathBuf> = None;
    let mut copy_error: Option<WriteFailure> = None;

    let driver_config = DriverConfig {
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: config.conflict_resolution,
        pre_known_conflicts: config.pre_known_conflicts.clone(),
        // Streaming path: `SerialLeafProgress` owns leaf-granular milestones.
        emit_per_source_milestone: false,
    };
    // The driver bounds its closures as
    // `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>` — the
    // returned future must be valid for any input lifetime `'a`,
    // including `'static`. Outer-fn `&` arg captures yield futures
    // bounded by those args' lifetimes, which the for-all bound
    // rejects. `config` and `operation_id` clone cheaply; `events` is
    // already an `Arc<dyn OperationEventSink>` on entry, so each
    // closure `Arc::clone(&events)`s into its environment.
    let config_owned: VolumeCopyConfig = config.clone();
    let operation_id_owned: String = operation_id.to_string();
    // Per-source mutable state shared with the driver's closures via
    // interior mutability. Avoids `&mut` captures (which would force
    // `AsyncFnMut` semantics; the driver bounds the closures as plain
    // `FnMut` returning `Pin<Box<dyn Future + Send>>`).
    let last_dest_cell: Arc<std::sync::Mutex<Option<PathBuf>>> = Arc::new(std::sync::Mutex::new(None));
    // Reuse the op-wide latch cell created above; the serial driver and the
    // deep merge share it so a "…all" choice propagates across both.
    let apply_to_all_cell = Arc::clone(&apply_to_all_cell);
    let copied_paths_for_closure = Arc::clone(&copied_paths);
    let created_dirs_for_closure = Arc::clone(&created_dirs);
    let source_hints_arc: Arc<HashMap<PathBuf, SourceHint>> = Arc::new(std::mem::take(&mut source_hints));
    // Operation-wide leaf-file counter for the File progress bar (see the
    // matching note in `volume::r#move`): the driver's `files_done` counts
    // top-level sources, but the bar's denominator is the preflight LEAF
    // count, so `SerialLeafProgress` bumps this once per inner file.
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
        pre_skip_paths,
        &driver_config,
        {
            let dest_volume = Arc::clone(&dest_volume);
            move |p: &Path| -> FetchFut<'_> {
                let dest_volume = Arc::clone(&dest_volume);
                let p_owned = p.to_path_buf();
                Box::pin(async move {
                    // `Some(_)` signals a conflict; preserve the existing
                    // "treat any successful stat as a conflict" semantics.
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
            let source_hints = Arc::clone(&source_hints_arc);
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
                    // Look up cached scan hints rather than re-probing;
                    // this wires `source_hints` into the conflict path
                    // and saves an MTP parent listing per conflicting
                    // source.
                    let source_hint = source_hints.get(&source_path_owned).copied();
                    let source_size_hint = source_hint.and_then(|h| (!h.is_directory).then_some(h.size));
                    // `Some` only when the preflight produced a hint, so the
                    // resolver keeps its trait-call fallback rather than
                    // trusting a defaulted `false`.
                    let source_is_directory_hint = source_hint.map(|h| h.is_directory);
                    log::debug!(
                        "copy_volumes_with_progress: conflict detected at {} (source_is_directory_hint={:?})",
                        initial_dest_owned.display(),
                        source_is_directory_hint,
                    );
                    // Take the apply-to-all latch into a stack local for
                    // the `&mut`-bounded resolver, then store it back.
                    // The serial driver guarantees single-threaded
                    // sequencing; the Mutex just keeps the closure
                    // `Fn`-shaped. `ApplyToAll` is `Copy`, so this is a
                    // value swap, not an option-take.
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
                                "copy_volumes_with_progress: skipping {} due to conflict resolution",
                                source_path_owned.display()
                            );
                            // Credit the source's byte size so the size
                            // progress bar matches the file counter when
                            // every source is skipped. Dirs report 0 in
                            // `source_hints` by convention (the recursive
                            // total isn't tracked there); per-file skips
                            // credit the real size.
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
            let state = Arc::clone(state);
            let events = Arc::clone(&events);
            let last_dest_cell = Arc::clone(&last_dest_cell);
            let copied_paths = Arc::clone(&copied_paths_for_closure);
            let created_dirs = Arc::clone(&created_dirs_for_closure);
            let source_hints = Arc::clone(&source_hints_arc);
            let operation_id = operation_id_owned.clone();
            let config_for_merge = config_owned.clone();
            let merge_apply_to_all = Arc::clone(&apply_to_all_cell);
            let leaf_files_done = Arc::clone(&leaf_files_done);
            let deep_skipped_files = Arc::clone(&deep_skipped_files);
            let deep_skipped_bytes = Arc::clone(&deep_skipped_bytes);
            let journal_volumes = journal_volumes.clone();
            // The serial path registers its one in-flight source too, so a
            // frozen bar during a directory copy gets the same "waiting on
            // the destination" answer a batch copy does. The counter only
            // labels rows in a dump; sources run one at a time here.
            let op_probe_serial = op_probe.clone();
            let serial_source_index = Arc::new(AtomicUsize::new(0));
            move |ctx: TransferContext<'_>| -> TransferFut<'_> {
                let op_probe_serial = op_probe_serial.clone();
                let serial_source_index = Arc::clone(&serial_source_index);
                let source_volume = Arc::clone(&source_volume);
                let dest_volume = Arc::clone(&dest_volume);
                let state = Arc::clone(&state);
                let events = Arc::clone(&events);
                let last_dest_cell = Arc::clone(&last_dest_cell);
                let copied_paths = Arc::clone(&copied_paths);
                let created_dirs = Arc::clone(&created_dirs);
                let source_hints = Arc::clone(&source_hints);
                let operation_id = operation_id.clone();
                let config_for_merge = config_for_merge.clone();
                let merge_apply_to_all = Arc::clone(&merge_apply_to_all);
                let leaf_files_done = Arc::clone(&leaf_files_done);
                let deep_skipped_files = Arc::clone(&deep_skipped_files);
                let deep_skipped_bytes = Arc::clone(&deep_skipped_bytes);
                let journal_volumes = journal_volumes.clone();
                let source_path = ctx.source_path.to_path_buf();
                let dest_item_path = ctx
                    .dest_path
                    .expect("async driver always supplies dest_path")
                    .to_path_buf();
                // `Some(orig)` ⇒ `dest_item_path` is a temp sibling; after a
                // successful write we delete `orig` and rename the temp into
                // place (safe-replace for file→file Overwrite).
                let replace_after_write = ctx.replace_after_write.map(Path::to_path_buf);
                let bytes_done_so_far = ctx.bytes_done_so_far;
                Box::pin(async move {
                    let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                    log::debug!(
                        "copy_volumes_with_progress: copying {} -> {}",
                        source_path.display(),
                        dest_item_path.display()
                    );

                    // No hint means UNKNOWN, not "file": a local scan preview
                    // completes with an empty `per_path`, so this map can be
                    // empty for a real directory source. Resolve it ONCE here —
                    // `copy_single_path` needs it to pick the streaming branch,
                    // and the ledger/cleanup branches below need the same
                    // answer, or a failed directory copy sweeps the merged dest
                    // ROOT and takes the user's dest-only files with it.
                    let hint = source_hints.get(&source_path).copied();
                    let source_is_dir = match super::strategy::resolve_source_is_directory(
                        &source_volume,
                        &source_path,
                        hint.map(|h| h.is_directory),
                    )
                    .await
                    {
                        Ok(is_dir) => is_dir,
                        Err(e) => return Err(map_volume_error(&source_path.display().to_string(), e)),
                    };
                    let source_size_hint = hint.and_then(|h| (!h.is_directory).then_some(h.size));

                    // Per-file intra-progress: a fresh per-source
                    // throttle mutex (the serial-path closure outlives
                    // a single iteration but the previous file's last-
                    // emit instant doesn't carry meaning across files).
                    let last_emit = Arc::new(std::sync::Mutex::new(Instant::now()));
                    let leaf_progress = SerialLeafProgress::new(
                        Arc::clone(&events),
                        Arc::clone(&state),
                        operation_id.clone(),
                        WriteOperationType::Copy,
                        file_name.clone(),
                        bytes_done_so_far,
                        Arc::clone(&leaf_files_done),
                        total_files,
                        total_bytes,
                        last_emit,
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

                    // Per-source rollback ledger: the files this transfer
                    // streams and the dirs it newly creates inside a
                    // directory source.
                    let created = super::strategy::CreatedPaths::default();

                    // Merge context: deep file clashes inside a merged
                    // directory honor the file policy via the resolver,
                    // sharing the op-wide apply-to-all latch with the
                    // top-level dispatch.
                    let merge_ctx = super::strategy::MergeCtx {
                        events: &*events,
                        operation_id: &operation_id,
                        config: &config_for_merge,
                        state: &state,
                        apply_to_all: &merge_apply_to_all,
                        source_hints: &source_hints,
                    };

                    *last_dest_cell.lock_ignore_poison() = Some(dest_item_path.clone());

                    // Held for this source's whole transfer; dropping it
                    // clears the row. Mirrors the concurrent path.
                    let task_probe = op_probe_serial.as_ref().map(|probe| {
                        probe.begin_task(
                            serial_source_index.fetch_add(1, Ordering::Relaxed),
                            &source_path.display().to_string(),
                            &dest_item_path.display().to_string(),
                        )
                    });
                    let probe_handle = task_probe
                        .as_ref()
                        .map(super::super::transfer_probe::TaskProbeHandle::probe);

                    let copy_fut = copy_single_path(
                        &source_volume,
                        &source_path,
                        Some(source_is_dir),
                        source_size_hint,
                        &dest_volume,
                        &dest_item_path,
                        &state,
                        &created,
                        &on_file_progress,
                        &on_file_complete,
                        Some(&merge_ctx),
                        super::strategy::staging_for(&replace_after_write),
                    );
                    // Bind this source's probe as a task-local for the whole
                    // copy, so `stream_pipe_file` and `CheckpointStream`
                    // record their phases with no signature threading.
                    let copy_result = match probe_handle {
                        Some(probe) => {
                            super::super::transfer_probe::CURRENT_TASK_PROBE
                                .scope(probe, copy_fut)
                                .await
                        }
                        None => copy_fut.await,
                    };
                    match copy_result {
                        Ok(bytes_copied) => {
                            // The write SUCCEEDED: the temp is now committed
                            // data, not a partial. Clear it from the
                            // partial-cleanup slot BEFORE finalize runs, so
                            // a finalize failure can't trigger the post-loop
                            // sweep to delete it. `finalize_safe_replace`
                            // deletes the original first, so if its rename
                            // then fails the temp is the ONLY complete copy
                            // of the new data — it must survive on disk as a
                            // recoverable `.cmdr-tmp-*` artifact.
                            *last_dest_cell.lock_ignore_poison() = None;
                            // Overwrote iff a top-level file→file safe-replace
                            // fires, OR a deep-merge child replaced a dest file.
                            // Captured before `replace_after_write` is consumed.
                            let source_overwrote = replace_after_write.is_some() || created.any_overwrote();
                            let landed_path = match replace_after_write {
                                Some(orig) => {
                                    if let Err(e) =
                                        super::conflict::finalize_safe_replace(&dest_volume, &dest_item_path, &orig)
                                            .await
                                    {
                                        return Err(map_volume_error(&source_path.display().to_string(), e));
                                    }
                                    orig
                                }
                                None => dest_item_path,
                            };
                            // For a DIRECTORY source, record the individual
                            // files and newly-created subdirs the op wrote —
                            // never the directory root — so rollback can't
                            // recursively delete a merged directory and
                            // destroy dest-only files. For a FILE source,
                            // record the landed path (the original after a
                            // safe-replace, else the dest); never the temp.
                            if source_is_dir {
                                let files = std::mem::take(&mut *created.files.lock_ignore_poison());
                                let dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                                // Journal the per-leaf rows under the REAL volume
                                // ids (dir source: rebased from `landed_path`, the
                                // dest dir root). Created dirs journal post-loop.
                                if let Some((src_vol, dst_vol)) = journal_volumes.as_ref() {
                                    journal::record_volume_transfer_source(
                                        &operation_id,
                                        src_vol,
                                        &source_path,
                                        dst_vol,
                                        &landed_path,
                                        true,
                                        &files,
                                        None,
                                        source_overwrote,
                                    );
                                }
                                copied_paths.lock_ignore_poison().extend(files);
                                created_dirs.lock_ignore_poison().extend(dirs);
                            } else {
                                if let Some((src_vol, dst_vol)) = journal_volumes.as_ref() {
                                    journal::record_volume_transfer_source(
                                        &operation_id,
                                        src_vol,
                                        &source_path,
                                        dst_vol,
                                        &landed_path,
                                        false,
                                        &[],
                                        Some(bytes_copied as i64),
                                        source_overwrote,
                                    );
                                }
                                copied_paths.lock_ignore_poison().push(landed_path);
                            }
                            // Fold this source's deep-merge skips into the op-wide
                            // tally; a source that landed with ZERO skips is fully
                            // extracted, so the out-of-zip move op may drop it.
                            let source_skipped = created.skipped_file_count();
                            deep_skipped_files.fetch_add(source_skipped, Ordering::Relaxed);
                            deep_skipped_bytes.fetch_add(created.skipped_byte_count(), Ordering::Relaxed);
                            if source_skipped == 0 {
                                events.note_source_landed_clean(&source_path);
                            }
                            Ok(TransferOutcome::Transferred { bytes: bytes_copied })
                        }
                        Err(e) => {
                            // For a DIRECTORY source interrupted mid-stream
                            // (cancel/rollback/error while still copying its
                            // children), hand the per-file ledger to the
                            // post-loop bookkeeping just like the success
                            // arm — record the individual files this op
                            // streamed and the subdirs it newly created, and
                            // CLEAR `last_dest_cell` so the post-loop cleanup
                            // never recursively deletes the dest directory
                            // ROOT. On a merge that root holds pre-existing
                            // dest-only files; recursively deleting it is
                            // silent data loss (the same class of bug the
                            // HIGH-A fix closed for the completed-copy path).
                            // The recorded per-file partials are cleaned
                            // individually (Stopped/error) or rolled back
                            // per-file (RollingBack); created dirs are pruned
                            // empty-only, so a dir still holding a sentinel
                            // survives. A FILE source keeps `last_dest_cell`
                            // pointing at its single partial dest/temp — a
                            // genuine partial that's safe to remove.
                            if source_is_dir {
                                *last_dest_cell.lock_ignore_poison() = None;
                                let files = std::mem::take(&mut *created.files.lock_ignore_poison());
                                let dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                                copied_paths.lock_ignore_poison().extend(files);
                                created_dirs.lock_ignore_poison().extend(dirs);
                            }
                            // Report the path the walker actually failed on (for a
                            // directory source, a file deep inside), ❌ never the
                            // top-level `source_path` the user selected.
                            Err(map_volume_error(&e.path.display().to_string(), e.error))
                        }
                    }
                })
            }
        },
    )
    .await;

    // Pull mutable cells back into function-scope locals so the
    // post-loop branch sees the same shape as the legacy serial loop
    // for `last_dest_path` (partial-cleanup state) and the failure
    // context (`WriteFailure` reconstruction below).
    //
    // `apply_to_all_resolution` and `source_hints` are subsumed by the
    // driver and never read post-loop — silenced with `_ =` rather than
    // assigned back to dead locals (which `#[deny(unused_assignments)]`
    // would flag).
    // ApplyToAll is `Copy + Default`; replace with default to drop the
    // latch (this is the legacy `.take()` shape preserved for symmetry).
    let _ = std::mem::take(&mut *apply_to_all_cell.lock_ignore_poison());
    if let Some(p) = last_dest_cell.lock_ignore_poison().take() {
        last_dest_path = Some(p);
    }
    let _ = source_hints_arc;

    let files_done = outcome.files_done;
    let bytes_done = outcome.bytes_done;
    let files_skipped = outcome.files_skipped;
    let bytes_skipped = outcome.bytes_skipped;
    match outcome.intent {
        PostLoopIntent::Completed | PostLoopIntent::Cancelled => {
            // Both drop into the post-loop branch below, which keys off
            // `load_intent(&state.intent)` for rollback vs cancel cleanup
            // and off `copy_error.is_none()` for the success arm.
        }
        PostLoopIntent::Failed(err) => {
            // `err` is already the typed `WriteOperationError` the FE renders from.
            copy_error = Some(WriteFailure::synthetic(err));
        }
    }

    SerialOutcome {
        files_done,
        bytes_done,
        files_skipped,
        bytes_skipped,
        last_dest_path,
        copy_error,
    }
}
