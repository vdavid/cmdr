//! Landing the scanned source directories the per-file copy loop didn't create
//! (empty dirs, and branches of only empty dirs).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::file_system::write_operations::state::{CopyTransaction, WriteOperationState, is_cancelled};
use crate::file_system::write_operations::types::WriteOperationError;
use crate::file_system::write_operations::validation::path_exists_or_is_symlink;

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
/// same-named dir is a merge (nothing to create), and a same-named file is a
/// type clash where silently replacing user data with an empty directory would
/// be worse than skipping.
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
        if is_cancelled(&state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }
        let Some(dest) = dir_dest_path(dir, sources, destination) else {
            continue;
        };
        let dest = super::apply_dir_remap(&dest, dir_remap);
        if created_dirs.contains(&dest) || path_exists_or_is_symlink(&dest) {
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
