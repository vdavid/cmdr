//! `statfs`-based filesystem-type primitives: trash support, network/SMB
//! classification, mount-point resolution, and read-only detection. All
//! non-blocking (pure `statfs` / `getfsstat` reads, no NSURL enrichment).

use cmdr_local::statfs::{get_mount_info, statfs_string};

/// Whether the OS keeps a Trash on a mount with this source (`f_mntfromname`) and filesystem
/// type: the one trash rule, `cmdr_fs::filesystem_kind::mount_has_trash`, fed the SAME two facts
/// `Volume::has_trash` reads at registration, so the volume list and a suggestion's "can't run"
/// can't disagree. A local disk, FAT32 and exFAT included, keeps one, and so does any block
/// device (`/dev/…`) whatever its type; a network or FUSE mount, or an unrecognized type that
/// isn't on a device, doesn't. Pass the source whenever the caller has the mount row: without
/// it, an unrecognized type on a USB disk reads "no Trash" here and "Trash" in the registry.
pub fn supports_trash_for_mount(source: Option<&str>, fs_type: Option<&str>) -> bool {
    cmdr_fs::filesystem_kind::mount_has_trash(source, fs_type)
}

pub fn is_smb_fs_type(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("smbfs" | "cifs"))
}

/// Returns true for network filesystem types whose metadata syscalls can block
/// indefinitely on a hung mount (SMB, NFS, AFP, WebDAV, FTP).
///
/// Volume discovery derives these volumes' fields purely from the non-blocking
/// `getfsstat` snapshot and skips the blocking NSURL / NSWorkspace / DiskArbitration
/// enrichment, so one dead network mount can't stall discovery of the others. See
/// `get_attached_volumes` and `DETAILS.md` § "Hung mounts".
pub fn is_network_fs_type(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("smbfs" | "cifs" | "nfs" | "afpfs" | "webdav" | "ftp"))
}

/// Resolve a path to its mount point and filesystem type via `statfs()`.
///
/// On APFS firmlinks, normalizes `/System/Volumes/Data` to `/` (because
/// `statfs("/Users/foo")` returns `/System/Volumes/Data` on modern macOS).
///
/// If `statfs` fails (ENOENT for a deleted directory), walks up parent
/// directories until one succeeds. Returns `None` only if even `/` fails.
///
/// Answers `(mount_point, fs_type, source)`, the source being `f_mntfromname`.
pub(crate) fn get_mount_point(path: &str) -> Option<(String, String, String)> {
    get_mount_info(path).map(|info| (info.mount_point, info.fs_type, info.source))
}

/// Read the filesystem type for a path using `libc::statfs`.
///
/// Returns `None` if the `statfs` call fails (for example, the volume was
/// ejected between listing and probing).
pub(crate) fn get_fs_type(path: &str) -> Option<String> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    Some(statfs_string(&stat.f_fstypename))
}

