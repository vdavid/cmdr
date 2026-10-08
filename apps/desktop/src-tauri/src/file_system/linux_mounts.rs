//! Linux `/proc/mounts` parsing for filesystem type detection.
//!
//! Parses `/proc/mounts` to determine the filesystem type for a given path.
//! Used by the copy engine to select native vs chunked copy, and by volume
//! discovery (milestone 2) for mount enumeration.

use std::path::Path;

/// A parsed mount entry from `/proc/mounts`.
#[derive(Debug, Clone)]
pub struct MountEntry {
    /// The device (for example, `/dev/sda1` or `server:/share`)
    pub device: String,
    /// The mount point path
    pub mountpoint: String,
    /// The filesystem type (for example, `ext4`, `nfs`, `cifs`)
    pub fstype: String,
    /// Mount options (for example, `rw,relatime`, or a FUSE mount's `user_id=1000,allow_other`)
    pub options: String,
}

/// Parses `/proc/mounts` and returns all mount entries, or `None` when it can't be read.
///
/// `None` is unknown, ❌ never an empty table: a caller that reads an unreadable
/// table as empty sees every volume, `/` included, as unmounted.
pub fn parse_proc_mounts() -> Option<Vec<MountEntry>> {
    read_mount_table(Path::new("/proc/mounts"))
}

/// Reads and parses the mount table at `path`, or `None` when it can't be read.
pub fn read_mount_table(path: &Path) -> Option<Vec<MountEntry>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Some(parse_proc_mounts_from_content(&contents)),
        Err(e) => {
            log::warn!("Failed to read {}: {}", path.display(), e);
            None
        }
    }
}

/// Parses mount file content from a string (testable without /proc/mounts).
pub fn parse_proc_mounts_from_content(contents: &str) -> Vec<MountEntry> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            // Format: device mountpoint fstype options dump pass
            let mut parts = line.splitn(6, ' ');
            let device = parts.next()?.to_string();
            let mountpoint = unescape_octal(parts.next()?);
            let fstype = parts.next()?.to_string();
            let options = parts.next()?.to_string();
            // dump and pass are ignored
            Some(MountEntry {
                device,
                mountpoint,
                fstype,
                options,
            })
        })
        .collect()
}

/// Whether `path` is a mount point in `/proc/mounts`. Reading the table never
/// touches the mount itself, so a hung network mount can't stall it. `None` when
/// the table couldn't be read. Eject asks this to tell a refusal from a volume
/// that's already gone.
pub fn is_mount_point(path: &str) -> Option<bool> {
    mount_table_lists(&parse_proc_mounts()?, path)
}

/// [`is_mount_point`] over a pre-parsed table. An empty table is unknown, never
/// "not mounted": a readable one always holds `/`.
fn mount_table_lists(mounts: &[MountEntry], path: &str) -> Option<bool> {
    if mounts.is_empty() {
        return None;
    }
    let path = Path::new(path);
    Some(mounts.iter().any(|entry| Path::new(&entry.mountpoint) == path))
}

/// Looks up the filesystem type for the given path by finding the mount
/// with the longest matching mountpoint prefix.
pub fn fs_type_for_path(path: &Path) -> Option<String> {
    fs_type_for_path_from_entries(path, &parse_proc_mounts()?)
}

/// Looks up filesystem type from a pre-parsed mount list (avoids repeated I/O).
pub fn fs_type_for_path_from_entries(path: &Path, mounts: &[MountEntry]) -> Option<String> {
    mount_entry_for_path_from_entries(path, mounts).map(|entry| entry.fstype.clone())
}

/// The mount `path` lives on (the longest matching mountpoint prefix), read from
/// `/proc/mounts`.
pub fn mount_entry_for_path(path: &Path) -> Option<MountEntry> {
    mount_entry_for_path_from_entries(path, &parse_proc_mounts()?).cloned()
}

/// The mount `path` lives on, from a pre-parsed mount list.
pub fn mount_entry_for_path_from_entries<'a>(path: &Path, mounts: &'a [MountEntry]) -> Option<&'a MountEntry> {
    let path_str = path.to_string_lossy();
    mounts
        .iter()
        .filter(|entry| {
            path_str == entry.mountpoint
                || path_str.starts_with(&format!("{}/", entry.mountpoint))
                || entry.mountpoint == "/"
        })
        .max_by_key(|entry| entry.mountpoint.len())
}

/// Returns true if the path is on a network filesystem (nfs, cifs, smbfs,
/// fuse.sshfs, or similar).
pub fn is_network_filesystem_linux(path: &Path) -> bool {
    let fstype = match fs_type_for_path(path) {
        Some(t) => t,
        None => return false,
    };
    is_network_fs_type(&fstype)
}

