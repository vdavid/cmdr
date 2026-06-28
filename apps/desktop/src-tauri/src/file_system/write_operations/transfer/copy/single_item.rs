//! Per-file copy: `copy_single_item` plus its progress-milestone helper.
//!
//! Shared by `copy_files_with_progress_inner` (via the sync driver) and
//! `move_op::move_with_staging` (which calls `copy_single_item` directly).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use super::super::macos_copy::copy_symlink;

use super::super::chunked_copy::ChunkedCopyProgressFn;
use super::super::copy_strategy::copy_file_with_strategy;

use crate::file_system::write_operations::conflict::{ApplyToAll, resolve_conflict};
use crate::file_system::write_operations::overwrite::safe_overwrite_dir;
use crate::file_system::write_operations::state::{
    CopyTransaction, WriteOperationState, is_cancelled, update_operation_status,
};
use crate::file_system::write_operations::types::{
    IoResultExt, OperationEventSink, WriteOperationConfig, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};
use crate::file_system::write_operations::validation::{is_same_file, path_exists_or_is_symlink, validate_path_length};

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
    super::super::transfer_driver::emit_progress_and_status(
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
pub(in crate::file_system::write_operations::transfer) fn copy_single_item(
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
    apply_to_all_resolution: &mut ApplyToAll,
    created_dirs: &mut HashSet<PathBuf>,
    // Maps an intended dest-subtree root to the path it was actually landed at.
    // Populated when a folder→file Rename redirects the incoming folder from
    // `<dest>/name` (a file is there) to `<dest>/name (1)`; every subsequent
    // child of that subtree must follow the redirect. Empty in the common case.
    dir_remap: &mut HashMap<PathBuf, PathBuf>,
    // Destinations already made durable by the copy strategy (chunked copy's
    // inline `sync_data`) or for which a flush is moot (APFS clonefile /
    // reflink). The end-of-op flush pass skips these so a long chunked batch
    // isn't fsynced twice. See `durability::flush_created_destinations`.
    already_synced: &mut HashSet<PathBuf>,
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

    // Apply any active subtree redirect (folder→file Rename) so the rest of
    // this function — parent creation, conflict resolution, the copy itself —
    // operates on the remapped path. `apply_dir_remap` is a no-op when no
    // ancestor of `dest_path` was redirected (the overwhelmingly common case).
    let mut dest_path = super::apply_dir_remap(&dest_path, dir_remap);

    // Ensure parent directories exist
    if let Some(parent) = dest_path.parent().map(Path::to_path_buf)
        && !created_dirs.contains(&parent)
    {
        let parent = parent.as_path();
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
                // A file exists where we need a directory (folder→file clash):
                // resolve it. We pass the blocking file path (not source) so the
                // conflict dialog shows the existing file's metadata.
                //
                // `apply_resolution` distinguishes the two non-skip outcomes by
                // path: Overwrite returns `path == blocking` (replace in place),
                // Rename returns `path == find_unique_name(blocking)` (land the
                // incoming folder aside, keep the existing file). We branch on
                // that difference rather than on `needs_safe_overwrite`, which is
                // now `true` for both (Rename also reserves a placeholder it must
                // consume).
                match resolve_conflict(
                    &blocking,
                    &blocking,
                    config,
                    events,
                    operation_id,
                    state,
                    apply_to_all_resolution,
                )? {
                    Some(resolved) if resolved.path == blocking => {
                        // Folder→file OVERWRITE: the dest tree wants a directory at
                        // `blocking` but a file is there. Route through
                        // `safe_overwrite_dir` so the dest file is renamed aside, the
                        // directory is created in its place, and on `create_dir_all`
                        // failure the aside is rolled back. The subtree is populated
                        // lazily by subsequent iterations.
                        safe_overwrite_dir(&blocking, |target| {
                            fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
                                path: target.display().to_string(),
                                message: format!("Failed to create directory after removing blocking file: {}", e),
                            })?;
                            // Honor the caller's full parent chain (we may have been called
                            // for a deeper blocker than `target == parent`).
                            if parent != target {
                                fs::create_dir_all(parent).map_err(|e| WriteOperationError::IoError {
                                    path: parent.display().to_string(),
                                    message: format!("Failed to create child directory: {}", e),
                                })?;
                            }
                            Ok(())
                        })?;
                        log::debug!(
                            "copy: replaced file with directory at {} (type mismatch overwrite)",
                            blocking.display()
                        );
                    }
                    Some(resolved) => {
                        // Folder→file RENAME: keep the existing file at `blocking`,
                        // land the incoming folder (and its whole subtree) at the
                        // reserved unique name. `resolved.path` is a 0-byte placeholder
                        // file that `find_unique_name` reserved; consume it by removing
                        // it and creating the directory in its place (the reservation
                        // still holds the name against concurrent writers). Record the
                        // redirect so every later child of this subtree follows it.
                        let renamed_root = resolved.path;
                        let _ = fs::remove_file(&renamed_root);
                        fs::create_dir_all(&renamed_root).map_err(|e| WriteOperationError::IoError {
                            path: renamed_root.display().to_string(),
                            message: format!("Failed to create renamed directory: {}", e),
                        })?;
                        transaction.record_dir(renamed_root.clone());
                        created_dirs.insert(renamed_root.clone());
                        log::debug!(
                            "copy: landing incoming folder at {} (type mismatch rename; existing file {} kept)",
                            renamed_root.display(),
                            blocking.display()
                        );
                        dir_remap.insert(blocking.clone(), renamed_root.clone());
                        // Redirect the current item and recompute its parent.
                        dest_path = super::apply_dir_remap(&dest_path, dir_remap);
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

            // Honor any redirect applied above: the child's effective parent may
            // now be the renamed subtree root (folder→file Rename) instead of the
            // original `parent` (which still holds the kept dest file).
            let effective_parent = dest_path.parent().map(Path::to_path_buf);
            let parent = effective_parent.as_deref().unwrap_or(parent);

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

        // Materializer closure: create the symlink at the target path.
        let source_for_symlink = source.to_path_buf();
        let create_symlink = |target: &Path| -> Result<(), WriteOperationError> {
            // Register the symlink destination with the downloads watcher's
            // ignore set before issuing the syscall; no-ops outside ~/Downloads.
            crate::downloads::note_pending_write_for_cmdr(target);
            #[cfg(target_os = "macos")]
            {
                copy_symlink(&source_for_symlink, target)?;
            }
            #[cfg(not(target_os = "macos"))]
            {
                let link_target = fs::read_link(&source_for_symlink).map_err(|e| WriteOperationError::IoError {
                    path: source_for_symlink.display().to_string(),
                    message: format!("Failed to read symlink: {}", e),
                })?;
                std::os::unix::fs::symlink(&link_target, target).map_err(|e| WriteOperationError::IoError {
                    path: target.display().to_string(),
                    message: format!("Failed to create symlink: {}", e),
                })?;
            }
            Ok(())
        };

        if needs_safe_overwrite && actual_dest.is_dir() {
            // File→folder overwrite for the symlink branch: route the existing
            // dest folder through `safe_overwrite_dir` so a mid-delete crash
            // can't lose the folder. Pre-fix this branch did a direct
            // `fs::remove_dir_all` then created the symlink, which had no
            // crash-recoverable intermediate state.
            safe_overwrite_dir(&actual_dest, create_symlink)?;
        } else {
            if needs_safe_overwrite {
                fs::remove_file(&actual_dest).with_path(&actual_dest)?;
            }
            create_symlink(&actual_dest)?;
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

        // Register the destination with the downloads watcher's ignore set
        // before issuing the syscall; no-ops outside ~/Downloads. Placed
        // after the cancellation check so a cancel-just-before-write doesn't
        // leak an entry — the 5 s TTL would clean it up anyway, but this
        // keeps the map tighter under cancel-heavy workloads.
        crate::downloads::note_pending_write_for_cmdr(&actual_dest);

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

        let outcome = copy_file_with_strategy(
            source,
            &actual_dest,
            needs_safe_overwrite,
            &state.intent,
            Some(progress_cb),
        )?;
        // Byte accounting uses `write_weight` below (matches the scan's
        // `total_bytes` even when a clonefile reports 0 copied bytes), so the
        // strategy's own byte count is intentionally unused here.
        let _ = outcome.bytes;

        // If the strategy already flushed this file (chunked copy) or a flush
        // is moot (APFS clonefile / reflink), record it so the end-of-op flush
        // pass skips it. Strategies that leave bytes in the page cache
        // (Linux `copy_file_range`, the std fallback) are NOT recorded, so the
        // pass `fdatasync`s them before we report complete.
        if outcome.already_durable {
            already_synced.insert(actual_dest.clone());
        }

        // Final accounting credits the full write weight (the file's size).
        // We use `write_weight` rather than the strategy's returned byte count
        // so the per-file milestone matches the scan's `total_bytes` exactly
        // even when a clonefile reports 0 copied bytes.
        transaction.record_file(actual_dest.clone());
        record_file_done(&progress_ctx, source, write_weight, files_done, bytes_done);
    }

    Ok(())
}
