//! The cross-volume move's source sweep: removing a top-level source once its
//! copy has landed.
//!
//! The sweep deletes a LEDGER, never a tree, the same rule the local
//! cross-filesystem sweep follows (`move_op/source_sweep.rs`). The copy walk
//! records every source file it carried, with what the source listing said about
//! it at the time ([`SourceStamp`]), and every source folder it walked. The sweep
//! lists each walked folder once more and removes only the files that are in the
//! ledger AND still match their stamp; a folder goes only once it's empty
//! (`Volume::delete` is empty-only for a directory on every backend).
//!
//! Everything else stays, because the source holds the only copy of it:
//!
//! - **Appeared**: an entry the walk never saw, a download or a sync landing
//!   while the folder copied. Counted once per unknown subtree.
//! - **Changed**: a carried file whose listing no longer matches its stamp, an
//!   app saving over it after the walk listed it. The destination has the old
//!   bytes.
//! - **Skipped**: a child a merge conflict resolved to Skip. The user asked for
//!   that, so it's not counted as news.
//!
//! ❌ Never go back to a recursive delete of the source (`remove_tree`): it acts
//! on what is on disk NOW rather than on what this move carried.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{EntryKind, Volume, VolumeError};

/// What a source entry looked like to the move: its size, its mtime, and its
/// inode where the backend has one. Compared field by field, so a field a backend
/// never reports (an MTP device with no mtime, every non-local inode) drops out
/// of the comparison rather than failing it.
///
/// An mtime is sound here though the in-flight ledgers refuse one
/// (`../DETAILS.md` § "What the in-flight ledgers record"): both reads ask the
/// SAME backend about the SAME file the same way, so a coarse clock truncates
/// both alike and can't invent a change. What it can hide is a same-size save
/// inside one tick (the listing's clock is whole seconds); a local source's
/// inode covers the save-by-rename case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::file_system::write_operations) struct SourceStamp {
    size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
}

impl SourceStamp {
    pub(super) fn of(entry: &FileEntry) -> Self {
        Self {
            size: entry.size,
            modified_at: entry.modified_at,
            inode: entry.inode,
        }
    }
}

/// What one top-level source's copy walk carried: the source files it copied
/// (with their stamps) and the source folders it listed.
// DEFAULT-OK: an empty ledger is a walk that has carried nothing yet, and a sweep
// over it removes nothing.
#[derive(Default)]
pub(in crate::file_system::write_operations) struct SourceLedger {
    files: HashMap<PathBuf, SourceStamp>,
    walked_dirs: HashSet<PathBuf>,
}

impl SourceLedger {
    pub(super) fn record_walked_dir(&mut self, dir: PathBuf) {
        self.walked_dirs.insert(dir);
    }

    pub(super) fn record_carried_file(&mut self, file: PathBuf, stamp: SourceStamp) {
        self.files.insert(file, stamp);
    }

    /// Whether the move carried this path, as a file or as a folder it walked.
    fn carried(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.walked_dirs.contains(path)
    }
}

/// What the sweep left in one source folder.
// DEFAULT-OK: zero is "the sweep has left nothing here yet", true of a sweep
// that hasn't run and of one that took everything.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::file_system::write_operations) struct FolderLeftovers {
    pub(in crate::file_system::write_operations) appeared: u32,
    pub(in crate::file_system::write_operations) changed: u32,
}

/// Removes a moved source folder from the ledger its copy walk kept, sparing
/// `skipped` (the merge's Skips) and whatever appeared or changed.
///
/// A child that refuses to go keeps its folder alive, and the FIRST such failure
/// comes out with the child's own path: the leaf is the diagnosis, the folder's
/// `ENOTEMPTY` would only be its symptom. The sweep keeps going past a failure,
/// so it clears what it can.
async fn sweep_moved_folder(
    volume: &Arc<dyn Volume>,
    folder: &Path,
    ledger: &SourceLedger,
    skipped: &HashSet<PathBuf>,
) -> Result<FolderLeftovers, PathedVolumeError> {
    let mut left = FolderLeftovers::default();
    sweep_level(volume, folder, ledger, skipped, &mut left).await?;
    Ok(left)
}

