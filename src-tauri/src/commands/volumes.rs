//! Tauri commands for volume operations.

use crate::volumes::{self, DEFAULT_VOLUME_ID, VolumeInfo};

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
