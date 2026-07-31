//! Sync status Tauri commands.

use std::collections::HashMap;

use super::util::TimedOut;

#[cfg(target_os = "macos")]
use tokio::time::Duration;

/// How long the frontend waits. Unlike the other command timeouts this one is
/// applied *inside* `file_system::sync_status`, not by `blocking_with_timeout_flag`:
/// the module has to keep its own work alive past the deadline (same reasoning as
/// `timeout_detached`) and it returns the paths it already knows rather than an
/// empty map. Wrapping it again out here would throw that partial answer away.
#[cfg(target_os = "macos")]
const SYNC_STATUS_TIMEOUT: Duration = Duration::from_secs(2);

/// Gets sync status for multiple file paths.
///
/// Returns a map from path to sync status string.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn get_sync_status(
    paths: Vec<String>,
) -> TimedOut<HashMap<String, crate::file_system::sync_status::SyncStatus>> {
    let (data, timed_out) = crate::file_system::sync_status::statuses_within(paths, SYNC_STATUS_TIMEOUT).await;
    TimedOut { data, timed_out }
}

/// Non-macOS fallback - returns empty map.
#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn get_sync_status(_paths: Vec<String>) -> TimedOut<HashMap<String, String>> {
    TimedOut {
        data: HashMap::new(),
        timed_out: false,
    }
}
