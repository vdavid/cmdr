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
use super::super::ledger::{WrittenFile, WrittenIdentity};
use super::super::overwrite::{rename_no_replace, safe_overwrite_dir};
use super::super::reversal::{Recheck, ReversalTally, recheck_local};
use super::super::scan::handle_dry_run;
use super::super::state::{WriteOperationState, is_cancelled};
use super::super::types::{
    SourceItemOutcome, WriteOperationConfig, WriteOperationError, WriteOperationType, WriteSourceItemDoneEvent,
};
use super::super::validation::{is_same_file, is_same_filesystem, path_exists_or_is_symlink};
use crate::operation_log::rollback::ItemResult;
use crate::operation_log::types::SkipReason;

mod cross_fs;
mod same_fs;

// ============================================================================
// Move rollback tracking
// ============================================================================

/// One item this move renamed into place.
struct MovedItem {
    /// Where it was before the rename — where a reversal puts it back.
    original_source: PathBuf,
    /// Where it sits now, with the identity a reversal rechecks before touching
    /// it. A rename carries the node id across untouched, so a snapshot taken of
    /// the SOURCE just before the rename describes the landed item exactly.
    landed: WrittenFile,
}

/// Tracks renames performed during same-FS move for rollback on cancellation.
///
/// A **stack**: [`MoveTransaction::pop`] takes the newest rename off as it's
/// reversed, so the ledger claims exactly what this operation currently has
/// sitting at the destination.
struct MoveTransaction {
    renames: Vec<MovedItem>,
}

impl MoveTransaction {
    fn new() -> Self {
        Self { renames: Vec::new() }
    }

    fn record(&mut self, source: PathBuf, landed: WrittenFile) {
        self.renames.push(MovedItem {
            original_source: source,
            landed,
        });
    }

    /// Take the newest rename off the ledger, to reverse it.
    fn pop(&mut self) -> Option<MovedItem> {
        self.renames.pop()
    }

    /// Reverses all recorded renames (dest → source) in reverse order, and
    /// reports what it left alone. Same-FS rename is instant, so this runs
    /// synchronously.
    ///
    /// Intentional: this reverses the moves THIS op made; it does NOT restore a
    /// destination that an Overwrite-with-rename replaced (no per-file backup is
    /// kept — see `overwrite::safe_overwrite_file` step 4). Keeping backups for the
    /// whole operation risks unexpectedly filling the user's drive on a large
    /// Overwrite. Revisit if users complain. See transfer/volume/DETAILS.md
    /// § "Overwrite isn't reversible".
    fn rollback(&mut self) -> ReversalTally {
        let mut tally = ReversalTally::default();
        while let Some(item) = self.pop() {
            tally.record(restore_moved_item(&item), &item.landed.path);
        }
        tally
    }
}

/// Put one moved item back where it came from — if what sits at the landed path
/// is still the item this move put there, AND nothing new has taken its original
/// place.
///
/// **Two guards, the pair the history engine pins.** The recheck stops the move
/// back from carrying off a file something else replaced at the destination. The
/// non-destructive restore stops the rename from silently destroying whatever the
/// user has since created at the original source — `rename(2)` replaces its target
/// without a word, and there is no backup to put back afterwards. Either refusal
/// leaves the item where it landed, which is recoverable; the alternative isn't.
fn restore_moved_item(item: &MovedItem) -> ItemResult {
    match recheck_local(&item.landed) {
        // Something took the moved item away. The end state a restore wanted
        // (nothing of ours at the destination) holds, so this is idempotent.
        Recheck::AlreadyGone => return ItemResult::Skipped(SkipReason::AlreadyGone),
        Recheck::Skip(reason) => return ItemResult::Skipped(reason),
        Recheck::Act => {}
    }
    // Advisory: this stat names the ordinary case with a log line, and it's the
    // only thing that can tell a real collision from a case-only self-collision.
    // The refusal itself lives in `restore_rename`, which is what makes the guard
    // hold against an entry that appears after this stat.
    let mut force = false;
    if let Ok(occupant) = fs::symlink_metadata(&item.original_source) {
        if occupant_is_the_item_itself(item, &occupant) {
            force = true;
        } else {
            log::info!(
                "move rollback: leaving {} where it is, something else now sits at {}",
                item.landed.path.display(),
                item.original_source.display()
            );
            return ItemResult::Skipped(SkipReason::RestoreTargetOccupied);
        }
    }
    restore_rename(&item.landed.path, &item.original_source, force)
}

