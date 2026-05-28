//! Tauri commands for write operations (create, copy, move, delete, trash) and scan preview.

use crate::file_system::write_operations::{
    ConflictResolution, ScanPreviewStartResult, cancel_scan_preview as ops_cancel_scan_preview,
    get_scan_preview_totals as ops_get_scan_preview_totals, resolve_write_conflict as ops_resolve_write_conflict,
    start_scan_preview as ops_start_scan_preview,
};
use crate::file_system::{
    OperationStatus, OperationSummary, SortColumn, SortOrder, WriteOperationConfig, WriteOperationError,
    WriteOperationStartResult, cancel_all_write_operations as ops_cancel_all_write_operations,
    cancel_write_operation as ops_cancel_write_operation, copy_files_start as ops_copy_files_start,
    delete_files_start as ops_delete_files_start, get_operation_status as ops_get_operation_status, get_volume_manager,
    list_active_operations as ops_list_active_operations, move_files_start as ops_move_files_start,
    trash_files_start as ops_trash_files_start,
};
use std::path::{Path, PathBuf};
use tokio::time::Duration;

use crate::commands::util::IpcError;

use super::expand_tilde;

#[tauri::command]
#[specta::specta]
pub async fn create_directory(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, IpcError> {
    let (new_path, expanded_path) = create_directory_core(volume_id.clone(), &parent_path, &name).await?;

    // Synthetic diff only works for volumes backed by the local filesystem.
    // Protocol-only volumes (MTP) handle UI updates through their own event systems.
    if should_emit_synthetic_diff(volume_id.as_deref()) {
        emit_synthetic_entry_diff(&new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn create_file(volume_id: Option<String>, parent_path: String, name: String) -> Result<String, IpcError> {
    let (new_path, expanded_path) = create_file_core(volume_id.clone(), &parent_path, &name).await?;

    if should_emit_synthetic_diff(volume_id.as_deref()) {
        emit_synthetic_entry_diff(&new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

/// Core mkdir logic, separated from the Tauri command so it can be tested without `AppHandle`.
pub(super) async fn create_directory_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), IpcError> {
    if name.is_empty() {
        return Err(IpcError::from_err("Folder name cannot be empty"));
    }
    if name.contains('/') || name.contains('\0') {
        return Err(IpcError::from_err("Folder name contains invalid characters"));
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);
        let new_path_clone = new_path.clone();
        let parent_path_owned = parent_path.to_string();
        let name_owned = name.to_string();

        // Register the new directory path with the downloads watcher's
        // ignore set; no-ops for paths outside ~/Downloads.
        crate::downloads::note_pending_write_for_cmdr(&new_path);

        // Use spawn_blocking to run the Volume operation in a context where
        // tokio::runtime::Handle::current() is available (needed for MtpVolume)
        tokio::time::timeout(Duration::from_secs(5), volume.create_directory(&new_path_clone))
            .await
            .map_err(|_| IpcError::timeout())?
            .map_err(|e| match e {
                crate::file_system::VolumeError::AlreadyExists(_) => {
                    IpcError::from_err(format!("'{}' already exists", name_owned))
                }
                crate::file_system::VolumeError::PermissionDenied(_) => IpcError::from_err(format!(
                    "Permission denied: cannot create '{}' in '{}'",
                    name_owned, parent_path_owned
                )),
                _ => IpcError::from_err(format!("Couldn't create folder: {}", e)),
            })?;

        return Ok((new_path, expanded_path));
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let mut new_path = PathBuf::from(&expanded_path);
    new_path.push(name);
    crate::downloads::note_pending_write_for_cmdr(&new_path);
    std::fs::create_dir(&new_path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!("'{}' already exists", name),
            std::io::ErrorKind::PermissionDenied => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create folder: {}", e),
        })
        .map_err(IpcError::from_err)?;

    Ok((new_path, expanded_path))
}

/// Core file creation logic, separated from the Tauri command so it can be tested without
/// `AppHandle`.
pub(super) async fn create_file_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), IpcError> {
    if name.is_empty() {
        return Err(IpcError::from_err("File name cannot be empty"));
    }
    if name.contains('/') || name.contains('\0') {
        return Err(IpcError::from_err("File name contains invalid characters"));
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);
        let new_path_clone = new_path.clone();
        let parent_path_owned = parent_path.to_string();
        let name_owned = name.to_string();

        // Register the new file path with the downloads watcher's ignore
        // set; no-ops for paths outside ~/Downloads.
        crate::downloads::note_pending_write_for_cmdr(&new_path);

        tokio::time::timeout(Duration::from_secs(5), volume.create_file(&new_path_clone, b""))
            .await
            .map_err(|_| IpcError::timeout())?
            .map_err(|e| match e {
                crate::file_system::VolumeError::AlreadyExists(_) => {
                    IpcError::from_err(format!("'{}' already exists", name_owned))
                }
                crate::file_system::VolumeError::PermissionDenied(_) => IpcError::from_err(format!(
                    "Permission denied: cannot create '{}' in '{}'",
                    name_owned, parent_path_owned
                )),
                _ => IpcError::from_err(format!("Couldn't create file: {}", e)),
            })?;

        return Ok((new_path, expanded_path));
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let mut new_path = PathBuf::from(&expanded_path);
    new_path.push(name);
    crate::downloads::note_pending_write_for_cmdr(&new_path);
    std::fs::File::create_new(&new_path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!("'{}' already exists", name),
            std::io::ErrorKind::PermissionDenied => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create file: {}", e),
        })
        .map_err(IpcError::from_err)?;

    Ok((new_path, expanded_path))
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/// Emits write-progress, write-complete, write-error, write-cancelled.
#[tauri::command]
#[specta::specta]
pub async fn copy_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    ops_copy_files_start(app, sources, destination, config).await
}

/// Uses rename() for same-filesystem (instant), copy+delete for cross-filesystem.
/// Same events as `copy_files`.
#[tauri::command]
#[specta::specta]
pub async fn move_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    ops_move_files_start(app, sources, destination, config).await
}

/// Recursively deletes files and directories. Same events as `copy_files`.
/// When `volume_id` is provided and is not "root", routes through the Volume trait.
#[tauri::command]
#[specta::specta]
pub async fn delete_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    volume_id: Option<String>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };
    let config = config.unwrap_or_default();

    ops_delete_files_start(app, sources, config, volume_id).await
}

