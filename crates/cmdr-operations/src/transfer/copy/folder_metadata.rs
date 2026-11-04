//! What a folder this operation created takes from its source folder: born no
//! wider than the source, then, once everything inside it has landed, the
//! source's xattrs, dates, mode, ACL, and flags through `cmdr_fs::metadata`.
//!
//! The order is forced. A folder's mode and flags can refuse its own children
//! (`0500` and Locked refuse new entries), so they wait for the subtree; the
//! dates go on after the last child bumped them; and Locked goes last of all,
//! since it refuses a later `chmod`. Contract and rationale: `../DETAILS.md` §
//! "Folders keep their metadata".

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, LandedMetadata, StagedItem, StagedTimes, apply_landed, apply_staged};

use crate::state::WriteOperationState;

/// Owner read, write, and search: what a folder needs while the copy fills it,
/// and what a staged folder needs for Phase 3 to rename it and its children.
const OWNER_RWX: u32 = 0o700;

/// The mode a folder is born with while the copy fills it: its source's
/// permission bits plus owner `rwx`, which `mkdir` then narrows by the umask.
/// Never wider than the source, so a `0700` folder's contents are never open
/// to others mid-copy, and always fillable, so a `0500` source can't refuse its
/// own children. Its exact mode goes on once it's full. A copy that stops
/// before then keeps it this way: as private as the source, never wider.
fn creation_mode(source_dir: Option<&Path>) -> u32 {
    match source_dir.map(fs::symlink_metadata) {
        // No source (a destination folder that didn't exist yet): a plain `mkdir`.
        None => 0o777,
        Some(Ok(meta)) => (crate::local_disk::mode_of(&meta.permissions()) & 0o777) | OWNER_RWX,
        // An unreadable source folder: the narrowest folder the copy can fill.
        Some(Err(_)) => OWNER_RWX,
    }
}

/// Which source folder each destination folder on the way down to one item
/// copies: `dest_leaf` copies `source_leaf`, and every level strictly below
/// `dest_root` maps to the source level the same distance up. `dest_root` and
/// anything above it copy nothing.
pub(super) struct DirMapping<'a> {
    pub dest_root: &'a Path,
    pub dest_leaf: &'a Path,
    pub source_leaf: &'a Path,
}

impl DirMapping<'_> {
    fn source_of(&self, dir: &Path) -> Option<PathBuf> {
        if dir == self.dest_root || !dir.starts_with(self.dest_root) || !self.dest_leaf.starts_with(dir) {
            return None;
        }
        let levels_up = self.dest_leaf.components().count() - dir.components().count();
        self.source_leaf.ancestors().nth(levels_up).map(Path::to_path_buf)
    }
}

