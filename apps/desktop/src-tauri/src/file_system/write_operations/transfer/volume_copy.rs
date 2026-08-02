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

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::super::conflict::ApplyToAll;
use super::super::journal;
use super::super::manager;
use super::super::state::{OperationIntent, WriteOperationState, is_cancelled, load_intent, update_operation_status};
use super::super::types::{
    OperationEventSink, VolumeCopyConfig, VolumeCopyScanResult, WriteCancelledEvent, WriteCompleteEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
    WriteProgressEvent,
};
use super::dest_name_index::{DestLookup, DestNameIndex};
use super::transfer_driver::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, SerialLeafProgress, TransferContext,
    TransferOutcome, build_pre_skip_set, drive_transfer_serial_async, make_concurrent_per_file_progress,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_preflight::{SourceHint, scan_volume_sources};
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::{DirectoryCreation, SourceItemInfo, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::OpKind;

// The transfer-error plumbing (`WriteFailure`, `map_volume_error`,
// `write_error_event_from`) lives in `volume_transfer_error`, and the volume
// cleanup/rollback helpers in `volume_cleanup`. Re-exported at their original
// `volume_copy::` paths so the sibling modules that still import them from here
// (`volume_preflight`, `volume_rename_merge`, `volume_conflict`) keep resolving.
pub(in crate::file_system::write_operations) use super::volume_cleanup::delete_volume_path_recursive;
use super::volume_cleanup::volume_rollback_with_progress;
pub(crate) use super::volume_transfer_error::WriteFailure;
pub(in crate::file_system::write_operations) use super::volume_transfer_error::map_volume_error;
use super::volume_transfer_error::write_error_event_from;

/// How long a cancelled or rolled-back operation waits for its in-flight copy
/// tasks to wind down before abandoning them.
///
/// A healthy task observes the cancel within one chunk, so this only ever elapses
/// for one wedged in a backend call that is never going to return. When it does,
/// getting the user unstuck wins: the alternative is the 2026-07-31 outcome,
/// where the only way out of a stalled transfer was force-quitting the app. An
/// abandoned task's open handle is left to the server to reap (SMB does, on its
/// own idle timeout), and its staged partial is swept — or, if the wedged handle
/// blocks even that, left under a recognizable `.cmdr-tmp-*` name.
///
/// Deliberately generous: nothing healthy should ever reach it, so hitting it is
/// news, and it is logged as such.
const CANCEL_DRAIN_DEADLINE: Duration = Duration::from_secs(15);

/// The drain deadline in force, honoring a test override.
fn cancel_drain_deadline() -> Duration {
    #[cfg(test)]
    if let Some(d) = wedge_test_support::cancel_drain_override() {
        return d;
    }
    CANCEL_DRAIN_DEADLINE
}

/// Per-call future shape for the driver's `dest_meta_fetcher` closure.
type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Per-call future shape for the driver's `conflict_resolver` closure.
type ResolveFut<'a> = Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for the driver's `transfer_one` closure.
type TransferFut<'a> = Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

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
struct CopyTaskSuccess {
    partial_path: PathBuf,
    recorded_path: PathBuf,
    source_is_dir: bool,
    bytes: u64,
    /// Whether this source replaced any existing dest file (a top-level file→file
    /// safe-replace, or a deep-merge child overwrite). Feeds the operation-log
    /// eligibility: a copy that overwrote isn't rollbackable (the original is gone).
    overwrote: bool,
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
    /// The top-level source this task copied, and how many children a deep
    /// merge skipped in its subtree. `skipped_count == 0` means the whole
    /// subtree landed durably, so the out-of-zip move op may drop it from the
    /// archive; any deep skip keeps it.
    source_path: PathBuf,
    skipped_count: usize,
    skipped_bytes: u64,
}

/// Failure payload for one concurrent copy task.
///
/// `failed_path` is the in-flight partial entry to remove from
/// `in_flight_partials` (the temp sibling under safe-replace, else the dest
/// item path). `cleanup_temp` distinguishes a STREAM failure (`true` — the
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
struct CopyTaskFailure {
    failed_path: PathBuf,
    error: VolumeError,
    cleanup_temp: bool,
    source_is_dir: bool,
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
}

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
/// * `source_volume_id` - Source volume ID (recorded in the "busy volumes" set)
/// * `source_volume` - The source volume to copy from
/// * `source_paths` - Paths of files/directories to copy (relative to source volume root)
/// * `dest_volume_id` - Destination volume ID (recorded in the "busy volumes" set)
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
#[allow(
    clippy::too_many_arguments,
    reason = "each volume travels with its ID (for the busy set) plus its Arc; bundling them would just shuffle the same fields into a struct at every call site"
)]
pub async fn copy_between_volumes(
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
        // and the pre-flight conflict list so local↔local copies get the same
        // bulk-skip-under-Skip UX as cross-volume copies.
        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            pre_known_conflicts: config.pre_known_conflicts,
            ..Default::default()
        };

        // Delegate to the existing copy implementation with full cancellation
        // support. Pass both volume IDs so a local→USB / DMG copy still marks
        // the ejectable destination busy (this branch handles every both-local
        // transfer, including ones whose dest is a removable local-FS volume).
        // Pass the real `Volume::lane_key()`s so the operation manager
        // serializes against the same mount (two copies to one USB disk wait).
        let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
        return super::super::copy_files_start(
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

    let operation_id = Uuid::new_v4().to_string();
    log::info!(
        "copy_between_volumes: operation_id={}, source_volume={}, dest_volume={}, {} sources, dest={}",
        operation_id,
        source_volume.name(),
        dest_volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    // The per-leaf record points inside `copy_volumes_with_progress` journal under
    // these REAL volume ids (carried on the op state so the ~80 test call sites
    // stay unchanged); the open/finalize bracket below uses them directly.
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(config.progress_interval_ms))
            .with_journal_volumes(source_volume_id.clone(), dest_volume_id.clone()),
    );
    let journal_source_volume_id = source_volume_id.clone();
    let journal_dest_volume_id = dest_volume_id.clone();

    // The op occupies both volumes' lanes (source AND destination); the manager
    // serializes it against anything else touching either lane. Both volume IDs
    // go in `volume_ids` so the picker disables Eject for the source and
    // destination devices (MTP/SMB/USB) while the copy runs.
    let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
    let source_volume_name = source_volume.name().to_string();
    let summary = manager::OperationSummaryText {
        source: Some(source_volume.name().to_string()),
        destination: Some(dest_volume.name().to_string()),
    };
    let descriptor = manager::OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Copy,
        lanes,
        volume_ids: vec![source_volume_id, dest_volume_id],
        summary,
    };

    // Deferred start: the manager spawns this only once both lanes are free.
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
            // terminal write-* event and after `on_settled`'s cache cleanup.
            let _settled_guard = crate::file_system::write_operations::state::WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Copy,
                Some(source_volume_name),
            );

            // Journal the cross-volume copy under the REAL volume ids (the local
            // helpers bake in `"root"`). Per-leaf rows land inside
            // `copy_volumes_with_progress`; this brackets the op.
            journal::open_volume_op(
                &op_id,
                OpKind::Copy,
                initiator,
                &journal_source_volume_id,
                Some(&journal_dest_volume_id),
                source_paths.len() as u64,
            );

            let result: Result<(), WriteFailure> = copy_volumes_with_progress(
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
                OpKind::Copy,
                journal::execution_status_from_error(result.as_ref().err().map(|f| &f.error)),
            );

            match result {
                Ok(()) => {
                    // write-complete already emitted by copy_volumes_with_progress
                }
                Err(WriteFailure { ref error, .. }) if matches!(error, WriteOperationError::Cancelled { .. }) => {
                    // write-cancelled was already emitted; don't also emit
                    // write-error (the FE would log a user cancel as an error).
                    log::info!("copy_between_volumes: operation {} cancelled", op_id);
                }
                Err(failure) if failure.error.is_expected_recoverable() => {
                    // Expected, recoverable control flow (an encrypted-archive
                    // source prompting for a password), NOT a reportable failure:
                    // log at `warn` so it stays below the error-reporter's
                    // auto-report threshold. See `WriteOperationError::is_expected_recoverable`.
                    log::warn!(
                        target: "copy",
                        "copy_between_volumes: operation {} needs user input: {:?}",
                        op_id,
                        failure.error
                    );
                    events.emit_error(write_error_event_from(op_id.clone(), WriteOperationType::Copy, failure));
                }
                Err(failure) => {
                    // Toast-visible failure for cross-volume copy (Local↔SMB↔MTP).
                    crate::log_error!("copy_between_volumes: operation {} failed: {:?}", op_id, failure.error,);
                    events.emit_error(write_error_event_from(op_id.clone(), WriteOperationType::Copy, failure));
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

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
                is_directory: metadata.as_ref().map(|m| m.is_directory).unwrap_or(false),
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

/// Hard ceiling on the concurrent driver's sliding window, matching smb2's
/// `MAX_PIPELINE_WINDOW`. Measured against a QNAP over gigabit, both corpus
/// shapes plateau at a window of 12, so this is nowhere near binding on the
/// deciding target: `docs/notes/transfer-concurrency-window-bench-2026-08-02.md`.
const MAX_TRANSFER_CONCURRENCY: usize = 32;

/// How many top-level sources the concurrent driver keeps in flight for a
/// `source → dest` pair.
///
/// **A LOCAL volume's cap doesn't bound a REMOTE peer.** The two sides report
/// `max_concurrent_ops()` for completely different reasons:
/// `LocalPosixVolume`'s is `clamp(logical_cpus / 2, 4, 16)`, a CPU-core
/// heuristic guarding against spawning hundreds of tasks (the `.min(32)` below
/// guards that too); `SmbVolume`'s is the user's `network.smbConcurrency`, and
/// `MtpVolume`'s 1 is a single USB bulk transport. A plain `min()` over those
/// treats a guard-rail and a transport limit as the same kind of number, and
/// the guard-rail wins: 8 on a 16-core M3 Max, 4 on an 8-core Air, on every Mac
/// Cmdr ships to. So `network.smbConcurrency` — advertised as 1-32, default 10
/// — did nothing above 4-8, and an 8-core Mac copied 500 files to a NAS in
/// 4.700 s where its own setting asked for 3.522 s, spreads disjoint. Measured:
/// `docs/notes/transfer-concurrency-window-bench-2026-08-02.md`.
///
/// ❌ Do NOT "simplify" this back to a `min()`, and ❌ do not fix the same
/// defect by raising `LocalPosixVolume::max_concurrent_ops()` instead — that
/// number also governs local→local copies, which nothing has measured.
///
/// A remote cap always binds, in both directions, which is what keeps MTP's 1
/// routing a phone to the serial driver (`use_concurrent_path` needs
/// `concurrency > 1`).
fn transfer_concurrency(source: &dyn Volume, dest: &dyn Volume) -> usize {
    let binding_cap = |volume: &dyn Volume| (!volume.operations_are_local()).then(|| volume.max_concurrent_ops());
    // Both local (or both remote): the smaller cap is the honest answer. One of
    // each: only the remote side's cap means anything.
    let pair = match (binding_cap(source), binding_cap(dest)) {
        (Some(src), Some(dst)) => src.min(dst),
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => source.max_concurrent_ops().min(dest.max_concurrent_ops()),
    };
    pair.min(MAX_TRANSFER_CONCURRENCY)
}

/// Formats the trailing "(of which skipped N file(s), X)" annotation for
/// the completion log. Returns an empty string when nothing was skipped, so
/// the log stays terse on the happy path. Byte counts go through
/// `search::query::format_size` so a 35 GB skip doesn't read as
/// `37656214069 bytes`.
fn format_skipped_suffix(files_skipped: usize, bytes_skipped: u64) -> String {
    if files_skipped == 0 {
        return String::new();
    }
    let noun = if files_skipped == 1 { "file" } else { "files" };
    format!(
        " (of which skipped {} {}, {})",
        files_skipped,
        noun,
        crate::search::query::format_size(bytes_skipped),
    )
}

/// Bumps `files_done` and `bytes_done` for a skipped source and (throttled)
/// emits a `write-progress` event. Without this, a "Skip all" choice silently
/// runs through dozens of conflicts with the progress bar pinned at 0% — the
/// user expects the bar to reflect skipped files since the operation is in
/// fact processing them.
#[allow(
    clippy::too_many_arguments,
    reason = "Helper bundles all the per-emit context the surrounding loop already has on hand"
)]
fn account_skipped_file(
    source_path: &Path,
    source_hints: &HashMap<PathBuf, SourceHint>,
    files_done_atomic: &Arc<AtomicUsize>,
    atomic_bytes_done: &Arc<AtomicU64>,
    files_skipped_atomic: &Arc<AtomicUsize>,
    bytes_skipped_atomic: &Arc<AtomicU64>,
    last_progress_mutex: &Arc<std::sync::Mutex<Instant>>,
    progress_interval: Duration,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    total_files: usize,
    total_bytes: u64,
) {
    let hint_size = source_hints
        .get(source_path)
        .map(|h| if h.is_directory { 0 } else { h.size })
        .unwrap_or(0);
    let new_files = files_done_atomic.fetch_add(1, Ordering::Relaxed) + 1;
    let new_bytes = atomic_bytes_done.fetch_add(hint_size, Ordering::Relaxed) + hint_size;
    files_skipped_atomic.fetch_add(1, Ordering::Relaxed);
    bytes_skipped_atomic.fetch_add(hint_size, Ordering::Relaxed);

    let mut last = last_progress_mutex.lock_ignore_poison();
    if last.elapsed() >= progress_interval {
        *last = Instant::now();
        drop(last);
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::Copying,
                source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                new_files,
                total_files,
                new_bytes,
                total_bytes,
            ),
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::Copying,
            source_path.file_name().map(|n| n.to_string_lossy().to_string()),
            new_files,
            total_files,
            new_bytes,
            total_bytes,
        );
    }
}