/// Moves files to macOS Trash. Same events as `copy_files` but with `operationType: trash`.
#[tauri::command]
#[specta::specta]
pub async fn trash_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    ops_trash_files_start(app, sources, item_sizes, config).await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_write_operation(operation_id: String, rollback: bool) {
    ops_cancel_write_operation(&operation_id, rollback);
}

#[tauri::command]
#[specta::specta]
pub fn cancel_all_write_operations() {
    ops_cancel_all_write_operations();
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Scans source files for Copy dialog stats. Results are cached for reuse by the actual copy.
/// Emits scan-preview-progress, scan-preview-complete, scan-preview-error, scan-preview-cancelled.
///
/// When `source_volume_id` is provided and is not "root", the scan uses the Volume trait
/// (enabling MTP and other non-local volumes). Otherwise, uses `std::fs` for local scanning.
#[tauri::command]
#[specta::specta]
pub async fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<String>,
    source_volume_id: Option<String>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: Option<u64>,
) -> ScanPreviewStartResult {
    let volume_id = source_volume_id.unwrap_or_else(|| "root".to_string());
    let is_local = volume_id == "root";

    // Only expand tilde for local paths
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };

    let source_volume = if is_local {
        None
    } else {
        get_volume_manager().get(&volume_id)
    };

    let progress_interval = progress_interval_ms.unwrap_or(500);
    ops_start_scan_preview(
        app,
        sources,
        source_volume,
        volume_id,
        sort_column,
        sort_order,
        progress_interval,
    )
}

#[tauri::command]
#[specta::specta]
pub fn cancel_scan_preview(preview_id: String) {
    ops_cancel_scan_preview(&preview_id);
}

/// Returns the cached totals from a completed scan preview, or `null` while the
/// scan is still running / cancelled / errored. The FE uses the presence of a
/// value both as a "scan done" signal and to repopulate display state when its
/// listeners missed the events (a watcher-backed oracle can finish before the
/// FE finishes the `startScanPreview()` IPC round-trip).
#[tauri::command]
#[specta::specta]
pub fn check_scan_preview_status(
    preview_id: String,
) -> Option<crate::file_system::write_operations::ScanPreviewTotals> {
    ops_get_scan_preview_totals(&preview_id)
}

/// In Stop mode, the operation pauses on conflict and waits for this call to proceed.
#[tauri::command]
#[specta::specta]
pub fn resolve_write_conflict(operation_id: String, resolution: ConflictResolution, apply_to_all: bool) {
    ops_resolve_write_conflict(&operation_id, resolution, apply_to_all);
}

#[tauri::command]
#[specta::specta]
pub fn list_active_operations() -> Vec<OperationSummary> {
    ops_list_active_operations()
}

#[tauri::command]
#[specta::specta]
pub fn get_operation_status(operation_id: String) -> Option<OperationStatus> {
    ops_get_operation_status(&operation_id)
}

// ============================================================================
// Helpers
// ============================================================================

/// Returns true if a synthetic entry diff should be emitted for this volume.
/// Protocol-only volumes (like MTP) don't support `std::fs` access, so synthetic
/// diffs would fail. These volumes handle UI updates through their own event systems.
fn should_emit_synthetic_diff(volume_id: Option<&str>) -> bool {
    match volume_id {
        None => true, // No volume_id means local filesystem
        Some(id) => get_volume_manager()
            .get(id)
            .is_none_or(|v| v.supports_local_fs_access()),
    }
}

/// Queues a synthetic `directory-diff` event for a newly created entry.
///
/// Best-effort: if any step fails (stat, cache lookup, etc.) we log a warning
/// and return. The watcher will pick up the change later.
fn emit_synthetic_entry_diff(entry_path: &Path, parent_path: &Path) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::listing::reading::get_single_entry;
    use crate::file_system::listing::{find_listings_for_path, insert_entry_sorted};
    use crate::file_system::watcher::DiffChange;

    // 1. Construct a FileEntry for the new entry
    let mut entry = match get_single_entry(entry_path) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Synthetic entry diff: couldn't stat new entry: {}", e);
            return;
        }
    };

    // 2. Enrich with index data
    crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));

    // 3. Find affected listings
    let listings = find_listings_for_path(parent_path);
    if listings.is_empty() {
        return;
    }

    // 4. For each listing, insert and enqueue
    for (listing_id, _sort_by, _sort_order, _dir_sort_mode) in listings {
        // insert_entry_sorted acquires LISTING_CACHE write lock and releases it on return
        let Some(index) = insert_entry_sorted(&listing_id, entry.clone()) else {
            continue; // Already exists or listing gone
        };

        enqueue_diff(
            &listing_id,
            vec![DiffChange {
                change_type: "add".to_string(),
                entry: entry.clone(),
                index,
            }],
        );
    }
}
