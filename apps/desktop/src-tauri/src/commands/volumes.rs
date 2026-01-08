//! Tauri commands for volume operations.

use crate::volumes::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo};

/// Lists all mounted volumes.
#[tauri::command]
pub fn list_volumes() -> Vec<VolumeInfo> {
    volumes::list_mounted_volumes()
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Finds the actual volume (not a favorite) that contains a given path.
/// Returns the volume info for the best matching volume, excluding favorites.
/// This is used to determine which volume to highlight when a favorite is selected.
#[tauri::command]
pub fn find_containing_volume(path: String) -> Option<VolumeInfo> {
    let locations = volumes::list_locations();

    // Only consider actual volumes, not favorites
    let volumes: Vec<_> = locations
        .into_iter()
        .filter(|loc| loc.category != LocationCategory::Favorite)
        .collect();

    // Find the volume with the longest matching path prefix
    let mut best_match: Option<VolumeInfo> = None;
    let mut best_len = 0;

    for vol in volumes {
        // Check if this volume's path is a prefix of the target path
        if path.starts_with(&vol.path) && vol.path.len() > best_len {
            best_len = vol.path.len();
            best_match = Some(vol);
        }
    }

    best_match
}
