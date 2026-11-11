//! Mount-table filesystem primitives: trash support, virtual-filesystem
//! classification, and mount-point resolution. All read `/proc/mounts`, never the
//! mounted filesystem itself. Free space is `cmdr_local::space::get_volume_space`.

use cmdr_local::linux_mounts;
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

/// Whether the OS keeps a Trash on a mount with this source (the `/proc/mounts` device) and
/// filesystem type: the one trash rule, `cmdr_fs::filesystem_kind::mount_has_trash`, fed the same
/// two facts `Volume::has_trash` reads at registration (the macOS twin says why both matter).
/// ❗ Verified on macOS only; on Linux the FreeDesktop trash on FAT, exFAT, and NTFS follows the
/// same rule unverified, and a refusal there trashes nothing (`TrashRefused`), never deletes.
pub fn supports_trash_for_mount(source: Option<&str>, fs_type: Option<&str>) -> bool {
    cmdr_fs::filesystem_kind::mount_has_trash(source, fs_type)
}

/// Check if a filesystem type is virtual (not a real disk).
pub(super) fn is_virtual_fs(fstype: &str) -> bool {
    VIRTUAL_FS_TYPES.contains(&fstype)
}

/// Resolve a path to its mount point and filesystem type by finding the
/// longest mount-point prefix match in `/proc/mounts`. Succeeds whenever the
/// table is readable, because `/` is always mounted, so even nonexistent paths
/// match root.
///
/// Answers `(mount_point, fs_type, source)`, the source being the mount's device.
pub(crate) fn get_mount_point(path: &str) -> Option<(String, String, String)> {
    let mounts = linux_mounts::parse_proc_mounts()?;
    let entry = linux_mounts::mount_entry_for_path_from_entries(Path::new(path), &mounts)?;
    let (fs_type, source) = (entry.fstype.clone(), entry.device.clone());
    let mount_point = mounts
        .iter()
        .filter(|entry| {
            path == entry.mountpoint || path.starts_with(&format!("{}/", entry.mountpoint)) || entry.mountpoint == "/"
        })
        .max_by_key(|entry| entry.mountpoint.len())
        .map(|entry| entry.mountpoint.clone())
        .unwrap_or_else(|| "/".to_string());
    Some((mount_point, fs_type, source))
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
        assert!(supports_trash_for_mount(None, Some("ext4")));
        assert!(supports_trash_for_mount(None, Some("ext3")));
        assert!(supports_trash_for_mount(None, Some("btrfs")));
        assert!(supports_trash_for_mount(None, Some("xfs")));
        assert!(supports_trash_for_mount(None, Some("zfs")));
        assert!(supports_trash_for_mount(None, Some("f2fs")));
    }

    #[test]
    fn test_supports_trash_linux_network() {
        assert!(!supports_trash_for_mount(None, Some("nfs")));
        assert!(!supports_trash_for_mount(None, Some("nfs4")));
        assert!(!supports_trash_for_mount(None, Some("cifs")));
        assert!(!supports_trash_for_mount(None, Some("fuse.sshfs")));
    }

    /// The same rule as macOS (`cmdr_fs::filesystem_kind::mount_has_trash`): a local disk type
    /// keeps a Trash, a FUSE block mount doesn't.
    #[test]
    fn test_supports_trash_removable_formats() {
        assert!(supports_trash_for_mount(None, Some("vfat")));
        assert!(supports_trash_for_mount(None, Some("exfat")));
        assert!(supports_trash_for_mount(None, Some("ntfs")));
        assert!(!supports_trash_for_mount(None, Some("fuseblk")));
    }

    #[test]
    fn test_supports_trash_unknown_and_none() {
        assert!(!supports_trash_for_mount(None, None));
        assert!(!supports_trash_for_mount(None, Some("somefs")));
    }

    /// A block device keeps a Trash whatever its type, as `Volume::has_trash` reads it.
    #[test]
    fn test_supports_trash_on_a_block_device_whatever_its_type() {
        assert!(supports_trash_for_mount(Some("/dev/sdb1"), Some("somefs")));
    }
}