/// Returns true if the filesystem type string represents a network filesystem.
///
/// Known network types are matched explicitly. Unknown `fuse.*` subtypes are
/// treated conservatively as network (we can't distinguish `fuse.mycloud`
/// from a local FUSE mount, and chunked copy is the safer default).
pub fn is_network_fs_type(fstype: &str) -> bool {
    match fstype {
        // Kernel-native network filesystems
        "nfs" | "nfs4" | "cifs" | "smbfs" | "ncpfs" | "9p" | "afs" => true,
        // FUSE-based network filesystems
        "fuse.sshfs" | "fuse.rclone" | "fuse.s3fs" | "fuse.gvfsd-fuse" => true,
        // FUSE-based local filesystems (known safe)
        "fuse.ntfs-3g" | "fuse.exfat" | "fuseblk" => false,
        // Unknown FUSE subtypes; could be network, use chunked copy to be safe
        s if s.starts_with("fuse.") => true,
        _ => false,
    }
}

/// One mount in `/proc/self/mountinfo`: where it's mounted, and the device number
/// (`major:minor`) that names its filesystem. `/proc/mounts` carries no device number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountDevice {
    /// The mount point path.
    pub mountpoint: String,
    /// The filesystem's device number, `makedev(major, minor)`.
    pub device: u64,
}

/// Parses `/proc/self/mountinfo`, or `None` when it can't be read. Reading it never
/// touches a mount, so a hung network mount can't stall it.
pub fn parse_mountinfo() -> Option<Vec<MountDevice>> {
    match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(contents) => Some(parse_mountinfo_from_content(&contents)),
        Err(e) => {
            log::warn!("Failed to read /proc/self/mountinfo: {e}");
            None
        }
    }
}

/// Parses mountinfo content: `id parent major:minor root mountpoint options ...`.
pub fn parse_mountinfo_from_content(contents: &str) -> Vec<MountDevice> {
    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let _mount_id = fields.next()?;
            let _parent_id = fields.next()?;
            let (major, minor) = fields.next()?.split_once(':')?;
            let _root = fields.next()?;
            let mountpoint = unescape_octal(fields.next()?);
            Some(MountDevice {
                mountpoint,
                device: libc::makedev(major.parse().ok()?, minor.parse().ok()?),
            })
        })
        .collect()
}

/// The device number of the filesystem mounted exactly at `path`, or `None` when
/// nothing is mounted there or the table couldn't be read. With mounts stacked on one
/// path, the last one listed is the one a lookup reaches.
pub fn mount_device_at(path: &str) -> Option<u64> {
    device_mounted_at(&parse_mountinfo()?, path)
}

/// Whether any mount in the table has `device`, or `None` when the table couldn't be
/// read. An empty table is unknown, never "not mounted": a readable one holds `/`.
pub fn device_is_mounted(device: u64) -> Option<bool> {
    table_lists_device(&parse_mountinfo()?, device)
}

fn device_mounted_at(mounts: &[MountDevice], path: &str) -> Option<u64> {
    let path = Path::new(path);
    mounts
        .iter()
        .rev()
        .find(|mount| Path::new(&mount.mountpoint) == path)
        .map(|mount| mount.device)
}

fn table_lists_device(mounts: &[MountDevice], device: u64) -> Option<bool> {
    if mounts.is_empty() {
        return None;
    }
    Some(mounts.iter().any(|mount| mount.device == device))
}

