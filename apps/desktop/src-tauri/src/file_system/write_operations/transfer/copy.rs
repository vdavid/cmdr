//! Copy implementation for write operations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use super::macos_copy::copy_symlink;

use super::super::helpers::{
    find_unique_name, is_same_file, path_exists_or_is_symlink, resolve_conflict, run_cancellable, spawn_async_sync,
    validate_disk_space, validate_path_length,
};
use super::super::scan::{
    SourceItemTracker, handle_dry_run, scan_sources, take_cached_scan_result, top_level_source_path,
};
use super::super::state::{
    CopyTransaction, OperationIntent, WriteOperationState, is_cancelled, load_intent, update_operation_status,
};
use super::super::types::{
    ConflictResolution, IoResultExt, OperationEventSink, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};
use super::chunked_copy::ChunkedCopyProgressFn;
use super::copy_strategy::copy_file_with_strategy;
use super::transfer_driver::{DriverConfig, PostLoopIntent, TransferOutcome, drive_transfer_serial_sync};

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

    // Phase 2: Copy files in sorted order with rollback support
    let mut transaction = CopyTransaction::new();
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();

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

            transaction.commit();
            spawn_async_sync();

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

/// Operation-wide context for the per-file milestone emit. Bundled into one
/// struct so the six emit sites in [`copy_single_item`] don't each restate
/// ten arguments to [`record_file_done`].
struct PerFileCtx<'a> {
    events: &'a dyn OperationEventSink,
    state: &'a Arc<WriteOperationState>,
    operation_id: &'a str,
    operation_type: WriteOperationType,
    files_total: usize,
    bytes_total: u64,
}

/// Marks one file as completed: bumps the cumulative counters and emits a
/// `Copying`-phase `WriteProgressEvent` carrying the bumped values.
///
/// Called from every `Ok`-return site in [`copy_single_item`] (regular file
/// copy, symlink copy, per-file Skip, type-mismatch parent Skip, same-file
/// no-op). Owning the milestone here — rather than in the driver's
/// `Transferred` arm — means both `copy_files_with_progress_inner` (which
/// goes through `drive_transfer_serial_sync`) and `move_with_staging` (which
/// calls `copy_single_item` directly inside its own copy loop) see the same
/// per-file milestone shape. Without that, single-file ops never see the FE's
/// files-done axis cross `0/1` before the dialog closes on the complete
/// event, because the chunked-copy callback (or an instant clonefile) leaves
/// the axis snapshotted at the pre-iteration value.
///
/// Fires unconditionally (no throttle): per-file milestones are bounded by
/// file count, and throttle suppression of this specific event is the bug
/// being fixed. The chunked intra-file emit inside `copy_single_item` keeps
/// its own throttle for the byte-axis stream.
fn record_file_done(
    ctx: &PerFileCtx<'_>,
    source: &Path,
    write_weight: u64,
    files_done: &mut usize,
    bytes_done: &mut u64,
) {
    *files_done += 1;
    *bytes_done += write_weight;
    super::transfer_driver::emit_progress_and_status(
        ctx.events,
        ctx.state,
        ctx.operation_id,
        ctx.operation_type,
        WriteOperationPhase::Copying,
        source.file_name().map(|n| n.to_string_lossy().to_string()),
        *files_done,
        ctx.files_total,
        *bytes_done,
        ctx.bytes_total,
    );
}

