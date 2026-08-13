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

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::super::super::conflict::ApplyToAll;
use super::super::super::journal;
use super::super::super::manager;
use super::super::super::state::{
    OperationIntent, WriteOperationState, is_cancelled, load_intent, update_operation_status,
};
use super::super::super::types::{
    OperationEventSink, VolumeCopyConfig, VolumeCopyScanResult, WriteCancelledEvent, WriteCompleteEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
    WriteProgressEvent,
};
use super::super::dest_name_index::DestNameIndex;
use super::super::transfer_driver::build_pre_skip_set;
use super::preflight::scan_volume_sources;
use crate::file_system::volume::{DirectoryCreation, SourceItemInfo, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::OpKind;

use super::cleanup::{clean_partial_writes, volume_rollback_with_progress};
use super::transfer_error::{WriteFailure, write_error_event_from};

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

/// The same wait once the hard-abort tier has fired (`state.backend_abort`).
///
/// A cooperative cancel can be generous because a task that hasn't answered yet
/// might still come back with its own cleanup done. An abort has already given up
/// on that: something is holding a deadline the app promised a person — today
/// that means "the app quits within two seconds" — and a second of grace is all
/// that's left to spend. Anything still running when it expires is abandoned
/// exactly as before, which staging makes safe.
const ABORT_DRAIN_DEADLINE: Duration = Duration::from_secs(1);

/// How long a wind-down waits for its in-flight tasks, chosen by whoever asked
/// for it rather than by a constant: `aborting` is the hard-abort tier being
/// live. Honors a test override.
pub(super) fn drain_deadline(aborting: bool) -> Duration {
    #[cfg(test)]
    if let Some(d) = wedge_test_support::drain_override(aborting) {
        return d;
    }
    if aborting {
        ABORT_DRAIN_DEADLINE
    } else {
        CANCEL_DRAIN_DEADLINE
    }
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

        // Convert relative paths to absolute paths. The dest is ANCHORED, not
        // joined: the IPC boundary already anchors what the transfer dialog
        // sends, and a raw join would re-root an absolute dest under itself
        // (`/Volumes/USB/sub` → `/Volumes/USB/Volumes/USB/sub`).
        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = cmdr_fs::volume::root_anchored(&dest_root, &dest_path);

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
        return super::super::super::copy_files_start(
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
        // Every file this writes is a NEW one at the destination, so cancelling
        // with rollback can delete them again (`cleanup.rs`).
        supports_rollback: true,
        preview_id: config.preview_id.clone(),
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

            // Wait out the confirming dialog's scan before journaling or
            // touching either device; see `write_operations::start_write_operation`.
            if crate::file_system::write_operations::scan_bridge::await_claimed_preview(
                &*events,
                &op_id,
                WriteOperationType::Copy,
                &state,
            )
            .await
            .stopped()
            {
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

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
    let dest_listing = super::cleanup::reap_stale_transfer_temps(&dest_volume, dest_path).await;

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
    // The CONCURRENT path's shared ledger: in-flight tasks roll their per-file
    // deltas into these and the post-loop reads them back. ❌ Nothing outside
    // that path may read them — the SERIAL path leaves them at zero and keeps
    // its running totals in `SerialOutcome` instead. (The stall watchdog used to
    // read `atomic_bytes_done` and so called every serial transfer stalled;
    // `transfer_probe.rs::OperationProbe::bytes_done` says what it reads now.)
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
        state.reseed_estimator_baseline(new_bytes, new_files);
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
    let mut last_dest_path: Option<PathBuf>;
    // Deep-merge skips (children a merge resolved to Skip) are invisible to the
    // driver's top-level skip accounting, so both paths fold each source's
    // `CreatedPaths::skipped_file_count` in here; the totals are added to the
    // op-wide `files_skipped` / `bytes_skipped` after the loop.
    let deep_skipped_files = Arc::new(AtomicUsize::new(0));
    let deep_skipped_bytes = Arc::new(AtomicU64::new(0));
    let mut copy_error: Option<WriteFailure>;

    // Live in-flight table + stall watchdog, for BOTH paths. The concurrent
    // path needs it to explain a wedge across N tasks; the serial path needs it
    // because a user staring at a frozen bar during a one-directory copy is owed
    // the same answer, and `TransferActivity` (which the UI reads to stop showing
    // a confident ETA) is derived from this table. Dropping the guard at the end
    // of this function deregisters the operation and stops the watchdog.
    let probe_guard = Some(super::super::transfer_probe::register_operation(
        operation_id,
        // The serial path runs exactly one source at a time.
        if use_concurrent_path { concurrency } else { 1 },
        total_files,
        // Both ends, so the watchdog can ask whether either connection has been
        // PROVEN dead before it acts on a stall (no backend can answer that yet
        // — see `Volume::connection_liveness`).
        vec![Arc::clone(&source_volume), Arc::clone(&dest_volume)],
        Arc::clone(state),
        Arc::clone(&events),
    ));
    let op_probe = probe_guard
        .as_ref()
        .map(super::super::transfer_probe::OperationProbeGuard::probe);

    if use_concurrent_path {
        // The `FuturesUnordered` sliding window, in `volume/copy_concurrent.rs`.
        // Everything it needs is already on hand here; the ledger Arcs are cloned
        // in, so the post-loop below keeps reading the same counters.
        let outcome = super::copy_concurrent::drive_transfer_concurrent(super::copy_concurrent::ConcurrentCopy {
            events: Arc::clone(&events),
            operation_id,
            state,
            source_volume: Arc::clone(&source_volume),
            source_paths,
            dest_volume: Arc::clone(&dest_volume),
            dest_path,
            config,
            concurrency,
            dest_dir_is_ours,
            dest_index: &dest_index,
            pre_skip_paths: &pre_skip_paths,
            source_hints: &source_hints,
            total_files,
            total_bytes,
            progress_interval,
            journal_volumes: &journal_volumes,
            op_probe: &op_probe,
            files_done_atomic: Arc::clone(&files_done_atomic),
            atomic_bytes_done: Arc::clone(&atomic_bytes_done),
            files_skipped_atomic: Arc::clone(&files_skipped_atomic),
            bytes_skipped_atomic: Arc::clone(&bytes_skipped_atomic),
            last_progress_mutex: Arc::clone(&last_progress_mutex),
            apply_to_all_cell: Arc::clone(&apply_to_all_cell),
            copied_paths: Arc::clone(&copied_paths),
            created_dirs: Arc::clone(&created_dirs),
            in_flight_partials: Arc::clone(&in_flight_partials),
            deep_skipped_files: Arc::clone(&deep_skipped_files),
            deep_skipped_bytes: Arc::clone(&deep_skipped_bytes),
        })
        .await?;
        last_dest_path = outcome.last_dest_path;
        copy_error = outcome.copy_error;
        // Sync counters for post-loop reporting.
        files_done = files_done_atomic.load(Ordering::Relaxed);
        bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
        files_skipped = files_skipped_atomic.load(Ordering::Relaxed);
        bytes_skipped = bytes_skipped_atomic.load(Ordering::Relaxed);
    } else {
        // One source at a time, in `volume/copy_serial.rs`: too few sources to be
        // worth spawning tasks, or a backend that admits one operation at a time.
        let outcome = super::copy_serial::drive_transfer_serial(super::copy_serial::SerialCopy {
            events: Arc::clone(&events),
            operation_id,
            state,
            source_volume: Arc::clone(&source_volume),
            source_paths,
            dest_volume: Arc::clone(&dest_volume),
            dest_path,
            config,
            total_files,
            total_bytes,
            bulk_skip_files,
            bulk_skip_bytes,
            pre_skip_paths: &pre_skip_paths,
            source_hints: std::mem::take(&mut source_hints),
            progress_interval,
            journal_volumes: &journal_volumes,
            op_probe: &op_probe,
            apply_to_all_cell: Arc::clone(&apply_to_all_cell),
            copied_paths: Arc::clone(&copied_paths),
            created_dirs: Arc::clone(&created_dirs),
            deep_skipped_files: Arc::clone(&deep_skipped_files),
            deep_skipped_bytes: Arc::clone(&deep_skipped_bytes),
        })
        .await;
        files_done = outcome.files_done;
        bytes_done = outcome.bytes_done;
        files_skipped = outcome.files_skipped;
        bytes_skipped = outcome.bytes_skipped;
        last_dest_path = outcome.last_dest_path;
        copy_error = outcome.copy_error;
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
    super::cleanup::clean_abandoned_staged_writes(&dest_volume, state).await;

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
        clean_partial_writes(&dest_volume, &partials_to_clean, operation_id).await;

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

// The `volume/copy_tests.rs` suite was split for size. The crash-safety and
// rollback suites live in their own files; both share `make_state` /
// `make_volumes` from `tests` (`super::tests`). The bench suite is a single
// `#[ignore]`d, network-gated test.
#[cfg(test)]
#[path = "copy_bench.rs"]
mod bench;
#[cfg(test)]
#[path = "copy_cancel_tests.rs"]
mod cancel_tests;
#[cfg(test)]
#[path = "copy_concurrency_bench.rs"]
mod concurrency_bench;
#[cfg(test)]
#[path = "copy_concurrent_tests.rs"]
mod concurrent_tests;
#[cfg(test)]
#[path = "copy_crashsafe_tests.rs"]
mod crashsafe_tests;
#[cfg(test)]
#[path = "copy_extract_out_tests.rs"]
mod extract_out_tests;
#[cfg(test)]
#[path = "merge_tests.rs"]
mod merge_tests;
#[cfg(test)]
#[path = "copy_precheck_tests.rs"]
mod precheck_tests;
#[cfg(test)]
#[path = "copy_retry_tests.rs"]
mod retry_tests;
#[cfg(test)]
#[path = "copy_rollback_tests.rs"]
mod rollback_tests;
#[cfg(test)]
#[path = "copy_source_hint_tests.rs"]
mod source_hint_tests;
#[cfg(test)]
#[path = "copy_staged_write_tests.rs"]
mod staged_write_tests;
#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "copy_wedge_test_support.rs"]
mod wedge_test_support;
#[cfg(test)]
#[path = "copy_window_tests.rs"]
mod window_tests;