/// One folder of the sweep. `Ok(true)` means something stays under it, so the
/// caller keeps it.
async fn sweep_level(
    volume: &Arc<dyn Volume>,
    dir: &Path,
    ledger: &SourceLedger,
    skipped: &HashSet<PathBuf>,
    left: &mut FolderLeftovers,
) -> Result<bool, PathedVolumeError> {
    let entries = match volume.list_directory(dir, None).await {
        Ok(entries) => entries,
        // Gone already: whoever removed it, nothing of it is left to protect.
        Err(VolumeError::NotFound(_)) => return Ok(false),
        Err(e) => return Err(e).at(dir),
    };

    let mut remains = false;
    let mut first_failure: Option<PathedVolumeError> = None;
    // The files this level clears, deleted together once the level is read:
    // one request per thousand on an object store (`Volume::delete_files`).
    let mut doomed: Vec<PathBuf> = Vec::new();
    for entry in &entries {
        let path = PathBuf::from(&entry.path);
        let outcome = if skipped.contains(&path) {
            Ok(true)
        } else {
            match kind_of(volume, entry, &path).await {
                Err(e) => Err(e),
                Ok(EntryKind::Directory) if ledger.walked_dirs.contains(&path) => {
                    Box::pin(sweep_level(volume, &path, ledger, skipped, left)).await
                }
                // A link the move carried goes as the link, ❌ never through it:
                // on a backend whose listing follows links the walk copied what
                // it points at, and that target is not the user's selection.
                Ok(EntryKind::Symlink) if ledger.carried(&path) => volume.delete(&path).await.at(&path).map(|()| false),
                Ok(EntryKind::Directory | EntryKind::Symlink) => {
                    left.appeared += 1;
                    Ok(true)
                }
                Ok(EntryKind::File) => match ledger.files.get(&path) {
                    None => {
                        left.appeared += 1;
                        Ok(true)
                    }
                    Some(stamp) if *stamp != SourceStamp::of(entry) => {
                        left.changed += 1;
                        Ok(true)
                    }
                    Some(_) => {
                        doomed.push(path);
                        Ok(false)
                    }
                },
            }
        };
        note_outcome(outcome, &mut remains, &mut first_failure);
    }
    let results = volume.delete_files(&doomed).await;
    for (path, result) in doomed.iter().zip(results) {
        note_outcome(result.at(path).map(|()| false), &mut remains, &mut first_failure);
    }

    if remains {
        return match first_failure {
            Some(failure) => Err(failure),
            None => Ok(true),
        };
    }
    match volume.delete(dir).await {
        // A folder that only existed through what was in it (an object
        // store's prefix with no marker) went with its last file.
        Ok(()) | Err(VolumeError::NotFound(_)) => Ok(false),
        Err(e) => Err(e).at(dir),
    }
}

/// Folds one entry's outcome into its level's: something that stays keeps the
/// folder, and the FIRST failure is the one reported, with its own path.
fn note_outcome(
    outcome: Result<bool, PathedVolumeError>,
    remains: &mut bool,
    first_failure: &mut Option<PathedVolumeError>,
) {
    match outcome {
        Ok(stays) => *remains |= stays,
        Err(e) => {
            log::warn!(
                target: "move",
                "source sweep: couldn't remove {}: {:?}",
                e.path.display(),
                e.error
            );
            *remains = true;
            first_failure.get_or_insert(e);
        }
    }
}

/// What a move carried out of one top-level source, for the sweep that removes
/// it once the destination is safe.
pub(in crate::file_system::write_operations) enum CarriedSource {
    /// A file, with its stamp from before its bytes were read. `None` when that
    /// stat failed, which proves nothing, so the sweep keeps the file.
    File(Option<SourceStamp>),
    /// A folder, with the ledger of what its walk listed and carried.
    Folder(SourceLedger),
}

