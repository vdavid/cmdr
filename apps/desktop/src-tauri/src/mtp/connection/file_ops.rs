//! MTP file transfer operations (download, upload, and streaming).

use log::debug;
use mtp_rs::{NewObjectInfo, ObjectHandle, StorageId};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::io::AsyncWriteExt;

use super::errors::{MtpConnectionError, is_stale_handle_rejection, map_mtp_error};
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
        self.download_file_with_progress(device_id, storage_id, object_path, local_dest, app, operation_id, None)
            .await
    }

    /// Downloads a file from the MTP device to a local path, with optional progress/cancellation
    /// callback.
    ///
    /// The `on_progress` callback receives `(bytes_done, bytes_total)` and returns
    /// `ControlFlow::Break(())` to cancel the transfer. On cancellation, the partial file is
    /// removed.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors download_file with added on_progress callback"
    )]
    pub async fn download_file_with_progress(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        local_dest: &Path,
        app: Option<&AppHandle>,
        operation_id: &str,
        on_progress: Option<&(dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Send + Sync)>,
    ) -> Result<MtpOperationResult, MtpConnectionError> {
        // Foreground priority: a user download preempts the background scan for
        // the whole transfer (the user is actively copying; the scan defers).
        let _fg = self.foreground_guard(device_id).await;

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
            let _ = MtpTransferProgress {
                operation_id: operation_id.to_string(),
                device_id: device_id.to_string(),
                transfer_type: MtpTransferType::Download,
                current_file: filename.clone(),
                bytes_done: 0,
                bytes_total: total_size,
            }
            .emit(app);
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

        // Write chunks to file, checking for cancellation between chunks.
        // On cancel: use mtp-rs's USB SIC cancel to abort the transfer cleanly (~300ms).
        let mut bytes_written = 0u64;
        let mut cancelled = false;
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

            // Report progress and check for cancellation
            if let Some(ref cb) = on_progress
                && cb(bytes_written, total_size).is_break()
            {
                cancelled = true;
                break;
            }
        }

        // On cancellation, abort the USB transfer cleanly before releasing the lock
        if cancelled {
            let _ = download.cancel(mtp_rs::DEFAULT_CANCEL_TIMEOUT).await;
        }

        // Release device lock after download completes (or is cancelled)
        drop(download);
        drop(storage);
        drop(device);

        // On cancellation, clean up the partial file
        if cancelled {
            drop(file);
            let _ = tokio::fs::remove_file(local_dest).await;
            return Err(MtpConnectionError::Cancelled {
                device_id: device_id.to_string(),
                message: "Download cancelled".to_string(),
            });
        }

        file.flush().await.map_err(|e| MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Failed to flush local file: {}", e),
        })?;

        // Emit completion progress
        if let Some(app) = app {
            let _ = MtpTransferProgress {
                operation_id: operation_id.to_string(),
                device_id: device_id.to_string(),
                transfer_type: MtpTransferType::Download,
                current_file: filename,
                bytes_done: bytes_written,
                bytes_total: total_size,
            }
            .emit(app);
        }

        debug!(
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
        // Foreground priority: a user upload preempts the background scan.
        let _fg = self.foreground_guard(device_id).await;

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
            let _ = MtpTransferProgress {
                operation_id: operation_id.to_string(),
                device_id: device_id.to_string(),
                transfer_type: MtpTransferType::Upload,
                current_file: filename.clone(),
                bytes_done: 0,
                bytes_total: file_size,
            }
            .emit(app);
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

        let upload_result = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?;

        let new_handle = match upload_result {
            Ok(handle) => handle,
            Err(upload_err) => {
                // See `upload_from_stream` for the full rationale: mtp-rs
                // surfaces a created-but-incomplete object as `partial` and does
                // NOT auto-delete it. cmdr best-effort deletes it so no corrupt
                // artifact lingers, then surfaces the original (mapped) error.
                // A failed delete (e.g. dead device) never masks the upload
                // error.
                if let Some(partial) = upload_err.partial {
                    if let Err(delete_err) = storage.delete(partial).await {
                        log::warn!(
                            target: "mtp_upload",
                            "Failed to delete partial object after upload error (device={device_id}, dest={dest_folder}/{filename}): {delete_err}"
                        );
                    } else {
                        log::debug!(
                            target: "mtp_upload",
                            "Deleted partial object after upload error (device={device_id}, dest={dest_folder}/{filename})"
                        );
                    }
                }
                return Err(map_mtp_error(upload_err.source, device_id));
            }
        };

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
            let _ = MtpTransferProgress {
                operation_id: operation_id.to_string(),
                device_id: device_id.to_string(),
                transfer_type: MtpTransferType::Upload,
                current_file: filename.clone(),
                bytes_done: file_size,
                bytes_total: file_size,
            }
            .emit(app);
        }

        debug!("MTP upload complete: {} -> {}", local_path.display(), new_path_str);

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
    /// * `path` - Virtual path on the device (like "DCIM/photo.jpg")
    pub async fn open_download_stream(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<(mtp_rs::FileDownload, u64), MtpConnectionError> {
        // Foreground priority for the stream SETUP (handle resolve + open). The
        // returned `FileDownload` reads chunks via mtp-rs's own `Arc`/operation
        // lock, not Cmdr's device lock, so per-chunk reads interleave with scan
        // units at mtp-rs's per-transaction granularity (no 30 s starvation). See
        // DETAILS § "Foreground-priority device scheduler".
        let _fg = self.foreground_guard(device_id).await;

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
    /// * `dest_folder` - Destination folder path on device (like "DCIM")
    /// * `filename` - Name for the new file
    /// * `size` - Total size in bytes
    /// * `data_stream` - Chunk stream that mtp-rs consumes lazily as the USB
    ///   transfer drains it. Don't pre-collect the source into a `Vec`; the
    ///   point of the stream is to keep the working set bounded for huge files.
    pub async fn upload_from_stream<S>(
        &self,
        device_id: &str,
        storage_id: u32,
        dest_folder: &str,
        filename: &str,
        size: u64,
        data_stream: S,
    ) -> Result<u64, MtpConnectionError>
    where
        S: futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin + Send,
    {
        // Foreground priority for the whole upload: mtp-rs drains `data_stream`
        // within this call, so the guard covers the entire transfer (and the
        // nested `refresh_dir_handle` re-list, which takes its own guard).
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP upload_from_stream: device={}, storage={}, dest={}/{}, size={}",
            device_id, storage_id, dest_folder, filename, size,
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

        let device = acquire_device_lock(&device_arc, device_id, "upload_from_stream").await?;

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

        let upload_result = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?;

        let new_handle = match upload_result {
            Ok(handle) => handle,
            Err(upload_err) => {
                // PTP uploads are two-phase: SendObjectInfo creates the object,
                // then SendObject streams the data. When the data phase fails
                // (genuine error OR user cancel), mtp-rs surfaces the created
                // object as `upload_err.partial` instead of auto-deleting it —
                // the caller owns the cleanup-or-resume decision. cmdr's
                // no-corrupt-artifact policy: best-effort delete the partial so
                // no half-file lingers on the user's phone. This covers the
                // cancel path too: `source` is `Error::Cancelled` and `partial`
                // is `Some`, so a cancelled upload also gets cleaned up, then
                // maps back to `MtpConnectionError::Cancelled` below (cancel
                // classification preserved).
                if let Some(partial) = upload_err.partial {
                    // Best-effort: a failed delete must NOT mask the upload
                    // error. The delete needs a live device/session; if the
                    // device just disconnected, this fails and the partial
                    // lingers (recognizable, nothing we can do with a dead
                    // device) — we log and move on.
                    if let Err(delete_err) = storage.delete(partial).await {
                        log::warn!(
                            target: "mtp_upload",
                            "Failed to delete partial object after upload error (device={device_id}, dest={dest_folder}/{filename}): {delete_err}"
                        );
                    } else {
                        log::debug!(
                            target: "mtp_upload",
                            "Deleted partial object after upload error (device={device_id}, dest={dest_folder}/{filename})"
                        );
                    }
                }
                // A stale cached parent handle (the device re-keyed its handles
                // since this folder was last listed; common on Android when
                // MediaProvider rescans between listing and upload) presents as
                // an `InvalidParentObject` / `InvalidObjectHandle` rejection of
                // `SendObjectInfo`. That's recoverable: refresh the folder's
                // handle and signal the caller to retry once with a fresh source
                // stream. Classify before `source` is moved into the mapper.
                let is_stale = is_stale_handle_rejection(&upload_err.source);

                // Release the device lock before any re-list: `refresh_dir_handle`
                // re-acquires it through `list_directory`, and the tokio Mutex
                // isn't reentrant — holding it here would deadlock the heal.
                drop(storage);
                drop(device);

                if is_stale {
                    log::warn!(
                        target: "mtp_upload",
                        "SendObjectInfo rejected for {dest_folder}/{filename} on {device_id}: cached parent handle is stale (device re-keyed). Refreshing handles and signaling a one-shot retry."
                    );
                    self.refresh_dir_handle(device_id, storage_id, Path::new(dest_folder))
                        .await;
                    return Err(MtpConnectionError::StaleParentHandle {
                        device_id: device_id.to_string(),
                        dest_folder: normalize_mtp_path(dest_folder).to_string_lossy().to_string(),
                    });
                }

                // Always surface the original upload error (mapped), never the
                // cleanup outcome. Log it: a bare protocol rejection here would
                // otherwise leave no trace (no `error-report` breadcrumb).
                log::warn!(
                    target: "mtp_upload",
                    "Upload failed for {dest_folder}/{filename} on {device_id}: {:?}",
                    upload_err.source
                );
                return Err(map_mtp_error(upload_err.source, device_id));
            }
        };

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

        debug!(
            "MTP upload_from_stream complete: {} bytes to {}/{}",
            size, dest_folder, filename
        );

        Ok(size)
    }
}
