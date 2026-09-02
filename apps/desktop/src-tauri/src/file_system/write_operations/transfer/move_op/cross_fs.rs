//! Cross-filesystem move: the staging path.
//!
//! `rename(2)` can't cross a filesystem boundary, so the move becomes a copy the
//! sources only lose once the copy is safely in place. Five phases: scan, copy
//! every file into a `.cmdr-staging-<op>` folder at the destination, rename the
//! staged tree into its final place, delete the originals, remove the staging
//! folder. The ordering IS the data-safety guarantee: nothing is deleted before
//! the destination is durable on disk, and a source whose staged copy was
//! discarded on Skip is never deleted at all.
//!
//! The same-filesystem engine is `move_op::same_fs`; the dispatcher that picks
//! between them, and the conflict-landing and directory-merge helpers both
//! engines share, live in `move_op` itself.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::copy::{JournalDestUnder, copy_single_item, create_scanned_dirs_at_destination};
use super::MoveTransaction;
use super::merge_move_directory;
use super::move_resolved_into_place;

use crate::file_system::write_operations::cancellable::remove_dir_all_in_background;
use crate::file_system::write_operations::conflict::{ApplyToAll, resolve_conflict};
use crate::file_system::write_operations::durability::flush_created_destinations;
use crate::file_system::write_operations::error_classification::IoResultExt;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::journal;
use crate::file_system::write_operations::ledger::CopyTransaction;
use crate::file_system::write_operations::scan::{SourceItemTracker, scan_sources};
use crate::file_system::write_operations::scan_cache::take_cached_scan_result;
use crate::file_system::write_operations::state::{WriteOperationState, is_cancelled, update_operation_status};
use crate::file_system::write_operations::types::{
    CancelRollback, SourceItemOutcome, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use crate::file_system::write_operations::validation::validate_file_sizes_for_filesystem;

/// Performs cross-filesystem move using atomic staging pattern.
/// This ensures source files remain intact if the operation fails.
/// `already_in_place`: see `move_with_rename`.
pub(super) fn move_with_staging(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
    already_in_place: usize,
) -> Result<(), WriteOperationError> {
    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file move loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        if let Some(cached) = take_cached_scan_result(preview_id, sources).filter(|c| !c.files.is_empty()) {
            log::debug!(
                "move_with_staging: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
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
                WriteOperationType::Move,
                config.sort_column,
                config.sort_order,
            )?
        }
    } else {
        scan_sources(
            sources,
            state,
            events,
            operation_id,
            WriteOperationType::Move,
            config.sort_column,
            config.sort_order,
        )?
    };

    // Pre-flight filesystem-limit check: a cross-FS move stages a full copy, so
    // the destination's per-file cap (FAT32's 4 GiB) applies. Block before
    // creating the staging dir or writing a byte. No-op for filesystems with no
    // known limit. (Same-FS moves rename in place and never reach here.)
    validate_file_sizes_for_filesystem(destination, &scan_result.files)?;

    // Create staging directory
    let staging_dir = destination.join(format!(".cmdr-staging-{}", operation_id));
    fs::create_dir(&staging_dir).map_err(|e| WriteOperationError::IoError {
        path: staging_dir.display().to_string(),
        message: format!("Failed to create staging directory: {}", e),
    })?;

    // Phase 2: Copy files to staging directory (using scan results, same as copy operation)
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution = ApplyToAll::default();
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();
    let mut dir_remap: std::collections::HashMap<PathBuf, PathBuf> = std::collections::HashMap::new();
    // Durability bookkeeping. The Phase-2 copy records each per-file STAGING
    // dest into `transaction.created_files` and (when the strategy already
    // flushed it) into `already_synced`. Phase 3 renames the staging tree into
    // place, so by flush time the staging paths are gone. After Phase 3 we
    // remap both sets from the staging prefix to the final `destination` prefix
    // and flush the FINAL per-file dests — this closes the gap where the
    // Phase-3 renames-into-place (including the `throwaway_tx` path) aren't in
    // the real transaction. A same-volume rename leaves data blocks in place,
    // so on macOS the bytes are already durable (chunked) and the remapped
    // `fdatasync` is a cheap no-op that still makes the new directory entry
    // durable; on Linux (`copy_file_range` to staging) it's the real flush.
    let mut already_synced: HashSet<PathBuf> = HashSet::new();

    // Emit initial copying phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
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

    log::debug!(
        "move_with_staging: starting copy loop for operation_id={}, {} files",
        operation_id,
        scan_result.files.len()
    );

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    let copy_result: Result<(), WriteOperationError> = (|| {
        for file_info in &scan_result.files {
            // Pause gate at the file boundary. This is the phase that actually
            // moves bytes, so it's the one a user who hits Pause means: the
            // per-file cancel checks live inside `copy_single_item`, and this
            // park returns immediately once one of them is due to fire.
            state.pause_gate.wait_while_paused_sync(&state.intent);

            log::debug!(
                "move_with_staging: copying file {} ({} bytes) to staging",
                file_info.path.display(),
                file_info.size
            );
            // Copy to staging directory instead of final destination
            copy_single_item(
                &file_info.path,
                file_info.dest_path(&staging_dir),
                // Phase 3 renames the staging tree into place, so the journal
                // records where each file will live, not where it's written.
                Some(JournalDestUnder {
                    write_root: &staging_dir,
                    final_root: destination,
                }),
                file_info.is_symlink,
                // Write footprint: a cross-FS move stages a full copy of every
                // file (including hardlink dupes) before deleting the sources.
                file_info.size,
                &mut files_done,
                &mut bytes_done,
                scan_result.file_count,
                scan_result.total_bytes,
                state,
                events,
                operation_id,
                WriteOperationType::Move,
                &state.progress_interval,
                config,
                &mut transaction,
                &mut apply_to_all_resolution,
                &mut created_dirs,
                &mut dir_remap,
                &mut already_synced,
            )?;

            if let Some(source_path) = tracker.record(file_info) {
                events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                    // Staging only: the source is still on disk, and a Skip in
                    // the rename phase can mean it stays for good. Phase 4 emits
                    // again with `source_removed: true` for the ones it deletes.
                    source_removed: false,
                    outcome: SourceItemOutcome::Done,
                });
            }
        }
        Ok(())
    })();

    if let Err(e) = copy_result {
        // Cleanup staging directory in background (may block on network mounts)
        remove_dir_all_in_background(staging_dir.clone());
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            e.clone(),
        ));
        return Err(e);
    }

    // Stage the scanned directories the per-file loop didn't create: an empty
    // dir has no files, so it never staged, Phase 3's rename never moved it,
    // and Phase 4's source delete then DESTROYED it — gone from the source
    // without ever arriving at the destination. Staging it here lets it ride
    // the normal rename + cleanup machinery.
    if let Err(e) = create_scanned_dirs_at_destination(
        &scan_result.dirs,
        sources,
        &staging_dir,
        state,
        &mut transaction,
        &mut created_dirs,
        &dir_remap,
    ) {
        remove_dir_all_in_background(staging_dir.clone());
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            e.clone(),
        ));
        return Err(e);
    }

    // Original source paths whose staged copy was discarded on Skip (the file
    // never reached the destination). Phase 4 consults this so it deletes ONLY
    // sources that actually landed — deleting a skipped source's original would
    // be silent data loss (the user clicked Skip to keep both copies). Holds
    // both whole top-level sources (single-file / type-mismatch skip) and
    // per-child paths inside a directory merge.
    let mut skipped_source_paths: HashSet<PathBuf> = HashSet::new();

    // Phase 3: Atomic rename from staging to final destination
    let rename_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            // Pause gate at the item boundary, so every loop in the engine parks
            // on the same promise. The recursive merge below carries its own.
            state.pause_gate.wait_while_paused_sync(&state.intent);

            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".to_string(),
            })?;

            let staged_path = staging_dir.join(file_name);
            let final_path = destination.join(file_name);

            // When both staged and final are directories, merge recursively.
            // No MoveTransaction needed here: staging cleanup handles rollback.
            let mut staging_move_tx = MoveTransaction::new();
            if staged_path.is_dir() && final_path.exists() && final_path.is_dir() {
                // Collect skipped children as STAGED paths, then remap each from
                // the staging prefix back to its original source path so Phase 4
                // preserves the originals that never landed.
                let mut staged_skips: HashSet<PathBuf> = HashSet::new();
                merge_move_directory(
                    &staged_path,
                    &final_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                    &mut staging_move_tx,
                    &mut files_skipped,
                    &mut Some(&mut staged_skips),
                )?;
                for staged_skip in staged_skips {
                    if let Ok(rel) = staged_skip.strip_prefix(&staged_path) {
                        skipped_source_paths.insert(source.join(rel));
                    }
                }
                // Same rule as the same-FS merge: the destination folder also
                // holds files this operation never touched.
                journal::note_not_rollbackable(
                    operation_id,
                    crate::operation_log::types::NotRollbackableReason::DirectoryMerge,
                );
            } else if final_path.exists() {
                // File conflict (or type mismatch)
                match resolve_conflict(
                    source,
                    &final_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => {
                        // Cross-FS move: stage→final lands the file at its
                        // final visible name. Register so the watcher
                        // suppresses; no-ops outside ~/Downloads.
                        crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                        // Reuse the same Rename / Overwrite / type-mismatch logic the
                        // same-FS path uses, operating on the staged copy. The staged
                        // item mirrors the source's type, so `staged_path` drives the
                        // file-vs-dir decision correctly. The local `staging_move_tx`
                        // is throwaway here (staging cleanup handles rollback).
                        let mut throwaway_tx = MoveTransaction::new();
                        // Phase 2 already journaled this item against the staging
                        // area, rebased onto the CONFLICT-FREE final path. This
                        // resolution moves it somewhere else (a fresh `name (N)`)
                        // or over a file whose bytes are now gone, and rows can't
                        // be amended after the fact — so the operation says
                        // honestly that it can't be reversed.
                        journal::note_not_rollbackable(
                            operation_id,
                            if resolved.path == final_path {
                                crate::operation_log::types::NotRollbackableReason::Overwrote
                            } else {
                                crate::operation_log::types::NotRollbackableReason::StagedConflictResolved
                            },
                        );
                        move_resolved_into_place(&staged_path, &final_path, &resolved, None, &mut throwaway_tx)?;
                    }
                    None => {
                        // Skip: discard the staged copy and remember the original
                        // so Phase 4 doesn't delete it (it never landed).
                        if staged_path.is_dir() {
                            let _ = fs::remove_dir_all(&staged_path);
                        } else {
                            let _ = fs::remove_file(&staged_path);
                        }
                        skipped_source_paths.insert(source.clone());
                        files_skipped += 1;
                        // The rows phase 2 wrote for this source name a
                        // destination that now holds the file the user chose to
                        // keep. Reversing them would carry THAT file to the
                        // source.
                        journal::note_not_rollbackable(
                            operation_id,
                            crate::operation_log::types::NotRollbackableReason::StagedConflictResolved,
                        );
                        continue;
                    }
                }
            } else {
                // No conflict, just rename from staging to final
                crate::downloads::note_pending_write_for_cmdr(&final_path);
                fs::rename(&staged_path, &final_path).map_err(|e| WriteOperationError::IoError {
                    path: staged_path.display().to_string(),
                    message: format!("Failed to move from staging: {}", e),
                })?;
            }
        }
        Ok(())
    })();

    if let Err(e) = rename_result {
        // Cleanup staging directory in background (may block on network mounts)
        remove_dir_all_in_background(staging_dir);
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            e.clone(),
        ));
        return Err(e);
    }

    // Durability MUST run BEFORE Phase 4's source delete. The source originals
    // are the only other copy of the data; deleting them before the Phase-3
    // rename-into-place is durable on disk widens the crash window — on power
    // loss in that gap the file could be absent from its final path while the
    // source is already gone. So we flush the final dests (and fsync their
    // parent dir entries) here, upholding the move invariant "never delete the
    // source if the destination isn't fully in place." Zero happy-path cost:
    // the files were already data-synced in Phase 2; this only reorders the
    // dir-entry fsync ahead of the delete.
    //
    // Remap the Phase-2 staging dests to their final paths (Phase 3 renamed
    // staging → destination). Emits a `Flushing`-phase event first so the FE
    // shows "Writing the last piece…".
    let remap = |p: &Path| -> PathBuf {
        match p.strip_prefix(&staging_dir) {
            Ok(rel) => destination.join(rel),
            // Shouldn't happen (every staging dest is under staging_dir), but
            // fall back to the original path rather than dropping it.
            Err(_) => p.to_path_buf(),
        }
    };
    let final_dests: Vec<PathBuf> = transaction.created_file_paths().iter().map(|p| remap(p)).collect();
    let final_already_synced: HashSet<PathBuf> = already_synced.iter().map(|p| remap(p)).collect();
    // Journal the destination directories this move created, under the paths they
    // LIVE at — the same staging→final rebase the leaf rows already got, since
    // phase 3 renamed the tree out of `.cmdr-staging-<op>/` moments ago. Without
    // these rows a reversal puts every file back and leaves the moved folder's
    // empty skeleton at the destination. They land after every leaf row (phase 2
    // wrote those), so a `seq DESC` reversal still removes files before dirs.
    let final_dirs: Vec<PathBuf> = transaction.created_dirs.iter().map(|p| remap(p)).collect();
    journal::record_created_dirs(operation_id, &final_dirs);
    // The staging tree is renamed into place, so nothing this transaction recorded
    // is still a partial to clean up: commit, or the `Drop` net runs a pointless
    // rollback over paths that moved.
    transaction.commit();
    flush_created_destinations(
        events,
        operation_id,
        WriteOperationType::Move,
        state,
        files_done,
        scan_result.file_count,
        bytes_done,
        scan_result.total_bytes,
        &final_dests,
        &final_already_synced,
    );

    // Phase 4: Delete source files (only after the destination is durable on
    // disk), skipping any source (or source child) whose copy was discarded on
    // Skip.
    let delete_result =
        delete_sources_after_move(events, operation_id, state, sources, files_done, &skipped_source_paths);

    // Phase 5: Remove the staging directory, on EVERY path out of Phase 4. Phase
    // 3 renamed the staged tree away, so this is an empty shell whichever way
    // Phase 4 ended; leaving it behind on a cancel puts a stray
    // `.cmdr-staging-<op>` folder in the user's destination for good. `remove_dir`
    // refuses a non-empty directory, so a surprise leaves the contents alone.
    let _ = fs::remove_dir(&staging_dir);
    delete_result?;

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done + already_in_place,
        files_skipped,
        bytes_processed: bytes_done,
    });

    Ok(())
}

