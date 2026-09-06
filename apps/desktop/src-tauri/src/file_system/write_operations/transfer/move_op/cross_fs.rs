//! Cross-filesystem move: the staging path.
//!
//! `rename(2)` can't cross a filesystem boundary, so the move becomes a copy the
//! sources only lose once the copy is safely in place. Five phases: scan, copy
//! every file into a `.cmdr-staging-<op>` folder at the destination, rename the
//! staged tree into its final place, delete the originals, remove the staging
//! folder. The ordering IS the data-safety guarantee: nothing is deleted before
//! the destination is durable on disk, and Phase 4 removes a LEDGER of what
//! landed rather than whatever the source tree holds by then (`source_sweep.rs`),
//! so a Skip and a file that arrived mid-move both keep their original.
//!
//! The same-filesystem engine is `move_op::same_fs`; the dispatcher that picks
//! between them, and the conflict-landing and directory-merge helpers both
//! engines share, live in `move_op` itself.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::copy::{JournalDestUnder, copy_single_item, create_scanned_dirs_at_destination};
use super::MoveTransaction;
use super::merge_move_directory;
use super::move_resolved_into_place;
use super::rename_onto_free_name;
use super::source_sweep::{SourceSweep, delete_sources_after_move};

use crate::file_system::write_operations::cancellable::remove_dir_all_in_background;
use crate::file_system::write_operations::conflict::{ApplyToAll, IncomingItem, resolve_conflict};
use crate::file_system::write_operations::durability::flush_created_destinations;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::journal;
use crate::file_system::write_operations::ledger::CopyTransaction;
use crate::file_system::write_operations::scan::{SourceItemTracker, scan_sources};
use crate::file_system::write_operations::scan_cache::take_cached_scan_result;
use crate::file_system::write_operations::state::{WriteOperationState, update_operation_status};
use crate::file_system::write_operations::types::{
    WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use crate::file_system::write_operations::validation::{
    is_real_directory, path_exists_or_is_symlink, validate_file_sizes_for_filesystem,
};

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

    // The originals Phase 2 actually staged. Phase 4 removes THIS list, so a
    // file that appears in the source after the scan is never in the delete set
    // and a leaf the staging copy skipped keeps its original. ❌ Never rebuild
    // it from the source tree: by Phase 4 the tree can hold anything.
    let mut landed_files: Vec<PathBuf> = Vec::with_capacity(scan_result.files.len());
    // Original source paths whose copy never reached the destination. Phase 4
    // consults this for what it SAYS about a source (the ledger above already
    // decides what it removes). Holds whole top-level sources (a single-file /
    // type-mismatch Skip) and per-child paths inside a directory merge.
    let mut skipped_source_paths: HashSet<PathBuf> = HashSet::new();

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
            let staged_before = transaction.created_files().len();
            let verdict = copy_single_item(
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

            // ❗ The LEDGER is the witness here, ❌ never `verdict`. Deleting an
            // original is destructive, so what authorizes it is what actually
            // landed on disk, not what the policy decided: `copy_single_item`
            // records a file it wrote and records nothing at all for one it
            // walked past (a Skip on a clash inside the staging area, a
            // type-mismatch parent Skip, a same-file no-op). An original nothing
            // was written for must stay. `verdict` speaks only to REPORTING
            // below, where being wrong costs a wrong label rather than a file.
            if transaction.created_files().len() > staged_before {
                landed_files.push(file_info.path.clone());
            } else {
                skipped_source_paths.insert(file_info.path.clone());
            }

            if let Some(finished) = tracker.record(file_info, verdict) {
                events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: finished.source_path.display().to_string(),
                    // Staging only: the source is still on disk, and a Skip in
                    // the rename phase can mean it stays for good. Phase 4 emits
                    // again with `source_removed: true` for the ones it deletes.
                    source_removed: false,
                    outcome: finished.outcome,
                });
            }
        }
        Ok(())
    })();

    // Read before the error branch: a failed op emits no complete event, but
    // the tally has to be taken while the tracker is still in scope either way.
    let staging_skipped = tracker.skipped_top_level();

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

            // When both staged and final are real directories, merge
            // recursively. No MoveTransaction needed here: staging cleanup
            // handles rollback. A staged symlink is still a symlink, so it stays
            // a leaf and takes the conflict branch as a type mismatch.
            let mut staging_move_tx = MoveTransaction::new();
            if is_real_directory(&staged_path) && is_real_directory(&final_path) {
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
            } else if path_exists_or_is_symlink(&final_path) {
                // File conflict (or type mismatch). The symlink-aware gate, like
                // the other two engine branches: `exists()` alone reads a
                // dangling link at the destination as free.
                match resolve_conflict(
                    source,
                    &final_path,
                    IncomingItem::of_local_source(source),
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
                rename_onto_free_name(&staged_path, &final_path).map_err(|e| WriteOperationError::IoError {
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
    // disk), removing exactly what this move landed. A Skip in either phase
    // takes its original out of the ledger, so the sweep never reaches it.
    let sweep = SourceSweep::plan(
        sources,
        landed_files
            .into_iter()
            .filter(|file| !skipped_source_paths.iter().any(|skipped| file.starts_with(skipped)))
            .collect(),
        &scan_result,
        skipped_source_paths,
    );
    let delete_result = delete_sources_after_move(events, operation_id, state, sources, files_done, &sweep);

    // Phase 5: Remove the staging directory, on EVERY path out of Phase 4. Phase
    // 3 renamed the staged tree away, so this is an empty shell whichever way
    // Phase 4 ended; leaving it behind on a cancel puts a stray
    // `.cmdr-staging-<op>` folder in the user's destination for good. `remove_dir`
    // refuses a non-empty directory, so a surprise leaves the contents alone.
    let _ = fs::remove_dir(&staging_dir);
    let leftovers = delete_result?;

    // Emit completion. The move succeeded; anything the sweep left behind rides
    // along as typed data, so the toast can say what stayed and where.
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done + already_in_place,
        files_skipped,
        bytes_processed: bytes_done,
        appeared_during_move: leftovers.appeared_during_move(),
        // The STAGING phase's verdict, which is where a conflict is resolved.
        // A source phase 3 later declines to rename into place re-reports
        // itself on `write-source-item-done`; this summary is written once.
        top_level_skipped: Some(staging_skipped),
    });

    Ok(())
}
