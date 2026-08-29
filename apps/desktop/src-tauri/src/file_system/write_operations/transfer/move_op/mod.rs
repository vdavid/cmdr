//! Local move: the dispatcher and what both move engines share.
//!
//! A move takes one of two shapes, decided here by whether every source sits on
//! the destination's filesystem: `same_fs` renames each item into place (no
//! bytes travel), `cross_fs` stages a full copy and only then deletes the
//! originals. This module owns the choice between them plus the three pieces
//! both need: `MoveTransaction` (the rename ledger a cancel reverses),
//! `move_resolved_into_place` (landing an item on a conflict resolution), and
//! `merge_move_directory` (the recursive dir-into-dir merge).
//!
//! `merge_move_directory` reads differently for each caller (`cross_fs` hands it
//! the STAGED tree and collects skipped children; `same_fs` walks the originals
//! and needs no skip set), so its contract is written in terms of both.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::conflict::{ApplyToAll, resolve_conflict};
use super::super::error_classification::IoResultExt;
use super::super::event_sinks::OperationEventSink;
use super::super::overwrite::safe_overwrite_dir;
use super::super::scan::handle_dry_run;
use super::super::state::{WriteOperationState, is_cancelled};
use super::super::types::{
    SourceItemOutcome, WriteOperationConfig, WriteOperationError, WriteOperationType, WriteSourceItemDoneEvent,
};
use super::super::validation::{is_same_file, is_same_filesystem, path_exists_or_is_symlink};

mod cross_fs;
mod same_fs;

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
    /// Overwrite. Revisit if users complain. See transfer/volume/DETAILS.md
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

    // An item asked to move into the folder it already lives in is already
    // where it was asked to go: nothing to write, and it reports itself done.
    // Identity is `dev+ino` (`is_same_file`), so a symlinked parent or a
    // case-differing path counts too. `transfer/DETAILS.md` § "Self-collision (duplicating in place)".
    //
    // Dropped HERE, above the same-FS / cross-FS split, so NEITHER engine can
    // see one: `move_with_rename` would hand a directory to
    // `merge_move_directory`, which threads the destination down through
    // recursion and would self-merge the tree; `move_with_staging` would land
    // its own staged copy over the original and then delete that original in
    // Phase 4.
    let (already_in_place, sources): (Vec<PathBuf>, Vec<PathBuf>) = sources.iter().cloned().partition(|source| {
        source
            .file_name()
            .map(|name| is_same_file(source, &destination.join(name)))
            .unwrap_or(false)
    });
    for source in &already_in_place {
        log::info!(
            "move: {} is already in the destination, nothing to do",
            source.display()
        );
        events.emit_source_item_done(WriteSourceItemDoneEvent {
            operation_id: operation_id.to_string(),
            source_path: source.display().to_string(),
            // Nothing moved, so the source is exactly where it always was.
            source_removed: false,
            outcome: SourceItemOutcome::Done,
        });
    }
    let already_in_place = already_in_place.len();
    let sources = &sources[..];

    // Check if all sources are on the same filesystem as destination
    let same_fs = sources
        .iter()
        .all(|s| is_same_filesystem(s, destination).unwrap_or(false));

    if same_fs {
        // Use instant rename for each source
        same_fs::move_with_rename(
            events,
            operation_id,
            state,
            sources,
            destination,
            config,
            already_in_place,
        )
    } else {
        // Use atomic staging pattern for cross-filesystem move
        cross_fs::move_with_staging(
            events,
            operation_id,
            state,
            sources,
            destination,
            config,
            already_in_place,
        )
    }
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
        if is_cancelled(&state.intent) {
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

/// The rig both local-move suites share (engine call-throughs, operation state).
#[cfg(test)]
#[path = "test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "move_op_tests.rs"]
mod tests;

/// One data-safety invariant swept across both engines and every resolution.
#[cfg(test)]
#[path = "safety_matrix_tests.rs"]
mod safety_matrix_tests;

#[cfg(test)]
#[path = "move_journal_tests.rs"]
mod journal_tests;
