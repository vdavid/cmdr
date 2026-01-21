//! Volume stubs for Linux/non-macOS platforms.
//!
//! Provides a minimal volume implementation that returns the root filesystem
//! and common Linux directories as "favorites".

use serde::{Deserialize, Serialize};

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
}

/// Information about a location (volume, folder, or cloud drive).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub is_ejectable: bool,
}

/// Information about volume space.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Lists all mounted volumes (Linux stub).
#[tauri::command]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let mut locations = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // Add favorites (common Linux directories)
    let favorites = [
        (home.join("Desktop"), "Desktop", "fav-desktop"),
        (home.join("Documents"), "Documents", "fav-documents"),
        (home.join("Downloads"), "Downloads", "fav-downloads"),
    ];

    for (path, name, id) in favorites {
        if path.exists() {
            locations.push(VolumeInfo {
                id: id.to_string(),
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                category: LocationCategory::Favorite,
                icon: None,
                is_ejectable: false,
            });
        }
    }

    // Add root volume
    locations.push(VolumeInfo {
        id: DEFAULT_VOLUME_ID.to_string(),
        name: "Root".to_string(),
        path: "/".to_string(),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
    });

    // Add home directory
    locations.push(VolumeInfo {
        id: "home".to_string(),
        name: "Home".to_string(),
        path: home.to_string_lossy().to_string(),
        category: LocationCategory::Favorite,
        icon: None,
        is_ejectable: false,
    });

    locations
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Finds the volume that contains a given path.
#[tauri::command]
pub fn find_containing_volume(path: String) -> Option<VolumeInfo> {
    let volumes = list_volumes();

    // Find the volume with the longest matching path prefix (excluding favorites)
    volumes
        .into_iter()
        .filter(|v| v.category != LocationCategory::Favorite)
        .filter(|v| path.starts_with(&v.path))
        .max_by_key(|v| v.path.len())
}

/// Gets space information for a volume at the given path.
#[tauri::command]
pub fn get_volume_space(path: String) -> Option<VolumeSpaceInfo> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            let block_size = stat.f_frsize as u64;
            Some(VolumeSpaceInfo {
                total_bytes: stat.f_blocks * block_size,
                available_bytes: stat.f_bavail * block_size,
            })
        } else {
            None
        }
    }
}

/// Stub for volume watcher - does nothing on Linux.
/// Kept for API compatibility but not called on Linux.
#[allow(dead_code)]
pub fn start_volume_watcher<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {
    // No-op on Linux - we don't watch for volume changes
}
