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
    #[allow(dead_code, reason = "Structural field from /proc/mounts, used in tests")]
    pub device: String,
    /// The mount point path
    pub mountpoint: String,
    /// The filesystem type (for example, `ext4`, `nfs`, `cifs`)
    pub fstype: String,
    /// Mount options (for example, `rw,relatime`)
    #[allow(dead_code, reason = "Structural field from /proc/mounts")]
    pub options: String,
}

/// Parses `/proc/mounts` and returns all mount entries.
pub fn parse_proc_mounts() -> Vec<MountEntry> {
    let contents = match std::fs::read_to_string("/proc/mounts") {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to read /proc/mounts: {}", e);
            return Vec::new();
        }
    };
    parse_proc_mounts_from_content(&contents)
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

/// Looks up the filesystem type for the given path by finding the mount
/// with the longest matching mountpoint prefix.
pub fn fs_type_for_path(path: &Path) -> Option<String> {
    let mounts = parse_proc_mounts();
    fs_type_for_path_from_entries(path, &mounts)
}

/// Looks up filesystem type from a pre-parsed mount list (avoids repeated I/O).
pub fn fs_type_for_path_from_entries(path: &Path, mounts: &[MountEntry]) -> Option<String> {
    let path_str = path.to_string_lossy();
    mounts
        .iter()
        .filter(|entry| {
            path_str == entry.mountpoint
                || path_str.starts_with(&format!("{}/", entry.mountpoint))
                || entry.mountpoint == "/"
        })
        .max_by_key(|entry| entry.mountpoint.len())
        .map(|entry| entry.fstype.clone())
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
pub fn is_network_fs_type(fstype: &str) -> bool {
    matches!(
        fstype,
        "nfs" | "nfs4" | "cifs" | "smbfs" | "fuse.sshfs" | "ncpfs" | "9p"
    )
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
        assert!(is_network_fs_type("nfs"));
        assert!(is_network_fs_type("nfs4"));
        assert!(is_network_fs_type("cifs"));
        assert!(is_network_fs_type("smbfs"));
        assert!(is_network_fs_type("fuse.sshfs"));
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
    fn test_fs_type_for_path_exact_mountpoint() {
        let entries = parse_proc_mounts_from_content(SAMPLE_MOUNTS);
        let fstype = fs_type_for_path_from_entries(Path::new("/tmp"), &entries);
        assert_eq!(fstype.as_deref(), Some("tmpfs"));
    }
}
