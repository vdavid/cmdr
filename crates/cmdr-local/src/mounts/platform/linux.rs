//! Linux: `/proc/mounts` through `crate::linux_mounts`.

use std::path::{Path, PathBuf};

pub(crate) fn list() -> Option<Vec<PathBuf>> {
    crate::linux_mounts::parse_proc_mounts()
        .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mountpoint)).collect())
}

pub(crate) fn is_mount_point(path: &Path) -> Option<bool> {
    crate::linux_mounts::is_mount_point(&path.to_string_lossy())
}
