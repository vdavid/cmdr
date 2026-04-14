//! Volume and location discovery for Linux.
//!
//! Provides a sidebar location picker with:
//! - Favorites (Home, Desktop, Documents, Downloads)
//! - Main volume (root `/`)
//! - Mounted volumes (real filesystems from /proc/mounts)
//! - Cloud drives (Dropbox, Google Drive, Nextcloud, OneDrive)
//! - Network mounts (GVFS SMB shares under /run/user/<uid>/gvfs/)
//! - Removable media under /run/media/ or /media/

pub mod watcher;

#[allow(
    unused_imports,
    reason = "API parity with macOS volumes module — used once SMB enrichment lands on Linux"
)]
pub use crate::file_system::volume::SmbConnectionState;

use crate::file_system::linux_mounts::{self, MountEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Information about an SMB mount extracted from `/proc/mounts`.
#[derive(Debug, Clone)]
pub struct SmbMountInfo {
    /// Server hostname or IP (for example, "192.168.1.111").
    pub server: String,
    /// Share name (for example, "naspi").
    pub share: String,
    /// Username if present in the mount source (for example, "david").
    pub username: Option<String>,
    /// Port from the mount source (for example, 10480). Defaults to 445.
    pub port: u16,
}

/// Extracts SMB server, share, and username from a mount path via `/proc/mounts`.
///
/// On Linux, CIFS mounts have a device field like:
/// - `//192.168.1.111/share` (no credentials in device)
/// - `//user@192.168.1.111/share` (some configurations)
///
/// Returns `None` if the path is not a CIFS mount or parsing fails.
pub fn get_smb_mount_info(mount_path: &str) -> Option<SmbMountInfo> {
    let mounts = linux_mounts::parse_proc_mounts();
    let entry = mounts
        .iter()
        .filter(|e| e.fstype == "cifs")
        .find(|e| e.mountpoint == mount_path)?;
    parse_smb_mount_source(&entry.device)
}

/// Parses an SMB mount source string like `//user@host/share` or `//host/share`.
fn parse_smb_mount_source(source: &str) -> Option<SmbMountInfo> {
    let rest = source.strip_prefix("//")?;
    let (server_part, share) = rest.split_once('/')?;
    if share.is_empty() {
        return None;
    }

    let (username, server) = if let Some((user, host)) = server_part.split_once('@') {
        (Some(user.to_string()), host.to_string())
    } else {
        (None, server_part.to_string())
    };

    // Extract port if present (for example, "192.168.1.111:10480")
    let (server, port) = if let Some((host, port_str)) = server.rsplit_once(':') {
        (host.to_string(), port_str.parse().unwrap_or(445))
    } else {
        (server, 445)
    };

    Some(SmbMountInfo {
        server,
        share: share.to_string(),
        username,
        port,
    })
}

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
    MobileDevice,
}

/// Information about a location (volume, folder, or cloud drive).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub is_ejectable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,
    pub supports_trash: bool,
    /// Whether this location is read-only (for example, MTP devices with locked storage).
    pub is_read_only: bool,
    /// SMB connection state indicator. Always `None` on Linux (no smb2 session tracking yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_connection_state: Option<String>,
}

/// Information about volume space.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

// Legacy compat alias
pub use LocationInfo as VolumeInfo;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

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

