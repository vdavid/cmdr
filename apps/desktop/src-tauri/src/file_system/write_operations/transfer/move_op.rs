//! Move implementation for write operations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::cancellable::remove_dir_all_in_background;
use super::super::conflict::{ApplyToAll, resolve_conflict};
use super::super::durability::flush_created_destinations;
use super::super::overwrite::safe_overwrite_dir;
use super::super::scan::{SourceItemTracker, handle_dry_run, scan_sources, take_cached_scan_result};
use super::super::state::{
    CopyTransaction, OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use super::super::types::{
    IoResultExt, OperationEventSink, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use super::super::validation::{is_same_filesystem, path_exists_or_is_symlink, validate_file_sizes_for_filesystem};
use super::copy::copy_single_item;

// ============================================================================
// Move rollback tracking
// ============================================================================

/// Tracks renames performed during same-FS move for rollback on cancellation.
/// Each entry is `(original_source, moved_to_dest)`. Rollback reverses them.
struct MoveTransaction {
    renames: Vec<(PathBuf, PathBuf)>,
}

impl MoveTransaction {
    fn new() -> Self {
        Self { renames: Vec::new() }
    }

    fn record(&mut self, source: PathBuf, dest: PathBuf) {
        self.renames.push((source, dest));
    }

    /// Reverses all recorded renames (dest → source) in reverse order.
    /// Same-FS rename is instant, so this runs synchronously.
    ///
    /// Intentional: this reverses the moves THIS op made; it does NOT restore a
    /// destination that an Overwrite-with-rename replaced (no per-file backup is
    /// kept — see `overwrite::safe_overwrite_file` step 4). Keeping backups for the
    /// whole operation risks unexpectedly filling the user's drive on a large
    /// Overwrite. Revisit if users complain. See transfer/CLAUDE.md
    /// § "Overwrite isn't reversible".
    fn rollback(&self) {
        for (original_source, moved_to_dest) in self.renames.iter().rev() {
            if let Err(e) = fs::rename(moved_to_dest, original_source) {
                log::warn!(
                    "move rollback: failed to rename {} back to {}: {}",
                    moved_to_dest.display(),
                    original_source.display(),
                    e
                );
            }
        }
    }
}

/// Lands a move source at the path a `resolve_conflict` result chose, honoring
/// cmdr's Rename / Overwrite semantics including the type-mismatch directions.
///
/// `resolve_conflict` distinguishes its two non-skip outcomes by path:
/// - **Overwrite** returns `resolved.path == dest_path` (replace in place). A
///   plain `rename(2)` from a file source onto a directory dest (or vice versa)
///   fails, so type-mismatch Overwrite routes through `safe_overwrite_dir`: the
///   dest is set aside, the source is renamed into place inside the closure, and
///   the aside is removed on success / rolled back on failure. Same-type
///   Overwrite renames directly (atomic replace).
/// - **Rename** returns `resolved.path == find_unique_name(dest_path)` — a fresh
///   `name (N)` that `find_unique_name` reserved with a 0-byte placeholder file.
///   The existing dest is kept untouched; the source lands at the reserved name.
///   A file source `rename`s atomically over the placeholder; a directory source
///   can't rename over a file, so we remove the placeholder first (the
///   reservation still holds the name against concurrent writers).
fn move_resolved_into_place(
    source: &Path,
    dest_path: &Path,
    resolved: &super::super::overwrite::ResolvedDestination,
    move_tx: &mut MoveTransaction,
) -> Result<(), WriteOperationError> {
    let source_is_dir = source.is_dir();
    let is_rename = resolved.path != dest_path;

    if is_rename {
        // Rename: keep the existing dest, land the source at the reserved name.
        if source_is_dir {
            // A directory can't `rename` over the reserved placeholder file;
            // remove it first. The name stays reserved logically.
            let _ = fs::remove_file(&resolved.path);
        }
        fs::rename(source, &resolved.path).with_path(source)?;
        move_tx.record(source.to_path_buf(), resolved.path.clone());
        return Ok(());
    }

    // Overwrite (`resolved.path == dest_path`).
    let dest_is_dir = resolved.path.is_dir();
    if source_is_dir != dest_is_dir {
        // Type-mismatch overwrite: set the dest aside, move the source in.
        let source_path = source.to_path_buf();
        safe_overwrite_dir(&resolved.path, |target| {
            fs::rename(&source_path, target).map_err(|e| WriteOperationError::IoError {
                path: source_path.display().to_string(),
                message: format!("Failed to rename across types: {}", e),
            })
        })?;
        move_tx.record(source.to_path_buf(), resolved.path.clone());
    } else {
        fs::rename(source, &resolved.path).with_path(source)?;
        move_tx.record(source.to_path_buf(), resolved.path.clone());
    }
    Ok(())
}

// ============================================================================
// Move implementation
// ============================================================================

pub(in crate::file_system::write_operations) fn move_files_with_progress_inner(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Handle dry-run mode
    if handle_dry_run(
        config.dry_run,
        sources,
        destination,
        state,
        events,
        operation_id,
        WriteOperationType::Move,
        state.progress_interval,
        config.max_conflicts_to_show,
    )? {
        return Ok(());
    }

    // Check if all sources are on the same filesystem as destination
    let same_fs = sources
        .iter()
        .all(|s| is_same_filesystem(s, destination).unwrap_or(false));

    if same_fs {
        // Use instant rename for each source
        move_with_rename(events, operation_id, state, sources, destination, config)
    } else {
        // Use atomic staging pattern for cross-filesystem move
        move_with_staging(events, operation_id, state, sources, destination, config)
    }
}

fn move_with_rename(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    let mut files_done = 0;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution = ApplyToAll::default();
    let mut move_tx = MoveTransaction::new();

    let result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            // Check cancellation
            if super::super::state::is_cancelled(&state.intent) {
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
            // whether we replaced an existing dest (⇒ not rollbackable, D3).
            let source_meta = fs::symlink_metadata(source).ok();
            let source_is_dir = source_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let source_mtime = source_meta.as_ref().and_then(super::super::journal::mtime_secs);
            let source_size = source_meta.as_ref().map(|m| m.len() as i64);
            let mut item_overwrote = false;

            // Enumerate the subtree's `search_only` leaves from the drive index
            // BEFORE the rename — the reconciler prunes the moved subtree on its
            // FSEvent, so a later read would miss them (M2e). Persisted only after
            // this item's move succeeds (below the top-level row).
            let buffered_leaves = if source_is_dir {
                Some(super::super::journal_search::enumerate_subtree_for_search(
                    "root",
                    source,
                    super::super::journal_search::SEARCH_LEAF_CAP,
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
                        move_resolved_into_place(source, &dest_path, &resolved, &mut move_tx)?;
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
                move_tx.record(source.clone(), dest_path.clone());
            }

            // Journal the top-level moved item as the rollback unit: one
            // rename-back reverses the whole subtree (D-granularity). The
            // subtree's `search_only` leaves are enumerated from the drive index
            // (M2e).
            let entry_type = if source_is_dir {
                crate::operation_log::types::EntryType::Dir
            } else {
                crate::operation_log::types::EntryType::File
            };
            super::super::journal::record_local_leaf(
                operation_id,
                entry_type,
                source,
                Some(&dest_path),
                source_size,
                source_mtime,
                item_overwrote,
                crate::operation_log::types::ItemOutcome::Done,
            );

            // Persist the buffered `search_only` leaves now that the move
            // succeeded; their dest is rebased onto the moved-to path.
            if let Some(buffered) = &buffered_leaves {
                super::super::journal_search::persist_and_note(operation_id, source, Some(&dest_path), buffered);
            }

            files_done += 1;

            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
            });
        }
        Ok(())
    })();

    // Handle cancellation: emit write-cancelled so the frontend can close the dialog.
    // The outer start_write_operation wrapper treats Cancelled as "already handled",
    // so we must emit the event here.
    if let Err(WriteOperationError::Cancelled { .. }) = &result {
        let rolled_back = match load_intent(&state.intent) {
            OperationIntent::RollingBack => {
                move_tx.rollback();
                true
            }
            _ => false,
        };

        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            rolled_back,
        });
        return result;
    }

    result?;

    // Durability: a same-FS rename moves no data blocks (the file's contents
    // were already durable before the move), but the new directory entries
    // still need flushing. `flush_created_destinations` emits a `Flushing`
    // event (so the FE shows "Writing the last piece…" for moves too) and
    // `fdatasync`s each moved file plus its parent directory, making the
    // rename-into-place durable. `already_synced` is empty: an `fdatasync` on
    // an already-durable file is cheap, and the parent-dir fsync is the point.
    let renamed_dests: Vec<PathBuf> = move_tx.renames.iter().map(|(_, dest)| dest.clone()).collect();
    let empty_synced: HashSet<PathBuf> = HashSet::new();
    flush_created_destinations(
        events,
        operation_id,
        WriteOperationType::Move,
        state,
        files_done,
        files_done,
        0,
        0,
        &renamed_dests,
        &empty_synced,
    );

    // Emit completion (instant, no progress needed)
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done,
        files_skipped,
        bytes_processed: 0, // Rename doesn't track bytes
    });

    Ok(())
}

