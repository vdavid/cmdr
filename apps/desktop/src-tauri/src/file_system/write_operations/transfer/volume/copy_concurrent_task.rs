//! One top-level source, copied end to end, as the concurrent driver's window
//! runs it.
//!
//! Everything the task needs arrives owned on [`CopyTask`], so the future is
//! `'static` and the driver's window borrows nothing. The two result payloads
//! carry back what the phase runner's cleanup and rollback have to know, and the
//! distinction their doc comments draw between a STREAM failure and a FINALIZE
//! failure is the most data-loss-sensitive line in this directory: read them
//! before touching the arms that build them.
//!
//! Conflict resolution is NOT here. It runs synchronously on the driver before a
//! task is built (`copy_concurrent_source.rs`), so the whole batch blocks on one
//! Stop prompt instead of copying into a folder the user is still being asked
//! about. Pinned by
//! `copy_concurrent_tests.rs::a_top_level_conflict_prompt_holds_the_whole_batch_still`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{VolumeCopyConfig, WriteOperationType};
use super::super::transfer_driver::make_concurrent_per_file_progress;
use super::super::transfer_probe::{CURRENT_TASK_PROBE, OperationProbe, TaskProbeHandle};
use super::preflight::SourceHint;
use super::strategy::{CreatedPaths, FileWindow, MergeCtx, copy_single_path, staging_for};
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Success payload for one concurrent copy task.
///
/// `partial_path` is the path the task pushed into `in_flight_partials` (the
/// temp sibling under safe-replace, else the dest) so the result handler can
/// remove the right entry. `recorded_path` is the top-level landed path (the
/// original after a safe-replace finalize, else the dest) — recorded for
/// rollback ONLY for a top-level file source. `created_files` / `created_dirs`
/// carry the per-file destinations and newly-created subdirectories from a
/// DIRECTORY source's recursive copy, so rollback removes exactly what the op
/// wrote into a (possibly pre-existing, merged) dest directory and never the
/// directory root.
pub(super) struct CopyTaskSuccess {
    pub(super) partial_path: PathBuf,
    pub(super) recorded_path: PathBuf,
    pub(super) source_is_dir: bool,
    pub(super) bytes: u64,
    /// Whether this source replaced any existing dest file (a top-level file→file
    /// safe-replace, or a deep-merge child overwrite). Feeds the operation-log
    /// eligibility: a copy that overwrote isn't rollbackable (the original is gone).
    pub(super) overwrote: bool,
    pub(super) created_files: Vec<PathBuf>,
    pub(super) created_dirs: Vec<PathBuf>,
    /// The top-level source this task copied, and how many children a deep
    /// merge skipped in its subtree. `skipped_count == 0` means the whole
    /// subtree landed durably, so the out-of-zip move op may drop it from the
    /// archive; any deep skip keeps it.
    pub(super) source_path: PathBuf,
    pub(super) skipped_count: usize,
    pub(super) skipped_bytes: u64,
}

/// Failure payload for one concurrent copy task.
///
/// `failed_path` is the in-flight partial entry to remove from
/// `in_flight_partials` (the temp sibling under safe-replace, else the dest
/// item path). ❌ It is NOT the path to report: it names where the partial sits
/// on the DESTINATION, which for a directory source is the dest dir root.
/// `reported_path` is the SOURCE item the walker actually failed on (a file deep
/// inside the subtree, not the top-level item the user selected), and is the only
/// one of the two a user can act on. Keep them separate. `cleanup_temp` distinguishes a STREAM failure (`true` — the
/// dest/temp is a half-written partial and must be cleaned) from a FINALIZE
/// failure after a SUCCESSFUL write (`false` — the temp holds the only complete
/// copy of the new data and MUST be left on disk).
///
/// `source_is_dir` plus `created_files` / `created_dirs` carry the per-file
/// rollback ledger for a DIRECTORY source interrupted mid-stream. Without it the
/// post-loop cleanup/rollback would fall back to recursively deleting the dest
/// directory ROOT — which on a merge destroys pre-existing dest-only files. With
/// it, the partials are cleaned per-file and the newly-created dirs pruned
/// empty-only, so a merged dir holding a sentinel survives.
pub(super) struct CopyTaskFailure {
    pub(super) failed_path: PathBuf,
    pub(super) reported_path: PathBuf,
    pub(super) error: VolumeError,
    pub(super) cleanup_temp: bool,
    pub(super) source_is_dir: bool,
    pub(super) created_files: Vec<PathBuf>,
    pub(super) created_dirs: Vec<PathBuf>,
}