/// Get all locations organized by category, deduplicated.
pub fn list_locations() -> Vec<LocationInfo> {
    let mounts = linux_mounts::parse_proc_mounts();
    let mut locations = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 1. Favorites
    for loc in get_favorites(&mounts) {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 2. Main volume
    if let Some(loc) = get_main_volume(&mounts)
        && seen_paths.insert(loc.path.clone())
    {
        locations.push(loc);
    }

    // 3. Mounted volumes (real filesystems, excluding root and virtual)
    for loc in get_mounted_volumes(&mounts) {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 4. Cloud drives
    for loc in get_cloud_drives(&mounts) {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 5. Network mounts (GVFS SMB shares)
    for loc in get_network_mounts() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    locations
}

/// Legacy compatibility wrapper.
pub fn list_mounted_volumes() -> Vec<LocationInfo> {
    list_locations()
}

/// Get common user directories as favorites.
fn get_favorites(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    let home = dirs::home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy().to_string();

    let candidates = [
        (home_str, "Home", "fav-home"),
        (
            home.join("Desktop").to_string_lossy().to_string(),
            "Desktop",
            "fav-desktop",
        ),
        (
            home.join("Documents").to_string_lossy().to_string(),
            "Documents",
            "fav-documents",
        ),
        (
            home.join("Downloads").to_string_lossy().to_string(),
            "Downloads",
            "fav-downloads",
        ),
    ];

    candidates
        .into_iter()
        .filter(|(path, _, _)| Path::new(path).exists())
        .map(|(path, name, id)| {
            let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new(&path), mounts);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
            LocationInfo {
                id: id.to_string(),
                name: name.to_string(),
                path,
                category: LocationCategory::Favorite,
                icon: None,
                is_ejectable: false,
                fs_type,
                supports_trash,
                is_read_only: false,
                smb_connection_state: None,
            }
        })
        .collect()
}

/// Get the root filesystem as the main volume.
fn get_main_volume(mounts: &[MountEntry]) -> Option<LocationInfo> {
    let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new("/"), mounts);
    let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
    Some(LocationInfo {
        id: DEFAULT_VOLUME_ID.to_string(),
        name: "Root".to_string(),
        path: "/".to_string(),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
        fs_type,
        supports_trash,
        is_read_only: false,
        smb_connection_state: None,
    })
}

/// Get mounted real filesystems, filtering out virtual ones and root.
pub fn get_mounted_volumes(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    let username = get_username();

    // Collect candidate mount points (real, non-hidden, non-root).
    let candidate_paths: Vec<&str> = mounts
        .iter()
        .filter(|e| !is_virtual_fs(&e.fstype) && e.mountpoint != "/" && !is_hidden_mount(&e.mountpoint))
        .map(|e| e.mountpoint.as_str())
        .collect();

    let mut volumes = Vec::new();

    for entry in mounts {
        if is_virtual_fs(&entry.fstype) {
            continue;
        }
        if entry.mountpoint == "/" {
            continue;
        }
        if is_hidden_mount(&entry.mountpoint) {
            continue;
        }
        // Skip sub-mounts (bind mounts nested under another real mount).
        if is_submount(&entry.mountpoint, &candidate_paths) {
            continue;
        }

        let is_removable = is_removable_mount(&entry.mountpoint, &username);
        let name = mount_display_name(&entry.mountpoint);
        let fs_type = Some(entry.fstype.clone());
        let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());

        volumes.push(LocationInfo {
            id: path_to_id(&entry.mountpoint),
            name,
            path: entry.mountpoint.clone(),
            category: LocationCategory::AttachedVolume,
            icon: None,
            is_ejectable: is_removable,
            fs_type,
            supports_trash,
            is_read_only: false,
            smb_connection_state: None,
        });
    }

    volumes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    volumes
}

/// Get cloud drives by checking common locations.
fn get_cloud_drives(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut drives = Vec::new();

    let candidates = [
        (home.join("Dropbox"), "Dropbox", "cloud-dropbox"),
        (home.join("Google Drive"), "Google Drive", "cloud-google-drive"),
        (home.join(".local/share/Nextcloud"), "Nextcloud", "cloud-nextcloud"),
        (home.join("OneDrive"), "OneDrive", "cloud-onedrive"),
    ];

    for (path, name, id) in candidates {
        if path.is_dir() {
            let path_str = path.to_string_lossy().to_string();
            let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new(&path_str), mounts);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
            drives.push(LocationInfo {
                id: id.to_string(),
                name: name.to_string(),
                path: path_str,
                category: LocationCategory::CloudDrive,
                icon: None,
                is_ejectable: false,
                fs_type,
                supports_trash,
                is_read_only: false,
                smb_connection_state: None,
            });
        }
    }

    drives.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    drives
}

