//! Volume and location discovery for macOS.
//!
//! Provides a Finder-like location picker with:
//! - Favorites (from Finder sidebar)
//! - Main volume (Macintosh HD)
//! - Attached volumes (external drives)
//! - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
//! - Network locations

pub mod watcher;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub use crate::file_system::volume::SmbConnectionState;

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
    /// Base64-encoded WebP.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub is_ejectable: bool,
    /// Filesystem type from `statfs` (for example, "apfs", "hfs", "smbfs").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,
    /// Whether this volume supports macOS trash. Derived from `fs_type`.
    pub supports_trash: bool,
    /// Whether this location is read-only (for example, MTP devices with locked storage).
    pub is_read_only: bool,
    /// SMB connection state indicator. Only set for volumes with an active `SmbVolume`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_connection_state: Option<SmbConnectionState>,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Determine whether a filesystem type supports trash.
///
/// Local filesystems (APFS, HFS+, ext4, btrfs, xfs, zfs) support trash.
/// Network filesystems (SMB, NFS, AFP, WebDAV, CIFS, FUSE-based SSH) and
/// non-Mac formats (FAT32/exFAT) don't reliably support it. Unknown types
/// default to `true` (optimistic — trash failure is caught at operation time).
pub fn supports_trash_for_fs_type(fs_type: Option<&str>) -> bool {
    let Some(fs) = fs_type else { return true };
    let fs_lower = fs.to_ascii_lowercase();
    match fs_lower.as_str() {
        "apfs" | "hfs" | "ext4" | "btrfs" | "xfs" | "zfs" => true,
        "smbfs" | "nfs" | "afpfs" | "webdav" | "cifs" | "fuse.sshfs" | "msdos" | "exfat" => false,
        _ => true,
    }
}

/// Returns true if the filesystem type is SMB (macOS `smbfs` or Linux `cifs`).
pub fn is_smb_fs_type(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("smbfs" | "cifs"))
}

/// Information about an SMB mount extracted from `statfs`.
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

/// Extracts SMB server, share, and username from a mount path via `statfs`.
///
/// On macOS, `statfs.f_mntfromname` for SMB mounts looks like:
/// - `//user@192.168.1.111/share` (authenticated)
/// - `//192.168.1.111/share` (guest)
///
/// Returns `None` if the path is not an SMB mount or parsing fails.
pub fn get_smb_mount_info(mount_path: &str) -> Option<SmbMountInfo> {
    use std::ffi::CString;

    let c_path = CString::new(mount_path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };

    // Check filesystem type is SMB
    let fs_type: String = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();
    if !is_smb_fs_type(Some(&fs_type)) {
        return None;
    }

    // Extract mount source (for example, "//david@192.168.1.111/naspi")
    let mount_from: String = stat
        .f_mntfromname
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();

    parse_smb_mount_source(&mount_from)
}

/// Parses an SMB mount source string like `//user@host/share` or `//host/share`.
fn parse_smb_mount_source(source: &str) -> Option<SmbMountInfo> {
    // Strip leading "//"
    let rest = source.strip_prefix("//")?;

    // Split into "user@host/share" or "host/share"
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
            let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if result == 0 {
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

/// Build a `VolumeInfo` for the volume containing `path` using only
/// `statfs()` and per-path NSURL resource queries. Does NOT call
/// `list_locations()` — avoids the blocking NSFileManager volume enumeration.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    let (mount_point, fs_type) = get_mount_point(path)?;

    // Drain autoreleased ObjC objects (NSURL, NSString).
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&mount_point));

        let name = get_volume_name(&url, &mount_point);
        let is_ejectable = get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false);
        let supports_trash = supports_trash_for_fs_type(Some(&fs_type));
        let category = if mount_point == "/" {
            LocationCategory::MainVolume
        } else {
            LocationCategory::AttachedVolume
        };
        let icon = get_icon_for_path(&mount_point);

        Some(VolumeInfo {
            id: path_to_id(&mount_point),
            name,
            path: mount_point,
            category,
            icon,
            is_ejectable,
            fs_type: Some(fs_type),
            supports_trash,
            is_read_only: false,
            smb_connection_state: None,
        })
    })
}

