//! No mount-table reader on Windows yet (`GetLogicalDriveStringsW` and
//! `GetVolumePathNamesForVolumeNameW` are its sources), so every answer is "can't say", which
//! the write engine's guards treat as a boundary.

use std::path::{Path, PathBuf};

pub(crate) fn list() -> Option<Vec<PathBuf>> {
    None
}

pub(crate) fn is_mount_point(_path: &Path) -> Option<bool> {
    None
}
