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
#[specta::specta]
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

    ops_copy_between_volumes(
        app,
        source_volume_id,
        source_volume,
        source_paths,
        dest_volume_id,
        dest_volume,
        dest_path,
        config,
    )
    .await
}

/// Unified move across volume types. Same events as `copy_between_volumes`.
/// Handles same-volume (native rename/move), both-local (native move), and cross-volume
/// (copy+delete).
#[tauri::command]
#[specta::specta]
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

    ops_move_between_volumes(
        app,
        source_volume_id,
        source_volume,
        source_paths,
        dest_volume_id,
        dest_volume,
        dest_path,
        config,
    )
    .await
}

/// Pre-flight scan: total count/bytes, available space, conflicts. Doesn't copy anything.
#[tauri::command]
#[specta::specta]
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
///
/// When `source_volume_id` and `source_paths` are both provided, each item's
/// `is_directory` and `size` are resolved authoritatively on the source volume
/// via ONE batched stat (`scan_for_copy_batch`, strictly O(top-level items),
/// never a subtree walk), overriding whatever the caller passed in `source_items`.
/// This lets the dialog classify dir-vs-dir collisions as silent merges without
/// the FE having to plumb per-item types. Callers that don't pass the source
/// volume keep the legacy name-only behavior.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, IpcError> {
    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume '{}' not found", volume_id)))?;

    let mut source_items: Vec<crate::file_system::SourceItemInfo> = source_items
        .into_iter()
        .map(|item| crate::file_system::SourceItemInfo {
            name: item.name,
            size: item.size,
            modified: item.modified,
            is_directory: item.is_directory,
        })
        .collect();

    // Resolve real per-item types and sizes from the source volume when the
    // caller supplied it. One batched stat, O(top-level items).
    if let (Some(src_volume_id), Some(src_paths)) = (source_volume_id, source_paths)
        && let Some(src_volume) = get_volume_manager().get(&src_volume_id) {
            let paths: Vec<PathBuf> = src_paths.iter().map(PathBuf::from).collect();
            match tokio::time::timeout(Duration::from_secs(30), src_volume.scan_for_copy_batch(&paths)).await {
                Ok(Ok(batch)) => merge_source_types_from_batch(&mut source_items, &batch),
                // A failed source-side stat is non-fatal: fall back to the
                // name-only items the caller sent. Conflict detection still
                // works by name; only the dir/size hints degrade.
                Ok(Err(e)) => {
                    log::debug!(target: "conflict_scan", "Source batch stat failed, using name-only items: {}", e);
                }
                Err(_) => {
                    log::debug!(target: "conflict_scan", "Source batch stat timed out, using name-only items");
                }
            }
        }

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

/// Overlays authoritative `is_directory` + `size` from a source-volume batch
/// stat onto the caller-supplied `source_items`, matched by base filename.
///
/// The match key is the path's final component, which is exactly the `name`
/// the FE derives for each `SourceItemInput`. An item with no batch hit keeps
/// the values the caller sent (the safe fallback). For a top-level directory
/// the batch's `total_bytes` is the recursive size, which we deliberately do
/// NOT copy into `size` — a directory's conflict-UI size is meaningless and the
/// dir-dir case never renders a size. Only files get their real size.
fn merge_source_types_from_batch(
    source_items: &mut [crate::file_system::SourceItemInfo],
    batch: &crate::file_system::BatchScanResult,
) {
    use std::collections::HashMap;
    let by_name: HashMap<&str, &crate::file_system::CopyScanResult> = batch
        .per_path
        .iter()
        .filter_map(|(path, scan)| path.file_name().and_then(|n| n.to_str()).map(|n| (n, scan)))
        .collect();
    for item in source_items.iter_mut() {
        if let Some(scan) = by_name.get(item.name.as_str()) {
            item.is_directory = scan.top_level_is_directory;
            if !scan.top_level_is_directory {
                item.size = scan.total_bytes;
            }
        }
    }
}

/// Input type for source item information (used by scan_volume_for_conflicts).
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceItemInput {
    /// File/directory name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix timestamp in seconds).
    pub modified: Option<i64>,
    /// `true` when the source item is a directory. The FE has this from the
    /// `FileEntry` it already holds; it lets `scan_for_conflicts` flag a
    /// dir-vs-dir collision the FE can classify as a silent merge.
    #[serde(default)]
    pub is_directory: bool,
}

#[cfg(test)]
mod tests {
    use super::merge_source_types_from_batch;
    use crate::file_system::{BatchScanResult, CopyScanResult, SourceItemInfo};
    use std::path::PathBuf;

    fn scan(is_dir: bool, bytes: u64) -> CopyScanResult {
        CopyScanResult {
            file_count: if is_dir { 0 } else { 1 },
            dir_count: if is_dir { 1 } else { 0 },
            total_bytes: bytes,
            dedup_bytes: bytes,
            top_level_is_directory: is_dir,
        }
    }

    fn item(name: &str) -> SourceItemInfo {
        SourceItemInfo {
            name: name.to_string(),
            size: 0,
            modified: None,
            is_directory: false,
        }
    }

    #[test]
    fn overlays_real_directory_flag_onto_placeholder_items() {
        let mut items = vec![item("photos"), item("readme.txt")];
        let batch = BatchScanResult {
            aggregate: scan(false, 0),
            per_path: vec![
                (PathBuf::from("/src/photos"), scan(true, 999_999)),
                (PathBuf::from("/src/readme.txt"), scan(false, 42)),
            ],
        };

        merge_source_types_from_batch(&mut items, &batch);

        // The directory item is now flagged as such; its recursive byte total
        // is deliberately NOT copied into `size` (a dir's conflict size is
        // meaningless).
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 0);
        // The file item gets its real size.
        assert!(!items[1].is_directory);
        assert_eq!(items[1].size, 42);
    }

    #[test]
    fn keeps_caller_values_when_no_batch_hit() {
        let mut items = vec![SourceItemInfo {
            name: "ghost".to_string(),
            size: 7,
            modified: Some(123),
            is_directory: true,
        }];
        let batch = BatchScanResult {
            aggregate: scan(false, 0),
            per_path: vec![(PathBuf::from("/src/other"), scan(false, 1))],
        };

        merge_source_types_from_batch(&mut items, &batch);

        // No matching name → the caller's values survive untouched.
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 7);
        assert_eq!(items[0].modified, Some(123));
    }
}
