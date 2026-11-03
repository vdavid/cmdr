//! Which kind of container asked for a password. A leaf of its own because both
//! sides read it: `VolumeError::NeedsPassword` carries it, and the unlock seam in
//! `password_locked` answers in `VolumeError`, so living in either one would
//! weld the two into a module cycle.

use serde::{Deserialize, Serialize};

/// Which kind of container is locked. Rides on `VolumeError::NeedsPassword` so
/// the frontend words the prompt and picks its unlock flow from what actually
/// asked, ❌ never from the path: a header-encrypted 7z backed up in a restic
/// snapshot sits under `Browse snapshots`, yet the archive is what's locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LockedContainer {
    /// An encrypted archive: the password is stored and checked on the next read.
    Archive,
    /// A restic backup repo: the password is checked before the unlock answers.
    ResticRepo,
}

impl LockedContainer {
    /// The kind's own wire name, from its serde representation, so the token an
    /// agent reads can't drift from the one the frontend sends over IPC.
    pub fn token(self) -> &'static str {
        match self {
            LockedContainer::Archive => "archive",
            LockedContainer::ResticRepo => "resticRepo",
        }
    }
}