/// Read the filesystem type for a path using `libc::statfs`.
///
/// Returns `None` if the `statfs` call fails (for example, the volume was
/// ejected between listing and probing).
/// Returns the filesystem ID (`f_fsid`) for a path via `statfs`, as raw bytes.
/// Two paths on the same filesystem have the same `f_fsid`.
/// Used to detect whether a mount point has settled (differs from root's fsid).
///
/// We compare as bytes because `libc::fsid_t`'s fields are private on macOS.
pub(crate) fn get_fsid(path: &str) -> Option<[u8; size_of::<libc::fsid_t>()]> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    // Safe: fsid_t is a plain-old-data struct, reading as bytes is well-defined
    let bytes: [u8; size_of::<libc::fsid_t>()] = unsafe { std::mem::transmute(stat.f_fsid) };
    Some(bytes)
}

fn get_fs_type(path: &str) -> Option<String> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

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

/// Get all locations organized by category, deduplicated.
pub fn list_locations() -> Vec<LocationInfo> {
    let mut locations = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 1. Favorites
    for loc in get_favorites() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 2. Main volume
    if let Some(loc) = get_main_volume()
        && seen_paths.insert(loc.path.clone())
    {
        locations.push(loc);
    }

    // 3. Attached volumes
    for loc in get_attached_volumes() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 4. Cloud drives (skip if already in favorites)
    for loc in get_cloud_drives() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 5. Network - commented out for now as /Network requires special handling
    // for loc in get_network_locations() {
    //     if seen_paths.insert(loc.path.clone()) {
    //         locations.push(loc);
    //     }
    // }

    locations
}

/// Get Finder favorites (common user folders).
fn get_favorites() -> Vec<LocationInfo> {
    let home = dirs::home_dir().unwrap_or_default();
    let desktop = home.join("Desktop");
    let documents = home.join("Documents");
    let downloads = home.join("Downloads");
    let desktop_str = desktop.to_string_lossy();
    let documents_str = documents.to_string_lossy();
    let downloads_str = downloads.to_string_lossy();
    let favorites_paths = [
        ("/Applications", "Applications"),
        (desktop_str.as_ref(), "Desktop"),
        (documents_str.as_ref(), "Documents"),
        (downloads_str.as_ref(), "Downloads"),
    ];

    favorites_paths
        .into_iter()
        .filter(|(path, _)| Path::new(*path).exists())
        .map(|(path, name)| {
            // Favorites are folders on the boot volume, not mount points.
            // statfs still works — it reports the underlying volume's fs type.
            let fs_type = get_fs_type(path);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
            LocationInfo {
                id: format!("fav-{}", name.to_lowercase()),
                name: name.to_string(),
                path: path.to_string(),
                category: LocationCategory::Favorite,
                icon: get_icon_for_path(path),
                is_ejectable: false,
                fs_type,
                supports_trash,
                is_read_only: false,
                smb_connection_state: None,
            }
        })
        .collect()
}

/// Get the main boot volume.
fn get_main_volume() -> Option<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSArray, NSFileManager, NSURL, NSVolumeEnumerationOptions};

    // Drain autoreleased ObjC objects (NSFileManager, NSArray, NSURL).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let file_manager = NSFileManager::defaultManager();
        let options = NSVolumeEnumerationOptions::SkipHiddenVolumes;

        let volume_urls: Option<objc2::rc::Retained<NSArray<NSURL>>> =
            file_manager.mountedVolumeURLsIncludingResourceValuesForKeys_options(None, options);

        let urls = volume_urls?;

        for url in urls.iter() {
            let path_str = url.path()?;
            let path = path_str.to_string();

            // Root volume
            if path == "/" {
                let name = get_volume_name(&url, &path);
                let fs_type = get_fs_type("/");
                let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
                return Some(LocationInfo {
                    id: DEFAULT_VOLUME_ID.to_string(),
                    name,
                    path,
                    category: LocationCategory::MainVolume,
                    icon: get_icon_for_path("/"),
                    is_ejectable: false,
                    fs_type,
                    supports_trash,
                    is_read_only: false,
                    smb_connection_state: None,
                });
            }
        }
        None
    })
}