/// Unescapes octal sequences in mount paths (for example, `\040` -> space).
/// `/proc/mounts` encodes special characters as `\NNN` octal sequences.
fn unescape_octal(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let octal = &s[i + 1..i + 4];
            if let Ok(byte) = u8::from_str_radix(octal, 8) {
                result.push(byte as char);
                i += 4;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MOUNTS: &str = "\
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/sda1 / ext4 rw,relatime 0 0
/dev/sda2 /home ext4 rw,relatime 0 0
/dev/sdb1 /mnt/data xfs rw,relatime 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev 0 0
server:/share /mnt/nfs nfs4 rw,relatime,vers=4.2 0 0
//server/share /mnt/smb cifs rw,relatime 0 0
user@host:/path /mnt/sshfs fuse.sshfs rw,relatime 0 0
";

    #[test]
    fn test_parse_mounts_content() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        assert_eq!(entries.len(), 9);
        assert_eq!(entries[2].device, "/dev/sda1");
        assert_eq!(entries[2].mountpoint, "/");
        assert_eq!(entries[2].fstype, "ext4");
    }

    const SAMPLE_MOUNTINFO: &str = "\
22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw
40 22 8:17 / /media/usb\\040stick rw,nosuid shared:20 - vfat /dev/sdb1 rw
41 22 0:45 / /mnt/nfs rw,relatime - nfs4 server:/share rw
";

    /// The index keys a drive's presence on its filesystem, so the table has to name
    /// each mount's device, octal-escaped mount points included, and say a device
    /// nothing has is mounted nowhere.
    #[test]
    fn mountinfo_names_each_mount_by_its_device() {
        let mounts = parse_mountinfo_from_content(SAMPLE_MOUNTINFO);
        assert_eq!(mounts.len(), 3);
        let stick = libc::makedev(8, 17);

        assert_eq!(device_mounted_at(&mounts, "/media/usb stick"), Some(stick));
        assert_eq!(device_mounted_at(&mounts, "/media/usb"), None, "not a mount point");
        assert_eq!(table_lists_device(&mounts, stick), Some(true));
        assert_eq!(table_lists_device(&mounts, libc::makedev(8, 33)), Some(false));
        assert_eq!(table_lists_device(&[], stick), None, "an empty table is unknown");
    }

    #[test]
    fn test_fs_type_for_path_root() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/var/log/syslog"), &entries);
        assert_eq!(fstype.as_deref(), Some("ext4"));
    }

    #[test]
    fn test_fs_type_for_path_home() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/home/user/docs"), &entries);
        // /home is ext4, longer prefix than /
        assert_eq!(fstype.as_deref(), Some("ext4"));
    }

    #[test]
    fn test_fs_type_for_path_nfs() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/mnt/nfs/somefile"), &entries);
        assert_eq!(fstype.as_deref(), Some("nfs4"));
    }

    #[test]
    fn test_fs_type_for_path_cifs() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/mnt/smb/shared/doc.txt"), &entries);
        assert_eq!(fstype.as_deref(), Some("cifs"));
    }

    #[test]
    fn test_is_network_fs_type() {
        // Kernel-native network filesystems
        assert!(is_network_fs_type("nfs"));
        assert!(is_network_fs_type("nfs4"));
        assert!(is_network_fs_type("cifs"));
        assert!(is_network_fs_type("smbfs"));
        assert!(is_network_fs_type("ncpfs"));
        assert!(is_network_fs_type("9p"));
        assert!(is_network_fs_type("afs"));

        // FUSE-based network filesystems
        assert!(is_network_fs_type("fuse.sshfs"));
        assert!(is_network_fs_type("fuse.rclone"));
        assert!(is_network_fs_type("fuse.s3fs"));
        assert!(is_network_fs_type("fuse.gvfsd-fuse"));

        // Unknown FUSE subtypes; conservatively treated as network
        assert!(is_network_fs_type("fuse.mycloud"));

        // Known-local FUSE types
        assert!(!is_network_fs_type("fuse.ntfs-3g"));
        assert!(!is_network_fs_type("fuse.exfat"));
        assert!(!is_network_fs_type("fuseblk"));

        // Local filesystems
        assert!(!is_network_fs_type("ext4"));
        assert!(!is_network_fs_type("xfs"));
        assert!(!is_network_fs_type("btrfs"));
        assert!(!is_network_fs_type("tmpfs"));
    }

    #[test]
    fn test_unescape_octal() {
        // \040 is space
        assert_eq!(unescape_octal("/mnt/my\\040drive"), "/mnt/my drive");
        // No escapes
        assert_eq!(unescape_octal("/mnt/data"), "/mnt/data");
        // Multiple escapes
        assert_eq!(unescape_octal("/mnt/a\\040b\\040c"), "/mnt/a b c");
    }

    #[test]
    fn test_parse_empty_and_comments() {
        let content = "\n# comment\n\n";
        let entries = parse_proc_mounts_from_content(content);
        assert!(entries.is_empty());
    }

    #[test]
    fn mount_table_lists_only_whole_mountpoints() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        assert_eq!(mount_table_lists(&entries, "/mnt/data"), Some(true));
        assert_eq!(mount_table_lists(&entries, "/mnt/data/"), Some(true));
        assert_eq!(
            mount_table_lists(&entries, "/mnt"),
            Some(false),
            "a parent of a mount isn't one"
        );
        assert_eq!(mount_table_lists(&entries, "/mnt/data/sub"), Some(false));

        let escaped = parse_proc_mounts_from_content("/dev/sdc1 /media/me/USB\\040Stick vfat rw 0 0\n");
        assert_eq!(mount_table_lists(&escaped, "/media/me/USB Stick"), Some(true));
    }

    #[test]
    fn an_unreadable_mount_table_is_unknown_never_unmounted() {
        // A real table always holds `/`, so an empty one can't be real either.
        // Reading it as "gone" would turn a real eject refusal into a silent success.
        assert_eq!(mount_table_lists(&[], "/mnt/data"), None);
    }

    #[test]
    fn an_unreadable_table_reads_as_none_and_a_readable_one_holds_root() {
        assert!(read_mount_table(Path::new("/proc/self/no-such-mount-table")).is_none());
        let table =
            read_mount_table(Path::new("/proc/self/mounts")).expect("the test process can read its own mount table");
        assert!(table.iter().any(|entry| entry.mountpoint == "/"));
    }

    #[test]
    fn test_fs_type_for_path_exact_mountpoint() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/tmp"), &entries);
        assert_eq!(fstype.as_deref(), Some("tmpfs"));
    }
}
