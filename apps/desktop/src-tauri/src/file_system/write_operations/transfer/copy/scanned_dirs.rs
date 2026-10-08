//! Landing the scanned source directories the per-file copy loop didn't create
//! (empty dirs, and branches of only empty dirs).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::dest_chain::leaf_in_the_way;
use crate::file_system::write_operations::ledger::CopyTransaction;
use crate::file_system::write_operations::state::{WriteOperationState, is_cancelled};
use crate::file_system::write_operations::types::WriteOperationError;

/// Creates destination directories for the scanned source dirs the per-file
/// loop didn't materialize. The loop creates directories only as FILE parents,
/// so an empty directory — or a branch holding nothing but empty directories —
/// used to complete "successfully" while never arriving at the destination
/// (and on a cross-FS move, Phase 4 then deleted the source: the empty dir was
/// destroyed without ever landing). The scan already collected every source
/// dir; this pass lands the missing ones.
///
/// Mirrors `FileInfo::dest_path`'s mapping (the path relative to its top-level
/// source's parent, joined onto `destination`), honors active folder→file
/// Rename redirects via `dir_remap`, and records created dirs in the
/// transaction for rollback.
///
/// Data-safety: a dest path that already holds ANYTHING is left untouched — a
/// same-named dir is a merge (nothing to create), and a same-named file or link
/// is a type clash where silently replacing user data with an empty directory
/// would be worse than skipping. Nothing is created THROUGH a link either
/// (`dest_chain.rs`).
pub(in crate::file_system::write_operations::transfer) fn create_scanned_dirs_at_destination(
    scanned_dirs: &[PathBuf],
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    transaction: &mut CopyTransaction,
    created_dirs: &mut HashSet<PathBuf>,
    dir_remap: &HashMap<PathBuf, PathBuf>,
) -> Result<(), WriteOperationError> {
    // `scanned_dirs` is deepest-first (the delete order); reverse so parents
    // come before children.
    for dir in scanned_dirs.iter().rev() {
        // The cooperative boundary, per directory. This pass runs after the
        // per-file loop, so "Paused" is already on screen when it starts.
        if state.stop_or_park_sync() {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }
        let Some(dest) = dir_dest_path(dir, sources, destination) else {
            continue;
        };
        let dest = super::apply_dir_remap(&dest, dir_remap);
        // A LEAF stands at this path or above it, below the destination: a
        // file, or a link whatever it points at. It stays, and nothing lands at
        // or under it. Under a file that's a folder→file clash that ended in
        // Skip (creating would fail ENOTDIR and take the operation down over a
        // clash it already settled). Under a link, creating would make the
        // directory inside the link's TARGET, a folder the user never picked.
        if let Some(kept) = leaf_in_the_way(destination, &dest, created_dirs)? {
            log::debug!(
                "copy: not landing {} at or under the kept {}",
                dest.display(),
                kept.display()
            );
            continue;
        }
        // Standing already: made by the file loop, or a real directory the
        // walk above just proved (a merge, nothing to create).
        if created_dirs.contains(&dest) {
            continue;
        }
        // Collect the missing ancestors first so rollback records exactly what
        // this pass created (same pattern as the file loop's parent creation).
        let mut dirs_to_create: Vec<PathBuf> = Vec::new();
        let mut walk = dest.clone();
        while !walk.exists() && !created_dirs.contains(&walk) {
            dirs_to_create.push(walk.clone());
            match walk.parent() {
                Some(p) => walk = p.to_path_buf(),
                None => break,
            }
        }
        fs::create_dir_all(&dest).map_err(|e| WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to create directory: {}", e),
        })?;
        for created in dirs_to_create.into_iter().rev() {
            transaction.record_dir(created.clone());
            created_dirs.insert(created);
        }
    }
    Ok(())
}

/// Dates every destination folder this operation created with its source
/// folder's date, once everything inside it has landed.
///
/// Writing a child bumps its folder's date, so this runs after the per-file loop
/// AND [`create_scanned_dirs_at_destination`], on the success path only: a
/// stopped or failed copy dates nothing. `scanned_dirs` is deepest-first, which
/// is also the order this needs (dating a folder doesn't touch its parent, but
/// it keeps the rule the same as the cross-volume engine's,
/// `../volume/folder_dates.rs`). Only folders in `created_dirs` are dated: a
/// folder the copy merged into is the user's, and keeps the date the filesystem
/// gives it. Best effort: a date that won't set is a `log::warn!`. The contract:
/// `../volume/DETAILS.md` § "Copies keep the source's date".
pub(in crate::file_system::write_operations::transfer) fn date_created_dirs_like_their_sources(
    scanned_dirs: &[PathBuf],
    sources: &[PathBuf],
    destination: &Path,
    state: &WriteOperationState,
    created_dirs: &[PathBuf],
    dir_remap: &HashMap<PathBuf, PathBuf>,
) {
    let created: HashSet<&Path> = created_dirs.iter().map(PathBuf::as_path).collect();
    for dir in scanned_dirs {
        if is_cancelled(&state.intent) {
            return;
        }
        let Some(dest) = dir_dest_path(dir, sources, destination) else {
            continue;
        };
        let dest = super::apply_dir_remap(&dest, dir_remap);
        if !created.contains(dest.as_path()) {
            continue;
        }
        let dated = fs::symlink_metadata(dir)
            .map(|meta| filetime::FileTime::from_last_modification_time(&meta))
            .and_then(|mtime| filetime::set_file_mtime(&dest, mtime));
        if let Err(e) = dated {
            log::warn!(
                target: "transfer",
                "copy: {} keeps the copy's date, not {}'s: {e}",
                dest.display(),
                dir.display()
            );
        }
    }
}

/// Maps a scanned source directory to its destination path, mirroring
/// `FileInfo::dest_path`: the path relative to its top-level source's parent,
/// joined onto `destination`. `None` when the dir isn't under any source
/// (can't happen for paths produced by the scan walker over these sources).
fn dir_dest_path(dir: &Path, sources: &[PathBuf], destination: &Path) -> Option<PathBuf> {
    sources.iter().find_map(|source| {
        if !dir.starts_with(source) {
            return None;
        }
        let root = source.parent().unwrap_or(source);
        dir.strip_prefix(root).ok().map(|relative| destination.join(relative))
    })
}