/// Get attached volumes (external drives, USB, etc.).
pub fn get_attached_volumes() -> Vec<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSArray, NSFileManager, NSURL, NSVolumeEnumerationOptions};

    let root_fsid = get_fsid("/");

    // Drain autoreleased ObjC objects (NSFileManager, NSArray, NSURL).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let file_manager = NSFileManager::defaultManager();
        let options = NSVolumeEnumerationOptions::SkipHiddenVolumes;

        let volume_urls: Option<objc2::rc::Retained<NSArray<NSURL>>> =
            file_manager.mountedVolumeURLsIncludingResourceValuesForKeys_options(None, options);

        let Some(urls) = volume_urls else {
            return vec![];
        };

        let mut volumes = Vec::new();

        for url in urls.iter() {
            let Some(path_str) = url.path() else { continue };
            let path = path_str.to_string();

            // Skip root (already handled as main volume)
            if path == "/" {
                continue;
            }

            // Skip system volumes
            if path.starts_with("/System") || path.contains("/Preboot") || path.contains("/Recovery") {
                continue;
            }

            // Skip cloud storage (handled separately)
            if path.contains("/Library/CloudStorage") {
                continue;
            }

            // Only include /Volumes/* paths (actual mounted volumes)
            if !path.starts_with("/Volumes/") {
                continue;
            }

            // Skip volumes whose mount hasn't settled yet. macOS fires the
            // FSEvent before NSFileManager metadata is ready — the path exists
            // but statfs still reports the root filesystem's ID. Including it
            // would show wrong metadata (root volume's name/icon/type).
            if let Some(root) = root_fsid
                && get_fsid(&path) == Some(root)
            {
                log::debug!("Skipping {} (fsid matches root — mount not settled)", path);
                continue;
            }

            let mut name = get_volume_name(&url, &path);
            let is_ejectable = get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false);
            let fs_type = get_fs_type(&path);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());

            // For SMB mounts, show "share on server" so the user knows which
            // server they're browsing (especially when multiple servers share
            // the same share name).
            if is_smb_fs_type(fs_type.as_deref())
                && let Some(info) = get_smb_mount_info(&path)
            {
                name = format!("{} on {}", info.share, info.server);
            }

            volumes.push(LocationInfo {
                id: path_to_id(&path),
                name,
                path: path.clone(),
                category: LocationCategory::AttachedVolume,
                icon: get_icon_for_path(&path),
                is_ejectable,
                fs_type,
                supports_trash,
                is_read_only: false,
                smb_connection_state: None,
            });
        }

        // Sort alphabetically
        volumes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        volumes
    })
}

/// Get cloud drives (Dropbox, iCloud, Google Drive, etc.).
pub fn get_cloud_drives() -> Vec<LocationInfo> {
    let mut drives = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // iCloud Drive
    let icloud_path = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud_path.exists() {
        let icloud_path_str = icloud_path.to_string_lossy().to_string();
        let fs_type = get_fs_type(&icloud_path_str);
        let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
        drives.push(LocationInfo {
            id: "cloud-icloud".to_string(),
            name: "iCloud Drive".to_string(),
            path: icloud_path_str,
            category: LocationCategory::CloudDrive,
            icon: get_icon_for_path(&icloud_path.to_string_lossy()),
            is_ejectable: false,
            fs_type,
            supports_trash,
            is_read_only: false,
            smb_connection_state: None,
        });
    }

    // Scan ~/Library/CloudStorage for other cloud providers
    let cloud_storage_path = home.join("Library/CloudStorage");
    if let Ok(entries) = std::fs::read_dir(&cloud_storage_path) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Parse cloud provider name from directory
                let (provider_name, id) = parse_cloud_provider_name(dir_name);
                if !provider_name.is_empty() {
                    let cloud_path = path.to_string_lossy().to_string();
                    let fs_type = get_fs_type(&cloud_path);
                    let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
                    drives.push(LocationInfo {
                        id,
                        name: provider_name,
                        path: cloud_path,
                        category: LocationCategory::CloudDrive,
                        icon: get_icon_for_path(&path.to_string_lossy()),
                        is_ejectable: false,
                        fs_type,
                        supports_trash,
                        is_read_only: false,
                        smb_connection_state: None,
                    });
                }
            }
        }
    }

    // Sort alphabetically
    drives.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    drives
}

