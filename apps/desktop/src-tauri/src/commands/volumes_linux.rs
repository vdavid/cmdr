//! Tauri commands for volume operations on Linux.

use super::util::TimedOut;
use crate::volumes_linux::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

/// Lists all mounted volumes.
#[tauri::command]
pub fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    TimedOut {
        data: volumes_linux::list_mounted_volumes(),
        timed_out: false,
    }
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Finds the actual volume (not a favorite) that contains a given path.
#[tauri::command]
pub fn find_containing_volume(path: String) -> TimedOut<Option<VolumeInfo>> {
    let locations = volumes_linux::list_locations();

    let volumes: Vec<_> = locations
        .into_iter()
        .filter(|loc| loc.category != LocationCategory::Favorite)
        .collect();

    let mut best_match: Option<VolumeInfo> = None;
    let mut best_len = 0;

    for vol in volumes {
        if path.starts_with(&vol.path) && vol.path.len() > best_len {
            best_len = vol.path.len();
            best_match = Some(vol);
        }
    }

    TimedOut {
        data: best_match,
        timed_out: false,
    }
}

/// Gets space information for a volume at the given path.
#[tauri::command]
pub fn get_volume_space(path: String) -> TimedOut<Option<VolumeSpaceInfo>> {
    TimedOut {
        data: volumes_linux::get_volume_space(&path),
        timed_out: false,
    }
}