/// Parse a GVFS SMB directory name into (server, share).
///
/// GVFS mounts SMB shares as subdirectories under `/run/user/<uid>/gvfs/`
/// with names like `smb-share:server=192.168.1.150,share=pihdd` (optionally
/// with `,user=X,domain=Y` suffixes). Returns None for non-SMB entries.
pub(crate) fn parse_gvfs_smb_dirname(dirname: &str) -> Option<(String, String)> {
    let rest = dirname.strip_prefix("smb-share:")?;
    let mut server = None;
    let mut share = None;
    for part in rest.split(',') {
        if let Some(val) = part.strip_prefix("server=") {
            server = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("share=") {
            share = Some(val.to_string());
        }
    }
    Some((server?, share?))
}

/// Discover GVFS-mounted SMB shares as network locations.
///
/// Scans `/run/user/<uid>/gvfs/` for `smb-share:*` directories. Each one
/// becomes a `Network` location. Skips silently if the GVFS directory
/// doesn't exist (non-GNOME systems).
fn get_network_mounts() -> Vec<LocationInfo> {
    let uid = unsafe { libc::getuid() };
    let gvfs_dir = format!("/run/user/{}/gvfs", uid);
    let gvfs_path = Path::new(&gvfs_dir);

    if !gvfs_path.is_dir() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(gvfs_path) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut mounts = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let dirname = name.to_string_lossy();
        if let Some((_server, share)) = parse_gvfs_smb_dirname(&dirname) {
            let path = entry.path().to_string_lossy().to_string();
            // Skip inaccessible entries (hung FUSE mount)
            if !entry.path().is_dir() {
                continue;
            }
            mounts.push(LocationInfo {
                id: path_to_id(&path),
                name: share,
                path,
                category: LocationCategory::Network,
                icon: None,
                is_ejectable: true,
                fs_type: None,
                supports_trash: false,
                is_read_only: false,
                smb_connection_state: None,
            });
        }
    }

    mounts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    mounts
}

/// Get space information for a volume using `statvfs`.
pub fn get_volume_space(path: &str) -> Option<VolumeSpaceInfo> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;

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

/// Resolve a path to its mount point and filesystem type by finding the
/// longest mount-point prefix match in `/proc/mounts`. Always succeeds
/// because `/` is always mounted — even nonexistent paths match root.
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

/// Build a `VolumeInfo` for the volume containing `path` using only
/// mount table data. Does NOT call `list_locations()`.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    let (mount_point, fs_type) = get_mount_point(path)?;

    let name = mount_display_name(&mount_point);
    let supports_trash = supports_trash_for_fs_type(Some(&fs_type));
    let category = if mount_point == "/" {
        LocationCategory::MainVolume
    } else {
        LocationCategory::AttachedVolume
    };

    Some(VolumeInfo {
        id: path_to_id(&mount_point),
        name,
        path: mount_point,
        category,
        icon: None,
        is_ejectable: false,
        fs_type: Some(fs_type),
        supports_trash,
        is_read_only: false,
        smb_connection_state: None,
    })
}

/// Find the volume that contains a given path using longest-prefix match.
#[allow(dead_code, reason = "Utility kept for future path-to-volume resolution")]
pub fn find_volume_for_path(path: &str) -> Option<String> {
    let locations = list_locations();
    locations
        .iter()
        .filter(|loc| loc.category != LocationCategory::Favorite)
        .filter(|loc| path.starts_with(&loc.path))
        .max_by_key(|loc| loc.path.len())
        .map(|loc| loc.id.clone())
}

pub(crate) use crate::file_system::volume::path_to_id;

/// Extract a display name from a mount path.
fn mount_display_name(mountpoint: &str) -> String {
    Path::new(mountpoint)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(mountpoint)
        .to_string()
}

/// Check if a filesystem type is virtual (not a real disk).
fn is_virtual_fs(fstype: &str) -> bool {
    VIRTUAL_FS_TYPES.contains(&fstype)
}

/// Mount paths that are system internals and should never appear as volumes.
/// These are path prefixes — any mount whose mountpoint starts with one of these is filtered out.
const HIDDEN_MOUNT_PREFIXES: &[&str] = &[
    "/snap/",            // Ubuntu snap loopback packages (squashfs)
    "/run/snapd/",       // Snap daemon internals
    "/boot/",            // EFI system partition, boot loaders
    "/run/user/",        // Per-user runtime mounts (XDG portals, GVFS)
    "/run/credentials/", // systemd credential mounts
];

/// Check if a mount is nested under another real mount (bind mount or sub-partition).
/// For example, `/mnt/share/project/node_modules` is a sub-mount of `/mnt/share`.
fn is_submount(mountpoint: &str, candidate_paths: &[&str]) -> bool {
    candidate_paths.iter().any(|parent| {
        *parent != mountpoint
            && mountpoint.starts_with(parent)
            && mountpoint.as_bytes().get(parent.len()) == Some(&b'/')
    })
}