/// Parse cloud provider name from CloudStorage directory name.
/// E.g., "Dropbox" -> "Dropbox", "GoogleDrive-email@gmail.com" -> "Google Drive"
fn parse_cloud_provider_name(dir_name: &str) -> (String, String) {
    if dir_name.starts_with("Dropbox") {
        return ("Dropbox".to_string(), "cloud-dropbox".to_string());
    }
    if dir_name.starts_with("GoogleDrive") {
        return ("Google Drive".to_string(), "cloud-google-drive".to_string());
    }
    if dir_name.starts_with("OneDrive") {
        // Handle OneDrive-Personal, OneDrive-Business, etc.
        if dir_name.contains("Business") {
            return (
                "OneDrive for Business".to_string(),
                "cloud-onedrive-business".to_string(),
            );
        }
        return ("OneDrive".to_string(), "cloud-onedrive".to_string());
    }
    if dir_name.starts_with("Box") {
        return ("Box".to_string(), "cloud-box".to_string());
    }
    if dir_name.starts_with("pCloud") {
        return ("pCloud".to_string(), "cloud-pcloud".to_string());
    }
    // Generic cloud provider
    if !dir_name.is_empty() {
        let clean_name = dir_name.split('-').next().unwrap_or(dir_name);
        return (clean_name.to_string(), format!("cloud-{}", clean_name.to_lowercase()));
    }
    (String::new(), String::new())
}

/// Get network locations.
#[allow(dead_code, reason = "Will be used when network locations sidebar is implemented")]
fn get_network_locations() -> Vec<LocationInfo> {
    let mut locations = Vec::new();

    // Always include Network like Finder does
    // Even if /Network doesn't exist as a directory, it's a browseable location in Finder
    let network_path = "/Network";
    locations.push(LocationInfo {
        id: "network".to_string(),
        name: "Network".to_string(),
        path: network_path.to_string(),
        category: LocationCategory::Network,
        icon: None, // Will use placeholder in frontend
        is_ejectable: false,
        fs_type: None,
        supports_trash: false,
        is_read_only: false,
        smb_connection_state: None,
    });

    locations
}

/// Get the display name for a volume.
fn get_volume_name(url: &objc2_foundation::NSURL, path: &str) -> String {
    // Try localized name first
    if let Some(name) = get_string_resource(url, "NSURLVolumeLocalizedNameKey") {
        return name;
    }
    if let Some(name) = get_string_resource(url, "NSURLVolumeNameKey") {
        return name;
    }
    // Fallback to path-based name
    if path == "/" {
        "Macintosh HD".to_string()
    } else {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }
}

pub(crate) use crate::file_system::volume::path_to_id;

/// Get icon for a path as base64-encoded WebP.
fn get_icon_for_path(path: &str) -> Option<String> {
    crate::icons::get_icon_for_path(path)
}

/// Get a resource value from an NSURL and convert it using the provided extractor.
fn get_nsurl_resource<T>(
    url: &objc2_foundation::NSURL,
    key: &str,
    extractor: impl FnOnce(objc2::rc::Retained<objc2::runtime::AnyObject>) -> Option<T>,
) -> Option<T> {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(extractor)
    } else {
        None
    }
}

/// Get a boolean resource value from an NSURL.
fn get_bool_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<bool> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
}

/// Get a string resource value from an NSURL.
fn get_string_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<String> {
    use objc2_foundation::NSString;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSString>().ok().map(|s| s.to_string()))
}

/// Get a u64 resource value from an NSURL (for capacity values).
fn get_u64_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<u64> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| {
        obj.downcast::<NSNumber>().ok().map(|n| n.unsignedLongLongValue())
    })
}

/// Information about volume space.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceInfo {
    /// In bytes.
    pub total_bytes: u64,
    /// In bytes.
    pub available_bytes: u64,
}

/// Get space information for a volume containing the given path.
pub fn get_volume_space(path: &str) -> Option<VolumeSpaceInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSURL;

    // Drain autoreleased ObjC objects (NSURL, NSString, NSNumber).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(path));

        let total = get_u64_resource(&url, "NSURLVolumeTotalCapacityKey")?;
        let available = get_u64_resource(&url, "NSURLVolumeAvailableCapacityForImportantUsageKey")
            .filter(|&v| v > 0)
            .or_else(|| get_u64_resource(&url, "NSURLVolumeAvailableCapacityKey"))?;

        Some(VolumeSpaceInfo {
            total_bytes: total,
            available_bytes: available,
        })
    })
}