/// Copies a single file or symlink to its destination.
/// Ensures parent directories exist before copying.
/// Used by both copy and cross-filesystem move operations.
///
/// Note: The parent-directory-creation and conflict-resolution pattern here is similar to
/// `merge_move_directory` in `move_op.rs`. The duplication is intentional: copy has progress
/// tracking, symlink handling, byte counting, strategy selection, and transaction recording
/// that don't apply to same-FS move's simple rename. A shared abstraction would be forced.
#[allow(
    clippy::too_many_arguments,
    reason = "File copy requires passing state through multiple levels"
)]
pub(super) fn copy_single_item(
    source: &Path,
    dest_path: PathBuf,
    is_symlink: bool,
    // `write_weight` = the bytes this file contributes to copy's `bytes_total`
    // denominator, which is the write footprint: the file's full `size`, even
    // for a hardlink dupe (a cross-volume copy writes every link in full).
    // Threaded from `FileInfo::size`. Delete dedupes via `progress_bytes`
    // instead; copy never does. See `ScanResult::total_bytes` vs `dedup_bytes`.
    write_weight: u64,
    files_done: &mut usize,
    bytes_done: &mut u64,
    files_total: usize,
    bytes_total: u64,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: &Duration,
    config: &WriteOperationConfig,
    transaction: &mut CopyTransaction,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    let progress_ctx = PerFileCtx {
        events,
        state,
        operation_id,
        operation_type,
        files_total,
        bytes_total,
    };

    // Check cancellation
    if is_cancelled(&state.intent) {
        log::debug!(
            "copy: cancellation detected op={} files_done={}",
            operation_id,
            *files_done
        );
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Ensure parent directories exist
    if let Some(parent) = dest_path.parent()
        && !created_dirs.contains(parent)
    {
        // Fast path: parent already exists and is a directory; record it and skip the ancestor walk
        if parent.is_dir() {
            created_dirs.insert(parent.to_path_buf());
        } else {
            // Check for type mismatch: a file exists where we need a directory.
            // This happens when source has a directory and dest has a file with the same name.
            // Walk up from parent to find any file blocking directory creation.
            let blocking_file = {
                let mut check = parent.to_path_buf();
                let mut found: Option<PathBuf> = None;
                loop {
                    if check.exists() && !check.is_dir() {
                        found = Some(check);
                        break;
                    }
                    if check.exists() || created_dirs.contains(&check) {
                        break;
                    }
                    match check.parent() {
                        Some(p) => check = p.to_path_buf(),
                        None => break,
                    }
                }
                found
            };

            if let Some(blocking) = blocking_file {
                // A file exists where we need a directory: resolve as a conflict.
                // Use the blocking file path (not source) so the conflict dialog shows correct metadata.
                match resolve_conflict(
                    &blocking,
                    &blocking,
                    config,
                    events,
                    operation_id,
                    state,
                    apply_to_all_resolution,
                )? {
                    Some(resolved) if resolved.needs_safe_overwrite => {
                        // Overwrite: rename blocking file to backup, create directory, then delete backup.
                        // This is safe: if create_dir_all fails, we can restore the backup.
                        let backup_path = blocking.with_extension(format!(
                            "{}.cmdr-backup-{}",
                            blocking
                                .extension()
                                .map(|e| e.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            uuid::Uuid::new_v4()
                        ));
                        fs::rename(&blocking, &backup_path).with_path(&blocking)?;

                        if let Err(e) = fs::create_dir_all(parent) {
                            // Restore backup on failure
                            let _ = fs::rename(&backup_path, &blocking);
                            return Err(WriteOperationError::IoError {
                                path: parent.display().to_string(),
                                message: format!("Failed to create directory after removing blocking file: {}", e),
                            });
                        }

                        // Directory created successfully; delete backup in background
                        super::super::helpers::remove_file_in_background(backup_path);
                        log::debug!(
                            "copy: replaced file with directory at {} (type mismatch)",
                            blocking.display()
                        );
                    }
                    Some(_) => {
                        // Rename: preserve the blocking file by renaming it, then create directory
                        let unique_path = find_unique_name(&blocking);
                        fs::rename(&blocking, &unique_path).with_path(&blocking)?;
                        log::debug!(
                            "copy: renamed blocking file {} to {} (type mismatch)",
                            blocking.display(),
                            unique_path.display()
                        );
                    }
                    None => {
                        // Skip: don't copy this file. Use `write_weight`
                        // (not `metadata.len()`) so the dedup decision baked
                        // in by scan stays consistent across skip paths.
                        let _ = fs::symlink_metadata(source).with_path(source)?;
                        record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
                        return Ok(());
                    }
                }
            }

            if !parent.exists() {
                // Collect directories that don't exist BEFORE creating them
                // (so we know exactly which ones we're creating for rollback)
                let mut dirs_to_create: Vec<PathBuf> = Vec::new();
                let mut dir = parent.to_path_buf();
                while !dir.exists() && !created_dirs.contains(&dir) {
                    dirs_to_create.push(dir.clone());
                    match dir.parent() {
                        Some(p) => dir = p.to_path_buf(),
                        None => break,
                    }
                }

                // Create all directories
                fs::create_dir_all(parent).map_err(|e| WriteOperationError::IoError {
                    path: parent.display().to_string(),
                    message: format!("Failed to create directory: {}", e),
                })?;

                // Record only the directories we actually created (in creation order: deepest last)
                // dirs_to_create is in reverse order (deepest first), so iterate in reverse
                for created_dir in dirs_to_create.into_iter().rev() {
                    transaction.record_dir(created_dir.clone());
                    created_dirs.insert(created_dir);
                }
            }
        }
    }

    // Validate the source still exists (and isn't a vanished symlink target).
    // Byte accounting uses `write_weight` (the scan-time size), not a fresh
    // stat, so we only need the existence check here.
    let _ = fs::symlink_metadata(source).with_path(source)?;

    let file_name = source.file_name().unwrap_or_default();

    if is_symlink {
        // Handle symlink
        let (actual_dest, needs_safe_overwrite) = if path_exists_or_is_symlink(&dest_path) {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file but still count it toward progress
                    record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        if needs_safe_overwrite {
            if actual_dest.is_dir() {
                fs::remove_dir_all(&actual_dest).with_path(&actual_dest)?;
            } else {
                fs::remove_file(&actual_dest).with_path(&actual_dest)?;
            }
        }

        #[cfg(target_os = "macos")]
        {
            copy_symlink(source, &actual_dest)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let target = fs::read_link(source).map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: format!("Failed to read symlink: {}", e),
            })?;
            std::os::unix::fs::symlink(&target, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: actual_dest.display().to_string(),
                message: format!("Failed to create symlink: {}", e),
            })?;
        }

        transaction.record_file(actual_dest);
        record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
    } else {
        // Handle regular file
        // Pre-fix this branch used `dest_path.exists()`, which follows symlinks
        // and returns false for dangling symlinks. The copy then opened the
        // symlink target for writing — silent clobber or a confusing ENOENT.
        // `path_exists_or_is_symlink` mirrors the symlink branch above.
        let (actual_dest, needs_safe_overwrite) = if path_exists_or_is_symlink(&dest_path) {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file but still count it toward progress
                    record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
                    return Ok(());
                }
            }
        } else {
            (dest_path.clone(), false)
        };

        // Validate destination path length limits
        validate_path_length(&actual_dest)?;

        // Prevent copying a file over itself via symlinks (same inode + device)
        if is_same_file(source, &actual_dest) {
            log::warn!(
                "copy: skipping {}: source and destination resolve to the same file",
                source.display()
            );
            record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
            return Ok(());
        }

        // Check cancellation before copy
        if is_cancelled(&state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Copy file using appropriate strategy (network, safe overwrite, or native)
        // Create progress callback for intra-file progress reporting on network filesystems
        let base_bytes_done = *bytes_done;
        let current_file_name = file_name.to_string_lossy().to_string();
        let last_emit_time = std::cell::Cell::new(Instant::now());

        // Mid-file progress credits raw chunk bytes against the write-footprint
        // denominator: a copy writes every byte (including hardlink dupes), so
        // no dedup scaling — the bar tracks actual bytes hitting the disk.
        let progress_cb: ChunkedCopyProgressFn = &|chunk_bytes: u64, _total: u64| {
            if last_emit_time.get().elapsed() >= *progress_interval {
                let effective_bytes_done = base_bytes_done + chunk_bytes;
                log::debug!(
                    "copy: emitting chunked progress op={} files={}/{} bytes={}/{}",
                    operation_id,
                    *files_done,
                    files_total,
                    effective_bytes_done,
                    bytes_total
                );
                state.emit_progress_via_sink(
                    events,
                    WriteProgressEvent::new(
                        operation_id.to_string(),
                        operation_type,
                        WriteOperationPhase::Copying,
                        Some(current_file_name.clone()),
                        *files_done,
                        files_total,
                        effective_bytes_done,
                        bytes_total,
                    ),
                );
                update_operation_status(
                    operation_id,
                    WriteOperationPhase::Copying,
                    Some(current_file_name.clone()),
                    *files_done,
                    files_total,
                    effective_bytes_done,
                    bytes_total,
                );
                last_emit_time.set(Instant::now());
            }
        };

        let _bytes = copy_file_with_strategy(
            source,
            &actual_dest,
            needs_safe_overwrite,
            &state.intent,
            Some(progress_cb),
        )?;

        // Final accounting credits the full write weight (the file's size).
        // We use `write_weight` rather than the strategy's returned byte count
        // so the per-file milestone matches the scan's `total_bytes` exactly
        // even when a clonefile reports 0 copied bytes.
        transaction.record_file(actual_dest.clone());
        record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
    }

    Ok(())
}