/// Recursively merges a source directory into an existing destination directory
/// using rename() for individual files. Dest-only files are preserved.
/// After all contents are moved, removes the now-empty source directory.
///
/// `skipped_paths`, when `Some`, collects the `source_dir`-rooted paths of every
/// child that was skipped (conflict resolved as Skip). The cross-FS Phase-3
/// caller passes the STAGED tree as `source_dir`, so the staged child paths it
/// collects map back to the originals by swapping the staging prefix for the
/// real source prefix. Phase 4 then knows NOT to delete those originals (they
/// never landed at the destination) — without this, a skipped child would be
/// silently lost when Phase 4 deletes the source. The same-FS caller passes
/// `None`: it operates on the original tree directly and a skipped child simply
/// leaves the source dir non-empty, so the `remove_dir` below won't fire.
///
/// Note: This duplicates the recursive-merge-with-conflict-resolution pattern from `copy.rs`.
/// The two look similar in structure but differ in every detail (copy has progress tracking,
/// symlink handling, byte counting, transaction recording, strategy selection). A shared
/// abstraction would be forced and fragile. See `copy.rs` `copy_single_item` for the copy side.
#[allow(clippy::too_many_arguments, reason = "intentional; see doc comment above")]
fn merge_move_directory(
    source_dir: &Path,
    dest_dir: &Path,
    config: &WriteOperationConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut ApplyToAll,
    move_tx: &mut MoveTransaction,
    files_skipped: &mut usize,
    skipped_paths: &mut Option<&mut HashSet<PathBuf>>,
) -> Result<(), WriteOperationError> {
    let entries = fs::read_dir(source_dir).with_path(source_dir)?;

    for entry in entries {
        let entry = entry.with_path(source_dir)?;
        let source_child = entry.path();
        let file_name = match source_child.file_name() {
            Some(n) => n.to_owned(),
            None => continue,
        };
        let dest_child = dest_dir.join(&file_name);

        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        if source_child.is_dir() && dest_child.exists() && dest_child.is_dir() {
            // Both are directories, recurse
            merge_move_directory(
                &source_child,
                &dest_child,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
                move_tx,
                files_skipped,
                skipped_paths,
            )?;
        } else if path_exists_or_is_symlink(&dest_child) {
            // File conflict (or type mismatch)
            match resolve_conflict(
                &source_child,
                &dest_child,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => {
                    // Hook the downloads watcher's ignore set for both
                    // halves of the rename; no-ops outside ~/Downloads.
                    crate::downloads::note_pending_write_for_cmdr(&source_child);
                    crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                    move_resolved_into_place(&source_child, &dest_child, &resolved, move_tx)?;
                }
                None => {
                    // Skip: source file stays in place. Record it so a cross-FS
                    // Phase 4 won't delete the original that never landed.
                    if let Some(set) = skipped_paths.as_deref_mut() {
                        set.insert(source_child.clone());
                    }
                    *files_skipped += 1;
                    continue;
                }
            }
        } else {
            // No conflict, just rename
            crate::downloads::note_pending_write_for_cmdr(&source_child);
            crate::downloads::note_pending_write_for_cmdr(&dest_child);
            fs::rename(&source_child, &dest_child).with_path(&source_child)?;
            move_tx.record(source_child, dest_child);
        }
    }

    // Remove the source directory if it's now empty
    if fs::read_dir(source_dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(source_dir);
    }

    Ok(())
}

