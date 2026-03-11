//! IPC commands for drive indexing.
//!
//! Thin wrappers around `indexing` module functions, exposed to the frontend via Tauri commands.

use tauri::AppHandle;

use crate::indexing::{self, IndexStatusResponse, PubScanPriority, store::DirStats};

#[tauri::command]
pub async fn start_drive_index(app: AppHandle) -> Result<(), String> {
    if indexing::is_active() {
        // Already running: force a fresh full scan (for example, from the debug "Start scan" button)
        indexing::force_scan()
    } else {
        indexing::start_indexing(&app)
    }
}

#[tauri::command]
pub async fn stop_drive_index() -> Result<(), String> {
    indexing::stop_scan()
}

#[tauri::command]
pub async fn get_index_status() -> Result<IndexStatusResponse, String> {
    indexing::get_status()
}

#[tauri::command]
pub async fn get_dir_stats(path: String) -> Result<Option<DirStats>, String> {
    indexing::get_dir_stats(&path)
}

#[tauri::command]
pub async fn get_dir_stats_batch(paths: Vec<String>) -> Result<Vec<Option<DirStats>>, String> {
    indexing::get_dir_stats_batch(&paths)
}

#[tauri::command]
pub async fn prioritize_dir(path: String, priority: String) -> Result<(), String> {
    let priority = match priority.as_str() {
        "user_selected" => PubScanPriority::UserSelected,
        "current_dir" => PubScanPriority::CurrentDir,
        _ => return Err(format!("Invalid priority: {priority}")),
    };
    indexing::prioritize_dir(&path, priority)
}

#[tauri::command]
pub async fn cancel_nav_priority(path: String) -> Result<(), String> {
    indexing::cancel_nav_priority(&path)
}

#[tauri::command]
pub async fn clear_drive_index() -> Result<(), String> {
    indexing::clear_index()
}

/// Toggle drive indexing on/off based on the user's setting.
#[tauri::command]
pub async fn set_indexing_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        if !indexing::is_active() {
            indexing::start_indexing(&app)?;
        }
    } else {
        indexing::stop_indexing()?;
    }
    Ok(())
}