/// Everything one task owns for the life of one top-level source.
///
/// Owned, not borrowed: it is what makes [`run_copy_task`]'s future `'static`,
/// so the driver's `FuturesUnordered` carries no lifetime and the phases around
/// it stay free to hold `&mut` on the window.
pub(super) struct CopyTask {
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: String,
    pub(super) state: Arc<WriteOperationState>,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) dest_volume: Arc<dyn Volume>,
    /// Per-task merge context inputs: deep file clashes inside a directory
    /// source landing on a merged dest honor the file policy, sharing the
    /// op-wide apply-to-all latch with every other task and the top-level
    /// dispatch.
    pub(super) config: VolumeCopyConfig,
    pub(super) apply_to_all: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) source_path: PathBuf,
    pub(super) source_is_dir: bool,
    pub(super) source_size_hint: Option<u64>,
    /// Where this task streams: the temp sibling when `replace_after_write` is
    /// `Some`, else the destination itself.
    pub(super) dest_path: PathBuf,
    /// `Some(orig)` ⇒ safe-replace: after a successful write, swap the temp over
    /// `orig`.
    pub(super) replace_after_write: Option<PathBuf>,
    pub(super) file_name: Option<String>,
    /// The operation's one file-copy window, shared with every merge walker.
    pub(super) window: FileWindow,
    pub(super) op_probe: Option<Arc<OperationProbe>>,
    /// This task's in-flight-table row, held for the task's whole life.
    pub(super) task_probe: Option<TaskProbeHandle>,
    pub(super) files_done: Arc<AtomicUsize>,
    pub(super) bytes_done: Arc<AtomicU64>,
    pub(super) last_progress: Arc<std::sync::Mutex<Instant>>,
    pub(super) progress_interval: Duration,
    pub(super) total_files: usize,
    pub(super) total_bytes: u64,
}

