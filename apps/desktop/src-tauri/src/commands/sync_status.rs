//! Sync status Tauri commands.

use std::collections::HashMap;
use tokio::time::Duration;

use super::util::blocking_with_timeout;

#[cfg(target_os = "macos")]
use crate::file_system::sync_status::{SyncStatus, get_sync_statuses};

const SYNC_STATUS_TIMEOUT: Duration = Duration::from_secs(2);

/// Gets sync status for multiple file paths.
///
/// Returns a map from path to sync status string.
#[tauri::command]
#[cfg(target_os = "macos")]
pub async fn get_sync_status(paths: Vec<String>) -> HashMap<String, SyncStatus> {
    blocking_with_timeout(SYNC_STATUS_TIMEOUT, HashMap::new(), move || get_sync_statuses(paths)).await
}

/// Non-macOS fallback - returns empty map.
#[tauri::command]
#[cfg(not(target_os = "macos"))]
pub async fn get_sync_status(_paths: Vec<String>) -> HashMap<String, String> {
    HashMap::new()
}
