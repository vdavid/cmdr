//! Picks this OS's mount-table reader.

use std::path::{Path, PathBuf};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> Option<Vec<PathBuf>> = list;
const _: fn(&Path) -> Option<bool> = is_mount_point;

/// Every mount point, or `None` when the table can't be read.
pub(super) use imp::list;

/// Whether `path` is a mount point, or `None` when the table can't say.
pub(super) use imp::is_mount_point;
