//! `statvfs`-based filesystem primitives: trash support, virtual-filesystem
//! classification, mount-point resolution, and free-space queries. All read
//! `/proc/mounts` or call `statvfs`, never the mounted filesystem itself.

use super::{VolumeSpaceInfo, linux_mounts};
use std::path::Path;

/// Virtual filesystem types to filter out of mount listings.
const VIRTUAL_FS_TYPES: &[&str] = &[
    "proc",
    "sysfs",
    "devpts",
    "tmpfs",
    "cgroup",
    "cgroup2",
    "devtmpfs",
    "hugetlbfs",
    "mqueue",
    "debugfs",
    "tracefs",
    "securityfs",
    "pstore",
    "configfs",
    "fusectl",
    "binfmt_misc",
    "autofs",
    "efivarfs",
    "ramfs",
    "rpc_pipefs",
    "nfsd",
    "nsfs",
    "bpf",
];

/// Determine whether a filesystem type supports trash.
///
/// Local filesystems (ext4, btrfs, xfs, zfs) support trash via the
/// FreeDesktop.org trash spec. Network filesystems (NFS, CIFS, SSHFS)
/// and non-native formats (FAT32/exFAT) don't reliably support it.
/// Unknown types default to `true` (optimistic).
pub fn supports_trash_for_fs_type(fs_type: Option<&str>) -> bool {
    let Some(fs) = fs_type else { return true };
    let fs_lower = fs.to_ascii_lowercase();

    // Network filesystems don't support the FreeDesktop trash spec
    if linux_mounts::is_network_fs_type(&fs_lower) {
        return false;
    }

    match fs_lower.as_str() {
        "ext4" | "ext3" | "ext2" | "btrfs" | "xfs" | "zfs" | "f2fs" | "reiserfs" => true,
        "vfat" | "exfat" | "msdos" | "ntfs" | "fuseblk" => false,
        _ => true,
    }
}

/// Check if a filesystem type is virtual (not a real disk).
pub(super) fn is_virtual_fs(fstype: &str) -> bool {
    VIRTUAL_FS_TYPES.contains(&fstype)
}

/// Resolve a path to its mount point and filesystem type by finding the
/// longest mount-point prefix match in `/proc/mounts`. Always succeeds
/// because `/` is always mounted, so even nonexistent paths match root.
pub(crate) fn get_mount_point(path: &str) -> Option<(String, String)> {
    let mounts = linux_mounts::parse_proc_mounts();
    let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new(path), &mounts)?;
    let mount_point = mounts
        .iter()
        .filter(|entry| {
            path == entry.mountpoint || path.starts_with(&format!("{}/", entry.mountpoint)) || entry.mountpoint == "/"
        })
        .max_by_key(|entry| entry.mountpoint.len())
        .map(|entry| entry.mountpoint.clone())
        .unwrap_or_else(|| "/".to_string());
    Some((mount_point, fs_type))
}

/// Get space information for a volume using `statvfs`.
pub fn get_volume_space(path: &str) -> Option<VolumeSpaceInfo> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;

    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`; `stat` is a zeroed,
    // correctly-typed `libc::statvfs` out-buffer the kernel fills, and its fields are only read on
    // the `== 0` (success) branch where the kernel initialized them.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            let block_size = stat.f_frsize;
            Some(VolumeSpaceInfo {
                total_bytes: stat.f_blocks * block_size,
                available_bytes: stat.f_bavail * block_size,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_virtual_fs() {
        assert!(is_virtual_fs("proc"));
        assert!(is_virtual_fs("sysfs"));
        assert!(is_virtual_fs("tmpfs"));
        assert!(is_virtual_fs("cgroup2"));
        assert!(!is_virtual_fs("ext4"));
        assert!(!is_virtual_fs("btrfs"));
        assert!(!is_virtual_fs("xfs"));
        assert!(!is_virtual_fs("ntfs"));
    }

    #[test]
    fn test_supports_trash_linux_local() {
        assert!(supports_trash_for_fs_type(Some("ext4")));
        assert!(supports_trash_for_fs_type(Some("ext3")));
        assert!(supports_trash_for_fs_type(Some("btrfs")));
        assert!(supports_trash_for_fs_type(Some("xfs")));
        assert!(supports_trash_for_fs_type(Some("zfs")));
        assert!(supports_trash_for_fs_type(Some("f2fs")));
    }

    #[test]
    fn test_supports_trash_linux_network() {
        assert!(!supports_trash_for_fs_type(Some("nfs")));
        assert!(!supports_trash_for_fs_type(Some("nfs4")));
        assert!(!supports_trash_for_fs_type(Some("cifs")));
        assert!(!supports_trash_for_fs_type(Some("fuse.sshfs")));
    }

    #[test]
    fn test_supports_trash_removable_formats() {
        assert!(!supports_trash_for_fs_type(Some("vfat")));
        assert!(!supports_trash_for_fs_type(Some("exfat")));
        assert!(!supports_trash_for_fs_type(Some("ntfs")));
        assert!(!supports_trash_for_fs_type(Some("fuseblk")));
    }

    #[test]
    fn test_supports_trash_unknown_and_none() {
        assert!(supports_trash_for_fs_type(None));
        assert!(supports_trash_for_fs_type(Some("somefs")));
    }

    #[test]
    fn test_get_volume_space_root() {
        let space = get_volume_space("/");
        // statvfs works on both macOS and Linux
        if let Some(space) = space {
            assert!(space.total_bytes > 0);
            assert!(space.available_bytes <= space.total_bytes);
        }
    }

    #[test]
    fn test_get_volume_space_nonexistent() {
        let space = get_volume_space("/nonexistent/path/does/not/exist");
        assert!(space.is_none());
    }
}