/// Internal function that performs the actual copy with progress reporting.
///
/// Exposed as `pub(crate)` under `cfg(test)` so integration tests in sibling
/// modules (for example the SMB concurrent-copy cross-contamination test in
/// `volume/backends/smb/`) can drive the real copy pipeline with a
/// `CollectorEventSink` instead of spinning up a full Tauri app. In
/// production, the only caller is `copy_between_volumes` in this file.
///
/// Takes `Arc<dyn OperationEventSink>` (not `&dyn`) because closures passed
/// to `drive_transfer_serial_async` are bounded
/// `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>` — their returned
/// futures must be valid for any input lifetime including `'static`, so the
/// closures can't borrow outer-fn `&` args. `Arc::clone(&events)` into each
/// closure is the clean way out; the caller and tests already wrap the sink
/// in an Arc so the boundary is a no-op.
#[allow(
    clippy::too_many_arguments,
    reason = "Volume copy requires passing multiple context parameters"
)]
pub(crate) async fn copy_volumes_with_progress(
    events: Arc<dyn OperationEventSink>,
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

    // The operation-log journal target (real source + dest volume ids), set by the
    // `copy_between_volumes` deferred. `None` for the both-local shortcut (journals
    // via `copy_files_start`) and in tests that don't install a journal — the
    // per-leaf record points below then no-op.
    let journal_volumes = state.journal_volumes.clone();

    // Phase 0: Reject copying a directory into its own descendant on the SAME
    // volume. `copy_directory_streaming` re-lists each subdirectory live, so a
    // dest inside the source subtree (e.g. copy `/A` into `/A/sub` on one
    // share/device) would re-discover and re-copy the files it just wrote —
    // unbounded recursion that grows the tree until the volume fills (or the
    // streaming copy overflows its own stack). The local-FS copy path already
    // rejects this via `validate_destination_not_inside_source`; this brings
    // the volume path to parity. Cross-DEVICE copies can't hit it (different
    // path spaces), so the guard only fires when source and dest are the same
    // volume.
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        for source in source_paths {
            // The copied item lands under `dest_path` (e.g. `/A/sub/A` for
            // source `/A` into dest dir `/A/sub`), so an overlap means
            // `dest_path` is at or below the source directory.
            // Only a DIRECTORY source can contain the destination; a file source
            // can't, and a missing source surfaces later as a per-source copy
            // error, so `Ok(false)` / `Err(_)` fall through without rejecting.
            if (dest_path == source.as_path() || dest_path.starts_with(source))
                && matches!(dest_volume.is_directory(source).await, Ok(true))
            {
                return Err(WriteFailure::synthetic(WriteOperationError::DestinationInsideSource {
                    source: source.display().to_string(),
                    destination: dest_path.display().to_string(),
                }));
            }
        }
    }

    // Phase 0.5: Ensure the destination directory exists, creating it and any
    // missing ancestors on the dest volume (local, SMB, MTP, in-memory). This
    // mirrors the local-FS `ensure_destination_dir` so a copy into a
    // not-yet-existing folder just works on every backend. It runs AFTER the
    // dest-inside-source guard above so we never create a folder inside a
    // source. A merge into an already-existing dest is a no-op create.
    let dest_dir_creation = dest_volume
        .create_directory_all(dest_path)
        .await
        .map_err(|e| WriteFailure::from_volume(dest_path, e))?;

    // How many top-level sources ride at once, and whether the concurrent driver
    // runs at all. Both are decided before Phase 0.6 because the destination
    // pre-check index below is only worth building for the concurrent path.
    let concurrency = transfer_concurrency(&*source_volume, &*dest_volume);
    let use_concurrent_path = source_paths.len() >= 3 && concurrency > 1;
    // Phase 0.5 created `dest_path` rather than finding it, so nothing the user
    // already had can be inside it and the concurrent loop's per-file conflict
    // probe below has nothing to find. See the comment at its call site for what
    // this does and does NOT license.
    let dest_dir_is_ours = dest_dir_creation == DirectoryCreation::Created;

    // Phase 0.6: clear `.cmdr-tmp-*` partials an earlier run's crash or force-quit
    // left here. Staging means an interrupted transfer leaves a recognizable temp
    // rather than a truncated file at a real name; this is what eventually takes
    // them away. One listing, age-gated so a live transfer's temp is never touched.
    let dest_listing = super::volume_cleanup::reap_stale_transfer_temps(&dest_volume, dest_path).await;

    // That listing is also the answer to "which top-level names are already
    // taken?", which the concurrent spawn loop otherwise asks one
    // `get_metadata` round trip at a time — 74% of a 500-file NAS copy. Index it
    // once here and the loop consults memory instead of the wire.
    //
    // Gates, all three load-bearing:
    // - **Concurrent path only.** The serial driver keeps its own per-file
    //   probe, which is what keeps MTP (`max_concurrent_ops() == 1`, so
    //   `concurrency == 1`) away from this entirely: an MTP listing is
    //   pathologically expensive (~18 s for 1046 photos on a cold cache), so
    //   trading one cheap probe for one costly listing there is a large loss.
    // - **A merge only.** A destination directory this operation created can
    //   hold nothing, so it needs neither probe nor index.
    // - **A destination whose operations cost a round trip.** On a local volume
    //   `get_metadata` is a microsecond `stat`, and folding every name in a
    //   200k-entry folder to copy three files into it is the worse trade. Local
    //   destinations keep exactly the behavior they had.
    //
    // ❌ A listing we couldn't take is not an answer of "nothing is there":
    // `None` falls through to per-file probes rather than skipping them.
    let dest_index = (use_concurrent_path && !dest_dir_is_ours && !dest_volume.operations_are_local())
        .then(|| dest_listing.map(DestNameIndex::build))
        .flatten();

    // Phase 1: Preflight scan (reuses the dialog's cached preview when one is
    // available). Populates `total_files`, `total_bytes`, and per-source
    // `is_directory` / `size` hints so the copy loop doesn't have to re-probe
    // each source. Shared with the move pipeline.
    let preflight = scan_volume_sources(
        &source_volume,
        source_paths,
        config,
        operation_id,
        WriteOperationType::Copy,
        state,
        &*events,
    )
    .await?;
    let total_files = preflight.total_files;
    let total_bytes = preflight.total_bytes;
    let known_directory_paths = preflight.known_directory_paths();
    let mut source_hints = preflight.source_hints;

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
    // Shared atomics, updated by in-flight tasks (under concurrency) or
    // the sequential closure below. The driver reads them after each file to
    // keep `files_done` / `bytes_done` in sync for post-loop bookkeeping.
    // The `*_skipped` atomics are a subset, counting only bulk-skip + per-iter
    // Skip resolutions; we use them to annotate the completion log.
    let files_done_atomic = Arc::new(AtomicUsize::new(0));
    let atomic_bytes_done = Arc::new(AtomicU64::new(0));
    let files_skipped_atomic = Arc::new(AtomicUsize::new(0));
    let bytes_skipped_atomic = Arc::new(AtomicU64::new(0));
    let last_progress_mutex = Arc::new(std::sync::Mutex::new(Instant::now()));
    let files_done;
    let bytes_done;
    let mut files_skipped;
    let mut bytes_skipped;
    let progress_interval = Duration::from_millis(config.progress_interval_ms);

    // The concurrency window and the sequential fallback (F7, for 1-2 file
    // batches where spawning tasks isn't worth it, and backends that return 1
    // from `max_concurrent_ops`) were decided above Phase 0.6.
    log::debug!(
        "copy_volumes_with_progress: {} sources, concurrency={} (src={}, dst={}), path={}, dest dir {}, top-level pre-check from {}",
        source_paths.len(),
        concurrency,
        source_volume.max_concurrent_ops(),
        dest_volume.max_concurrent_ops(),
        if use_concurrent_path {
            "concurrent"
        } else {
            "sequential"
        },
        match dest_dir_creation {
            DirectoryCreation::Created => "created by this op",
            DirectoryCreation::AlreadyExisted => "pre-existing",
        },
        if dest_dir_is_ours {
            "nothing (the dest dir is ours)"
        } else if dest_index.is_some() {
            "one destination listing"
        } else {
            "a probe per source"
        },
    );

    // Emit initial copying phase event
    state.emit_progress_via_sink(
        &*events,
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

    // Bulk-skip pre-known conflicts when the user chose Skip upfront. The FE's
    // `scan_for_conflicts` already found these; without this bulk pass, the
    // main loop would re-discover them one at a time via per-file
    // `dest_volume.get_metadata` stats, interleaved with the copies of
    // non-conflicting files, so the progress bar would only advance by 1 per
    // conflict instead of jumping to the full skipped count immediately.
    // Bulk-skip is **file-only**: a top-level directory's name matching a
    // pre-known conflict means only some of its children collide at dest, so
    // dropping the whole subtree would lose non-conflicting files. Top-level
    // directory paths come from `preflight.known_directory_paths()` (computed
    // from the batched scan's `is_directory` hints).
    let pre_skip_paths: HashSet<PathBuf> = build_pre_skip_set(
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

    // The concurrent path keeps its own bulk-skip emit so its shared atomics
    // stay consistent; the serial path delegates the bulk-skip prelude to
    // `drive_transfer_serial_async` (which emits one progress event from its
    // own prelude using `bulk_skip_files` / `bulk_skip_bytes`).
    if use_concurrent_path && bulk_skip_files > 0 {
        let new_files = files_done_atomic.fetch_add(bulk_skip_files, Ordering::Relaxed) + bulk_skip_files;
        let new_bytes = atomic_bytes_done.fetch_add(bulk_skip_bytes, Ordering::Relaxed) + bulk_skip_bytes;
        files_skipped_atomic.fetch_add(bulk_skip_files, Ordering::Relaxed);
        bytes_skipped_atomic.fetch_add(bulk_skip_bytes, Ordering::Relaxed);
        log::info!(
            "copy_volumes_with_progress: bulk-skipping {} pre-known conflicts ({} bytes) before main iteration",
            bulk_skip_files,
            bulk_skip_bytes
        );
        // Re-anchor the rate estimator: bulk-skip credit is past work, not
        // throughput. Without this the first per-task progress callback's
        // delta against `(0, 0)` pins `bytes_per_second` at GB/s level.
        // Same pattern as the driver's serial preludes.
        if let Ok(mut est) = state.estimator.lock() {
            est.reseed_baseline(Instant::now(), new_bytes, new_files);
        }
        state.emit_progress_via_sink(
            &*events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::Copying,
                None,
                new_files,
                total_files,
                new_bytes,
                total_bytes,
            ),
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::Copying,
            None,
            new_files,
            total_files,
            new_bytes,
            total_bytes,
        );
    }

    // Track "apply to all" resolution for conflicts. Shared op-wide between the
    // top-level conflict dispatch and every deep merge level (via `MergeCtx`), so
    // a "…all" choice from any prompt applies everywhere. Both the concurrent and
    // serial paths reuse this one cell.
    let apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>> = Arc::new(std::sync::Mutex::new(ApplyToAll::default()));

    // Track successfully copied destination FILE paths for rollback/cleanup.
    // Wrapped in Arc<Mutex> so concurrent tasks can push independently. The
    // sequential path uses the same container for a uniform post-loop flow.
    // For a directory source these are the individual files the op streamed
    // into the (possibly pre-existing) dest directory — NOT the directory root
    // — so rollback never recursively deletes a merged directory and destroys
    // dest-only files the user already had there.
    let copied_paths: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    // Destination directories this operation NEWLY created (create_directory
    // returned Ok, not AlreadyExists), in creation order (shallowest first).
    // Rollback removes these AFTER the files, deepest first, with a
    // non-recursive empty-only delete — so a dir we created but which still
    // holds a pre-existing sibling (or a kept-partial under cancel) survives.
    let created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    // In concurrent mode, in-flight tasks each pin down their own partial
    // destination path so a cancel/error can delete all of them. Sequential
    // mode keeps the legacy single-slot behavior via a 1-element vec.
    let in_flight_partials: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut last_dest_path: Option<PathBuf> = None;
    // Deep-merge skips (children a merge resolved to Skip) are invisible to the
    // driver's top-level skip accounting, so both paths fold each source's
    // `CreatedPaths::skipped_file_count` in here; the totals are added to the
    // op-wide `files_skipped` / `bytes_skipped` after the loop.
    let deep_skipped_files = Arc::new(AtomicUsize::new(0));
    let deep_skipped_bytes = Arc::new(AtomicU64::new(0));
    let mut copy_error: Option<WriteFailure> = None;

    // Live in-flight table + stall watchdog, for BOTH paths. The concurrent
    // path needs it to explain a wedge across N tasks; the serial path needs it
    // because a user staring at a frozen bar during a one-directory copy is owed
    // the same answer, and `TransferActivity` (which the UI reads to stop showing
    // a confident ETA) is derived from this table. Dropping the guard at the end
    // of this function deregisters the operation and stops the watchdog.
    let probe_guard = Some(super::transfer_probe::register_operation(
        operation_id,
        // The serial path runs exactly one source at a time.
        if use_concurrent_path { concurrency } else { 1 },
        total_files,
        Arc::clone(&atomic_bytes_done),
        // Both ends, so the watchdog can ask whether either connection has been
        // PROVEN dead before it acts on a stall (no backend can answer that yet
        // — see `Volume::connection_liveness`).
        vec![Arc::clone(&source_volume), Arc::clone(&dest_volume)],
        Arc::clone(state),
        Arc::clone(&events),
    ));
    let op_probe = probe_guard
        .as_ref()
        .map(super::transfer_probe::OperationProbeGuard::probe);

    if use_concurrent_path {
        // Concurrent path: FuturesUnordered-driven sliding window sized by
        // `concurrency`. Each task streams one top-level source item end-to-end.
        // Conflict resolution runs synchronously on this driver before the task
        // is spawned (F14) so the whole batch blocks on a single Stop prompt
        // instead of racing per-task prompts.
        // Ok payload is `(partial_path, recorded_path, bytes)`: `partial_path`
        // is the path the task pushed into `in_flight_partials` (the temp
        // sibling under safe-replace, else the dest itself) so the result
        // handler can remove the right entry; `recorded_path` is the final
        // landed path for rollback bookkeeping (the original after a
        // safe-replace finalize, else the dest).
        //
        // Err payload is a `CopyTaskFailure`: `cleanup_temp` distinguishes a
        // STREAM failure (`true` — the dest/temp is a partial and must be
        // cleaned) from a FINALIZE failure after a SUCCESSFUL write (`false` —
        // the temp now holds the only complete copy of the new data and MUST be
        // left on disk; the original was already deleted by
        // `finalize_safe_replace`'s delete step). Deleting the temp in the
        // finalize-failure case would be total data loss. The `created_*` ledger
        // carries a DIRECTORY source's per-file partials out of the error arm so
        // post-loop cleanup never recursively deletes a merged dest dir root.
        type CopyTaskFuture<'a> = Pin<Box<dyn Future<Output = Result<CopyTaskSuccess, CopyTaskFailure>> + Send + 'a>>;
        let mut in_flight: FuturesUnordered<CopyTaskFuture<'_>> = FuturesUnordered::new();

        // Inline helper: drains ONE future from `in_flight`, updates tracking.
        // Returns Err on the first task failure (caller breaks + stores copy_error).
        // `in_flight` is threaded through as a mutable borrow so the helper is
        // just a local lambda in shape, but we inline below for borrow clarity.

        let mut iter = source_paths.iter().enumerate();
        // Set once the driver has SEEN the user's cancel / rollback: from then on
        // it stops waiting indefinitely for its tasks and gives them a bounded
        // window to wind down. See `CANCEL_DRAIN_DEADLINE`.
        let mut drain_deadline: Option<tokio::time::Instant> = None;
        loop {
            // Keep pushing new tasks until either sources run out or the window is full.
            while in_flight.len() < concurrency {
                if is_cancelled(&state.intent) {
                    break;
                }
                let Some((source_index, source_path)) = iter.next() else {
                    break;
                };

                // Pre-known conflict already accounted upfront in the bulk skip.
                if pre_skip_paths.contains(source_path) {
                    continue;
                }

                // Resolve destination path + conflict synchronously.
                let mut dest_item_path = if let Some(name) = source_path.file_name() {
                    dest_path.join(name)
                } else {
                    dest_path.to_path_buf()
                };
                // For a file→file Overwrite, conflict resolution hands back a
                // temp sibling to stream into plus the original path to swap in
                // after the write fully lands (safe-replace). `None` ⇒ write
                // `dest_item_path` directly.
                let mut replace_after_write: Option<PathBuf> = None;
                // The destination pre-check: something already at this name ⇒
                // resolve the conflict. Asked as a `get_metadata` per source it
                // is ONE ROUND TRIP PER FILE, serialized here on the driver, and
                // on a NAS at 3.7 ms RTT it measured 2.378 s of a 3.224 s best
                // run for 500 files — 74%, and no window width can overlap it
                // (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
                //
                // Two things answer it more cheaply, in order:
                //
                // 1. **A destination directory THIS OPERATION created** (Phase
                //    0.5): nothing the user already had can be inside a folder
                //    that didn't exist a moment ago, so every probe is a
                //    guaranteed miss and there's nothing even to index.
                //    ❌ Never widen this to "the destination is empty". A
                //    pre-existing empty directory can gain an entry from another
                //    process between any two instants; one we just created cannot
                //    have held anything BEFORE we made it. Only the second claim
                //    is safe, and the difference is silent when you get it wrong.
                // 2. **The destination listing Phase 0.6 already paid for**, for
                //    a merge into a pre-existing folder — the ordinary F5 copy.
                //    `DestNameIndex` answers `Absent` only when no name in that
                //    listing can resolve to this one on any backend; anything it
                //    can't settle comes back `Unknown` and falls through to the
                //    probe below, which stays authoritative.
                //
                // The listing is taken once, at the start: by the 400th file of a
                // large batch it can be minutes old, so a file that ARRIVES at
                // the destination mid-batch is missed and an Overwrite replaces
                // it with no prompt. That trade is deliberate and David chose it
                // (2026-08-02); ❌ don't answer it with re-listing, polling, or a
                // freshness window. `DETAILS.md` § "Answering the pre-check from
                // one listing".
                let existing_dest_meta = if dest_dir_is_ours {
                    None
                } else {
                    match dest_index.as_ref().map(|index| index.lookup(source_path.file_name())) {
                        Some(DestLookup::Absent) => None,
                        Some(DestLookup::Present(entry)) => Some(*entry),
                        // No index (a local destination, or a listing that
                        // failed), or a name only the backend can settle.
                        Some(DestLookup::Unknown) | None => {
                            // Record the pre-check BEFORE awaiting it. In the
                            // 2026-07-31 incident this destination `get_metadata`
                            // was the driver's last log line and nothing said
                            // whether it returned, so a dump has to be able to
                            // name it as the step in progress.
                            if let Some(probe) = op_probe.as_ref() {
                                probe.set_driver_phase(
                                    super::transfer_probe::DriverPhase::PreparingNext,
                                    &format!("#{source_index} {}", dest_item_path.display()),
                                );
                            }
                            dest_volume.get_metadata(&dest_item_path).await.ok()
                        }
                    }
                };
                if let Some(dest_meta) = existing_dest_meta {
                    // Reuse the per-source hint from the scan instead of re-statting.
                    let source_hint = source_hints.get(source_path).copied();
                    let source_is_dir = source_hint.map(|h| h.is_directory).unwrap_or(false);
                    // Pass file sizes to `resolve_volume_conflict` so it doesn't
                    // re-scan to populate the conflict dialog (an MTP `scan_for_copy`
                    // lists the parent dir, ~18 s for 1046 photos on a cold cache).
                    let source_size_hint = source_hint.and_then(|h| (!h.is_directory).then_some(h.size));
                    // `Some` only when the preflight actually produced a hint, so
                    // `resolve_volume_conflict` keeps its trait-call fallback for
                    // the no-hint case instead of trusting a defaulted `false`.
                    let source_is_directory_hint = source_hint.map(|h| h.is_directory);
                    let dest_size_hint = dest_meta.size;
                    log::debug!(
                        "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                        dest_item_path.display(),
                        source_is_dir,
                        dest_meta.is_directory,
                    );
                    // Copy the op-wide latch out, run the resolver on the stack
                    // local, store it back — mirroring the serial path. The
                    // resolver's `conflict_dispatch_lock` (acquired inside) is
                    // what serializes the human against in-flight deep merges
                    // spawned by earlier loop iterations; this same lock is why a
                    // top-level prompt and a deep prompt can't race the one
                    // oneshot slot. The known acceptable residual: an already-
                    // emitted prompt isn't retroactively resolved by another
                    // task's "…all" latch — a rare extra prompt, never data loss.
                    let mut latched = *apply_to_all_cell.lock_ignore_poison();
                    let resolved = resolve_volume_conflict(
                        &source_volume,
                        source_path,
                        &dest_volume,
                        &dest_item_path,
                        config,
                        &*events,
                        operation_id,
                        state,
                        &mut latched,
                        source_size_hint,
                        dest_size_hint,
                        source_is_directory_hint,
                    )
                    .await
                    .map_err(WriteFailure::synthetic);
                    *apply_to_all_cell.lock_ignore_poison() = latched;
                    let resolved = resolved?;
                    match resolved {
                        None => {
                            log::debug!(
                                "copy_volumes_with_progress: skipping {} due to conflict resolution",
                                source_path.display()
                            );
                            account_skipped_file(
                                source_path,
                                &source_hints,
                                &files_done_atomic,
                                &atomic_bytes_done,
                                &files_skipped_atomic,
                                &bytes_skipped_atomic,
                                &last_progress_mutex,
                                progress_interval,
                                state,
                                &*events,
                                operation_id,
                                total_files,
                                total_bytes,
                            );
                            continue;
                        }
                        Some(rc) => {
                            dest_item_path = rc.write_path;
                            replace_after_write = rc.replace_after_write;
                        }
                    }
                }

                let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
                log::debug!(
                    "copy_volumes_with_progress: spawning copy {} -> {}",
                    source_path.display(),
                    dest_item_path.display()
                );

                let hint = source_hints.get(source_path).copied().unwrap_or_default();
                let source_is_dir_hint = hint.is_directory;

                // Mark this destination as in-flight so cancel/error can clean it
                // up — but ONLY for a FILE source. A DIRECTORY source's dest is a
                // (possibly pre-existing, merged) dir whose cleanup path is
                // `delete_volume_path_recursive`; recording the dir ROOT here and
                // then recursively deleting it on keep-partials/rollback would
                // destroy pre-existing dest-only files (the merge invariant). A
                // directory source's cleanup is owned entirely by the per-file
                // `created`/`copied_paths` ledger threaded out of the task's
                // result arms. (A dir task dropped mid-flight on abort leaves its
                // in-flight `.cmdr-tmp-<uuid>` for the backend writer's abort to
                // clean; never the merged root.) Pinned by
                // `cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file`.
                if !source_is_dir_hint {
                    in_flight_partials.lock_ignore_poison().push(dest_item_path.clone());
                }

                let src_vol = Arc::clone(&source_volume);
                let dst_vol = Arc::clone(&dest_volume);
                let state_clone = Arc::clone(state);
                let events_task = Arc::clone(&events);
                let op_id = operation_id;
                let files_done_a = Arc::clone(&files_done_atomic);
                let bytes_done_a = Arc::clone(&atomic_bytes_done);
                let last_prog_a = Arc::clone(&last_progress_mutex);
                let source_owned = source_path.clone();
                let dest_owned = dest_item_path.clone();
                let replace_after_write_owned = replace_after_write.clone();
                let file_name_owned = file_name.clone();
                let source_size_hint = if hint.is_directory { None } else { Some(hint.size) };
                // Per-task merge context: deep file clashes inside a directory
                // source landing on a merged dest honor the file policy, sharing
                // the op-wide apply-to-all latch with every other task and the
                // top-level dispatch.
                let merge_config = config.clone();
                let merge_op_id = operation_id.to_string();
                let merge_apply_to_all = Arc::clone(&apply_to_all_cell);
                // Register before the task is pushed, so a task that never gets
                // polled still shows up in a dump as `spawned`.
                let task_probe = op_probe.as_ref().map(|probe| {
                    probe.begin_task(
                        source_index,
                        &source_path.display().to_string(),
                        &dest_item_path.display().to_string(),
                    )
                });

                in_flight.push(Box::pin(async move {
                    // Held for the task's whole life; dropping it (completion,
                    // abort, panic) removes the row from the in-flight table.
                    let task_probe = task_probe;
                    let probe_handle = task_probe.as_ref().map(super::transfer_probe::TaskProbeHandle::probe);
                    // Per-task `last_file_bytes` tracks bytes reported for the
                    // file this task is copying; deltas roll up into the
                    // shared `bytes_done_a` so the throttle emits an aggregate.
                    // Owned by the task; the helper closure carries its own
                    // Arc clone, the post-call compensation reads the same
                    // counter to detect "volume never invoked on_progress."
                    let last_file_bytes = Arc::new(AtomicU64::new(0));
                    // Per-source rollback ledger: the files this task streams
                    // and the dirs it newly creates inside a directory source.
                    let created = super::volume_strategy::CreatedPaths::default();
                    // Deep merge children are never top-level sources, so the
                    // resolver never keys into per-source hints for them — an
                    // empty map is correct (and avoids capturing the function's
                    // `source_hints` into the `'static` task).
                    let merge_hints: HashMap<PathBuf, SourceHint> = HashMap::new();
                    let merge_ctx = super::volume_strategy::MergeCtx {
                        events: &*events_task,
                        operation_id: &merge_op_id,
                        config: &merge_config,
                        state: &state_clone,
                        apply_to_all: &merge_apply_to_all,
                        source_hints: &merge_hints,
                    };
                    let on_file_progress = make_concurrent_per_file_progress(
                        Arc::clone(&events_task),
                        Arc::clone(&state_clone),
                        op_id.to_string(),
                        WriteOperationType::Copy,
                        file_name_owned.clone(),
                        Arc::clone(&last_file_bytes),
                        Arc::clone(&bytes_done_a),
                        Arc::clone(&files_done_a),
                        total_files,
                        total_bytes,
                        Arc::clone(&last_prog_a),
                        progress_interval,
                    );
                    // The byte count is rolled into the aggregate by the progress
                    // callback's per-chunk delta (and the post-task compensation),
                    // so this only advances the leaf-file axis.
                    let on_file_complete = |_leaf_bytes: u64| {
                        files_done_a.fetch_add(1, Ordering::Relaxed);
                    };
                    let copy_fut = copy_single_path(
                        &src_vol,
                        &source_owned,
                        source_is_dir_hint,
                        source_size_hint,
                        &dst_vol,
                        &dest_owned,
                        &state_clone,
                        &created,
                        &on_file_progress,
                        &on_file_complete,
                        Some(&merge_ctx),
                        super::volume_strategy::staging_for(&replace_after_write_owned),
                    );
                    // Bind this task's probe as a task-local for the whole copy, so
                    // `stream_pipe_file` and `CheckpointStream` can record their
                    // phases without threading a handle through every signature.
                    let result = match probe_handle {
                        Some(probe) => super::transfer_probe::CURRENT_TASK_PROBE.scope(probe, copy_fut).await,
                        None => copy_fut.await,
                    };
                    let created_files = std::mem::take(&mut *created.files.lock_ignore_poison());
                    let created_dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                    // Deep-merge skips in this source's subtree; `0` means the
                    // whole subtree landed durably (the move op may drop it from
                    // the archive).
                    let task_skipped_count = created.skipped_file_count();
                    let task_skipped_bytes = created.skipped_byte_count();
                    // Overwrote iff a top-level file→file safe-replace fires below,
                    // OR a deep-merge child replaced an existing dest file. Computed
                    // before `replace_after_write_owned` is consumed. Feeds the
                    // operation-log eligibility (a copy that overwrote can't roll back).
                    let task_overwrote = replace_after_write_owned.is_some() || created.any_overwrote();
                    match result {
                        Ok(bytes) => {
                            // If the volume didn't call the progress callback,
                            // add bytes_copied to the aggregate so the total is
                            // right. Same compensation the sequential path does.
                            if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes > 0 {
                                bytes_done_a.fetch_add(bytes, Ordering::Relaxed);
                            }
                            // Safe-replace finalize: the temp now holds the
                            // complete new data; delete the original and rename
                            // the temp into place. On finalize error, surface
                            // it as this file's failure with `cleanup_temp =
                            // false` — the write SUCCEEDED, so the temp is
                            // committed data (the only complete copy, since
                            // finalize's delete step may already have removed
                            // the original). It must survive as a recoverable
                            // `.cmdr-tmp-*` artifact, NOT be cleaned.
                            if let Some(orig) = replace_after_write_owned {
                                if let Err(e) =
                                    super::volume_conflict::finalize_safe_replace(&dst_vol, &dest_owned, &orig).await
                                {
                                    // Finalize is file→file only (safe-replace),
                                    // so there's no directory ledger to carry.
                                    return Err(CopyTaskFailure {
                                        failed_path: dest_owned,
                                        error: e,
                                        cleanup_temp: false,
                                        source_is_dir: false,
                                        created_files,
                                        created_dirs,
                                    });
                                }
                                // Landed at `orig`; the temp `dest_owned` is
                                // gone after the rename. Report the temp as the
                                // partial to remove and `orig` as the recorded
                                // path for rollback bookkeeping. Safe-replace is
                                // file→file only, so there are no created dirs.
                                return Ok(CopyTaskSuccess {
                                    partial_path: dest_owned,
                                    recorded_path: orig,
                                    source_is_dir: false,
                                    bytes,
                                    overwrote: task_overwrote,
                                    created_files,
                                    created_dirs,
                                    source_path: source_owned,
                                    skipped_count: task_skipped_count,
                                    skipped_bytes: task_skipped_bytes,
                                });
                            }
                            Ok(CopyTaskSuccess {
                                partial_path: dest_owned.clone(),
                                recorded_path: dest_owned,
                                source_is_dir: source_is_dir_hint,
                                bytes,
                                overwrote: task_overwrote,
                                created_files,
                                created_dirs,
                                source_path: source_owned,
                                skipped_count: task_skipped_count,
                                skipped_bytes: task_skipped_bytes,
                            })
                        }
                        // Stream failure (incl. mid-stream cancel): the dest/temp
                        // is a half-written partial → clean it
                        // (`cleanup_temp = true`). For a DIRECTORY source, carry
                        // the per-file ledger so the result handler records the
                        // individual partials instead of the dir root — the
                        // post-loop must never recursively delete a merged dest
                        // dir and destroy pre-existing dest-only files.
                        Err(e) => Err(CopyTaskFailure {
                            failed_path: dest_owned,
                            error: e,
                            cleanup_temp: true,
                            source_is_dir: source_is_dir_hint,
                            created_files,
                            created_dirs,
                        }),
                    }
                }));
            }

            if in_flight.is_empty() {
                break;
            }

            // The await the driver parked on for 20 minutes in the 2026-07-31
            // incident. Naming it means a dump distinguishes "the driver is
            // waiting for tasks" from "the driver is stuck preparing the next
            // source", which the log could not tell apart.
            if let Some(probe) = op_probe.as_ref() {
                probe.set_driver_phase(super::transfer_probe::DriverPhase::AwaitingTasks, "");
            }

            // Observing the user's intent HERE is what makes Cancel and Rollback
            // work at all. The spawn loop above checks `is_cancelled` too, but a
            // driver whose window is full — or whose sources have run out — never
            // gets back to it while its tasks are parked, which is why Rollback
            // did nothing in the incident. `backend_cancel` fires on every
            // transition out of `Running`, so both Cancel and Rollback land here.
            let next = match drain_deadline {
                // Already winding down: bounded, so one task that never returns
                // can't hold the operation (and the user's dialog) open.
                Some(deadline) => match tokio::time::timeout_at(deadline, in_flight.next()).await {
                    Ok(next) => next,
                    Err(_) => {
                        crate::log_error!(
                            target: "copy",
                            "copy_volumes_with_progress: op={} abandoning {} task(s) that did not wind down within {:?} of the cancel. \
                             Their handles are left for the backend to reap; their staged partials are cleaned up below.{}",
                            operation_id,
                            in_flight.len(),
                            cancel_drain_deadline(),
                            op_probe
                                .as_ref()
                                .map(|p| format!("\n{}", p.render_dump("abandoning wedged tasks")))
                                .unwrap_or_default(),
                        );
                        break;
                    }
                },
                None => {
                    tokio::select! {
                        // Cancel wins the race: a task result that was also ready
                        // is re-polled on the next pass, so nothing is dropped.
                        biased;
                        () = state.backend_cancel.cancelled() => {
                            log::info!(
                                target: "copy",
                                "copy_volumes_with_progress: op={} observed {:?} while awaiting {} in-flight task(s); \
                                 winding them down (up to {:?})",
                                operation_id,
                                load_intent(&state.intent),
                                in_flight.len(),
                                cancel_drain_deadline(),
                            );
                            drain_deadline = Some(tokio::time::Instant::now() + cancel_drain_deadline());
                            continue;
                        }
                        next = in_flight.next() => next,
                    }
                }
            };
            match next {
                Some(Ok(CopyTaskSuccess {
                    partial_path,
                    recorded_path,
                    source_is_dir,
                    bytes,
                    overwrote,
                    created_files,
                    created_dirs: task_created_dirs,
                    source_path: done_source,
                    skipped_count: task_skipped_count,
                    skipped_bytes: task_skipped_bytes,
                })) => {
                    // Fold this source's deep-merge skips into the op-wide tally,
                    // and — when the source landed with ZERO skips — record it as
                    // fully extracted so the out-of-zip move op may drop it.
                    deep_skipped_files.fetch_add(task_skipped_count, Ordering::Relaxed);
                    deep_skipped_bytes.fetch_add(task_skipped_bytes, Ordering::Relaxed);
                    if task_skipped_count == 0 {
                        events.note_source_landed_clean(&done_source);
                    }
                    // Remove the in-flight partial (the temp under safe-replace,
                    // else the dest) and record what the op wrote for rollback.
                    let mut partials = in_flight_partials.lock_ignore_poison();
                    if let Some(pos) = partials.iter().position(|p| p == &partial_path) {
                        partials.swap_remove(pos);
                    }
                    drop(partials);
                    let file_name_done = recorded_path.file_name().map(|n| n.to_string_lossy().to_string());
                    // Journal the per-leaf `rollback_unit` rows under the REAL
                    // volume ids (a file source → one leaf at `recorded_path`; a dir
                    // source → one leaf per `created_files` entry, source rebased
                    // from `done_source`). Created dirs journal post-loop (after all
                    // files, so their `seq` follows the contents). No-ops for the
                    // both-local shortcut (that path journals via `copy_files_start`)
                    // and in tests that don't set a journal target.
                    if let Some((src_vol, dst_vol)) = journal_volumes.as_ref() {
                        journal::record_volume_transfer_source(
                            operation_id,
                            src_vol,
                            &done_source,
                            dst_vol,
                            &recorded_path,
                            source_is_dir,
                            &created_files,
                            (!source_is_dir).then_some(bytes as i64),
                            overwrote,
                        );
                    }
                    // For a DIRECTORY source, record the individual files and the
                    // newly-created subdirs the op wrote — never the directory
                    // root — so rollback can't recursively delete a merged
                    // directory and destroy dest-only files. For a FILE source,
                    // record the landed path (the original after a safe-replace
                    // finalize, else the dest); `created_*` are empty.
                    if source_is_dir {
                        copied_paths.lock_ignore_poison().extend(created_files);
                        created_dirs.lock_ignore_poison().extend(task_created_dirs);
                    } else {
                        copied_paths.lock_ignore_poison().push(recorded_path);
                    }
                    // Per-file milestone emit. The task's `on_file_complete`
                    // bumped `files_done_atomic`; the FE's files-axis needs
                    // a Copying event that observes the bumped value, but
                    // no chunked emit fires after `on_file_complete` (the
                    // file's transfer is over). Mirrors the serial path's
                    // milestone in `transfer_driver.rs::drive_transfer_serial_async`.
                    *last_progress_mutex.lock_ignore_poison() = Instant::now();
                    let current_files = files_done_atomic.load(Ordering::Relaxed);
                    let current_bytes = atomic_bytes_done.load(Ordering::Relaxed);
                    state.emit_progress_via_sink(
                        &*events,
                        WriteProgressEvent::new(
                            operation_id.to_string(),
                            WriteOperationType::Copy,
                            WriteOperationPhase::Copying,
                            file_name_done.clone(),
                            current_files,
                            total_files,
                            current_bytes,
                            total_bytes,
                        ),
                    );
                    update_operation_status(
                        operation_id,
                        WriteOperationPhase::Copying,
                        file_name_done,
                        current_files,
                        total_files,
                        current_bytes,
                        total_bytes,
                    );
                }
                Some(Err(CopyTaskFailure {
                    failed_path: failed_dest,
                    error: e,
                    cleanup_temp,
                    source_is_dir,
                    created_files,
                    created_dirs: task_created_dirs,
                })) => {
                    // Remove from in-flight partials; this one's own partial
                    // cleanup (if any) the post-loop logic will do.
                    let mut partials = in_flight_partials.lock_ignore_poison();
                    if let Some(pos) = partials.iter().position(|p| p == &failed_dest) {
                        partials.swap_remove(pos);
                    }
                    drop(partials);
                    if source_is_dir {
                        // DIRECTORY source interrupted mid-stream. Record the
                        // per-file partials and newly-created subdirs the op
                        // wrote, NOT the dir root `failed_dest`. The post-loop
                        // then cleans/rolls back per-file (and prunes created
                        // dirs empty-only), so a merged dir holding a
                        // pre-existing dest-only file survives — recursively
                        // deleting the root would be silent data loss.
                        copied_paths.lock_ignore_poison().extend(created_files);
                        created_dirs.lock_ignore_poison().extend(task_created_dirs);
                    } else if cleanup_temp {
                        // FILE source stream failure: `failed_dest` is the single
                        // half-written partial. Clean it.
                        //
                        // `cleanup_temp == false` ⇒ finalize failed AFTER a
                        // successful write: `failed_dest` is the temp holding the
                        // ONLY complete copy of the new data (finalize already
                        // deleted the original). Do NOT designate it for cleanup —
                        // leaving it on disk as a `.cmdr-tmp-*` artifact is the
                        // correct, safe outcome. Cleaning it would be total data
                        // loss.
                        last_dest_path = Some(failed_dest.clone());
                    }
                    copy_error = Some(WriteFailure::from_volume(&failed_dest, e));
                    // Drop remaining in-flight tasks; their streams close,
                    // temp files get cleaned up by the per-backend write
                    // abort + delete path. Partial cleanup is done below.
                    break;
                }
                None => break,
            }
        }

        // Drain whatever's left on cancel/error. On success, `in_flight` is
        // already empty. On abort, drop cancels the remaining futures (F10).
        if let Some(probe) = op_probe.as_ref() {
            probe.set_driver_phase(super::transfer_probe::DriverPhase::PostLoop, "draining in-flight");
        }
        drop(in_flight);
        // Sync counters for post-loop reporting.
        files_done = files_done_atomic.load(Ordering::Relaxed);
        bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
        files_skipped = files_skipped_atomic.load(Ordering::Relaxed);
        bytes_skipped = bytes_skipped_atomic.load(Ordering::Relaxed);
    } else {
        // Serial path: delegate the per-iter scaffolding (cancellation check,
        // pre-skip, conflict detect/resolve, skip accounting, paired progress
        // emit) to `drive_transfer_serial_async`. The two closures below own
        // only the per-source work: conflict resolver dispatch and the actual
        // copy via `copy_single_path`. Post-loop bookkeeping (rollback,
        // partial cleanup, write-cancelled / write-complete emits) stays in
        // this function, keyed off `outcome.intent` and `state.intent`.
        //
        // Why the concurrent path above stays inline rather than living in
        // the driver: only `copy_volumes_with_progress` uses
        // `FuturesUnordered` (moves are serial; local-FS copy is serial).
        // Abstracting it would be a 1-of-4 abstraction with very different
        // trait bounds (`Fn + Send + Sync` for `FuturesUnordered` polling vs
        // the serial driver's per-call `FnMut`). See plan §
        // "Concurrent driver scope" option (a).
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
        // matching note in `volume_move`): the driver's `files_done` counts
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
            &pre_skip_paths,
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
                        let source_is_dir = source_hint.map(|h| h.is_directory).unwrap_or(false);
                        let source_size_hint = source_hint.and_then(|h| (!h.is_directory).then_some(h.size));
                        // `Some` only when the preflight produced a hint, so the
                        // resolver keeps its trait-call fallback rather than
                        // trusting a defaulted `false`.
                        let source_is_directory_hint = source_hint.map(|h| h.is_directory);
                        log::debug!(
                            "copy_volumes_with_progress: conflict detected at {} (source_is_dir={})",
                            initial_dest_owned.display(),
                            source_is_dir,
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

                        let hint = source_hints.get(&source_path).copied().unwrap_or_default();
                        let source_is_dir_hint = hint.is_directory;
                        let source_size_hint = if hint.is_directory { None } else { Some(hint.size) };

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
                        let created = super::volume_strategy::CreatedPaths::default();

                        // Merge context: deep file clashes inside a merged
                        // directory honor the file policy via the resolver,
                        // sharing the op-wide apply-to-all latch with the
                        // top-level dispatch.
                        let merge_ctx = super::volume_strategy::MergeCtx {
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
                        let probe_handle = task_probe.as_ref().map(super::transfer_probe::TaskProbeHandle::probe);

                        let copy_fut = copy_single_path(
                            &source_volume,
                            &source_path,
                            source_is_dir_hint,
                            source_size_hint,
                            &dest_volume,
                            &dest_item_path,
                            &state,
                            &created,
                            &on_file_progress,
                            &on_file_complete,
                            Some(&merge_ctx),
                            super::volume_strategy::staging_for(&replace_after_write),
                        );
                        // Bind this source's probe as a task-local for the whole
                        // copy, so `stream_pipe_file` and `CheckpointStream`
                        // record their phases with no signature threading.
                        let copy_result = match probe_handle {
                            Some(probe) => super::transfer_probe::CURRENT_TASK_PROBE.scope(probe, copy_fut).await,
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
                                        if let Err(e) = super::volume_conflict::finalize_safe_replace(
                                            &dest_volume,
                                            &dest_item_path,
                                            &orig,
                                        )
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
                                if source_is_dir_hint {
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
                                if source_is_dir_hint {
                                    *last_dest_cell.lock_ignore_poison() = None;
                                    let files = std::mem::take(&mut *created.files.lock_ignore_poison());
                                    let dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                                    copied_paths.lock_ignore_poison().extend(files);
                                    created_dirs.lock_ignore_poison().extend(dirs);
                                }
                                Err(map_volume_error(&source_path.display().to_string(), e))
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

        files_done = outcome.files_done;
        bytes_done = outcome.bytes_done;
        files_skipped = outcome.files_skipped;
        bytes_skipped = outcome.bytes_skipped;
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
    }

    // Fold the deep-merge skips (invisible to the driver's top-level accounting)
    // into the op-wide skip tally so the terminal `files_skipped` is honest.
    files_skipped += deep_skipped_files.load(Ordering::Relaxed);
    bytes_skipped += deep_skipped_bytes.load(Ordering::Relaxed);

    // Unwrap shared containers for post-loop logic.
    let mut copied_paths: Vec<PathBuf> = Arc::try_unwrap(copied_paths)
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_else(|arc| arc.lock_ignore_poison().clone());
    let created_dirs: Vec<PathBuf> = Arc::try_unwrap(created_dirs)
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_else(|arc| arc.lock_ignore_poison().clone());
    let in_flight_partials: Vec<PathBuf> = Arc::try_unwrap(in_flight_partials)
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_else(|arc| arc.lock_ignore_poison().clone());

    // Post-loop: handle success, cancellation, or error
    let intent = load_intent(&state.intent);

    // A `VolumeError::Cancelled` from a per-task stream (concurrent path's
    // `Err((dest, e))` arm, or the serial driver's `PostLoopIntent::Failed`
    // arm) maps to `WriteOperationError::Cancelled` and ends up here in
    // `copy_error`. That's not a transport failure: it's the cooperative
    // response to the user's cancel click. Reclassify it as cancellation so
    // the gate below emits `write-cancelled` instead of dropping the terminal
    // event entirely and wedging the FE dialog.
    if is_cancelled(&state.intent)
        && matches!(
            copy_error.as_ref().map(|f| &f.error),
            Some(WriteOperationError::Cancelled { .. }),
        )
    {
        copy_error = None;
    }

    if copy_error.is_none() && !is_cancelled(&state.intent) {
        // All files copied successfully
        log::info!(
            "copy_volumes_with_progress: completed op={} files={} bytes={}{}",
            operation_id,
            files_done,
            bytes_done,
            format_skipped_suffix(files_skipped, bytes_skipped),
        );

        // Journal the directories the copy created as `dir` rows on the dest
        // volume, AFTER all the file leaves (so their `seq` follows the contents
        // and the rollback removes files before their dirs). Mirrors the local
        // copy's post-success `record_created_dirs`.
        if let Some((_, dst_vol)) = journal_volumes.as_ref() {
            journal::record_created_dirs_on(operation_id, dst_vol, &created_dirs);
        }

        events.emit_complete(WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            files_processed: files_done,
            files_skipped,
            bytes_processed: bytes_done,
        });

        return Ok(());
    }

    // Cancelled or errored. Before either branch, remove the staged partials of
    // tasks the driver abandoned mid-write: their futures were dropped, so
    // nothing else will. A temp whose write finished is already off this list, so
    // committed data is never in scope here.
    super::volume_cleanup::clean_abandoned_staged_writes(&dest_volume, state).await;

    // Decide between rollback and cancel.
    if intent == OperationIntent::RollingBack {
        // Include the last in-progress item in rollback (it was partially created)
        if let Some(partial_path) = last_dest_path.take() {
            copied_paths.push(partial_path);
        }
        // Under concurrency there can be multiple partials. The tasks we
        // dropped on abort each left a .cmdr-tmp-<uuid> that the backend's
        // writer.abort() cleaned up, but the destination path itself may have
        // an already-renamed file. Roll those back too.
        for partial in in_flight_partials.iter() {
            if !copied_paths.contains(partial) {
                copied_paths.push(partial.clone());
            }
        }

        // User requested rollback: delete all copied files in reverse order with progress
        log::info!(
            "copy_volumes_with_progress: rolling back op={}, {} paths to delete",
            operation_id,
            copied_paths.len()
        );

        let rollback_completed = volume_rollback_with_progress(
            &dest_volume,
            &copied_paths,
            &created_dirs,
            &*events,
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
        // Stopped or error: keep completed files, clean up partial files.
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

// The `volume_copy_tests.rs` suite was split for size. The crash-safety and
// rollback suites live in their own files; both share `make_state` /
// `make_volumes` from `tests` (`super::tests`). The bench suite is a single
// `#[ignore]`d, network-gated test.
#[cfg(test)]
#[path = "volume_copy_bench.rs"]
mod bench;
#[cfg(test)]
#[path = "volume_copy_cancel_tests.rs"]
mod cancel_tests;
#[cfg(test)]
#[path = "volume_copy_concurrency_bench.rs"]
mod concurrency_bench;
#[cfg(test)]
#[path = "volume_copy_concurrent_tests.rs"]
mod concurrent_tests;
#[cfg(test)]
#[path = "volume_copy_crashsafe_tests.rs"]
mod crashsafe_tests;
#[cfg(test)]
#[path = "volume_copy_extract_out_tests.rs"]
mod extract_out_tests;
#[cfg(test)]
#[path = "volume_merge_tests.rs"]
mod merge_tests;
#[cfg(test)]
#[path = "volume_copy_precheck_tests.rs"]
mod precheck_tests;
#[cfg(test)]
#[path = "volume_copy_retry_tests.rs"]
mod retry_tests;
#[cfg(test)]
#[path = "volume_copy_rollback_tests.rs"]
mod rollback_tests;
#[cfg(test)]
#[path = "volume_copy_staged_write_tests.rs"]
mod staged_write_tests;
#[cfg(test)]
#[path = "volume_copy_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "volume_copy_wedge_test_support.rs"]
mod wedge_test_support;
#[cfg(test)]
#[path = "volume_copy_window_tests.rs"]
mod window_tests;
