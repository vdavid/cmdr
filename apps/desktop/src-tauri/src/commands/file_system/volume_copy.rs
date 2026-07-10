//! Tauri commands for cross-volume copy/move operations.

use crate::file_system::Volume;
use crate::file_system::{
    OperationEventSink, ScanConflict, TauriEventSink, VolumeCopyConfig, VolumeCopyScanResult, WriteOperationError,
    WriteOperationStartResult, compress_start as ops_compress_start, copy_between_volumes as ops_copy_between_volumes,
    get_volume_manager, move_between_volumes as ops_move_between_volumes,
    route_archive_copy_into as ops_route_archive_copy_into, scan_for_volume_copy as ops_scan_for_volume_copy,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::Duration;

use crate::commands::util::IpcError;
use crate::file_system::volume::backends::archive;
use crate::operation_log::types::Initiator;

/// Expands a leading `~` in the destination path when the destination is a local
/// volume. The transfer dialog accepts the home shortcut (`~`, `~/…`) in its
/// destination box; MTP and network volumes never use `~`, so their paths pass
/// through untouched (per the "never tilde-expand MTP/network paths" rule).
fn expand_local_dest(dest_volume: &Arc<dyn Volume>, dest_path: String) -> PathBuf {
    if dest_volume.local_path().is_some() {
        PathBuf::from(super::expand_tilde(&dest_path))
    } else {
        PathBuf::from(dest_path)
    }
}

/// Resolves a batch's source volume, routing a source INSIDE an archive to its
/// `ArchiveVolume` (extract-out is a supported source). One `source_volume_id`
/// per batch means no straddle risk — every path shares the same archive or none
/// — so the first path decides. The `bool` is "source is inside an archive": the
/// `.zip` file itself is a plain file (copied/moved as a file, via its parent
/// volume), so only a genuinely-inner source flips it true.
async fn resolve_source(volume_id: &str, first_path: Option<&PathBuf>) -> Option<(Arc<dyn Volume>, bool)> {
    let manager = get_volume_manager();
    let Some(path) = first_path else {
        return manager.get(volume_id).map(|v| (v, false));
    };
    // Only a non-empty inner component can be archive-inner; the `.zip` file itself
    // (empty inner) is a plain file copied via its parent volume. This is a pure
    // string pre-filter, so a plain local/remote path skips the resolve below.
    let is_inner_candidate =
        archive::archive_boundary_candidate(path).is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty());
    if !is_inner_candidate {
        return manager.get(volume_id).map(|v| (v, false));
    }
    // Parent-aware resolve (local `std::fs` OR remote via the parent's own I/O):
    // a confirmed archive routes to the `ArchiveVolume` (extract-out) with
    // `is_inside = true`; a mislabeled `.zip` degrades to the parent, `false`.
    let resolved = manager.resolve(volume_id, path).await;
    let is_inside = resolved.is_archive;
    resolved
        .volume
        .or_else(|| manager.get(volume_id))
        .map(|v| (v, is_inside))
}

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
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();

    // Route an archive-inner source batch to its ArchiveVolume (extract-out).
    let (source_volume, _source_is_archive) = resolve_source(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    // Resolve the destination. A `.zip`-crossing dest routes the whole copy to
    // the managed archive-edit driver (one `{ add }` changeset).
    let dest_resolved = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await;
    let dest_volume = dest_resolved.volume.ok_or_else(|| WriteOperationError::IoError {
        path: dest_volume_id.clone(),
        message: format!("Destination volume '{}' not found", dest_volume_id),
    })?;
    let config = config.unwrap_or_default();
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));

    if dest_resolved.is_archive {
        return ops_route_archive_copy_into(
            events,
            source_volume,
            source_paths,
            PathBuf::from(&dest_path),
            dest_volume_id,
            config.conflict_resolution,
            config.progress_interval_ms,
            false,
            config.compression_level,
        )
        .await;
    }

    let dest_path = expand_local_dest(&dest_volume, dest_path);
    ops_copy_between_volumes(
        events,
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
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();

    // An archive SOURCE routes to the compound move-out op (extract via the copy
    // engine, then a batch `{ delete }` archive rewrite once the extract lands).
    let (source_volume, source_is_archive) = resolve_source(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    let dest_resolved = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await;
    let dest_volume = dest_resolved.volume.ok_or_else(|| WriteOperationError::IoError {
        path: dest_volume_id.clone(),
        message: format!("Destination volume '{}' not found", dest_volume_id),
    })?;
    let config = config.unwrap_or_default();
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));

    // Move OUT of a zip. Takes precedence over the dest-archive branch: a
    // zip→zip move extracts out first (the dest-archive case degrades to a copy
    // failure inside the extract, never data loss).
    if source_is_archive {
        let dest_path = expand_local_dest(&dest_volume, dest_path);
        return crate::file_system::route_archive_move_out(
            events,
            source_volume_id,
            source_volume,
            source_paths,
            dest_volume_id,
            dest_volume,
            dest_path,
            config,
        )
        .await;
    }

    // A move INTO a zip routes to the managed edit driver as one `{ add }`
    // changeset; the local sources are deleted after the commit (move invariant).
    if dest_resolved.is_archive {
        return ops_route_archive_copy_into(
            events,
            source_volume,
            source_paths,
            PathBuf::from(&dest_path),
            dest_volume_id,
            config.conflict_resolution,
            config.progress_interval_ms,
            true,
            config.compression_level,
        )
        .await;
    }

    let dest_path = expand_local_dest(&dest_volume, dest_path);
    ops_move_between_volumes(
        events,
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

/// Compresses `source_paths` into a NEW zip at `dest_zip_path` on `dest_volume_id`.
/// Reuses the archive-edit machinery: seed a valid empty zip, then copy the sources
/// in as one changeset (`compress_start`). Same events as `copy_between_volumes`.
/// The destination may be LOCAL or REMOTE (SMB/MTP): `compress_start` seeds a local
/// target on the FS and a remote one THROUGH the parent volume.
#[tauri::command]
#[specta::specta]
pub async fn compress_files(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_zip_path: String,
    config: Option<VolumeCopyConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();

    // Route an archive-inner source batch to its ArchiveVolume (compress-from-zip).
    let (source_volume, _source_is_archive) = resolve_source(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    // The new `.zip` doesn't exist yet, so `resolve` returns the PARENT drive volume
    // (`is_archive = false` for a non-existent path) — the drive the seed is written
    // to. `compress_start` bypasses the archive-boundary resolve on its own.
    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_zip_path))
        .await
        .volume
        .ok_or_else(|| WriteOperationError::IoError {
            path: dest_volume_id.clone(),
            message: format!("Destination volume '{}' not found", dest_volume_id),
        })?;

    let config = config.unwrap_or_default();
    let dest_zip_path = expand_local_dest(&dest_volume, dest_zip_path);
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));

    ops_compress_start(
        events,
        source_volume,
        source_paths,
        dest_zip_path,
        dest_volume_id,
        config.conflict_resolution,
        config.progress_interval_ms,
        config.compression_level,
        // Initiator threading through the volume commands lands with the
        // provenance-completion pass; defaults to `user` here.
        Initiator::User,
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
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);

    // Resolve both so an archive-inner source scans through its ArchiveVolume
    // (sizing an extract-out) and the dest routes consistently with the copy op.
    let (source_volume, _) = resolve_source(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| IpcError::from_err(format!("Source volume '{}' not found", source_volume_id)))?;

    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, &dest_path)
        .await
        .volume
        .ok_or_else(|| IpcError::from_err(format!("Destination volume '{}' not found", dest_volume_id)))?;

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
    let dest_path = PathBuf::from(dest_path);

    // Resolve the destination so a conflict scan against an archive-inner dest
    // routes to its ArchiveVolume (consistent with the copy op's routing).
    let volume = get_volume_manager()
        .resolve(&volume_id, &dest_path)
        .await
        .volume
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
    // caller supplied it. One batched stat, O(top-level items). `resolve_source`
    // routes an archive-inner source through its ArchiveVolume.
    if let (Some(src_volume_id), Some(src_paths)) = (source_volume_id, source_paths) {
        let paths: Vec<PathBuf> = src_paths.iter().map(PathBuf::from).collect();
        if let Some((src_volume, _)) = resolve_source(&src_volume_id, paths.first()).await {
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
    }

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
    use super::{merge_source_types_from_batch, resolve_source};
    use crate::file_system::{BatchScanResult, CopyScanResult, SourceItemInfo};
    use std::path::PathBuf;

    #[tokio::test]
    async fn resolve_source_treats_the_zip_file_itself_as_a_plain_file() {
        use crate::file_system::get_volume_manager;
        use crate::file_system::volume::LocalPosixVolume;
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");
        // The parent drive holds the `.zip`. (nextest isolates the global per test.)
        get_volume_manager().register(
            "root",
            Arc::new(LocalPosixVolume::new("Root", dir.path().to_str().unwrap())),
        );

        // The `.zip` FILE itself is copied as a plain file: routed to the PARENT
        // volume, `is_inside = false` (NOT the ArchiveVolume, which would scan its
        // contents instead of copying the file).
        let (vol, is_inside) = resolve_source("root", Some(&zip)).await.expect("source volume");
        assert!(!is_inside, "the .zip file itself is not archive-inner");
        assert_eq!(vol.name(), "Root", "routed to the parent volume, not the archive");

        // A path INSIDE the archive routes to the ArchiveVolume, is_inside = true.
        let (inner_vol, inner_is_inside) = resolve_source("root", Some(&zip.join("entry.txt")))
            .await
            .expect("inner volume");
        assert!(inner_is_inside, "an inner path is archive-inner");
        assert_eq!(inner_vol.root(), zip, "the archive volume's root is the .zip");
    }

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