// ============================================================================
// Tracked rollback
// ============================================================================

/// Rolls back created files with progress events, checking for cancellation between deletions.
///
/// Emits progress events with _decreasing_ `files_done` / `bytes_done` so the frontend's
/// progress bars count backwards from the cancellation point toward zero (no UI flicker,
/// no separate rollback view).
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it
/// (intent transitioned to `Stopped`). Does NOT call `transaction.rollback()` or
/// `transaction.commit()`. The caller must commit unconditionally (this function
/// already deleted whatever it deleted).
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
fn rollback_with_progress(
    transaction: &CopyTransaction,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    operation_type: WriteOperationType,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    let files_to_delete = transaction.created_files.len();
    let mut files_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event (same values as cancellation point)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::RollingBack,
            None,
            files_at_cancel,
            files_total,
            bytes_at_cancel,
            bytes_total,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::RollingBack,
        None,
        files_at_cancel,
        files_total,
        bytes_at_cancel,
        bytes_total,
    );

    // Delete files in reverse order (newest first), checking for cancellation
    for file in transaction.created_files.iter().rev() {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "rollback_with_progress: rollback cancelled at {}/{} files, keeping remaining",
                files_deleted,
                files_to_delete,
            );
            return false;
        }

        if let Err(e) = fs::remove_file(file) {
            log::warn!("rollback: failed to remove {}: {}", file.display(), e);
        }
        files_deleted += 1;

        // Throttled progress events with decreasing values
        if last_progress_time.elapsed() >= state.progress_interval {
            // Linearly interpolate bytes based on file deletion progress
            let remaining_files = files_at_cancel.saturating_sub(files_deleted);
            let remaining_bytes = if files_to_delete > 0 {
                bytes_at_cancel - (bytes_at_cancel as f64 * files_deleted as f64 / files_to_delete as f64) as u64
            } else {
                0
            };

            let current_file_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    operation_type,
                    WriteOperationPhase::RollingBack,
                    Some(current_file_name.clone()),
                    remaining_files,
                    files_total,
                    remaining_bytes,
                    bytes_total,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::RollingBack,
                Some(current_file_name),
                remaining_files,
                files_total,
                remaining_bytes,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete created directories (no progress events; this is fast)
    for dir in transaction.created_dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }

    true
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