// Legacy compatibility - maintain VolumeInfo type for backwards compatibility
pub use LocationInfo as VolumeInfo;

/// Legacy function - now calls list_locations
pub fn list_mounted_volumes() -> Vec<LocationInfo> {
    list_locations()
}

#[allow(dead_code, reason = "Utility kept for future path-to-volume resolution")]
pub fn find_volume_for_path(path: &str) -> Option<String> {
    let locations = list_locations();
    let mut best_match: Option<&LocationInfo> = None;
    let mut best_len = 0;

    for loc in &locations {
        if path.starts_with(&loc.path) && loc.path.len() > best_len {
            best_match = Some(loc);
            best_len = loc.path.len();
        }
    }

    best_match.map(|v| v.id.clone())
}

#[allow(dead_code, reason = "Utility kept for future volume status checks")]
pub fn is_volume_mounted(volume_id: &str) -> bool {
    list_locations().iter().any(|v| v.id == volume_id)
}

#[allow(dead_code, reason = "Utility kept for future volume lookup operations")]
pub fn get_volume_by_id(volume_id: &str) -> Option<LocationInfo> {
    list_locations().into_iter().find(|v| v.id == volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_locations_includes_root() {
        let locations = list_locations();
        assert!(!locations.is_empty(), "Should have at least one location");
        // Should have main volume
        assert!(
            locations.iter().any(|l| l.category == LocationCategory::MainVolume),
            "Should include main volume"
        );
    }

    #[test]
    fn test_locations_are_deduplicated() {
        let locations = list_locations();
        let mut seen_paths = HashSet::new();
        for loc in &locations {
            assert!(seen_paths.insert(&loc.path), "Duplicate path found: {}", loc.path);
        }
    }

    #[test]
    fn test_parse_cloud_provider_name() {
        assert_eq!(
            parse_cloud_provider_name("Dropbox"),
            ("Dropbox".to_string(), "cloud-dropbox".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("GoogleDrive-user@gmail.com"),
            ("Google Drive".to_string(), "cloud-google-drive".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("OneDrive-Personal"),
            ("OneDrive".to_string(), "cloud-onedrive".to_string())
        );
    }

    #[test]
    fn test_path_to_id() {
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }

    #[test]
    fn test_get_volume_space_root() {
        let space = get_volume_space("/");
        assert!(space.is_some(), "Should get space info for root volume");

        let space = space.unwrap();
        assert!(space.total_bytes > 0, "Total bytes should be positive");
        assert!(space.available_bytes > 0, "Available bytes should be positive");
        assert!(
            space.available_bytes <= space.total_bytes,
            "Available should be <= total"
        );
    }

    #[test]
    fn test_get_volume_space_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let space = get_volume_space(home.to_str().unwrap());
        assert!(space.is_some(), "Should get space info for home directory");
    }

    #[test]
    fn test_get_volume_space_nonexistent() {
        // Nonexistent paths return None - the NSURL resource API doesn't resolve to ancestor volumes
        let space = get_volume_space("/nonexistent/path/that/does/not/exist");
        assert!(space.is_none(), "Nonexistent paths should return None");
    }

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
    fn test_resolve_path_volume_fast_root() {
        let result = resolve_path_volume_fast("/");
        assert!(result.is_some(), "Root should resolve to a VolumeInfo");
        let vol = result.unwrap();
        assert_eq!(vol.id, "root");
        assert_eq!(vol.path, "/");
        assert_eq!(vol.category, LocationCategory::MainVolume);
        assert!(vol.fs_type.is_some());
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
    fn test_locations_have_fs_type_and_supports_trash() {
        let locations = list_locations();
        // Every location should have supports_trash set
        for loc in &locations {
            // Main volume and favorites on APFS should support trash
            if loc.category == LocationCategory::MainVolume {
                assert!(loc.fs_type.is_some(), "Main volume should have fs_type");
                assert!(loc.supports_trash, "Main volume should support trash");
            }
        }
    }
}