/// Performs cross-filesystem move using atomic staging pattern.
/// This ensures source files remain intact if the operation fails.
fn move_with_staging(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file move loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        if let Some(cached) = take_cached_scan_result(preview_id).filter(|c| !c.files.is_empty()) {
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
            log::debug!(
                "move_with_staging: copying file {} ({} bytes) to staging",
                file_info.path.display(),
                file_info.size
            );
            // Copy to staging directory instead of final destination
            copy_single_item(
                &file_info.path,
                file_info.dest_path(&staging_dir),
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
    if let Err(e) = super::copy::create_scanned_dirs_at_destination(
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
                        move_resolved_into_place(&staged_path, &final_path, &resolved, &mut throwaway_tx)?;
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
    let final_dests: Vec<PathBuf> = transaction.created_files.iter().map(|p| remap(p)).collect();
    let final_already_synced: HashSet<PathBuf> = already_synced.iter().map(|p| remap(p)).collect();
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
    delete_sources_after_move(events, operation_id, state, sources, files_done, &skipped_source_paths)?;

    // Phase 5: Remove empty staging directory
    let _ = fs::remove_dir(&staging_dir);

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done,
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
fn delete_sources_after_move(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
    skipped_source_paths: &HashSet<PathBuf>,
) -> Result<(), WriteOperationError> {
    for source in sources {
        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_done,
                rolled_back: false, // Source deletion phase - nothing to rollback
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // A whole top-level source skipped on a file / type-mismatch conflict:
        // leave the original exactly where it is.
        if skipped_source_paths.contains(source) {
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
            });
        }
    }

    Ok(())
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

#[cfg(test)]
#[path = "move_op_tests.rs"]
mod tests;
