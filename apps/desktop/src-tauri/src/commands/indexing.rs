//! IPC commands for drive indexing.
//!
//! Thin wrappers around `IndexManager` methods, exposed to the frontend via Tauri commands.

use tauri::{AppHandle, Manager, State};

use crate::indexing::store::DirStats;
use crate::indexing::{self, IndexManagerState, IndexStatusResponse, PubScanPriority};

#[tauri::command]
pub async fn start_drive_index(app: AppHandle, state: State<'_, IndexManagerState>) -> Result<(), String> {
    // Initialize indexing if not yet done (for example, manual trigger from debug window).
    // `start_indexing` now auto-calls `resume_or_scan`, so if we initialize here, we're done.
    {
        let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        if guard.is_none() {
            drop(guard);
            indexing::start_indexing(&app)?;
            return Ok(());
        }
    }

    // Already initialized: force a fresh full scan (for example, from the debug "Start scan" button)
    let mut guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_mut() {
        Some(mgr) => mgr.start_scan(),
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn stop_drive_index(state: State<'_, IndexManagerState>) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_mut() {
        Some(mgr) => {
            mgr.stop_scan();
            Ok(())
        }
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn get_index_status(state: State<'_, IndexManagerState>) -> Result<IndexStatusResponse, String> {
    let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_ref() {
        Some(mgr) => mgr.get_status(),
        None => Ok(IndexStatusResponse {
            initialized: false,
            scanning: false,
            entries_scanned: 0,
            dirs_found: 0,
            index_status: None,
            db_file_size: None,
        }),
    }
}

#[tauri::command]
pub async fn get_dir_stats(state: State<'_, IndexManagerState>, path: String) -> Result<Option<DirStats>, String> {
    let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_ref() {
        Some(mgr) => mgr.get_dir_stats(&path),
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn get_dir_stats_batch(
    state: State<'_, IndexManagerState>,
    paths: Vec<String>,
) -> Result<Vec<Option<DirStats>>, String> {
    let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_ref() {
        Some(mgr) => mgr.get_dir_stats_batch(&paths),
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn prioritize_dir(state: State<'_, IndexManagerState>, path: String, priority: String) -> Result<(), String> {
    let priority = match priority.as_str() {
        "user_selected" => PubScanPriority::UserSelected,
        "current_dir" => PubScanPriority::CurrentDir,
        _ => return Err(format!("Invalid priority: {priority}")),
    };

    let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_ref() {
        Some(mgr) => {
            mgr.prioritize_dir(&path, priority);
            Ok(())
        }
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn cancel_nav_priority(state: State<'_, IndexManagerState>, path: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match guard.as_ref() {
        Some(mgr) => {
            mgr.cancel_nav_priority(&path);
            Ok(())
        }
        None => Err("Indexing not initialized".to_string()),
    }
}

#[tauri::command]
pub async fn clear_drive_index(app: AppHandle) -> Result<(), String> {
    indexing::clear_index(&app)
}

/// Toggle drive indexing on/off based on the user's setting.
///
/// When enabled=true: starts indexing (resumes from existing DB if available).
/// When enabled=false: stops all scans and watchers; DB stays on disk.
#[tauri::command]
pub async fn set_indexing_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        // Check if already initialized
        let state = app.state::<IndexManagerState>();
        let already_running = {
            let guard = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
            guard.is_some()
        };
        if !already_running {
            indexing::start_indexing(&app)?;
        }
    } else {
        indexing::stop_indexing(&app)?;
    }
    Ok(())
}