/// The restore's own rename, which refuses to replace whatever is at
/// `original_source` unless `force` says the entry there IS the item coming back.
///
/// This is where the non-destructive guarantee actually lives. The caller's stat
/// only reports what was there when it looked; an entry created in the window
/// between that stat and this rename would still be in the way, and a plain
/// `rename(2)` would carry it off without a word. `rename_no_replace` closes the
/// window with the kernel's atomic no-replace flag.
///
/// `force` is the case-only self-collision, where the entry the target reports IS
/// the item being restored, so the rename has to be allowed to land on it.
fn restore_rename(landed: &Path, original_source: &Path, force: bool) -> ItemResult {
    let renamed = if force {
        fs::rename(landed, original_source)
    } else {
        rename_no_replace(landed, original_source)
    };
    match renamed {
        Ok(()) => ItemResult::Reversed,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            log::info!(
                "move rollback: leaving {} where it is, something took {} while the reversal ran",
                landed.display(),
                original_source.display()
            );
            ItemResult::Skipped(SkipReason::RestoreTargetOccupied)
        }
        Err(e) => {
            log::warn!(
                "move rollback: couldn't rename {} back to {}: {}",
                landed.display(),
                original_source.display(),
                e
            );
            ItemResult::Skipped(SkipReason::Failed)
        }
    }
}

/// Is the entry occupying the original source actually the item being restored,
/// rather than a real collision?
///
/// A case-insensitive filesystem folds `dog.jpg` and `dog.JPG` onto one entry, so
/// a move that only changed a name's case finds its own destination sitting at the
/// source. Same node id ⇒ same entry, and the rename back is then a case
/// correction, not an overwrite. This is one local filesystem, so `(dev, ino)`
/// settles it exactly and no path folding is needed.
///
/// The recorded node id is the right side to compare: the recheck above just
/// proved the entry still at the landed path carries it.
fn occupant_is_the_item_itself(item: &MovedItem, occupant: &fs::Metadata) -> bool {
    match (item.landed.identity.node(), WrittenIdentity::node_of_stat(occupant)) {
        (Some(recorded), Some(live)) => recorded == live,
        // Nothing to compare, so nothing is proven — and an unproven restore that
        // might overwrite is one we don't make.
        _ => false,
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
    // The source's own metadata, stat'd before the rename. The rename carries the
    // node id across, so this IS the landed item's identity; `None` (the stat
    // failed) leaves the ledger entry unverifiable.
    source_stat: Option<&fs::Metadata>,
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
        move_tx.record(
            source.to_path_buf(),
            WrittenFile::local_stat(resolved.path.clone(), source_stat),
        );
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
        move_tx.record(
            source.to_path_buf(),
            WrittenFile::local_stat(resolved.path.clone(), source_stat),
        );
    } else {
        fs::rename(source, &resolved.path).with_path(source)?;
        move_tx.record(
            source.to_path_buf(),
            WrittenFile::local_stat(resolved.path.clone(), source_stat),
        );
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

        // Snapshot the child before the rename carries it across. The rename
        // preserves the node id, so this describes what lands at `dest_child` —
        // and it's the ONLY identity a cancelled merge has to reverse from, since
        // the journal marks a directory merge unreversible.
        let child_stat = fs::symlink_metadata(&source_child).ok();

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
                    move_resolved_into_place(&source_child, &dest_child, &resolved, child_stat.as_ref(), move_tx)?;
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
            move_tx.record(source_child, WrittenFile::local_stat(dest_child, child_stat.as_ref()));
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

/// What each ledger entry records about the rename it stands for.
#[cfg(test)]
#[path = "move_reversal_tests.rs"]
mod move_reversal_tests;

#[cfg(test)]
#[path = "move_ledger_tests.rs"]
mod move_ledger_tests;

/// One data-safety invariant swept across both engines and every resolution.
#[cfg(test)]
#[path = "safety_matrix_tests.rs"]
mod safety_matrix_tests;

#[cfg(test)]
#[path = "move_journal_tests.rs"]
mod journal_tests;
