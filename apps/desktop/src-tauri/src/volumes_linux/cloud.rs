//! Cloud-drive discovery: the sync clients that park a plain directory in the
//! user's home. Each is a well-known path rather than a mount, so detection is
//! an `is_dir()` probe, and the filesystem type comes from whichever real mount
//! contains it.

use super::fs_type::supports_trash_for_fs_type;
use super::{LocationCategory, LocationInfo, MountEntry, linux_mounts};
use std::path::Path;

/// Get cloud drives by checking common locations.
pub(super) fn get_cloud_drives(mounts: &[MountEntry]) -> Vec<LocationInfo> {
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
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
                capabilities: None,
            });
        }
    }

    drives.sort_by_key(|d| d.name.to_lowercase());
    drives
}