/// Creates `dirs`, outermost first, each born with [`creation_mode`] of the
/// source folder it copies. One that a racing writer made meanwhile counts as
/// made, the way `create_dir_all` treats it.
pub(super) fn create_dirs_like_their_sources(dirs: &[PathBuf], mapping: &DirMapping<'_>) -> io::Result<()> {
    for dir in dirs {
        let mode = creation_mode(mapping.source_of(dir).as_deref());
        match crate::local_disk::create_dir_with_mode(dir, mode) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Creates the one folder `dir` (its parent exists) for `source_dir`, born
/// with [`creation_mode`].
pub(super) fn create_dir_like_its_source(dir: &Path, source_dir: &Path) -> io::Result<()> {
    crate::local_disk::create_dir_with_mode(dir, creation_mode(Some(source_dir)))
}

/// Narrows a folder another helper just made with a plain `mkdir` to what
/// [`creation_mode`] would have given it. Best effort: the final pass sets the
/// exact mode on success either way.
pub(super) fn narrow_like_its_source(dir: &Path, source_dir: &Path) {
    let Ok(born) = fs::symlink_metadata(dir).map(|meta| crate::local_disk::mode_of(&meta.permissions()) & 0o7777)
    else {
        return;
    };
    let narrowed = born & creation_mode(Some(source_dir));
    if narrowed != born
        && let Err(e) = crate::local_disk::set_mode(dir, narrowed)
    {
        log::debug!(target: "transfer", "copy: {} stays {born:o}, not {narrowed:o}: {e}", LogPrivate::dir_path(dir));
    }
}

/// Puts each destination folder this operation created on its source folder's
/// metadata, once everything inside it has landed.
///
/// Runs after the per-file loop AND the empty-folder pass, on the success path
/// only: a stopped or failed copy dates nothing. `scanned_dirs` is
/// deepest-first, so a folder's mode and flags go on after its children's.
/// Only folders in `created_dirs` take anything: a folder the copy merged into
/// is the user's and keeps its own. Best effort: a class that won't set is a
/// `log::warn!`, never a failed copy.
///
/// `deferred` is `Some` on a cross-filesystem move, whose staged folders Phase 3
/// renames again: only what can't refuse that rename (the xattrs, the dates,
/// and a mode that keeps owner `rwx`) goes on now, and the rest goes in the map
/// under the staged path, for the move to apply deepest first once Phase 3 has
/// put each folder in its final place.
pub(in crate::transfer) fn apply_created_dirs_metadata(
    scanned_dirs: &[PathBuf],
    sources: &[PathBuf],
    destination: &Path,
    state: &WriteOperationState,
    created_dirs: &[PathBuf],
    dir_remap: &HashMap<PathBuf, PathBuf>,
    mut deferred: Option<&mut HashMap<PathBuf, LandedMetadata>>,
) {
    let created: HashSet<&Path> = created_dirs.iter().map(PathBuf::as_path).collect();
    for dir in scanned_dirs {
        if state.intent().is_cancelled() {
            return;
        }
        let Some(dest) = dir_dest_path(dir, sources, destination) else {
            continue;
        };
        let dest = super::apply_dir_remap(&dest, dir_remap);
        if !created.contains(dest.as_path()) {
            continue;
        }
        let meta = match fs::symlink_metadata(dir) {
            Ok(meta) => meta,
            Err(e) => {
                log::warn!(target: "transfer", "copy: {} keeps the copy's metadata, {} can't be read: {e}", LogPrivate::dir_path(&dest), LogPrivate::dir_path(dir));
                continue;
            }
        };
        let (carried, landed) = folder_sets(dir, &meta);
        let times = StagedTimes {
            modified: meta.modified().ok(),
            accessed: meta.accessed().ok(),
        };
        let staged = apply_staged(StagedItem::Path(&dest), &carried, times);
        let landed_report = match deferred.as_deref_mut() {
            _ if landed.is_empty() => None,
            Some(map) => {
                map.insert(dest.clone(), landed);
                None
            }
            None => Some(apply_landed(&dest, &landed)),
        };
        if staged.any_refused() || landed_report.is_some_and(|report| report.any_refused()) {
            log::warn!(
                target: "transfer",
                "copy: {} landed without some of {}'s metadata: {staged:?} {landed_report:?}",
                LogPrivate::dir_path(&dest),
                LogPrivate::dir_path(dir)
            );
        }
    }
}

/// A source folder's metadata split into the half that may go on while the
/// folder can still be filled or renamed and the half that can't. A mode that
/// keeps owner `rwx` refuses nothing, so it rides the first; one without (a
/// `0500` or `0555` folder) waits with the ACL and the flags.
fn folder_sets(dir: &Path, meta: &fs::Metadata) -> (CarriedMetadata, LandedMetadata) {
    let mut carried = CarriedMetadata::of_local_dir(dir, meta);
    let mut landed = LandedMetadata::of_source_acl(dir).with_flags(carried.flags);
    if let Some(mode) = carried.mode
        && mode & OWNER_RWX != OWNER_RWX
    {
        carried.mode = None;
        landed = landed.with_mode(mode);
    }
    (carried, landed)
}

/// Maps a scanned source directory to its destination path, mirroring
/// `FileInfo::dest_path`: the path relative to its top-level source's parent,
/// joined onto `destination`. `None` when the dir isn't under any source
/// (can't happen for paths produced by the scan walker over these sources).
pub(in crate::transfer) fn dir_dest_path(dir: &Path, sources: &[PathBuf], destination: &Path) -> Option<PathBuf> {
    sources.iter().find_map(|source| {
        if !dir.starts_with(source) {
            return None;
        }
        let root = source.parent().unwrap_or(source);
        dir.strip_prefix(root).ok().map(|relative| destination.join(relative))
    })
}