/// Check if a mount path should be hidden from the volume list.
fn is_hidden_mount(mountpoint: &str) -> bool {
    HIDDEN_MOUNT_PREFIXES
        .iter()
        .any(|prefix| mountpoint.starts_with(prefix))
}

/// Check if a mount point is under a removable media path.
fn is_removable_mount(mountpoint: &str, username: &str) -> bool {
    if username.is_empty() {
        return false;
    }
    let run_media = format!("/run/media/{}/", username);
    let media_user = format!("/media/{}/", username);
    mountpoint.starts_with(&run_media) || mountpoint.starts_with(&media_user)
}

/// Get the current username for removable media path detection.
fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default()
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
/dev/sdc1 /run/media/testuser/USB btrfs rw,relatime 0 0
";

    fn parse_test_mounts() -> Vec<MountEntry> {
        linux_mounts::parse_proc_mounts_from_content(SAMPLE_MOUNTS)
    }

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
    fn test_is_removable_mount() {
        assert!(is_removable_mount("/run/media/user/USB", "user"));
        assert!(is_removable_mount("/media/user/SD", "user"));
        assert!(!is_removable_mount("/mnt/data", "user"));
        assert!(!is_removable_mount("/home", "user"));
        assert!(!is_removable_mount("/run/media/other/USB", "user"));
        assert!(!is_removable_mount("/run/media/user/USB", ""));
    }

    #[test]
    fn test_path_to_id() {
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/mnt/data"), "mntdata");
        assert_eq!(path_to_id("/run/media/user/My-Drive"), "runmediausermy-drive");
    }

    #[test]
    fn test_mount_display_name() {
        assert_eq!(mount_display_name("/mnt/data"), "data");
        assert_eq!(mount_display_name("/run/media/user/USB"), "USB");
        assert_eq!(mount_display_name("/home"), "home");
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
    fn test_is_hidden_mount() {
        assert!(is_hidden_mount("/snap/firefox/7764"));
        assert!(is_hidden_mount("/snap/core22/2134"));
        assert!(is_hidden_mount("/run/snapd/ns/something.mnt"));
        assert!(is_hidden_mount("/boot/efi"));
        assert!(is_hidden_mount("/run/user/1000/doc"));
        assert!(is_hidden_mount("/run/user/1000/gvfs"));
        assert!(is_hidden_mount("/run/credentials/systemd-journald.service"));
        assert!(!is_hidden_mount("/mnt/data"));
        assert!(!is_hidden_mount("/home"));
        assert!(!is_hidden_mount("/media/user/USB"));
        assert!(!is_hidden_mount("/run/media/user/USB"));
    }

    #[test]
    fn test_snap_mounts_filtered_from_volumes() {
        let mounts_with_snaps = "\
/dev/sda1 / ext4 rw,relatime 0 0
/dev/loop0 /snap/bare/5 squashfs ro,nodev,relatime 0 0
/dev/loop2 /snap/firefox/7764 squashfs ro,nodev,relatime 0 0
/dev/loop8 /snap/snap-store/1271 squashfs ro,nodev,relatime 0 0
/dev/sdb1 /mnt/data xfs rw,relatime 0 0
tmpfs /run/user/1000 tmpfs rw,nosuid,nodev,relatime 0 0
portal /run/user/1000/doc fuse.portal rw 0 0
gvfsd-fuse /run/user/1000/gvfs fuse.gvfsd-fuse rw 0 0
/dev/vda1 /boot/efi vfat rw,relatime 0 0
";
        let mounts = linux_mounts::parse_proc_mounts_from_content(mounts_with_snaps);
        let volumes = get_mounted_volumes(&mounts);
        let paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"/mnt/data"), "Should include real mount");
        assert!(
            !paths.iter().any(|p| p.starts_with("/snap/")),
            "Should filter snap mounts"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("/boot/")),
            "Should filter boot mounts"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("/run/user/")),
            "Should filter user runtime mounts"
        );
    }

    #[test]
    fn test_is_submount() {
        let candidates = vec!["/mnt/cmdr", "/mnt/cmdr/cmdr/node_modules", "/media/user/USB"];
        assert!(is_submount("/mnt/cmdr/cmdr/node_modules", &candidates));
        assert!(!is_submount("/mnt/cmdr", &candidates));
        assert!(!is_submount("/media/user/USB", &candidates));
        // Not a submount just because of a shared prefix without a path separator
        assert!(!is_submount("/mnt/cmdr2", &["/mnt/cmdr"]));
    }

    #[test]
    fn test_bind_mounts_filtered_from_volumes() {
        let mounts_with_binds = "\
/dev/vda2 / ext4 rw,relatime 0 0
share /mnt/cmdr virtiofs rw,relatime 0 0
/dev/vda2 /mnt/cmdr/cmdr/node_modules ext4 rw,relatime 0 0
/dev/vda2 /mnt/cmdr/cmdr/apps/desktop/node_modules ext4 rw,relatime 0 0
/dev/sda1 /media/user/Ubuntu\\04025.10\\040arm64 iso9660 ro,relatime 0 0
";
        let mounts = linux_mounts::parse_proc_mounts_from_content(mounts_with_binds);
        let volumes = get_mounted_volumes(&mounts);
        let paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();

        assert!(paths.contains(&"/mnt/cmdr"), "Should include the parent mount");
        assert!(
            !paths.iter().any(|p| p.contains("node_modules")),
            "Should filter bind mounts nested under another volume"
        );
        assert!(
            paths.iter().any(|p| p.contains("Ubuntu")),
            "Should keep independent mounts"
        );
    }

    #[test]
    fn test_get_mounted_volumes_filters_virtual() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        for vol in &volumes {
            assert_ne!(vol.path, "/proc");
            assert_ne!(vol.path, "/sys");
            assert_ne!(vol.path, "/tmp");
        }
    }

    #[test]
    fn test_get_mounted_volumes_excludes_root() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        assert!(
            !volumes.iter().any(|v| v.path == "/"),
            "Root should not be in mounted volumes"
        );
    }

    #[test]
    fn test_get_mounted_volumes_includes_real_fs() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        assert!(volumes.iter().any(|v| v.path == "/home"), "Should include /home");
        assert!(
            volumes.iter().any(|v| v.path == "/mnt/data"),
            "Should include /mnt/data"
        );
    }

    #[test]
    fn test_get_main_volume() {
        let mounts = parse_test_mounts();
        let main = get_main_volume(&mounts);
        assert!(main.is_some());
        let main = main.unwrap();
        assert_eq!(main.id, "root");
        assert_eq!(main.path, "/");
        assert_eq!(main.category, LocationCategory::MainVolume);
        assert_eq!(main.fs_type.as_deref(), Some("ext4"));
    }

    #[test]
    fn test_removable_volume_is_ejectable() {
        // Set USER env var for this test
        let prev = std::env::var("USER").ok();
        // SAFETY: This test is not run concurrently with other tests that read USER.
        unsafe { std::env::set_var("USER", "testuser") };

        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        let usb = volumes.iter().find(|v| v.path.contains("USB"));
        assert!(usb.is_some(), "Should find USB volume");
        assert!(usb.unwrap().is_ejectable, "USB volume should be ejectable");
        assert_eq!(usb.unwrap().fs_type.as_deref(), Some("btrfs"));

        // Restore
        if let Some(prev) = prev {
            // SAFETY: Same as above — restoring original value.
            unsafe { std::env::set_var("USER", prev) };
        }
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

    #[test]
    fn test_parse_gvfs_smb_dirname_basic() {
        let result = parse_gvfs_smb_dirname("smb-share:server=192.168.1.150,share=pihdd");
        assert_eq!(result, Some(("192.168.1.150".to_string(), "pihdd".to_string())));
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_with_extra_params() {
        let result = parse_gvfs_smb_dirname("smb-share:server=mynas.local,share=photos,user=alice,domain=WORKGROUP");
        assert_eq!(result, Some(("mynas.local".to_string(), "photos".to_string())));
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_non_smb() {
        assert_eq!(parse_gvfs_smb_dirname("dav+sd:host=example.com"), None);
        assert_eq!(parse_gvfs_smb_dirname("ftp:host=ftp.example.com"), None);
        assert_eq!(parse_gvfs_smb_dirname("some-random-dir"), None);
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_missing_fields() {
        assert_eq!(parse_gvfs_smb_dirname("smb-share:server=192.168.1.1"), None);
        assert_eq!(parse_gvfs_smb_dirname("smb-share:share=data"), None);
        assert_eq!(parse_gvfs_smb_dirname("smb-share:"), None);
    }
}
