//! Tauri commands for volume operations.

use tokio::time::Duration;

use super::util::blocking_with_timeout;
use crate::volumes::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

const VOLUME_TIMEOUT: Duration = Duration::from_secs(2);

/// Lists all mounted volumes.
#[tauri::command]
pub async fn list_volumes() -> Vec<VolumeInfo> {
    blocking_with_timeout(VOLUME_TIMEOUT, vec![], volumes::list_mounted_volumes).await
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Finds the actual volume (not a favorite) that contains a given path.
/// Returns the volume info for the best matching volume, excluding favorites.
/// This is used to determine which volume to set as active when a favorite is chosen.
#[tauri::command]
pub async fn find_containing_volume(path: String) -> Option<VolumeInfo> {
    blocking_with_timeout(VOLUME_TIMEOUT, None, move || {
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
            if path.starts_with(&vol.path) && vol.path.len() > best_len {
                best_len = vol.path.len();
                best_match = Some(vol);
            }
        }

        best_match
    })
    .await
}

/// Gets space information for a volume at the given path.
/// Returns total and available bytes for the volume.
#[tauri::command]
pub async fn get_volume_space(path: String) -> Option<VolumeSpaceInfo> {
    blocking_with_timeout(VOLUME_TIMEOUT, None, move || volumes::get_volume_space(&path)).await
}
