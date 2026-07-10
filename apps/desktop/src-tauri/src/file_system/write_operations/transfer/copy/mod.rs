//! Copy implementation for write operations.
//!
//! `copy_files_with_progress_inner` orchestrates the local-FS copy: scan, disk-
//! space preflight, the per-file loop (driven through `transfer_driver`), and
//! post-loop completion / cancel / rollback. The per-file work, empty-dir
//! landing, and tracked rollback live in the submodules.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::cancellable::run_cancellable;
use super::super::conflict::ApplyToAll;
use super::super::durability::flush_created_destinations;
use super::super::scan::{
    SourceItemTracker, handle_dry_run, scan_sources, take_cached_scan_result, top_level_source_path,
};
use super::super::state::{
    CopyTransaction, OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use super::super::types::{
    ConflictResolution, OperationEventSink, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};
use super::super::validation::{validate_disk_space, validate_file_sizes_for_filesystem};
use super::transfer_driver::{DriverConfig, PostLoopIntent, TransferOutcome, drive_transfer_serial_sync};

mod rollback;
mod scanned_dirs;
mod single_item;

use rollback::rollback_with_progress;
pub(super) use scanned_dirs::create_scanned_dirs_at_destination;
pub(super) use single_item::copy_single_item;

// ============================================================================
// Cancellation-aware helpers
// ============================================================================

/// Runs `validate_disk_space` with polling-based cancellation.
/// This ensures we respond quickly to cancellation even if `statvfs` blocks on slow network drives.
fn validate_disk_space_cancellable(
    destination: &Path,
    required_bytes: u64,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
) -> Result<(), WriteOperationError> {
    let destination = destination.to_path_buf();
    run_cancellable(
        move || validate_disk_space(&destination, required_bytes),
        state,
        "disk_space_check",
        operation_id,
    )
}

/// Rewrites `dest` by replacing the longest ancestor that appears as a key in
/// `dir_remap` with its mapped value. Used to follow a folder→file Rename
/// redirect: once the subtree root `<dest>/name` is redirected to
/// `<dest>/name (1)`, every child path `<dest>/name/child` becomes
/// `<dest>/name (1)/child`. Returns `dest` unchanged when no ancestor is
/// remapped (the common case, so the map is almost always empty).
pub(super) fn apply_dir_remap(dest: &Path, dir_remap: &HashMap<PathBuf, PathBuf>) -> PathBuf {
    if dir_remap.is_empty() {
        return dest.to_path_buf();
    }
    // Find the longest mapped ancestor so nested redirects compose correctly.
    let mut best: Option<(&PathBuf, &PathBuf)> = None;
    for (from, to) in dir_remap {
        if dest.starts_with(from) && best.is_none_or(|(b, _)| from.as_os_str().len() > b.as_os_str().len()) {
            best = Some((from, to));
        }
    }
    match best {
        Some((from, to)) => {
            // `strip_prefix` can't fail here: `starts_with(from)` held above.
            let suffix = dest.strip_prefix(from).unwrap_or(Path::new(""));
            to.join(suffix)
        }
        None => dest.to_path_buf(),
    }
}

// ============================================================================
// Copy implementation
// ============================================================================

pub(in crate::file_system::write_operations) fn copy_files_with_progress_inner(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    log::debug!(
        "copy_files_with_progress: starting operation_id={}, {} sources",
        operation_id,
        sources.len()
    );

    // Handle dry-run mode
    if handle_dry_run(
        config.dry_run,
        sources,
        destination,
        state,
        events,
        operation_id,
        WriteOperationType::Copy,
        state.progress_interval,
        config.max_conflicts_to_show,
    )? {
        return Ok(());
    }

    // Phase 1: Scan (or reuse cached preview results)
    let mut scan_result = if let Some(preview_id) = &config.preview_id {
        // Try to reuse cached scan results from preview.
        // Volume scans (MTP, etc.) cache aggregate stats only (empty `files` list).
        // This per-file copy path needs the file list, so treat an empty-files cache
        // hit the same as a miss and fall through to a fresh local scan.
        if let Some(cached) = take_cached_scan_result(preview_id).filter(|c| !c.files.is_empty()) {
            log::debug!(
                "copy_files_with_progress: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
            // Cache miss despite frontend coordination: scan may not have completed yet
            log::warn!(
                "preview_id={} cache miss despite frontend coordination, starting fresh scan for operation_id={}",
                preview_id,
                operation_id
            );
            scan_sources(
                sources,
                state,
                events,
                operation_id,
                WriteOperationType::Copy,
                config.sort_column,
                config.sort_order,
            )?
        }
    } else {
        // No preview ID, do normal scan
        log::debug!(
            "copy_files_with_progress: starting scan phase for operation_id={}",
            operation_id
        );
        scan_sources(
            sources,
            state,
            events,
            operation_id,
            WriteOperationType::Copy,
            config.sort_column,
            config.sort_order,
        )?
    };
    log::debug!(
        "copy_files_with_progress: scan complete for operation_id={}, files={}, bytes={}",
        operation_id,
        scan_result.file_count,
        scan_result.total_bytes
    );

    // Pre-flight disk space check: verify destination has enough free space
    // Use polling-based cancellation to remain responsive on slow network drives
    log::debug!(
        "copy_files_with_progress: starting disk space check for operation_id={}",
        operation_id
    );
    validate_disk_space_cancellable(destination, scan_result.total_bytes, state, operation_id)?;
    log::debug!(
        "copy_files_with_progress: disk space check complete for operation_id={}",
        operation_id
    );

    // Pre-flight filesystem-limit check: block before writing a byte if any file
    // is too large for the destination filesystem (FAT32's 4 GiB cap). No-op for
    // filesystems with no known limit.
    validate_file_sizes_for_filesystem(destination, &scan_result.files)?;

    // Phase 2: Copy files in sorted order with rollback support
    let mut transaction = CopyTransaction::new();
    let mut apply_to_all_resolution = ApplyToAll::default();
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();
    let mut dir_remap: HashMap<PathBuf, PathBuf> = HashMap::new();
    // Destinations the copy strategy already flushed (chunked) or for which a
    // flush is moot (clonefile/reflink); the end-of-op flush pass skips these.
    let mut already_synced: HashSet<PathBuf> = HashSet::new();

    // Emit initial copying phase event (important when reusing cached scan - no scanning events were
    // emitted)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::Copying,
            None,
            0,
            scan_result.file_count,
            0,
            scan_result.total_bytes,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        0,
        scan_result.file_count,
        0,
        scan_result.total_bytes,
    );

    // Bulk-skip pre-known conflicts (Skip mode only). For local↔local, the
    // per-file `get_metadata` is cheap (microseconds), so the user-facing bug
    // is much milder than the cross-volume case — but we still want the bar
    // to reflect the user's "Skip all" decision immediately. The set is keyed
    // by the absolute top-level source path (matching what
    // `top_level_source_path(file_info)` returns). We filter `scan_result.files`
    // pre-loop so the driver iterates only the surviving files; bulk-skip
    // counters are handed to the driver via `bulk_skip_files` / `bulk_skip_bytes`
    // and the driver emits the one bulk progress event.
    //
    // **Directories never bulk-skip.** A top-level directory's name matching a
    // pre-known conflict only means SOME of its children collide at dest. The
    // bulk-skip would drop the whole subtree, including non-conflicting
    // children. We exclude directories here and fall through to per-iter
    // conflict resolution inside the copy loop (where each conflicting child
    // gets skipped individually while non-conflicting ones copy). Symlinks
    // count as files (they're replaced atomically, not merged), so they stay
    // in the bulk-skip set.
    let pre_skip_top_levels: HashSet<PathBuf> =
        if config.conflict_resolution == ConflictResolution::Skip && !config.pre_known_conflicts.is_empty() {
            let names: HashSet<&str> = config.pre_known_conflicts.iter().map(String::as_str).collect();
            sources
                .iter()
                .filter(|p| {
                    let name_matches = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| names.contains(n))
                        .unwrap_or(false);
                    if !name_matches {
                        return false;
                    }
                    // Only stat candidates whose filenames match (typically
                    // few). `symlink_metadata` keeps symlinks classified as
                    // files. If the stat fails (race / permission denied),
                    // fall back to NOT bulk-skipping — safer to let the loop
                    // discover the conflict and resolve it per-iter than to
                    // wholesale drop a subtree we couldn't classify.
                    fs::symlink_metadata(p).map(|m| !m.is_dir()).unwrap_or(false)
                })
                .cloned()
                .collect()
        } else {
            HashSet::new()
        };

    let mut bulk_skip_files = 0usize;
    let mut bulk_skip_bytes = 0u64;
    if !pre_skip_top_levels.is_empty() {
        scan_result.files.retain(|fi| {
            let top = top_level_source_path(fi);
            if pre_skip_top_levels.contains(&top) {
                bulk_skip_files += 1;
                // Full `size` (write footprint) so the bulk-skip seed agrees
                // with the `total_bytes` denominator copy uses. Copy writes
                // every hardlink in full, so each counts toward the bar.
                bulk_skip_bytes += fi.size;
                false
            } else {
                true
            }
        });
        log::info!(
            "copy_files_with_progress: bulk-skipping {} files ({} bytes) for {} pre-known conflicting top-level sources",
            bulk_skip_files,
            bulk_skip_bytes,
            pre_skip_top_levels.len()
        );
    }

    log::debug!(
        "copy_files_with_progress: starting copy loop for operation_id={}, {} files",
        operation_id,
        scan_result.files.len()
    );

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    // The sync driver iterates a `&[PathBuf]`; the closure needs the matching
    // `FileInfo` per iteration (for `dest_path`, `is_symlink`, `size`, and the
    // tracker key). We pre-collect aligned `paths` and walk `files` via an
    // iterator the closure owns, advancing it in lock-step with the driver.
    // Driver `pre_skip_paths` is empty: we already filtered above.
    let files_for_loop: Vec<_> = scan_result.files.iter().collect();
    let source_paths: Vec<PathBuf> = files_for_loop.iter().map(|fi| fi.path.clone()).collect();
    let file_count = scan_result.file_count;
    let total_bytes = scan_result.total_bytes;
    let progress_interval = state.progress_interval;
    let empty_pre_skip: HashSet<PathBuf> = HashSet::new();
    let driver_config = DriverConfig {
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: config.conflict_resolution,
        pre_known_conflicts: config.pre_known_conflicts.clone(),
        // Sync driver ignores this; `copy_single_item` owns its emits.
        emit_per_source_milestone: false,
    };

    let mut file_iter = files_for_loop.iter();

    let outcome = drive_transfer_serial_sync(
        events,
        state,
        operation_id,
        &source_paths,
        file_count,
        total_bytes,
        bulk_skip_files,
        bulk_skip_bytes,
        &empty_pre_skip,
        &driver_config,
        |ctx| {
            let file_info = file_iter
                .next()
                .expect("file_iter aligned with driver iteration over source_paths");
            log::debug!(
                "copy_files_with_progress: copying file {} ({} bytes)",
                file_info.path.display(),
                file_info.size
            );
            // `copy_single_item` bumps `files_done` / `bytes_done` by reference
            // and uses them to emit cumulative progress; seed with the
            // driver-supplied cumulative snapshot so emitted events stay
            // consistent across iterations.
            let mut local_files = ctx.files_done_so_far;
            let mut local_bytes = ctx.bytes_done_so_far;
            copy_single_item(
                &file_info.path,
                file_info.dest_path(destination),
                file_info.is_symlink,
                file_info.size,
                &mut local_files,
                &mut local_bytes,
                file_count,
                total_bytes,
                state,
                ctx.events,
                operation_id,
                WriteOperationType::Copy,
                &progress_interval,
                config,
                &mut transaction,
                &mut apply_to_all_resolution,
                &mut created_dirs,
                &mut dir_remap,
                &mut already_synced,
            )?;
            let bytes_delta = local_bytes.saturating_sub(ctx.bytes_done_so_far);

            if let Some(source_path) = tracker.record(file_info) {
                ctx.events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                });
            }

            // E2E-only per-file throttle. In production (env + IPC override both
            // unset), `effective_copy_throttle_ms()` returns None and this is a
            // single atomic load (zero added latency). Under E2E it gives the
            // spec a deterministic window to click Cancel/Rollback. Strictly
            // additive: see `crate::test_mode` for the convention.
            if let Some(ms) = crate::test_mode::effective_copy_throttle_ms()
                && ms > 0
            {
                std::thread::sleep(Duration::from_millis(ms));
            }

            Ok(TransferOutcome::Transferred { bytes: bytes_delta })
        },
    );

    let files_done = outcome.files_done;
    let bytes_done = outcome.bytes_done;

    match outcome.intent {
        PostLoopIntent::Completed => {
            // The loop succeeded, but the user may have clicked Rollback between the last
            // file's `is_cancelled` check and the loop's exit (or, with APFS clonefile, the
            // whole 170 MB / 23 file copy can finish in <100 ms so the click lands after the
            // loop completes but before this match arm runs). Honor the rollback intent
            // before emitting write-complete: if we don't, the user explicitly requested
            // "delete what was copied" and got "everything's still there" instead.
            if load_intent(&state.intent) == OperationIntent::RollingBack {
                log::info!(
                    "copy_files_with_progress: rollback requested after loop completion op={}, {} files",
                    operation_id,
                    transaction.created_files.len()
                );
                let rollback_completed = rollback_with_progress(
                    &transaction,
                    events,
                    operation_id,
                    state,
                    WriteOperationType::Copy,
                    files_done,
                    bytes_done,
                    file_count,
                    total_bytes,
                );
                transaction.commit();

                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    files_processed: files_done,
                    rolled_back: rollback_completed,
                });
                return Ok(());
            }

            // Land the scanned directories the per-file loop didn't create:
            // empty dirs (and branches of only empty dirs) have no files, so
            // without this they'd silently never arrive at the destination.
            // Outcomes mirror the loop's arms: Cancelled keeps what's copied
            // (commit, so the Drop safety-net can't roll it back), any other
            // error rolls back like `PostLoopIntent::Failed`.
            if let Err(e) = create_scanned_dirs_at_destination(
                &scan_result.dirs,
                sources,
                destination,
                state,
                &mut transaction,
                &mut created_dirs,
                &dir_remap,
            ) {
                if matches!(e, WriteOperationError::Cancelled { .. }) {
                    transaction.commit();
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        files_processed: files_done,
                        rolled_back: false,
                    });
                } else {
                    transaction.rollback();
                    events.emit_error(WriteErrorEvent::new(
                        operation_id.to_string(),
                        WriteOperationType::Copy,
                        e.clone(),
                    ));
                }
                return Err(e);
            }

            // Flush every created destination to disk before reporting
            // complete, so "complete" means durable. Reuses the transaction's
            // own `created_files`; skips paths the strategy already flushed.
            // Emits a `Flushing`-phase event first so the FE shows "Writing the
            // last piece…" instead of a bar frozen at 100% on slow media.
            flush_created_destinations(
                events,
                operation_id,
                WriteOperationType::Copy,
                state,
                files_done,
                file_count,
                bytes_done,
                total_bytes,
                &transaction.created_files,
                &already_synced,
            );
            // Journal the directories this copy created as `dir` rows, after the
            // leaf files (which recorded themselves as they landed), so a `seq
            // DESC` rollback removes files before their dirs (D2, Finding 2).
            crate::file_system::write_operations::journal::record_created_dirs(operation_id, &transaction.created_dirs);
            transaction.commit();

            log::info!(
                "copy_files_with_progress: completed op={} files={} bytes={}",
                operation_id,
                files_done,
                bytes_done
            );

            events.emit_complete(WriteCompleteEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                files_processed: files_done,
                files_skipped: outcome.files_skipped,
                bytes_processed: bytes_done,
            });
            Ok(())
        }
        PostLoopIntent::Cancelled => {
            let cancellation = WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            };
            match load_intent(&state.intent) {
                OperationIntent::RollingBack => {
                    // User requested rollback: tracked rollback with progress events.
                    // Pass the progress state at cancellation so the frontend sees
                    // the bars counting backwards from where they were.
                    log::info!(
                        "copy_files_with_progress: rolling back op={}, {} files",
                        operation_id,
                        transaction.created_files.len()
                    );
                    let rollback_completed = rollback_with_progress(
                        &transaction,
                        events,
                        operation_id,
                        state,
                        WriteOperationType::Copy,
                        files_done,
                        bytes_done,
                        file_count,
                        total_bytes,
                    );
                    transaction.commit();
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        files_processed: files_done,
                        rolled_back: rollback_completed,
                    });
                }
                _ => {
                    // Stopped (or unknown): keep partial files. `transaction.commit()`
                    // prevents the `Drop` safety-net from rolling back what the user
                    // chose to keep.
                    log::info!(
                        "copy_files_with_progress: cancelled op={}, keeping {} partial files",
                        operation_id,
                        transaction.created_files.len()
                    );
                    transaction.commit();
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        files_processed: files_done,
                        rolled_back: false,
                    });
                }
            }
            Err(cancellation)
        }
        PostLoopIntent::Failed(e) => {
            // Non-cancellation error - always rollback. Routed through `log_error!`
            // so opt-in users get an auto error report (copy failures are exactly
            // the kind of "this didn't work" we want signal on).
            crate::log_error!(
                "copy_files_with_progress: failed op={} error={:?}, rolling back",
                operation_id,
                e,
            );
            transaction.rollback();
            events.emit_error(WriteErrorEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                e.clone(),
            ));
            Err(e)
        }
    }
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
