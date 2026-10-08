//! Dating the folders a cross-volume copy created with their source folders'
//! dates, once everything inside them has landed.
//!
//! Writing a child bumps its folder's date on every real store, so a folder can
//! only keep its source's date if it's set AFTER its last child: the merge walk
//! notes each folder it created as it leaves it (post-order), and the copy
//! stamps the whole list once the subtree's leaves have drained. A subtree that
//! failed or was cancelled is never stamped: a folder holding part of its
//! contents must not claim the date of the whole. `DETAILS.md` § "Copies keep
//! the source's date".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use super::super::super::state::{WriteOperationState, is_cancelled};
use crate::file_system::volume::{Volume, VolumeError};

/// The folders one top-level source's walk created, each with its source
/// folder's date, in the order they're dated.
// DEFAULT-OK: an empty list is a walk that hasn't created a folder yet.
#[derive(Default)]
pub(super) struct FolderDates {
    /// In the order the walk LEFT them, which puts every folder after the
    /// folders inside it, and the top-level one last.
    folders: Vec<(PathBuf, u64)>,
}

impl FolderDates {
    /// A folder the walk created at `dest` and has just finished filling.
    /// `None` (a source that carried no date, like an S3 prefix) leaves it the
    /// store's own.
    pub(super) fn note_filled(&mut self, dest: PathBuf, source_date: Option<u64>) {
        if let Some(secs) = source_date {
            self.folders.push((dest, secs));
        }
    }

    /// Sets every noted folder's date on `dest_volume`, deepest first.
    ///
    /// Call it only once the subtree landed in full. Best effort: a refusal is a
    /// `log::warn!` and the copy stands; a destination with no folder dates to
    /// set (`NotSupported`) ends the pass at its first answer.
    pub(super) async fn stamp(self, dest_volume: &Arc<dyn Volume>, state: &WriteOperationState) {
        for (dest, secs) in self.folders {
            if !stamp_one(dest_volume, &dest, secs, state).await {
                return;
            }
        }
    }
}

/// Dates one folder. `false` means stop: the operation was cancelled, or the
/// destination can't date a folder at all.
async fn stamp_one(dest_volume: &Arc<dyn Volume>, dest: &Path, secs: u64, state: &WriteOperationState) -> bool {
    if is_cancelled(&state.intent) {
        return false;
    }
    match dest_volume
        .set_modified(dest, UNIX_EPOCH + Duration::from_secs(secs))
        .await
    {
        Ok(()) => true,
        Err(VolumeError::NotSupported) => false,
        Err(e) => {
            log::warn!(
                target: "transfer",
                "folder dates: {} keeps the copy's date, the destination refused the source's: {e:?}",
                dest.display()
            );
            true
        }
    }
}
