//! Bulk and recursive MTP operations (scan, recursive download/upload).

use log::debug;
use std::path::Path;

use super::errors::MtpConnectionError;
use super::{MtpConnectionManager, normalize_mtp_path};
use crate::file_system::CopyScanResult;
use crate::file_system::listing::FileEntry;

impl MtpConnectionManager {
    /// Scans an MTP path recursively to get statistics for a copy operation.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path on the device to scan
    ///
    /// # Returns
    ///
    /// Statistics including file count, directory count, and total bytes.
    pub async fn scan_for_copy(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        debug!(
            "MTP scan_for_copy: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Try to list the path as a directory
        match self.list_directory(device_id, storage_id, path).await {
            Ok(entries) if !entries.is_empty() => {
                // Directory with contents — recurse using entries directly
                self.scan_entries_recursive(device_id, storage_id, entries).await
            }
            Ok(_) => {
                // Empty result: either an empty directory or a file (some MTP devices
                // return empty for files instead of an error). Check parent to disambiguate.
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                // Empty directory
                Ok(CopyScanResult {
                    file_count: 0,
                    dir_count: 1,
                    total_bytes: 0,
                })
            }
            Err(e) => {
                // list_directory failed — likely because path is a file, not a directory.
                debug!(
                    "MTP scan_for_copy: list_directory failed for '{}', checking if it's a file: {:?}",
                    path, e
                );
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                Err(e)
            }
        }
    }

    /// Recursively accumulates scan stats from a list of directory entries.
    ///
    /// Unlike `scan_for_copy`, this function takes entries that are already known
    /// from a parent listing. For file entries, it counts them directly without
    /// any USB calls. For directory entries, it lists their contents (one USB call
    /// per directory) and recurses.
    ///
    /// This ensures exactly one `list_directory` call per directory in the tree,
    /// with zero calls for files.
    async fn scan_entries_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        entries: Vec<FileEntry>,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        let mut file_count = 0usize;
        let mut dir_count = 0usize;
        let mut total_bytes = 0u64;

        for entry in &entries {
            if entry.is_directory {
                dir_count += 1;
                // One list_directory call per subdirectory
                let children = self.list_directory(device_id, storage_id, &entry.path).await?;
                if !children.is_empty() {
                    let child_result = Box::pin(self.scan_entries_recursive(device_id, storage_id, children)).await?;
                    file_count += child_result.file_count;
                    dir_count += child_result.dir_count;
                    total_bytes += child_result.total_bytes;
                }
            } else {
                file_count += 1;
                total_bytes += entry.size.unwrap_or(0);
            }
        }

        debug!(
            "MTP scan_entries_recursive: {} files, {} dirs, {} bytes",
            file_count, dir_count, total_bytes
        );

        Ok(CopyScanResult {
            file_count,
            dir_count,
            total_bytes,
        })
    }

    /// Helper to check if a path is a file by listing its parent directory.
    /// Returns Some(CopyScanResult) if path is a file, None otherwise.
    async fn try_scan_as_file(&self, device_id: &str, storage_id: u32, path: &str) -> Option<CopyScanResult> {
        let path_buf = normalize_mtp_path(path);
        let parent = path_buf.parent()?;
        let name = path_buf.file_name()?.to_str()?;

        let parent_entries = self
            .list_directory(device_id, storage_id, &parent.to_string_lossy())
            .await
            .ok()?;

        let entry = parent_entries.iter().find(|e| e.name == name)?;

        if entry.is_directory {
            // It's a directory, not a file — let caller handle it
            return None;
        }

        debug!(
            "MTP scan_for_copy: path '{}' is a file with size {}",
            path,
            entry.size.unwrap_or(0)
        );

        Some(CopyScanResult {
            file_count: 1,
            dir_count: 0,
            total_bytes: entry.size.unwrap_or(0),
        })
    }

    /// Uploads a file or directory from local filesystem to MTP device recursively.
    ///
    /// If the source is a directory, creates the directory on the device and
    /// recursively uploads all contents.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `local_source` - Local source path (file or directory)
    /// * `dest_folder` - Destination folder path on device
    ///
    /// # Returns
    ///
    /// Total bytes transferred.
    ///
    /// Currently unused since `MtpVolume::import_from_local` was removed
    /// in Phase 4; cross-volume copies now stream via `write_from_stream`.
    /// Kept as a reference for future batch-upload work.
    #[allow(dead_code, reason = "Retained for future batch-upload API — see Phase 4 notes")]
    pub async fn upload_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        local_source: &Path,
        dest_folder: &str,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP upload_recursive: device={}, storage={}, source={}, dest={}",
            device_id,
            storage_id,
            local_source.display(),
            dest_folder
        );

        let metadata = tokio::fs::metadata(local_source)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local path: {}", e),
            })?;

        if metadata.is_file() {
            // Upload single file
            let operation_id = format!("upload-{}", uuid::Uuid::new_v4());
            let result = self
                .upload_file(device_id, storage_id, local_source, dest_folder, None, &operation_id)
                .await?;
            Ok(result.size.unwrap_or(0))
        } else if metadata.is_dir() {
            // Create directory on device and upload contents
            let dir_name = local_source
                .file_name()
                .ok_or_else(|| MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "Invalid directory path".to_string(),
                })?
                .to_string_lossy()
                .to_string();

            // Create the directory on the device
            let new_folder = self
                .create_folder(device_id, storage_id, dest_folder, &dir_name)
                .await?;
            let new_folder_path = new_folder.path;

            // Upload all contents
            let mut total_bytes = 0u64;
            let mut entries = tokio::fs::read_dir(local_source)
                .await
                .map_err(|e| MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("Failed to read local directory: {}", e),
                })?;

            while let Some(entry) = entries.next_entry().await.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read directory entry: {}", e),
            })? {
                let entry_path = entry.path();
                let bytes =
                    Box::pin(self.upload_recursive(device_id, storage_id, &entry_path, &new_folder_path)).await?;
                total_bytes += bytes;
            }

            debug!(
                "MTP upload_recursive: directory {} complete, {} bytes",
                local_source.display(),
                total_bytes
            );
            Ok(total_bytes)
        } else {
            // Not a file or directory (symlink, etc.) - skip
            debug!(
                "MTP upload_recursive: skipping non-file/non-directory: {}",
                local_source.display()
            );
            Ok(0)
        }
    }
}
