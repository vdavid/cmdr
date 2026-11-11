//! macOS: `getfsstat` through `crate::mount_table`.

use std::path::{Path, PathBuf};

pub(crate) fn list() -> Option<Vec<PathBuf>> {
    crate::mount_table::enumerate_mounts()
        .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mount_point)).collect())
}

pub(crate) fn is_mount_point(path: &Path) -> Option<bool> {
    crate::mount_table::is_mount_point(&path.to_string_lossy())
}
