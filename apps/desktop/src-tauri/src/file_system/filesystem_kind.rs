//! Resolving a path to its OS filesystem-type string.
//!
//! The platform-specific half of filesystem identity. The classification itself
//! (`FilesystemKind`, `MaxFileSize`, `FilesystemInfo`, and the kind → limit map)
//! is platform-free and lives in `cmdr_fs::filesystem_kind`, re-exported here so
//! `file_system::filesystem_kind::FilesystemKind` keeps resolving.

pub use cmdr_fs::filesystem_kind::*;

/// One mount probe: the filesystem a path lives on, plus the mount's source
/// (`/dev/disk3s1`, `//me@nas/share`, whatever a FUSE mount chose). `None`
/// source when the probe couldn't resolve the mount.
pub struct MountProbe {
    pub info: FilesystemInfo,
    pub source: Option<String>,
}

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
pub fn detect_filesystem_for_path(path: &std::path::Path) -> FilesystemInfo {
    probe_mount_for_path(path).info
}

/// [`detect_filesystem_for_path`] plus the mount's source, from the same single probe.
#[cfg(target_os = "macos")]
pub fn probe_mount_for_path(path: &std::path::Path) -> MountProbe {
    let mount = crate::volumes::get_mount_info(&path.to_string_lossy());
    let (raw, source) = match mount {
        Some(m) => (Some(m.fs_type), Some(m.source)),
        None => (None, None),
    };
    MountProbe {
        info: FilesystemInfo::from_raw_type(raw),
        source,
    }
}

#[cfg(target_os = "linux")]
pub fn probe_mount_for_path(path: &std::path::Path) -> MountProbe {
    let entry = crate::file_system::linux_mounts::mount_entry_for_path(path);
    let (raw, source) = match entry {
        Some(e) => (Some(e.fstype), Some(e.device)),
        None => (None, None),
    };
    MountProbe {
        info: FilesystemInfo::from_raw_type(raw),
        source,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn probe_mount_for_path(_path: &std::path::Path) -> MountProbe {
    MountProbe {
        info: FilesystemInfo::from_raw_type(None),
        source: None,
    }
}
