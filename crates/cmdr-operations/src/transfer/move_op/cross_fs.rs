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

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{LandedMetadata, apply_landed};

use super::super::copy::{
    JournalDestUnder, apply_created_dirs_metadata, apply_dir_remap, copy_single_item,
    create_scanned_dirs_at_destination, dir_dest_path,
};
use super::MoveTransaction;
use super::merge_move_directory;
use super::move_resolved_into_place;
use super::rename_onto_free_name;
use super::source_sweep::{LandedOriginal, SourceStamp, SourceSweep, delete_sources_after_move};

use crate::cancellable::remove_staging_tree_in_background;
use crate::conflict::{ApplyToAll, IncomingItem, resolve_conflict};
use crate::durability::flush_created_destinations;
use crate::event_sinks::OperationEventSink;
use crate::in_flight_temps::{self, ItemKind};
use crate::journal;
use crate::ledger::CopyTransaction;
use crate::locked_sources::{LockedPlan, LockedScan, plan_locked_local};
use crate::scan::scan_sources;
use crate::scan_source_tracker::SourceItemTracker;
use crate::state::WriteOperationState;
use crate::transfer_sides::transfer_stop_event;
use crate::transfer_tree::{TreeLanding, TreeOrder};
use crate::types::{
    WriteCompleteEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent, WriteSourceItemDoneEvent,
};
use crate::validation::{is_real_directory, path_exists_or_is_symlink, validate_file_sizes_for_filesystem};
use cmdr_fs::staging::StagingTemp;

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
    super::super::super::validation::validate_transfer_destination_name(
        sources,
        destination,
        config.destination_name.as_deref(),
    )?;
    // A source that is or holds a mount point would have the drive's files
    // copied off it and then deleted there. Refused before Phase 1, so neither
    // a fresh scan nor a cached preview's file list gets the chance.
    crate::mount_boundary::refuse_moving_a_mount(sources)?;
    // Phase 1: Scan (or reuse cached preview results)
    let mut scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file move loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        let previews = state.scope().stores().previews();
        if let Some(cached) = previews
            .take_cached_scan_result(preview_id, sources)
            .filter(|c| !c.files.is_empty())
        {
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
            scan_sources(sources, state, events, operation_id, WriteOperationType::Move)?
        }
    } else {
        scan_sources(sources, state, events, operation_id, WriteOperationType::Move)?
    };

    // Where the copy phase stages everything before Phase 3 renames it out.
    let staging_dir = destination.join(format!("{}{}", cmdr_fs::staging::STAGING_DIR_PREFIX, operation_id));
    state.transfer_tree.install(
        scan_result.order_by_transfer_tree(sources, &TreeOrder::active(state.scope().host())),
        TreeLanding::under(destination, config.destination_name.as_deref()).and_first_under(staging_dir.clone()),
    );
    let tree = state.transfer_tree.reporter();

    // Pre-flight filesystem-limit check: a cross-FS move stages a full copy, so
    // the destination's per-file cap (FAT32's 4 GiB) applies. Block before
    // creating the staging dir or writing a byte. No-op for filesystems with no
    // known limit. (Same-FS moves rename in place and never reach here.)
    validate_file_sizes_for_filesystem(destination, &scan_result.files)?;

    // Asked before the first byte is staged: Phase 4 has to delete every
    // original, and Locked ones refuse, so the person decides now whether to
    // unlock them for it (`locked_sources.rs`). Nothing else would stop a move
    // that copied everything and then couldn't remove the originals.
    let Some(locked) = plan_locked_local(
        events,
        operation_id,
        WriteOperationType::Move,
        state,
        LockedScan::of_paths(scanned_items(sources, &scan_result)),
    ) else {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".into(),
        });
    };

    // Create staging directory
    fs::create_dir(&staging_dir).map_err(|e| WriteOperationError::IoError {
        path: staging_dir.display().to_string(),
        message: format!("Failed to create staging directory: {}", e).into(),
    })?;
    // Hidden from the pane while this move owns it, and recorded so an ending
    // that never reaches Phase 5 leaves something that knows what it is. ❗ It
    // can hold the whole staged tree: a destination that left the mount table
    // returns before Phase 3 ever ran, and the sweep's `remove_dir` is what
    // keeps the user's only copy of those files where it is.
    let _staging_guard = StagingTemp::adopt(staging_dir.clone(), state.liveness_token());
    let staging_record = in_flight_temps::track(state, ItemKind::StagingDir, &staging_dir);

    // Phase 2: Copy files to staging directory (using scan results, same as copy operation)
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution = ApplyToAll::default();
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();
    let mut dir_remap =
        super::super::named_destination::initial_remap(sources, &staging_dir, config.destination_name.as_deref())?;
    // A Locked item the person said to leave stays out of the copy whole, the
    // way a folder→file Skip keeps a subtree out (`copy_single_item`).
    let mut skipped_subtrees: HashSet<PathBuf> = locked
        .skipped()
        .filter_map(|skipped| dir_dest_path(skipped, sources, &staging_dir))
        .map(|staged| apply_dir_remap(&staged, &dir_remap))
        .collect();
    // Those files, which the walk below counts as done like any other.
    let mut locked_skipped_files = 0usize;
    // Durability bookkeeping. The Phase-2 copy records each file's STAGING dest
    // in `transaction.created_files` (and in `already_synced` when the strategy
    // already synced its data) and each directory it makes in
    // `transaction.created_dirs`. Phase 3 moves the staged tree out, so before
    // the flush every one of them is mapped to where it landed (`landed_at`).
    let mut already_synced: HashSet<PathBuf> = HashSet::new();
    // The ACL and Locked flag of each staged file that has one, keyed by STAGED
    // path. Either would refuse Phase 3's rename (and a failed move's staging
    // cleanup), so they go on only once Phase 3 has put the item in its final
    // place. Sparse: a move of ordinary files holds an empty map.
    let mut deferred_landed: HashMap<PathBuf, LandedMetadata> = HashMap::new();

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
    state.scope().status().update(
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
    let mut landed_files: Vec<LandedOriginal> = Vec::with_capacity(scan_result.files.len());
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
            state.pause_gate.wait_while_paused_sync(state.intent());

            log::debug!(
                "move_with_staging: copying file {} ({} bytes) to staging",
                LogPrivate::file_path(&file_info.path),
                file_info.size
            );
            // What the original looks like BEFORE the copy reads it, so a save
            // that lands during the copy counts as a change too, not only one
            // that lands after it. Phase 4 keeps any original that no longer
            // matches.
            let stamp = SourceStamp::read(&file_info.path);
            // Copy to staging directory instead of final destination
            let staged_before = transaction.created_files().len();
            // The tree tracks this COPY phase; the source sweep after it is a
            // phase of the header, and the rows stay as the copy left them.
            let row = tree.leaf(file_info.tree_node);
            let verdict = copy_single_item(
                &file_info.path,
                file_info.dest_path(&staging_dir),
                &staging_dir,
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
                &mut skipped_subtrees,
                &mut already_synced,
                Some(&mut deferred_landed),
                &row,
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
                landed_files.push(LandedOriginal {
                    path: file_info.path.clone(),
                    stamp,
                });
            } else {
                skipped_source_paths.insert(file_info.path.clone());
                if locked.skips_under(&file_info.path) {
                    locked_skipped_files += 1;
                }
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
        remove_staging_tree_in_background(staging_dir.clone());
        let event = transfer_stop_event(operation_id, WriteOperationType::Move, state, e, None);
        let e = event.error.clone();
        events.emit_error(event);
        return Err(e);
    }

    // Stage the scanned directories the per-file loop didn't create: an empty
    // dir has no files, so it never staged, Phase 3's rename never moved it,
    // and Phase 4's source delete then DESTROYED it — gone from the source
    // without ever arriving at the destination. Staging it here lets it ride
    // the normal rename + cleanup machinery.
    // The folders to stage: every scanned one, less what a Locked skip keeps out.
    let staged_dirs: std::borrow::Cow<'_, [PathBuf]> = if locked.skips_any() {
        scan_result
            .dirs
            .iter()
            .filter(|dir| !locked.skips_under(dir))
            .cloned()
            .collect::<Vec<_>>()
            .into()
    } else {
        std::borrow::Cow::Borrowed(&scan_result.dirs)
    };
    if let Err(e) = create_scanned_dirs_at_destination(
        &staged_dirs,
        sources,
        &staging_dir,
        state,
        &mut transaction,
        &mut created_dirs,
        &dir_remap,
    ) {
        remove_staging_tree_in_background(staging_dir.clone());
        let event = transfer_stop_event(operation_id, WriteOperationType::Move, state, e, None);
        let e = event.error.clone();
        events.emit_error(event);
        return Err(e);
    }
    tree.empty_folders_in_place();

    // Everything is staged, so the staged folders take their source's xattrs
    // and dates now, and Phase 3's rename carries them to the destination.
    // What would refuse that rename (a mode without owner write, the ACL, the
    // flags) joins the deferred set, keyed by staged path like the files'.
    apply_created_dirs_metadata(
        &staged_dirs,
        sources,
        &staging_dir,
        state,
        &transaction.created_dirs,
        &dir_remap,
        Some(&mut deferred_landed),
    );

    // Where Phase 3 put a staged item under a name other than the one it staged
    // with, or with its own rename inside a merge: staged path → landed path. And
    // the staged items a Skip discarded, which never landed at all. The flush
    // reads both to reach the directories entries actually landed in.
    let mut landings: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut discarded_staged: HashSet<PathBuf> = HashSet::new();
    // Staged top-level folders a merge folded into the user's folder of the same
    // name: their children landed, they didn't, so the user's folder keeps its
    // own metadata.
    let mut merged_staged: HashSet<PathBuf> = HashSet::new();

    // Phase 3: Atomic rename from staging to final destination
    let rename_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            // Pause gate at the item boundary, so every loop in the engine parks
            // on the same promise. The recursive merge below carries its own.
            state.pause_gate.wait_while_paused_sync(state.intent());

            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".into(),
            })?;

            let leaf = config
                .destination_name
                .as_deref()
                .map(Path::new)
                .unwrap_or_else(|| Path::new(file_name));
            let staged_path = staging_dir.join(leaf);
            let final_path = destination.join(leaf);

            // Kept out of the copy on a Locked skip: nothing was staged to land.
            if locked.skips(source) {
                skipped_source_paths.insert(source.clone());
                continue;
            }

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
                merged_staged.insert(staged_path.clone());
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
                    // The staged tree is this move's own: nothing in it is Locked.
                    &LockedPlan::none(),
                )?;
                for staged_skip in &staged_skips {
                    if let Ok(rel) = staged_skip.strip_prefix(&staged_path) {
                        skipped_source_paths.insert(source.join(rel));
                    }
                }
                discarded_staged.extend(staged_skips);
                landings.extend(
                    staging_move_tx
                        .renamed_items()
                        .map(|item| (item.original_source.clone(), item.landed.path.clone())),
                );
                // Same rule as the same-FS merge: the destination folder also
                // holds files this operation never touched.
                journal::note_not_rollbackable(
                    state.scope(),
                    cmdr_operation_log::types::NotRollbackableReason::DirectoryMerge,
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
                        let landing = state.scope().host().announce(&resolved.path);
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
                            state.scope(),
                            if resolved.path == final_path {
                                cmdr_operation_log::types::NotRollbackableReason::Overwrote
                            } else {
                                cmdr_operation_log::types::NotRollbackableReason::StagedConflictResolved
                            },
                        );
                        move_resolved_into_place(state, &staged_path, &final_path, landing, None, &mut throwaway_tx)?;
                        landings.insert(staged_path.clone(), resolved.path);
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
                        discarded_staged.insert(staged_path.clone());
                        files_skipped += 1;
                        // The rows phase 2 wrote for this source name a
                        // destination that now holds the file the user chose to
                        // keep. Reversing them would carry THAT file to the
                        // source.
                        journal::note_not_rollbackable(
                            state.scope(),
                            cmdr_operation_log::types::NotRollbackableReason::StagedConflictResolved,
                        );
                        continue;
                    }
                }
            } else {
                // No conflict, just rename from staging to final
                let landing = state.scope().host().announce(&final_path);
                rename_onto_free_name(&staged_path, landing).map_err(|e| WriteOperationError::IoError {
                    path: staged_path.display().to_string(),
                    message: format!("Failed to move from staging: {}", e).into(),
                })?;
            }
        }
        Ok(())
    })();

    if let Err(e) = rename_result {
        // Cleanup staging directory in background (may block on network mounts)
        remove_staging_tree_in_background(staging_dir);
        let event = transfer_stop_event(operation_id, WriteOperationType::Move, state, e, None);
        let e = event.error.clone();
        events.emit_error(event);
        return Err(e);
    }

    // Durability MUST run BEFORE Phase 4's source delete, and Phases 4 and 5 run
    // only once it answers `Ok`. The originals are the only other copy of the
    // data: on power loss or a pulled drive before the new directory entries are
    // on disk, the moved files can be missing from the destination, and FAT/exFAT
    // keep no journal to replay them. So the flush syncs the data the copy didn't
    // already sync AND fsyncs every final directory that gained an entry, Phase
    // 3's renames included, whether or not its files' data was already synced.
    // A failure keeps every source and everything that landed. Emits a
    // `Flushing`-phase event first so the FE shows "Writing the last piece…".
    //
    // Map each staged path to where Phase 3 put it: the nearest ancestor a
    // conflict resolution or merge renamed decides (a `name (N)` included), one a
    // Skip discarded drops the path (nothing of it landed), and otherwise the
    // name carried over from `staging_dir` to `destination`.
    let remap = |p: &Path| -> PathBuf {
        match p.strip_prefix(&staging_dir) {
            Ok(rel) => destination.join(rel),
            // Shouldn't happen (every staging dest is under staging_dir), but
            // fall back to the original path rather than dropping it.
            Err(_) => p.to_path_buf(),
        }
    };
    let landed_at = |p: &Path| -> Option<PathBuf> {
        for ancestor in p.ancestors().take_while(|a| *a != staging_dir.as_path()) {
            if discarded_staged.contains(ancestor) {
                return None;
            }
            if let Some(landed) = landings.get(ancestor) {
                return Some(match p.strip_prefix(ancestor) {
                    Ok(rest) if !rest.as_os_str().is_empty() => landed.join(rest),
                    _ => landed.clone(),
                });
            }
        }
        Some(remap(p))
    };
    // Every staged item is in its final place now, so what would have refused
    // its rename goes on: a folder's mode, the ACL, then the flags
    // (`apply_landed`). Deepest first, so a folder's own mode can't refuse the
    // lookups its children's need. Before the flush, which then makes them
    // durable with everything else. Best effort: a refusal costs the item those
    // classes, never the move.
    //
    // ❗ Only to an item that landed as ITSELF. A staged folder a merge folded
    // into the user's folder never landed: what `landed_at` maps it to is the
    // user's folder, which keeps its own metadata.
    let landed_as_itself = |staged: &Path| -> Option<PathBuf> {
        for ancestor in staged.ancestors().take_while(|a| *a != staging_dir.as_path()) {
            if landings.contains_key(ancestor) || discarded_staged.contains(ancestor) {
                break;
            }
            if merged_staged.contains(ancestor) {
                return None;
            }
        }
        landed_at(staged)
    };
    let mut deferred: Vec<(PathBuf, &LandedMetadata)> = deferred_landed
        .iter()
        .filter_map(|(staged, set)| landed_as_itself(staged).map(|landed| (landed, set)))
        .collect();
    deferred.sort_by_key(|(landed, _)| std::cmp::Reverse(landed.components().count()));
    for (landed, set) in deferred {
        let report = apply_landed(&landed, set);
        if report.any_refused() {
            log::warn!(
                target: "copy",
                "move_with_staging: op={} {} landed without some of its metadata: {report:?}",
                operation_id,
                LogPrivate::path(&landed)
            );
        }
    }
    let final_dests: Vec<PathBuf> = transaction
        .created_file_paths()
        .iter()
        .filter_map(|p| landed_at(p))
        .collect();
    let final_already_synced: HashSet<PathBuf> = already_synced.iter().filter_map(|p| landed_at(p)).collect();
    let final_created_dirs: Vec<PathBuf> = transaction.created_dirs.iter().filter_map(|p| landed_at(p)).collect();
    // Journal the destination directories this move created, under the paths they
    // LIVE at — the same staging→final rebase the leaf rows already got, since
    // phase 3 renamed the tree out of `.cmdr-staging-<op>/` moments ago. Without
    // these rows a reversal puts every file back and leaves the moved folder's
    // empty skeleton at the destination. They land after every leaf row (phase 2
    // wrote those), so a `seq DESC` reversal still removes files before dirs.
    let final_dirs: Vec<PathBuf> = transaction.created_dirs.iter().map(|p| remap(p)).collect();
    journal::record_created_dirs(state.scope(), &final_dirs);
    // The staging tree is renamed into place, so nothing this transaction recorded
    // is still a partial to clean up: commit, or the `Drop` net runs a pointless
    // rollback over paths that moved.
    transaction.commit();
    if let Err(failure) = flush_created_destinations(
        events,
        operation_id,
        WriteOperationType::Move,
        state,
        files_done,
        scan_result.file_count,
        bytes_done,
        scan_result.total_bytes,
        &final_dests,
        &final_created_dirs,
        &final_already_synced,
    ) {
        log::warn!(
            target: "write_durability",
            "move_with_staging: op={} keeps every source, the destination isn't provably durable ({failure})",
            operation_id, failure = LogError(&failure)
        );
        // Typed, so the user reads "Cmdr kept your originals" rather than a
        // generic failure: the errno and the path come straight from the flush,
        // and the destination's name from the sides captured at start.
        let e = WriteOperationError::MoveNotConfirmed {
            path: failure.path.display().to_string(),
            errno: failure.errno,
            volume_name: state.sides.as_ref().map(|sides| sides.destination.volume_name.clone()),
        };
        let event = transfer_stop_event(operation_id, WriteOperationType::Move, state, e, None);
        let e = event.error.clone();
        events.emit_error(event);
        return Err(e);
    }

    // Phase 4 also needs the destination to STILL BE THERE. A flush can answer
    // `Ok` for writes the kernel accepted moments before the drive left, and a
    // sweep that ran then would delete the only copies of files that never
    // reached the disk. So the mount table is asked once more, right here,
    // between the flush and the first delete. An unreadable table is not
    // evidence of leaving, so it lets the sweep run.
    if let Some(sides) = state.sides.as_ref()
        && sides.destination.has_left()
    {
        log::warn!(
            "move_with_staging: op={} keeps every source, the destination volume '{}' left the mount table",
            operation_id,
            LogPrivate::volume(&sides.destination.volume_name)
        );
        let e = WriteOperationError::DeviceDisconnected {
            path: sides.destination.root.display().to_string(),
            side: None,
        };
        let event = transfer_stop_event(operation_id, WriteOperationType::Move, state, e, None);
        let e = event.error.clone();
        events.emit_error(event);
        return Err(e);
    }

    // Phase 4: Delete source files (only after the destination is durable on
    // disk), removing exactly what this move landed. A Skip in either phase
    // takes its original out of the ledger, so the sweep never reaches it, and
    // a Locked item kept out of the copy keeps everything under it.
    skipped_source_paths.extend(locked.skipped().map(Path::to_path_buf));
    let sweep = SourceSweep::plan(
        sources,
        landed_files
            .into_iter()
            .filter(|original| {
                !skipped_source_paths
                    .iter()
                    .any(|skipped| original.path.starts_with(skipped))
            })
            .collect(),
        &scan_result,
        skipped_source_paths,
    );
    let delete_result = delete_sources_after_move(events, operation_id, state, sources, files_done, &sweep, &locked);

    // Phase 5: Remove the staging directory, on EVERY path out of Phase 4. Phase
    // 3 renamed the staged tree away, so this is an empty shell whichever way
    // Phase 4 ended; leaving it behind on a cancel puts a stray
    // `.cmdr-staging-<op>` folder in the user's destination for good. `remove_dir`
    // refuses a non-empty directory, so a surprise leaves the contents alone —
    // and then the record stays, so the sweep meets it at the next launch or the
    // drive's return rather than nothing ever looking at it again.
    match fs::remove_dir(&staging_dir) {
        Ok(()) => in_flight_temps::retire(state, &staging_record),
        Err(e) => {
            log::warn!(
                "move_with_staging: op={} left {} in place: {e}",
                operation_id,
                LogPrivate::dir_path(&staging_dir)
            );
            in_flight_temps::keep_for_arrival(state, staging_record);
        }
    }
    let leftovers = match delete_result {
        Ok(leftovers) => leftovers,
        Err(stopped) => {
            let event = transfer_stop_event(
                operation_id,
                WriteOperationType::Move,
                state,
                stopped.error,
                stopped.counts,
            );
            let e = event.error.clone();
            // A cancel has already emitted `write-cancelled`; a second terminal
            // event for one operation would race the first on the way out.
            if !matches!(e, WriteOperationError::Cancelled { .. }) {
                events.emit_error(event);
            }
            return Err(e);
        }
    };

    // Emit completion. The move succeeded; anything the sweep left behind rides
    // along as typed data, so the toast can say what stayed and where.
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done + already_in_place - locked_skipped_files,
        files_skipped: files_skipped + locked_skipped_files,
        bytes_processed: bytes_done,
        appeared_during_move: leftovers.appeared_during_move(),
        // The STAGING phase's verdict, which is where a conflict is resolved.
        // A source phase 3 later declines to rename into place re-reports
        // itself on `write-source-item-done`; this summary is written once.
        top_level_skipped: Some(staging_skipped),
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: locked.kept(),
    });

    Ok(())
}

/// Every item a cross-FS move's Phase 4 may delete, each once: the sources,
/// then the scanned folders and files under them.
fn scanned_items<'a>(sources: &'a [PathBuf], scan_result: &'a crate::state::ScanResult) -> Vec<&'a Path> {
    let mut seen: HashSet<&Path> = HashSet::new();
    sources
        .iter()
        .chain(scan_result.dirs.iter())
        .chain(scan_result.files.iter().map(|file| &file.path))
        .map(PathBuf::as_path)
        .filter(|path| seen.insert(path))
        .collect()
}
