//! Resolving a path to its OS filesystem-type string.
//!
//! The platform-specific half of filesystem identity. The classification itself
//! (`FilesystemKind`, `MaxFileSize`, `FilesystemInfo`, and the kind → limit map)
//! is platform-free and lives in `cmdr_fs::filesystem_kind`, re-exported here so
//! `file_system::filesystem_kind::FilesystemKind` keeps resolving.

pub use cmdr_fs::filesystem_kind::*;

/// Detects the filesystem at `path` by resolving its mount and reading the OS
/// filesystem-type string, then classifying it.
///
/// Returns [`FilesystemKind::Other`] / [`MaxFileSize::Unknown`] when the type
/// can't be resolved, so the write guard never blocks on a guess.
///
/// macOS resolves via `statfs.f_fstypename`; other Unix via `/proc/mounts`.
/// The single `statfs` is fast on local mounts (the only ones that reach the
/// local-FS copy/move path); a hung network mount would already have stalled the
/// preceding free-space query on the same destination.
#[cfg(target_os = "macos")]
pub fn detect_filesystem_for_path(path: &std::path::Path) -> FilesystemInfo {
    let raw = crate::volumes::get_mount_point(&path.to_string_lossy()).map(|(_, fs_type)| fs_type);
    FilesystemInfo::from_raw_type(raw)
}

#[cfg(target_os = "linux")]
pub fn detect_filesystem_for_path(path: &std::path::Path) -> FilesystemInfo {
    let raw = crate::file_system::linux_mounts::fs_type_for_path(path);
    FilesystemInfo::from_raw_type(raw)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn detect_filesystem_for_path(_path: &std::path::Path) -> FilesystemInfo {
    FilesystemInfo::from_raw_type(None)
}
