//! Resolving a path to its OS filesystem-type string.
//!
//! The platform-specific half of filesystem identity. The classification itself
//! (`FilesystemKind`, `MaxFileSize`, `FilesystemInfo`, and the kind → limit map)
//! is platform-free and lives in `cmdr_fs::filesystem_kind`; callers name it
//! there. Also the "is this on a network mount?" question asked of the same
//! probe.

use cmdr_fs::filesystem_kind::{FilesystemInfo, mount_is_local_disk};
#[cfg(doc)]
use cmdr_fs::filesystem_kind::{FilesystemKind, MaxFileSize};

/// One mount probe: the filesystem a path lives on, plus the mount's source
/// (`/dev/disk3s1`, `//me@nas/share`, whatever a FUSE mount chose). `None`
/// source when the probe couldn't resolve the mount.
pub struct MountProbe {
    /// The filesystem the path lives on.
    pub info: FilesystemInfo,
    /// The mount's source, `None` when the probe couldn't resolve the mount.
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
    let mount = super::statfs::get_mount_info(&path.to_string_lossy());
    let (raw, source) = match mount {
        Some(m) => (Some(m.fs_type), Some(m.source)),
        None => (None, None),
    };
    MountProbe {
        info: FilesystemInfo::from_raw_type(raw),
        source,
    }
}

/// [`detect_filesystem_for_path`] plus the mount's source, from the same single probe.
#[cfg(target_os = "linux")]
pub fn probe_mount_for_path(path: &std::path::Path) -> MountProbe {
    let entry = super::linux_mounts::mount_entry_for_path(path);
    let (raw, source) = match entry {
        Some(e) => (Some(e.fstype), Some(e.device)),
        None => (None, None),
    };
    MountProbe {
        info: FilesystemInfo::from_raw_type(raw),
        source,
    }
}

/// No mount-table reader on Windows yet: the type is unknown.
#[cfg(target_os = "windows")]
pub fn probe_mount_for_path(_path: &std::path::Path) -> MountProbe {
    MountProbe {
        info: FilesystemInfo::from_raw_type(None),
        source: None,
    }
}

/// Whether `path` sits on anything but a known local disk: a network share (SMB,
/// NFS, AFP, WebDAV, ...), a FUSE or cloud mount (`pcloudfs`, rclone, sshfs), or a
/// mount the probe couldn't resolve.
///
/// One `statfs` (a `/proc/mounts` read on Linux), so ❌ never call it in a loop
/// over entries or on a hot read path: it can block for as long as the mount
/// takes to answer. The index's `mount_facts` is one caller (the walker
/// choice); `watcher::start_watching` is another, deciding a watch's
/// [`WatchCoverage`](cmdr_fs::volume::WatchCoverage) once at arm time rather than
/// per query. Both want the same answer: a remote tree can't be walked at local
/// speed, and its other writers never reach FSEvents.
pub fn path_is_on_network_mount(path: &std::path::Path) -> bool {
    is_network(&probe_mount_for_path(path))
}

/// "Network" here means "not a known local disk" (`cmdr_fs::filesystem_kind::mount_is_local_disk`), so
/// FUSE and cloud mounts count too. Split out so one probe can answer both of
/// the index's `mount_facts` questions.
pub fn is_network(probe: &MountProbe) -> bool {
    !mount_is_local_disk(probe.source.as_deref(), probe.info.raw_type.as_deref())
}
