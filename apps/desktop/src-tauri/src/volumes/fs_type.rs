//! `statfs`-based filesystem-type primitives: trash support, network/SMB
//! classification, mount-point resolution, and read-only detection. All
//! non-blocking (pure `statfs` / `getfsstat` reads, no NSURL enrichment).

use super::*;

/// Determine whether a filesystem type supports trash.
///
/// Local filesystems (APFS, HFS+, ext4, btrfs, xfs, zfs) support trash.
/// Network filesystems (SMB, NFS, AFP, WebDAV, CIFS, FUSE-based SSH) and
/// non-Mac formats (FAT32/exFAT) don't reliably support it. Unknown types
/// default to `true` (optimistic: trash failure is caught at operation time).
pub fn supports_trash_for_fs_type(fs_type: Option<&str>) -> bool {
    let Some(fs) = fs_type else { return true };
    let fs_lower = fs.to_ascii_lowercase();
    match fs_lower.as_str() {
        "apfs" | "hfs" | "ext4" | "btrfs" | "xfs" | "zfs" => true,
        "smbfs" | "nfs" | "afpfs" | "webdav" | "cifs" | "fuse.sshfs" | "msdos" | "exfat" => false,
        _ => true,
    }
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
pub(crate) fn get_mount_point(path: &str) -> Option<(String, String)> {
    use std::ffi::CString;

    let mut current = path.to_string();
    loop {
        if let Ok(c_path) = CString::new(current.as_str()) {
            let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
            // SAFETY: `c_path` is a valid NUL-terminated C string from `current`, and `stat` is an
            // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
            let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if result == 0 {
                // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
                let stat = unsafe { stat.assume_init() };

                let mount_point: String = stat
                    .f_mntonname
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                let fs_type: String = stat
                    .f_fstypename
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                // APFS firmlink normalization: /System/Volumes/Data → /
                let mount_point = if mount_point == "/System/Volumes/Data" {
                    "/".to_string()
                } else {
                    mount_point
                };

                return Some((mount_point, fs_type));
            }
        }

        // Walk up to parent on failure (handles deleted directories)
        if current == "/" || current.is_empty() {
            return None;
        }
        current = Path::new(&current)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if current.is_empty() {
            current = "/".to_string();
        }
    }
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
    // f_fstypename is [c_char; 16] on macOS. Convert to &str.
    let name_bytes: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(name_bytes).ok()
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
        assert!(supports_trash_for_fs_type(Some("apfs")));
        assert!(supports_trash_for_fs_type(Some("hfs")));
        assert!(supports_trash_for_fs_type(Some("ext4")));
        assert!(supports_trash_for_fs_type(Some("btrfs")));
        assert!(supports_trash_for_fs_type(Some("xfs")));
        assert!(supports_trash_for_fs_type(Some("zfs")));
    }

    #[test]
    fn test_supports_trash_network_filesystems() {
        assert!(!supports_trash_for_fs_type(Some("smbfs")));
        assert!(!supports_trash_for_fs_type(Some("nfs")));
        assert!(!supports_trash_for_fs_type(Some("afpfs")));
        assert!(!supports_trash_for_fs_type(Some("webdav")));
        assert!(!supports_trash_for_fs_type(Some("cifs")));
        assert!(!supports_trash_for_fs_type(Some("fuse.sshfs")));
    }

    #[test]
    fn test_supports_trash_removable_formats() {
        assert!(!supports_trash_for_fs_type(Some("msdos")));
        assert!(!supports_trash_for_fs_type(Some("exfat")));
    }

    #[test]
    fn test_supports_trash_case_insensitive() {
        assert!(supports_trash_for_fs_type(Some("APFS")));
        assert!(supports_trash_for_fs_type(Some("HFS")));
        assert!(supports_trash_for_fs_type(Some("EXT4")));
        assert!(supports_trash_for_fs_type(Some("BTRFS")));
        assert!(!supports_trash_for_fs_type(Some("SMBFS")));
        assert!(!supports_trash_for_fs_type(Some("NFS")));
        assert!(!supports_trash_for_fs_type(Some("CIFS")));
        assert!(!supports_trash_for_fs_type(Some("ExFAT")));
        assert!(!supports_trash_for_fs_type(Some("MSDOS")));
    }

    #[test]
    fn test_supports_trash_unknown_defaults_true() {
        assert!(supports_trash_for_fs_type(Some("ntfs")));
    }

    #[test]
    fn test_supports_trash_none_defaults_true() {
        assert!(supports_trash_for_fs_type(None));
    }

    // ========================================================================
    // Mount point resolution tests
    // ========================================================================

    #[test]
    fn test_get_mount_point_root() {
        let result = get_mount_point("/");
        assert!(result.is_some(), "Root should resolve to a mount point");
        let (mount_point, fs_type) = result.unwrap();
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
        let (mount_point, _fs_type) = result.unwrap();
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
        let (mount_point, _fs_type) = result.unwrap();
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
