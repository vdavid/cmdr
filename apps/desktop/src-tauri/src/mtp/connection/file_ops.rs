//! MTP file transfer operations (download, upload, and streaming).

use log::{debug, info};
use mtp_rs::{NewObjectInfo, ObjectHandle, StorageId};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::errors::{MtpConnectionError, map_mtp_error};
use super::{
    MTP_TIMEOUT_SECS, MtpConnectionManager, MtpObjectInfo, MtpOperationResult, MtpTransferProgress, MtpTransferType,
    acquire_device_lock, normalize_mtp_path,
};

impl MtpConnectionManager {
    /// Downloads a file from the MTP device to a local path.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device (for example, "/DCIM/photo.jpg")
    /// * `local_dest` - Local destination path
    /// * `app` - Optional app handle for emitting progress events
    /// * `operation_id` - Unique operation ID for progress tracking
    pub async fn download_file(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        local_dest: &Path,
        app: Option<&AppHandle>,
        operation_id: &str,
    ) -> Result<MtpOperationResult, MtpConnectionError> {
        debug!(
            "MTP download_file: device={}, storage={}, path={}, dest={}",
            device_id,
            storage_id,
            object_path,
            local_dest.display()
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to handle
            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "download_file").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine size
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let total_size = object_info.size;
        let filename = object_info.filename.clone();

        // Emit initial progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Download,
                    current_file: filename.clone(),
                    bytes_done: 0,
                    bytes_total: total_size,
                },
            );
        }

        // Download the file as a stream (holds session lock until complete)
        let mut download = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.download_stream(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create the local file
        let mut file = tokio::fs::File::create(local_dest)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to create local file: {}", e),
            })?;

        // Write chunks to file (must complete before releasing device lock)
        let mut bytes_written = 0u64;
        while let Some(chunk_result) = download.next_chunk().await {
            let chunk = chunk_result.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Download error: {}", e),
            })?;

            file.write_all(&chunk).await.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to write local file: {}", e),
            })?;

            bytes_written += chunk.len() as u64;
        }

        // Release device lock after download completes
        drop(storage);
        drop(device);

        file.flush().await.map_err(|e| MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Failed to flush local file: {}", e),
        })?;

        // Emit completion progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Download,
                    current_file: filename,
                    bytes_done: bytes_written,
                    bytes_total: total_size,
                },
            );
        }

        info!(
            "MTP download complete: {} bytes to {}",
            bytes_written,
            local_dest.display()
        );

        Ok(MtpOperationResult {
            operation_id: operation_id.to_string(),
            files_processed: 1,
            bytes_transferred: bytes_written,
        })
    }

    /// Uploads a file from the local filesystem to the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `local_path` - Local file path to upload
    /// * `dest_folder` - Destination folder path on device (for example, "/DCIM")
    /// * `app` - Optional app handle for emitting progress events
    /// * `operation_id` - Unique operation ID for progress tracking
    pub async fn upload_file(
        &self,
        device_id: &str,
        storage_id: u32,
        local_path: &Path,
        dest_folder: &str,
        app: Option<&AppHandle>,
        operation_id: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        debug!(
            "MTP upload_file: device={}, storage={}, local={}, dest={}",
            device_id,
            storage_id,
            local_path.display(),
            dest_folder
        );

        // Get file metadata
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local file metadata: {}", e),
            })?;

        if metadata.is_dir() {
            return Err(MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: "Cannot upload directories with upload_file. Use create_folder instead.".to_string(),
            });
        }

        let file_size = metadata.len();
        let filename = local_path
            .file_name()
            .ok_or_else(|| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: "Invalid file path".to_string(),
            })?
            .to_string_lossy()
            .to_string();

        // Read the file data
        let data = tokio::fs::read(local_path)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local file: {}", e),
            })?;

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = self.resolve_path_to_handle(entry, storage_id, dest_folder)?;
            (Arc::clone(&entry.device), parent)
        };

        // Emit initial progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Upload,
                    current_file: filename.clone(),
                    bytes_done: 0,
                    bytes_total: file_size,
                },
            );
        }

        let device = acquire_device_lock(&device_arc, device_id, "upload_file").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create object info for the upload (format is auto-detected from filename)
        let object_info = NewObjectInfo::file(&filename, file_size);

        // Upload the file - create a stream from the data
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        // Create a single-chunk stream from the data
        // Using iter instead of once because iter's items are ready, making it Unpin
        let data_stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(bytes::Bytes::from(data))]);

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Build the new object path
        let new_path = normalize_mtp_path(dest_folder).join(&filename);
        let new_path_str = new_path.to_string_lossy().to_string();

        // Update path cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Emit completion progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Upload,
                    current_file: filename.clone(),
                    bytes_done: file_size,
                    bytes_total: file_size,
                },
            );
        }

        info!("MTP upload complete: {} -> {}", local_path.display(), new_path_str);

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        Ok(MtpObjectInfo {
            handle: new_handle.0,
            name: filename,
            path: new_path_str,
            is_directory: false,
            size: Some(file_size),
        })
    }

    /// Opens a streaming download for a file.
    ///
    /// Returns the FileDownload stream and the file size.
    /// The caller must consume the entire stream before releasing it.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path on the device (e.g., "DCIM/photo.jpg")
    pub async fn open_download_stream(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<(mtp_rs::FileDownload, u64), MtpConnectionError> {
        debug!(
            "MTP open_download_stream: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "open_download_stream").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine size
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let total_size = object_info.size;

        // Open the download stream
        let download = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.download_stream(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Note: We intentionally don't drop 'storage' and 'device' here.
        // The FileDownload holds a reference to the storage session internally.
        // The caller must consume the entire download before other operations.
        // This is a design limitation of the current mtp-rs streaming API.
        // In practice, the Volume trait methods run in spawn_blocking, so
        // the device lock is released when the blocking task completes.

        debug!("MTP open_download_stream: stream opened for {} bytes", total_size);

        Ok((download, total_size))
    }

    /// Uploads pre-collected chunks to the MTP device.
    ///
    /// This variant takes already-collected chunks instead of a stream reference,
    /// avoiding nested `block_on` issues when the stream uses `block_on` internally.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `dest_folder` - Destination folder path on device (e.g., "DCIM")
    /// * `filename` - Name for the new file
    /// * `size` - Total size in bytes
    /// * `chunks` - Pre-collected data chunks
    pub async fn upload_from_chunks(
        &self,
        device_id: &str,
        storage_id: u32,
        dest_folder: &str,
        filename: &str,
        size: u64,
        chunks: Vec<bytes::Bytes>,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP upload_from_chunks: device={}, storage={}, dest={}/{}, size={}, chunks={}",
            device_id,
            storage_id,
            dest_folder,
            filename,
            size,
            chunks.len()
        );

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = if dest_folder.is_empty() {
                ObjectHandle::ROOT
            } else {
                self.resolve_path_to_handle(entry, storage_id, dest_folder)?
            };
            (Arc::clone(&entry.device), parent)
        };

        let device = acquire_device_lock(&device_arc, device_id, "upload_from_chunks").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create object info for the upload
        let object_info = NewObjectInfo::file(filename, size);

        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        // Convert chunks to stream format expected by mtp-rs
        let chunk_results: Vec<Result<bytes::Bytes, std::io::Error>> = chunks.into_iter().map(Ok).collect();
        let data_stream = futures_util::stream::iter(chunk_results);

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Update path cache
        let new_path = normalize_mtp_path(dest_folder).join(filename);
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        info!(
            "MTP upload_from_chunks complete: {} bytes to {}/{}",
            size, dest_folder, filename
        );

        Ok(size)
    }
}
