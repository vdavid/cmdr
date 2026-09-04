//! Same-filesystem move: the `rename(2)` path.
//!
//! Each top-level source is renamed into the destination, so no bytes travel and
//! a directory arrives whole in one call. That shapes the rest: the status
//! counts ITEMS rather than leaves, the journal writes one rollback unit per
//! top-level item (one rename-back reverses a whole subtree), and a cancel
//! reverses the recorded renames synchronously.
//!
//! The cross-filesystem engine is `move_op::cross_fs`; the dispatcher that picks
//! between them, and the conflict-landing and directory-merge helpers both
//! engines share, live in `move_op` itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::ledger::WrittenFile;
use super::{MoveTransaction, merge_move_directory, move_resolved_into_place};

use crate::file_system::write_operations::conflict::{ApplyToAll, resolve_conflict};
use crate::file_system::write_operations::durability::flush_touched_directories;
use crate::file_system::write_operations::error_classification::IoResultExt;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::state::{
    OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use crate::file_system::write_operations::types::{
    CancelRollback, SourceItemOutcome, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteSourceItemDoneEvent,
};
use crate::file_system::write_operations::validation::path_exists_or_is_symlink;
use crate::file_system::write_operations::{journal, journal_search};

/// `already_in_place` counts the top-level sources the caller dropped as already
/// living at the destination. They wrote nothing, but the user asked for them,
/// so they belong in the completion tally.
pub(super) fn move_with_rename(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
    already_in_place: usize,
) -> Result<(), WriteOperationError> {
    let mut files_done = 0;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution = ApplyToAll::default();
    let mut move_tx = MoveTransaction::new();
    // A same-FS move is journaled and reversed at TOP-LEVEL granularity (one
    // rename-back takes a whole subtree), so its status counts ITEMS, not leaves.
    // There's no scan to seed it from and the renames are instant, so the totals
    // are known up front. Without this the status row stays at zeros, the queue
    // row shows no progress, and — because `finalize_op` reads its header
    // aggregates from this cache — a finished move journals `items_done = 0`,
    // which then seeds the inverse operation's `item_count` with a zero.
    let total_items = sources.len() + already_in_place;
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        already_in_place,
        total_items,
        0,
        0,
    );

    let result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            // The cooperative boundary, between items. This is the whole pause
            // story for a rename engine: a `rename(2)` is one syscall, so the
            // item boundary is the only place to park.
            if state.stop_or_park_sync() {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".to_string(),
            })?;
            let dest_path = destination.join(file_name);

            // Snapshot the source (kind + mtime) BEFORE the rename for the
            // journal's top-level `rollback_unit` row; `item_overwrote` records
            // whether we replaced an existing dest (⇒ not rollbackable).
            let source_meta = fs::symlink_metadata(source).ok();
            let source_is_dir = source_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let source_mtime = source_meta.as_ref().and_then(journal::mtime_secs);
            let source_size = source_meta.as_ref().map(|m| m.len() as i64);
            let mut item_overwrote = false;
            // Where the item ends up, which is `dest_path` unless a conflict sends
            // it aside to a fresh `name (N)`. The journal records THIS, not the
            // name that was taken: a row naming the pre-existing file aims the
            // reversal at a file this operation never touched, and a duplicate
            // (same size, same mtime) passes the snapshot recheck.
            let mut landed_path = dest_path.clone();

            // Enumerate the subtree's `search_only` leaves from the drive index
            // BEFORE the rename — the reconciler prunes the moved subtree on its
            // FSEvent, so a later read would miss them (search-leaf enumeration). Persisted only after
            // this item's move succeeds (below the top-level row).
            let buffered_leaves = if source_is_dir {
                Some(journal_search::enumerate_subtree_for_search(
                    "root",
                    source,
                    journal_search::SEARCH_LEAF_CAP,
                ))
            } else {
                None
            };

            // When both source and dest are directories, merge recursively
            // instead of replacing (which would destroy dest-only files).
            if source.is_dir() && dest_path.exists() && dest_path.is_dir() {
                // Same-FS merge operates on the original tree directly, so a
                // skipped child just leaves the source non-empty; no skip-set
                // bookkeeping is needed (there's no later source-delete phase).
                merge_move_directory(
                    source,
                    &dest_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                    &mut move_tx,
                    &mut files_skipped,
                    &mut None,
                )?;
                // The one row this item journals names the PRE-EXISTING
                // destination folder, which also holds files this operation never
                // touched — renaming it back would carry them off to the source.
                // The disqualifying condition is "merged", not "overwrote
                // something": a merge that overwrote nothing has the same row and
                // the same failure. Per-child rows would make it reversible;
                // `operation_log/DETAILS.md` § "Why a directory merge isn't
                // reversible" holds that decision.
                journal::note_not_rollbackable(
                    operation_id,
                    crate::operation_log::types::NotRollbackableReason::DirectoryMerge,
                );
            } else if path_exists_or_is_symlink(&dest_path) {
                // File-to-file (or type mismatch) conflict
                match resolve_conflict(
                    source,
                    &dest_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => {
                        // Register both halves with the downloads watcher's
                        // ignore set: destination so rename-arrival is
                        // suppressed, source so a Cmdr move OUT of Downloads
                        // is also suppressed. No-ops outside ~/Downloads.
                        crate::downloads::note_pending_write_for_cmdr(source);
                        crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                        // Landing on the original dest name replaced a pre-existing
                        // file; a rename-aside (different name) did not.
                        item_overwrote = resolved.path == dest_path;
                        landed_path = resolved.path.clone();
                        move_resolved_into_place(source, &dest_path, &resolved, source_meta.as_ref(), &mut move_tx)?;
                    }
                    None => {
                        // Skip this file
                        files_skipped += 1;
                        continue;
                    }
                }
            } else {
                // No conflict, so just rename
                crate::downloads::note_pending_write_for_cmdr(source);
                crate::downloads::note_pending_write_for_cmdr(&dest_path);
                fs::rename(source, &dest_path).with_path(source)?;
                move_tx.record(
                    source.clone(),
                    WrittenFile::local_stat(dest_path.clone(), source_meta.as_ref()),
                );
            }

            // Journal the top-level moved item as the rollback unit: one
            // rename-back reverses the whole subtree. The
            // subtree's `search_only` leaves are enumerated from the drive index
            // (search-leaf enumeration).
            let entry_type = if source_is_dir {
                crate::operation_log::types::EntryType::Dir
            } else {
                crate::operation_log::types::EntryType::File
            };
            journal::record_local_leaf(
                operation_id,
                entry_type,
                source,
                Some(&landed_path),
                source_size,
                source_mtime,
                item_overwrote,
                crate::operation_log::types::ItemOutcome::Done,
            );

            // Persist the buffered `search_only` leaves now that the move
            // succeeded; their dest is rebased onto the moved-to path.
            if let Some(buffered) = &buffered_leaves {
                journal_search::persist_and_note(
                    operation_id,
                    crate::file_system::volume::DEFAULT_VOLUME_ID,
                    source,
                    crate::file_system::volume::DEFAULT_VOLUME_ID,
                    Some(&landed_path),
                    buffered,
                );
            }

            files_done += 1;
            update_operation_status(
                operation_id,
                WriteOperationPhase::Copying,
                Some(file_name.to_string_lossy().into_owned()),
                files_done + already_in_place,
                total_items,
                0,
                0,
            );

            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
                // Usually true (the rename took the whole item), but a directory
                // MERGE whose children were partly skipped leaves the source dir
                // standing. One `lstat` per top-level item is cheap next to the
                // move itself, and it's the only honest answer.
                source_removed: !path_exists_or_is_symlink(source),
                outcome: SourceItemOutcome::Done,
            });
        }
        Ok(())
    })();

    // Handle cancellation: emit write-cancelled so the frontend can close the dialog.
    // The outer start_write_operation wrapper treats Cancelled as "already handled",
    // so we must emit the event here.
    if let Err(WriteOperationError::Cancelled { .. }) = &result {
        let rollback = match load_intent(&state.intent) {
            OperationIntent::RollingBack => move_tx.rollback().into_cancel_rollback(),
            _ => CancelRollback::none(),
        };

        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            rollback,
        });
        return result;
    }

    result?;

    // The loop drained, but the user may have clicked Rollback between the last
    // item's `is_cancelled` check and the loop's exit — a rename takes a whole
    // subtree in one call, so on a small move the entire loop fits inside that
    // window. Read the intent once more before reporting anything: without this
    // the operation goes on to flush and emit `write-complete`, and the person
    // who asked for "put it back" is told "everything's where you sent it"
    // instead. `copy/mod.rs`'s `PostLoopIntent::Completed` arm is the same guard
    // on the copy side; the reversal itself is `MoveTransaction::rollback`,
    // which rechecks each item and never overwrites an occupied source.
    if load_intent(&state.intent) == OperationIntent::RollingBack {
        log::info!(
            "move_with_rename: rollback requested after loop completion op={}, {} items",
            operation_id,
            move_tx.renames.len()
        );
        let rollback = move_tx.rollback().into_cancel_rollback();
        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            rollback,
        });
        // Cancelled, like the in-loop arm above: the wrapper reads it as
        // "already emitted its terminal event", and journals the op as canceled
        // rather than done — which is what a reversed move is.
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Durability: a same-FS rename moves directory ENTRIES, so the directories
    // on both sides of each rename are exactly what needs flushing — the moved
    // files' data blocks and inodes were already durable before the move, and
    // syncing them would cost an `fcntl(F_FULLFSYNC)` per file on macOS (a
    // device-level barrier) for a write that never happened.
    // `flush_touched_directories` emits the `Flushing` event too, so the FE
    // shows "Writing the last piece…" for both move kinds.
    let touched_dirs = move_tx.touched_directories();
    flush_touched_directories(
        events,
        operation_id,
        WriteOperationType::Move,
        state,
        files_done,
        files_done,
        0,
        0,
        &touched_dirs,
    );

    // Emit completion (instant, no progress needed)
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done + already_in_place,
        files_skipped,
        bytes_processed: 0, // Rename doesn't track bytes
    });

    Ok(())
}
