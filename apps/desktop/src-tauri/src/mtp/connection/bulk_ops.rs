//! Bulk and recursive MTP operations (scan, recursive download/upload).

use log::debug;
use std::path::Path;

use super::errors::MtpConnectionError;
use super::{MtpConnectionManager, normalize_mtp_path};
use crate::file_system::CopyScanResult;

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

        // Try to list the directory - if it fails or returns empty, it might be a file
        let entries = match self.list_directory(device_id, storage_id, path).await {
            Ok(entries) => entries,
            Err(e) => {
                // list_directory failed - this might be because path is a file, not a directory.
                // Try to check by listing the parent directory.
                debug!(
                    "MTP scan_for_copy: list_directory failed for '{}', checking if it's a file: {:?}",
                    path, e
                );
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                // Not a file either, propagate the original error
                return Err(e);
            }
        };

        let mut file_count = 0usize;
        let mut dir_count = 0usize;
        let mut total_bytes = 0u64;

        // If entries is empty, it might be an empty directory OR a file (some MTP devices
        // return empty for files instead of an error)
        if entries.is_empty() {
            if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                return Ok(result);
            }
            // Empty directory
            return Ok(CopyScanResult {
                file_count: 0,
                dir_count: 1,
                total_bytes: 0,
            });
        }

        // Process entries recursively
        for entry in &entries {
            if entry.is_directory {
                dir_count += 1;
                // Recursively scan subdirectory
                let child_result = Box::pin(self.scan_for_copy(device_id, storage_id, &entry.path)).await?;
                file_count += child_result.file_count;
                dir_count += child_result.dir_count;
                total_bytes += child_result.total_bytes;
            } else {
                file_count += 1;
                total_bytes += entry.size.unwrap_or(0);
            }
        }

        debug!(
            "MTP scan_for_copy: {} files, {} dirs, {} bytes for {}",
            file_count, dir_count, total_bytes, path
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
            // It's a directory, not a file - let caller handle it
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

    /// Downloads a file or directory recursively from the MTP device to a local path.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device to download
    /// * `local_dest` - Local destination path
    ///
    /// # Returns
    ///
    /// Total bytes transferred.
    pub async fn download_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        local_dest: &Path,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP download_recursive: device={}, storage={}, path={}, dest={}",
            device_id,
            storage_id,
            object_path,
            local_dest.display()
        );

        // Try to list the path as a directory first
        let entries = self.list_directory(device_id, storage_id, object_path).await;

        match entries {
            Ok(entries) if !entries.is_empty() => {
                // It's a directory with contents - create local directory and download contents
                debug!(
                    "MTP download_recursive: {} is a directory with {} entries",
                    object_path,
                    entries.len()
                );

                tokio::fs::create_dir_all(local_dest)
                    .await
                    .map_err(|e| MtpConnectionError::Other {
                        device_id: device_id.to_string(),
                        message: format!("Failed to create local directory: {}", e),
                    })?;

                let mut total_bytes = 0u64;
                for entry in entries {
                    let child_dest = local_dest.join(&entry.name);
                    let bytes =
                        Box::pin(self.download_recursive(device_id, storage_id, &entry.path, &child_dest)).await?;
                    total_bytes += bytes;
                }

                debug!(
                    "MTP download_recursive: directory {} complete, {} bytes",
                    object_path, total_bytes
                );
                Ok(total_bytes)
            }
            Ok(_) => {
                // Empty directory or file - check if it's a file by checking parent listing
                let path_buf = normalize_mtp_path(object_path);
                let is_file = if let Some(parent) = path_buf.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Ok(parent_entries) = self.list_directory(device_id, storage_id, &parent_str).await {
                        if let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) {
                            parent_entries
                                .iter()
                                .find(|e| e.name == name)
                                .is_some_and(|e| !e.is_directory)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_file {
                    // It's a file - download it
                    debug!("MTP download_recursive: {} is a file, downloading", object_path);
                    let operation_id = format!("download-{}", uuid::Uuid::new_v4());
                    let result = self
                        .download_file(device_id, storage_id, object_path, local_dest, None, &operation_id)
                        .await?;
                    Ok(result.bytes_transferred)
                } else {
                    // Empty directory - create it
                    debug!("MTP download_recursive: {} is an empty directory", object_path);
                    tokio::fs::create_dir_all(local_dest)
                        .await
                        .map_err(|e| MtpConnectionError::Other {
                            device_id: device_id.to_string(),
                            message: format!("Failed to create local directory: {}", e),
                        })?;
                    Ok(0)
                }
            }
            Err(e) => {
                // list_directory failed - might be a file (MTP returns ObjectNotFound when
                // trying to list children of a file). Try to check by listing the parent.
                debug!(
                    "MTP download_recursive: list failed for '{}', checking if it's a file: {:?}",
                    object_path, e
                );

                let path_buf = normalize_mtp_path(object_path);
                let is_file = if let Some(parent) = path_buf.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Ok(parent_entries) = self.list_directory(device_id, storage_id, &parent_str).await {
                        if let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) {
                            parent_entries
                                .iter()
                                .find(|e| e.name == name)
                                .is_some_and(|entry| !entry.is_directory)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_file {
                    debug!("MTP download_recursive: {} is a file, downloading", object_path);
                    let operation_id = format!("download-{}", uuid::Uuid::new_v4());
                    let result = self
                        .download_file(device_id, storage_id, object_path, local_dest, None, &operation_id)
                        .await?;
                    Ok(result.bytes_transferred)
                } else {
                    // Not a file, propagate the original error
                    Err(e)
                }
            }
        }
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