/// Whether the volume mounted at `path` is read-only, from the `statfs` `MNT_RDONLY` flag.
///
/// Covers any read-only mount (a read-only `.dmg`, a locked SD card, an optical disc),
/// powering the copy/move write guard and the 🔒 indicator. Returns `false` if `statfs`
/// fails (treat an unprobeable mount as writable: the OS write attempt is the backstop).
pub(crate) fn read_only_from_statfs(path: &str) -> bool {
    use std::ffi::CString;

    let Ok(c_path) = CString::new(path) else {
        return false;
    };
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return false;
    }
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    (stat.f_flags & libc::MNT_RDONLY as u32) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Filesystem type and trash support tests
    // ========================================================================

    #[test]
    fn test_supports_trash_local_filesystems() {
        assert!(supports_trash_for_mount(None, Some("apfs")));
        assert!(supports_trash_for_mount(None, Some("hfs")));
        assert!(supports_trash_for_mount(None, Some("ext4")));
        assert!(supports_trash_for_mount(None, Some("btrfs")));
        assert!(supports_trash_for_mount(None, Some("xfs")));
        assert!(supports_trash_for_mount(None, Some("zfs")));
    }

    #[test]
    fn test_supports_trash_network_filesystems() {
        assert!(!supports_trash_for_mount(None, Some("smbfs")));
        assert!(!supports_trash_for_mount(None, Some("nfs")));
        assert!(!supports_trash_for_mount(None, Some("afpfs")));
        assert!(!supports_trash_for_mount(None, Some("webdav")));
        assert!(!supports_trash_for_mount(None, Some("cifs")));
        assert!(!supports_trash_for_mount(None, Some("fuse.sshfs")));
    }

    /// FAT32 and exFAT keep a `.Trashes` like any local disk: `move_to_trash_sync` lands an item
    /// in `/Volumes/<image>/.Trashes/<uid>/` (cmdr-fs `DETAILS.md` § "Which mounts keep a Trash").
    #[test]
    fn test_supports_trash_removable_formats() {
        assert!(supports_trash_for_mount(None, Some("msdos")));
        assert!(supports_trash_for_mount(None, Some("exfat")));
    }

    #[test]
    fn test_supports_trash_case_insensitive() {
        assert!(supports_trash_for_mount(None, Some("APFS")));
        assert!(supports_trash_for_mount(None, Some("HFS")));
        assert!(supports_trash_for_mount(None, Some("EXT4")));
        assert!(supports_trash_for_mount(None, Some("BTRFS")));
        assert!(!supports_trash_for_mount(None, Some("SMBFS")));
        assert!(!supports_trash_for_mount(None, Some("NFS")));
        assert!(!supports_trash_for_mount(None, Some("CIFS")));
        assert!(supports_trash_for_mount(None, Some("ExFAT")));
        assert!(supports_trash_for_mount(None, Some("MSDOS")));
    }

    #[test]
    fn test_supports_trash_on_a_known_local_disk_type() {
        assert!(supports_trash_for_mount(None, Some("ntfs")));
    }

    /// An unknown or unread type promises nothing: `Volume::has_trash` reads it as a network
    /// mount too, and the two must agree.
    #[test]
    fn test_supports_trash_unknown_or_none_promises_nothing() {
        assert!(!supports_trash_for_mount(None, None));
        assert!(!supports_trash_for_mount(None, Some("zalgofs")));
    }

    /// An unrecognized type on a USB disk: the registry's `MountClass` reads the device node and
    /// says Trash, so the volume list has to read it too, or the delete dialog and a trash
    /// suggestion disagree about the same drive.
    #[test]
    fn test_supports_trash_on_a_block_device_whatever_its_type() {
        assert!(supports_trash_for_mount(Some("/dev/disk4s1"), Some("zalgofs")));
        assert!(!supports_trash_for_mount(Some("//me@nas/share"), Some("zalgofs")));
    }

    // ========================================================================
    // Mount point resolution tests
    // ========================================================================

    #[test]
    fn test_get_mount_point_root() {
        let result = get_mount_point("/");
        assert!(result.is_some(), "Root should resolve to a mount point");
        let (mount_point, fs_type, _source) = result.unwrap();
        assert_eq!(mount_point, "/", "Root mount point should be /");
        assert!(
            fs_type == "apfs" || fs_type == "hfs",
            "Root should be apfs or hfs, got: {fs_type}"
        );
    }

    #[test]
    fn test_get_mount_point_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let result = get_mount_point(home.to_str().unwrap());
        assert!(result.is_some(), "Home should resolve to a mount point");
        let (mount_point, _fs_type, _source) = result.unwrap();
        // APFS firmlink normalization: must NOT return /System/Volumes/Data
        assert_eq!(
            mount_point, "/",
            "Home mount point should be / (not /System/Volumes/Data)"
        );
    }

    #[test]
    fn test_get_mount_point_nonexistent() {
        let result = get_mount_point("/nonexistent/deeply/nested/path");
        assert!(result.is_some(), "Nonexistent path should walk up to root");
        let (mount_point, _fs_type, _source) = result.unwrap();
        assert_eq!(mount_point, "/", "Nonexistent path should resolve to /");
    }

    #[test]
    fn test_get_fs_type_root() {
        let fs_type = get_fs_type("/");
        assert!(fs_type.is_some(), "Root volume should have a filesystem type");
        let fs = fs_type.unwrap();
        assert!(!fs.is_empty(), "Filesystem type should not be empty");
        // On modern macOS, root is APFS
        assert!(fs == "apfs" || fs == "hfs", "Root should be apfs or hfs, got: {fs}");
    }

    #[test]
    fn test_get_fs_type_nonexistent_path() {
        let fs_type = get_fs_type("/nonexistent/path/that/does/not/exist");
        // statfs on a nonexistent path fails
        assert!(fs_type.is_none(), "Nonexistent path should return None");
    }

    #[test]
    fn test_get_fs_type_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let fs_type = get_fs_type(home.to_str().unwrap());
        assert!(fs_type.is_some(), "Home dir should have a filesystem type");
    }

    /// `statfs`'s name fields hold UTF-8 paths, so they have to be read as UTF-8.
    /// Verified against a live mount: `/Volumes/公開` (`smb-consumer-unicode`)
    /// came back as `/Volumes/å¬é` under the latin-1 read, and the share then
    /// resolved to an unknown volume with a `path-volumes-å-é-…` id
    /// (macOS 15.5, 2026-08-22).
    #[test]
    fn statfs_fields_are_read_as_utf8() {
        fn field(text: &str) -> Vec<libc::c_char> {
            let mut out: Vec<libc::c_char> = text.bytes().map(|b| b as libc::c_char).collect();
            out.push(0);
            out.push(b'x' as libc::c_char); // trailing garbage past the NUL, as the kernel leaves
            out
        }
        assert_eq!(statfs_string(&field("/Volumes/公開")), "/Volumes/公開");
        assert_eq!(statfs_string(&field("/Volumes/café")), "/Volumes/café");
        assert_eq!(statfs_string(&field("smbfs")), "smbfs");
        assert_eq!(statfs_string(&field("")), "");
    }

    #[test]
    fn is_network_fs_type_covers_the_hanging_filesystems() {
        for fs in ["smbfs", "cifs", "nfs", "afpfs", "webdav", "ftp"] {
            assert!(is_network_fs_type(Some(fs)), "{fs} should count as network");
        }
        for fs in ["apfs", "hfs", "exfat", "msdos"] {
            assert!(!is_network_fs_type(Some(fs)), "{fs} should count as local");
        }
        assert!(!is_network_fs_type(None));
    }
}