/// Deletes the originals after a successful cross-FS copy+rename, preserving any
/// source (or source child) listed in `skipped_source_paths` — those never
/// reached the destination, so deleting them would be data loss.
///
/// A whole top-level source in the skip set (single-file / type-mismatch Skip)
/// is left untouched. A directory source with NO skipped descendants is removed
/// wholesale via `remove_dir_all`. A directory source WITH skipped descendants
/// is walked: every non-skipped child is deleted and directories are removed
/// only once they're empty, so the skipped child's original survives inside a
/// surviving source directory.
///
/// ## Why this phase reports progress
///
/// It is the last real work of the move and it is unbounded: one `remove_file`
/// or `remove_dir_all` per top-level source, over however large a tree. Running
/// it silently left the frontend on the copy phase's last tick, `files_done ==
/// files_total`, so the dialog read 100% (and "Paused" over a full bar if the
/// user parked it here) with the whole sweep still ahead — the exact "looks
/// finished when it isn't" the honest-progress principle forbids.
///
/// The denominator is the TOP-LEVEL sources, because that's what the loop
/// iterates: `remove_dir_all` takes a subtree in one call and reports nothing
/// from inside it, so a leaf-granular bar here would be an invention. Bytes stay
/// zero throughout — nothing is transferred — which is how the readout knows to
/// drop its size bar rather than freeze it at whatever the copy left.
fn delete_sources_after_move(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
    skipped_source_paths: &HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    let sources_total = sources.len();
    let mut sources_done = 0usize;
    let mut last_progress_time = Instant::now();

    // The opening tick, unthrottled: it's what flips the frontend off the copy's
    // full bar and onto this phase's own, and the first source can take minutes.
    emit_source_sweep_progress(events, state, operation_id, None, 0, sources_total);

    for source in sources {
        // Check cancellation
        if is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_done,
                rollback: CancelRollback::none(), // Source deletion phase - nothing to rollback
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Pause gate, after the cancel check like every other loop in the
        // engine. The destination is already durable by now, so parking here
        // holds the originals in place, which is exactly what a paused move
        // should look like.
        state.pause_gate.wait_while_paused_sync(&state.intent);

        // A whole top-level source skipped on a file / type-mismatch conflict:
        // leave the original exactly where it is, and say so. The staging phase
        // already reported this source as `Done` (it staged fine); this later
        // event is the operation's real verdict on it, which is why the LAST
        // event a source gets is the one that counts (`SourceItemOutcome`).
        if skipped_source_paths.contains(source) {
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
                source_removed: false,
                outcome: SourceItemOutcome::Skipped,
            });
            // Counted anyway: the bar measures how far through the sources the
            // sweep has got, and a deliberate skip is as finished with as a
            // deletion. Leaving it out would strand the bar short of full.
            sources_done += 1;
            continue;
        }

        // Use symlink_metadata to check if it still exists
        if fs::symlink_metadata(source).is_ok() {
            if source.is_dir() {
                // Fast path: nothing under this source was skipped, so the whole
                // tree landed and can be removed wholesale.
                let has_skipped_descendant = skipped_source_paths.iter().any(|p| p.starts_with(source));
                if has_skipped_descendant {
                    delete_dir_preserving_skipped(source, skipped_source_paths)?;
                } else {
                    fs::remove_dir_all(source).with_path(source)?;
                }
            } else {
                fs::remove_file(source).with_path(source)?;
            }

            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
                // Phase 4 only reaches here for a source it just removed; a
                // skipped one is reported above.
                source_removed: true,
                outcome: SourceItemOutcome::Done,
            });
        }

        sources_done += 1;
        if last_progress_time.elapsed() >= state.progress_interval {
            let name = source.file_name().map(|n| n.to_string_lossy().into_owned());
            emit_source_sweep_progress(events, state, operation_id, name, sources_done, sources_total);
            last_progress_time = Instant::now();
        }
    }

    // The closing tick, unthrottled for the same reason as the opening one: the
    // throttle would otherwise leave the bar short of full on a fast sweep, and
    // the next thing the user sees is `write-complete`.
    emit_source_sweep_progress(events, state, operation_id, None, sources_done, sources_total);

    Ok(())
}

