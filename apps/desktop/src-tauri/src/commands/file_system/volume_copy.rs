//! Tauri commands for cross-volume copy/move operations.

use crate::file_system::{
    ScanConflict, VolumeCopyConfig, VolumeCopyScanResult, WriteOperationError, WriteOperationStartResult,
    copy_between_volumes as ops_copy_between_volumes, get_volume_manager,
    move_between_volumes as ops_move_between_volumes, scan_for_volume_copy as ops_scan_for_volume_copy,
};
use std::path::PathBuf;
use tokio::time::Duration;

use crate::commands::util::IpcError;

/// Unified copy across volume types (local, MTP, etc.). Same events as `copy_files`.
#[tauri::command]
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: dest_volume_id.clone(),
            message: format!("Destination volume '{}' not found", dest_volume_id),
        })?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let config = config.unwrap_or_default();

    ops_copy_between_volumes(app, source_volume, source_paths, dest_volume, dest_path, config).await
}

/// Unified move across volume types. Same events as `copy_between_volumes`.
/// Handles same-volume (native rename/move), both-local (native move), and cross-volume (copy+delete).
#[tauri::command]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: dest_volume_id.clone(),
            message: format!("Destination volume '{}' not found", dest_volume_id),
        })?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let config = config.unwrap_or_default();

    ops_move_between_volumes(app, source_volume, source_paths, dest_volume, dest_path, config).await
}

/// Pre-flight scan: total count/bytes, available space, conflicts. Doesn't copy anything.
#[tauri::command]
pub async fn scan_volume_for_copy(
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    max_conflicts: Option<usize>,
) -> Result<VolumeCopyScanResult, IpcError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Source volume '{}' not found", source_volume_id)))?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Destination volume '{}' not found", dest_volume_id)))?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let max_conflicts = max_conflicts.unwrap_or(100);

    // Run scan (now async)
    tokio::time::timeout(
        Duration::from_secs(30),
        ops_scan_for_volume_copy(&*source_volume, &source_paths, &*dest_volume, &dest_path, max_conflicts),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(e.to_string()))
}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
#[tauri::command]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
) -> Result<Vec<ScanConflict>, IpcError> {
    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume '{}' not found", volume_id)))?;

    let source_items: Vec<crate::file_system::SourceItemInfo> = source_items
        .into_iter()
        .map(|item| crate::file_system::SourceItemInfo {
            name: item.name,
            size: item.size,
            modified: item.modified,
        })
        .collect();
    let dest_path = PathBuf::from(dest_path);

    // Run conflict scan (now async)
    tokio::time::timeout(
        Duration::from_secs(30),
        volume.scan_for_conflicts(&source_items, &dest_path),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(e.to_string()))
}

/// Input type for source item information (used by scan_volume_for_conflicts).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItemInput {
    /// File/directory name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix timestamp in seconds).
    pub modified: Option<i64>,
}
