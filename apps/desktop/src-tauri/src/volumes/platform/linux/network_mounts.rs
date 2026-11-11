//! The GVFS SMB shares as `Network` locations: one row per share folder under
//! `/run/user/<uid>/gvfs/`, which never reaches the mount table. Apart from `smb.rs`, which only
//! parses, so the parser needn't know how a volume ID is minted.

use std::path::Path;

use super::ids::volume_id_for_mount;
use super::smb::parse_gvfs_smb_dirname;
use super::{LocationCategory, LocationInfo};

/// The `Network` location a GVFS SMB share directory publishes, the same row discovery lists
/// it as ([`get_network_mounts`]) and the one `resolve_path_volume_fast` answers for a path
/// inside it, so the two can't name one share two ways.
pub(super) fn gvfs_share_location(path: String, share: String) -> LocationInfo {
    LocationInfo {
        id: volume_id_for_mount(&path),
        name: share.clone(),
        path,
        category: LocationCategory::Network,
        icon: None,
        is_ejectable: true,
        fs_type: None,
        supports_trash: false,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: Some(share),
        mount_account: None,
    }
}

/// Discover GVFS-mounted SMB shares as network locations.
///
/// Scans `/run/user/<uid>/gvfs/` for `smb-share:*` directories. Each one
/// becomes a `Network` location. Skips silently if the GVFS directory
/// doesn't exist (non-GNOME systems).
pub(super) fn get_network_mounts() -> Vec<LocationInfo> {
    // SAFETY: `getuid` reads the process's real UID; always safe, no args or pointers.
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
        if let Some(gvfs) = parse_gvfs_smb_dirname(&dirname) {
            let path = entry.path().to_string_lossy().to_string();
            // Skip inaccessible entries (hung FUSE mount)
            if !entry.path().is_dir() {
                continue;
            }
            mounts.push(gvfs_share_location(path, gvfs.share));
        }
    }

    mounts.sort_by_key(|m| m.name.to_lowercase());
    mounts
}