/// One `Deleting`-phase tick for the source sweep, paired with its status-cache
/// update so no caller can emit one without the other.
fn emit_source_sweep_progress(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    current_file: Option<String>,
    sources_done: usize,
    sources_total: usize,
) {
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            WriteOperationPhase::Deleting,
            current_file.clone(),
            sources_done,
            sources_total,
            0,
            0,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Deleting,
        current_file,
        sources_done,
        sources_total,
        0,
        0,
    );
}

/// Recursively deletes `dir`'s contents, skipping any path in
/// `skipped_source_paths`, and removes a directory only once it's empty. A
/// directory that still holds a skipped child (directly or transitively) is
/// left in place. Used by the cross-FS source-delete phase when some children
/// were Skipped and the parent therefore can't be removed wholesale.
fn delete_dir_preserving_skipped(
    dir: &Path,
    skipped_source_paths: &HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    let entries = fs::read_dir(dir).with_path(dir)?;
    for entry in entries {
        let child = entry.with_path(dir)?.path();
        if skipped_source_paths.contains(&child) {
            continue;
        }
        if fs::symlink_metadata(&child).map(|m| m.is_dir()).unwrap_or(false) {
            let has_skipped_descendant = skipped_source_paths.iter().any(|p| p.starts_with(&child));
            if has_skipped_descendant {
                delete_dir_preserving_skipped(&child, skipped_source_paths)?;
            } else {
                fs::remove_dir_all(&child).with_path(&child)?;
            }
        } else {
            fs::remove_file(&child).with_path(&child)?;
        }
    }

    // Remove the directory only if it's now empty (a skipped child keeps it).
    if fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
        fs::remove_dir(dir).with_path(dir)?;
    }
    Ok(())
}
