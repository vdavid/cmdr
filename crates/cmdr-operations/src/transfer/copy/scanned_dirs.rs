//! Landing the scanned source directories the per-file copy loop didn't create
//! (empty dirs, and branches of only empty dirs).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::dest_chain::leaf_in_the_way;
use super::folder_metadata::{DirMapping, create_dirs_like_their_sources, dir_dest_path};
use crate::ledger::CopyTransaction;
use crate::state::WriteOperationState;
use crate::types::WriteOperationError;
use cmdr_fs::log_private::LogPrivate;

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
pub(in crate::transfer) fn create_scanned_dirs_at_destination(
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
                message: "Operation cancelled by user".into(),
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
                LogPrivate::dir_path(&dest),
                LogPrivate::path(&kept)
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
        dirs_to_create.reverse();
        let mapping = DirMapping {
            dest_root: destination,
            dest_leaf: &dest,
            source_leaf: dir,
        };
        create_dirs_like_their_sources(&dirs_to_create, &mapping).map_err(|e| WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to create directory: {}", e).into(),
        })?;
        for created in dirs_to_create {
            transaction.record_dir(created.clone());
            created_dirs.insert(created);
        }
    }
    Ok(())
}