/// Stamps one top-level source BEFORE a move reads it, for a caller whose copy
/// doesn't run through the merge walk (which records its own ledger as it
/// lists): an into-zip move reads its sources straight off the disk or through a
/// scratch pull. A folder is walked the way the sweep will list it, one
/// `list_directory` per folder.
///
/// Anything that turns up between this walk and the read goes into the move but
/// not into the ledger, so it stays in the source too: a duplicate, never a
/// loss.
pub(in crate::file_system::write_operations) async fn stamp_source(
    volume: &Arc<dyn Volume>,
    source: &Path,
) -> Result<CarriedSource, PathedVolumeError> {
    if volume.entry_kind(source).await.at(source)? != EntryKind::Directory {
        return Ok(CarriedSource::File(stamp_file(volume, source).await));
    }
    let mut ledger = SourceLedger::default();
    let mut pending = vec![source.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in volume.list_directory(&dir, None).await.at(&dir)? {
            let path = PathBuf::from(&entry.path);
            match kind_of(volume, &entry, &path).await? {
                EntryKind::Directory => pending.push(path),
                EntryKind::File => ledger.record_carried_file(path, SourceStamp::of(&entry)),
                // A link is never walked through (it could loop, and a move
                // carries a link as itself); unrecorded, it stays in the source.
                EntryKind::Symlink => {}
            }
        }
        ledger.record_walked_dir(dir);
    }
    Ok(CarriedSource::Folder(ledger))
}

/// Removes one moved top-level source from what the move carried, sparing
/// `skipped` (a merge's Skips) and whatever appeared or changed since. Answers
/// what stayed.
pub(in crate::file_system::write_operations) async fn sweep_carried_source(
    volume: &Arc<dyn Volume>,
    source: &Path,
    carried: &CarriedSource,
    skipped: &HashSet<PathBuf>,
) -> Result<FolderLeftovers, PathedVolumeError> {
    match carried {
        CarriedSource::Folder(ledger) => match volume.entry_kind(source).await {
            // A selected link the walk followed: the link goes, its target stays.
            Ok(EntryKind::Symlink) => {
                volume.delete(source).await.at(source)?;
                Ok(FolderLeftovers::default())
            }
            Ok(_) | Err(VolumeError::NotFound(_)) => sweep_moved_folder(volume, source, ledger, skipped).await,
            Err(e) => Err(e).at(source),
        },
        CarriedSource::File(before) => {
            if file_is_unchanged(volume, source, *before).await? {
                volume.delete(source).await.at(source)?;
                Ok(FolderLeftovers::default())
            } else {
                Ok(FolderLeftovers {
                    appeared: 0,
                    changed: 1,
                })
            }
        }
    }
}

/// The stamp of one top-level source FILE, read right before its copy so a save
/// during the copy counts as a change too. `None` when the stat fails.
pub(super) async fn stamp_file(volume: &Arc<dyn Volume>, file: &Path) -> Option<SourceStamp> {
    volume
        .get_metadata(file)
        .await
        .ok()
        .map(|entry| SourceStamp::of(&entry))
}

/// Whether a top-level source file still looks the way it did before its copy.
/// A stamp that couldn't be taken proves nothing and keeps the file. A file
/// already gone answers `true` and leaves the verdict to the delete.
async fn file_is_unchanged(
    volume: &Arc<dyn Volume>,
    file: &Path,
    before: Option<SourceStamp>,
) -> Result<bool, PathedVolumeError> {
    match volume.get_metadata(file).await {
        Ok(now) => Ok(before == Some(SourceStamp::of(&now))),
        Err(VolumeError::NotFound(_)) => Ok(true),
        Err(e) => Err(e).at(file),
    }
}

/// What a listed entry is. A plain file is taken at the listing's word; anything
/// that might be a directory or a link asks `Volume::entry_kind`, because on some
/// backends a listing's `is_directory` is true for a link to a folder
/// (`../DETAILS.md` § "Symlinks are opaque to a move").
async fn kind_of(volume: &Arc<dyn Volume>, entry: &FileEntry, path: &Path) -> Result<EntryKind, PathedVolumeError> {
    if entry.is_directory || entry.is_symlink {
        volume.entry_kind(path).await.at(path)
    } else {
        Ok(EntryKind::File)
    }
}
