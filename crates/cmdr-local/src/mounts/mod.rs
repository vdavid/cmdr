//! The mount table, asked one way on every OS: which mount points exist, and whether a path is
//! one. Callers (the write engine's mount guards) ask here and never name the OS. Where this OS
//! mounts a person's volumes is pure path knowledge, so it lives in `cmdr_fs::mount_layout`.

mod platform;

use std::path::{Path, PathBuf};

/// Every mount point in the OS mount table, or `None` when it can't be read.
/// ❗ A syscall (macOS `getfsstat`) or a `/proc/mounts` read: never on a hot path.
pub fn list() -> Option<Vec<PathBuf>> {
    platform::list()
}

/// Whether `path` is where a filesystem is mounted, by the mount table; `None` when the table
/// can't say. Compared as spelled: a caller resolves links first.
pub fn is_mount_point(path: &Path) -> Option<bool> {
    platform::is_mount_point(path)
}

#[cfg(test)]
mod tests;