/// Streams one top-level source item end to end.
///
/// The `Err` payload's `cleanup_temp` is the whole reason this returns a struct
/// rather than a `VolumeError`: see [`CopyTaskFailure`].
pub(super) async fn run_copy_task(task: CopyTask) -> Result<CopyTaskSuccess, CopyTaskFailure> {
    let CopyTask {
        events,
        operation_id,
        state,
        source_volume,
        dest_volume,
        config,
        apply_to_all,
        source_path,
        source_is_dir,
        source_size_hint,
        dest_path,
        replace_after_write,
        file_name,
        window,
        op_probe,
        // Held for the task's whole life; dropping it (completion, abort, panic)
        // removes the row from the in-flight table.
        task_probe,
        files_done,
        bytes_done,
        last_progress,
        progress_interval,
        total_files,
        total_bytes,
    } = task;

    let probe_handle = task_probe.as_ref().map(TaskProbeHandle::probe);
    // Per-task `last_file_bytes` tracks bytes reported for the file this task is
    // copying; deltas roll up into the shared `bytes_done` so the throttle emits
    // an aggregate. Owned by the task; the helper closure carries its own Arc
    // clone, the post-call compensation reads the same counter to detect "volume
    // never invoked on_progress."
    let last_file_bytes = Arc::new(AtomicU64::new(0));
    // Per-source rollback ledger: the files this task streams and the dirs it
    // newly creates inside a directory source.
    let created = CreatedPaths::default();
    // Deep merge children are never top-level sources, so the resolver never
    // keys into per-source hints for them — an empty map is correct.
    let merge_hints: HashMap<PathBuf, SourceHint> = HashMap::new();
    // A skipped child reports no chunks, so unlike a copied one its bytes never
    // reach `bytes_done` through the progress callback: credit both axes here.
    // `note_skipped` is what keeps them off the rate.
    let on_file_skipped = {
        let bytes_done = Arc::clone(&bytes_done);
        let files_done = Arc::clone(&files_done);
        let state = Arc::clone(&state);
        move |leaf_bytes: u64| {
            bytes_done.fetch_add(leaf_bytes, Ordering::Relaxed);
            files_done.fetch_add(1, Ordering::Relaxed);
            state.note_skipped(1, leaf_bytes);
        }
    };
    let merge_ctx = MergeCtx {
        events: &*events,
        operation_id: &operation_id,
        config: &config,
        state: &state,
        apply_to_all: &apply_to_all,
        source_hints: &merge_hints,
        on_file_skipped: &on_file_skipped,
        window: window.clone(),
        op_probe,
    };
    let on_file_progress = make_concurrent_per_file_progress(
        Arc::clone(&events),
        Arc::clone(&state),
        operation_id.clone(),
        WriteOperationType::Copy,
        file_name,
        Arc::clone(&last_file_bytes),
        Arc::clone(&bytes_done),
        Arc::clone(&files_done),
        total_files,
        total_bytes,
        Arc::clone(&last_progress),
        progress_interval,
    );
    // The byte count is rolled into the aggregate by the progress callback's
    // per-chunk delta (and the post-task compensation), so this only advances the
    // leaf-file axis.
    let on_file_complete = |_leaf_bytes: u64| {
        files_done.fetch_add(1, Ordering::Relaxed);
    };
    // A top-level FILE source IS a leaf, so it takes its slot from the same
    // op-wide window a directory's children take theirs from. Otherwise a batch
    // mixing files and folders would carry `W` file tasks PLUS the walkers' `W`
    // leaves — twice the width the user's setting asked for, on one connection.
    //
    // ❌ A DIRECTORY source takes none. A walker that held a slot while waiting
    // for its own children to get theirs would deadlock the operation outright at
    // width 1.
    let _leaf_permit = if source_is_dir { None } else { window.reserve().await };
    let copy_fut = copy_single_path(
        &source_volume,
        &source_path,
        Some(source_is_dir),
        source_size_hint,
        &dest_volume,
        &dest_path,
        &state,
        &created,
        &on_file_progress,
        &on_file_complete,
        Some(&merge_ctx),
        staging_for(&replace_after_write),
    );
    // Bind this task's probe as a task-local for the whole copy, so
    // `stream_pipe_file` and `CheckpointStream` can record their phases without
    // threading a handle through every signature.
    let result = match probe_handle {
        Some(probe) => CURRENT_TASK_PROBE.scope(probe, copy_fut).await,
        None => copy_fut.await,
    };
    let created_files = std::mem::take(&mut *created.files.lock_ignore_poison());
    let created_dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
    // Deep-merge skips in this source's subtree; `0` means the whole subtree
    // landed durably (the move op may drop it from the archive).
    let task_skipped_count = created.skipped_file_count();
    let task_skipped_bytes = created.skipped_byte_count();
    // Overwrote iff a top-level file→file safe-replace fires below, OR a
    // deep-merge child replaced an existing dest file. Computed before
    // `replace_after_write` is consumed. Feeds the operation-log eligibility (a
    // copy that overwrote can't roll back).
    let task_overwrote = replace_after_write.is_some() || created.any_overwrote();
    match result {
        Ok(bytes) => {
            // If the volume didn't call the progress callback, add bytes_copied
            // to the aggregate so the total is right. Same compensation the
            // sequential path does.
            if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes > 0 {
                bytes_done.fetch_add(bytes, Ordering::Relaxed);
            }
            // Safe-replace finalize: the temp now holds the complete new data;
            // delete the original and rename the temp into place. On finalize
            // error, surface it as this file's failure with `cleanup_temp =
            // false` — the write SUCCEEDED, so the temp is committed data (the
            // only complete copy, since finalize's delete step may already have
            // removed the original). It must survive as a recoverable
            // `.cmdr-tmp-*` artifact, NOT be cleaned.
            if let Some(orig) = replace_after_write {
                if let Err(e) = super::conflict::finalize_safe_replace(&dest_volume, &dest_path, &orig).await {
                    // Finalize is file→file only (safe-replace), so there's no
                    // directory ledger to carry.
                    return Err(CopyTaskFailure {
                        failed_path: dest_path,
                        reported_path: source_path,
                        error: e,
                        cleanup_temp: false,
                        source_is_dir: false,
                        created_files,
                        created_dirs,
                    });
                }
                // Landed at `orig`; the temp `dest_path` is gone after the
                // rename. Report the temp as the partial to remove and `orig` as
                // the recorded path for rollback bookkeeping. Safe-replace is
                // file→file only, so there are no created dirs.
                return Ok(CopyTaskSuccess {
                    partial_path: dest_path,
                    recorded_path: orig,
                    source_is_dir: false,
                    bytes,
                    overwrote: task_overwrote,
                    created_files,
                    created_dirs,
                    source_path,
                    skipped_count: task_skipped_count,
                    skipped_bytes: task_skipped_bytes,
                });
            }
            Ok(CopyTaskSuccess {
                partial_path: dest_path.clone(),
                recorded_path: dest_path,
                source_is_dir,
                bytes,
                overwrote: task_overwrote,
                created_files,
                created_dirs,
                source_path,
                skipped_count: task_skipped_count,
                skipped_bytes: task_skipped_bytes,
            })
        }
        // Stream failure (incl. mid-stream cancel): the dest/temp is a
        // half-written partial → clean it (`cleanup_temp = true`). For a
        // DIRECTORY source, carry the per-file ledger so the result handler
        // records the individual partials instead of the dir root — the post-loop
        // must never recursively delete a merged dest dir and destroy
        // pre-existing dest-only files.
        Err(e) => Err(CopyTaskFailure {
            failed_path: dest_path,
            reported_path: e.path,
            error: e.error,
            cleanup_temp: true,
            source_is_dir,
            created_files,
            created_dirs,
        }),
    }
}
