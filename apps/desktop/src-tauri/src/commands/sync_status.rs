//! Sync status Tauri commands.

use std::collections::HashMap;

#[cfg(target_os = "macos")]
use crate::file_system::sync_status::{SyncStatus, get_sync_statuses};

/// Gets sync status for multiple file paths.
///
/// Returns a map from path to sync status string.
#[tauri::command]
#[cfg(target_os = "macos")]
pub fn get_sync_status(paths: Vec<String>) -> HashMap<String, SyncStatus> {
    get_sync_statuses(paths)
}

/// Non-macOS fallback - returns empty map.
#[tauri::command]
#[cfg(not(target_os = "macos"))]
pub fn get_sync_status(_paths: Vec<String>) -> HashMap<String, String> {
    HashMap::new()
}
