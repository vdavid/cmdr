//! A mount root that MOVED while its filesystem stayed mounted: a renamed drive.
//!
//! Renaming a mounted volume moves its mount point (`/Volumes/Old` →
//! `/Volumes/New`) with no unmount in between, so the id, which keys on the
//! filesystem, stays put and only the root it's reached at changes. Unmount and
//! remount would be the wrong model: the unmount half stops the drive's index and
//! sends every pane on it home. Rationale: `../DETAILS.md` § "A renamed drive".

use std::path::Path;

use super::VolumeManager;
use crate::ignore_poison::RwLockIgnorePoison;

/// What [`VolumeManager::move_root`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum RootMove {
    /// No registration knows the old root.
    Unknown,
    /// The ACTIVE root moved: the volume serves the id from the new root now, and
    /// the old one left the entry's root set.
    Moved { id: String },
    /// A FALLBACK root moved; the active root and the volume are untouched.
    SiblingMoved { id: String },
    /// The active root moved, but the backend can't be re-rooted, so it stays
    /// anchored to the old root, now gone and marked stale. No shipping local
    /// backend declines (`LocalPosixVolume` re-roots), so this is the safety net.
    BackendCantReroot { id: String },
}

impl VolumeManager {
    /// Move the mount root `from` to `to`, keeping the id it serves.
    ///
    /// Pure registry work under the write lock, like `remove_root`: telling the
    /// panes and the index belongs to the caller, which is why every arm names the
    /// id. ❗ The caller has to know it's the SAME filesystem (the OS said it was a
    /// rename, and the id `to` derives is this one); this doesn't ask.
    ///
    /// The re-rooted instance is ❌ not retired, the same as a promotion: it's the
    /// same mount, and nothing about it ended.
    pub fn move_root(&self, from: &Path, to: &Path) -> RootMove {
        let mut volumes = self.volumes.write_ignore_poison();
        let Some((id, entry)) = volumes.iter_mut().find(|(_, entry)| entry.knows_root(from)) else {
            return RootMove::Unknown;
        };
        let id = id.clone();

        if entry.volume.root() != from {
            entry.roots.retain(|root| root.path != from);
            entry.record_root(to);
            return RootMove::SiblingMoved { id };
        }
        match entry.volume.rerooted(to) {
            Some(rerooted) => {
                // The instance it replaced is the same mount's previous handle, and
                // nothing about that mount ended, so it goes without retiring.
                let _previous = entry.replace_root(rerooted);
                RootMove::Moved { id }
            }
            None => {
                entry.readd_stale_root(from);
                entry.tell_volume_if_its_root_is_dead();
                RootMove::BackendCantReroot { id }
            }
        }
    }
}

#[cfg(test)]
#[path = "root_move_tests.rs"]
mod root_move_tests;
