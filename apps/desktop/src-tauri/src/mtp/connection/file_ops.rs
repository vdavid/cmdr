//! MTP file transfer operations (download, upload, and streaming).

use log::debug;
use mtp_rs::{MtpDevice, NewObjectInfo, ObjectHandle, Storage, StorageId};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use super::errors::{MtpConnectionError, is_stale_handle_rejection, map_mtp_error};
use super::{
    MTP_READ_WINDOW, MTP_TIMEOUT_SECS, MtpConnectionManager, MtpObjectInfo, MtpOperationResult, MtpTransferProgress,
    MtpTransferType, acquire_device_lock, mtp_window_len, normalize_mtp_path,
};

/// Cached state for a bounded-window MTP read.
///
/// `open_read_session` resolves the object handle, obtains the mtp-rs `Storage`,
/// and reads `total_size` ONCE under the device lock; each `read_window` then
/// reuses this, paying only `acquire_device_lock` + one `GetPartialObject64` per
/// window (no per-window handle/`Storage` re-resolve, which would cost an extra
/// `GetStorageInfo`/resolve USB roundtrip per window). The `Storage` carries its
/// own `Arc<MtpDeviceInner>`, so it outlives the device-lock guard taken per
/// window.
pub(crate) struct MtpReadSession {
    device_arc: Arc<Mutex<MtpDevice>>,
    storage: Storage,
    object_handle: ObjectHandle,
    /// Full object size in bytes (anchors progress and the EOF check). Read by
    /// the volume backend's read stream.
    pub(crate) total_size: u64,
}

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
        // A download takes NO foreground_guard. A transfer is a *background* user
        // of the device gate — it yields TO a foreground listing/nav, so raising
        // `foreground_pending` would make the copy contend with itself. Instead it
        // reads in bounded windows: each `read_window` takes the per-device lock
        // for just one `GetPartialObject64` (~80 ms), freeing the PTP session
        // between windows so a foreground listing slips in. See
        // `mtp/connection/DETAILS.md` § "Bounded-window reads".
        debug!(
            "MTP download_file: device={}, storage={}, path={}, dest={}",
            device_id,
            storage_id,
            object_path,
            local_dest.display()
        );

        // Get the device and resolve path to handle.
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        // Resolve the Storage + size ONCE under the device lock (the filename is
        // for the progress event), then release the lock so windows can interleave
        // with foreground ops.
        let (storage, total_size, filename) = {
            let device = acquire_device_lock(&device_arc, device_id, "download_file").await?;
            let storage = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                device.storage(StorageId(storage_id)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            let object_info = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                storage.get_object_info(object_handle),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            (storage, object_info.size, object_info.filename.clone())
        };

        let session = MtpReadSession {
            device_arc,
            storage,
            object_handle,
            total_size,
        };

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

        // Create the local file
        let mut file = tokio::fs::File::create(local_dest)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to create local file: {}", e),
            })?;

        // Read the file as a sequence of bounded windows. Each window takes +
        // releases the device lock, so a foreground listing slips in between
        // windows. `offset` advances by the bytes the device actually returned (a
        // short read mid-file is legal); a 0-byte read before EOF is a stall, not
        // loop continuation.
        let mut offset = 0u64;
        let mut cancelled = false;
        while let Some(len) = mtp_window_len(offset, total_size, MTP_READ_WINDOW) {
            let chunk = self.read_window(&session, device_id, offset, len).await?;
            if chunk.is_empty() {
                return Err(MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("MTP read returned 0 bytes at offset {offset} of {total_size} bytes"),
                });
            }

            file.write_all(&chunk).await.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to write local file: {}", e),
            })?;

            offset += chunk.len() as u64;

            // Report progress and check for cancellation
            if let Some(ref cb) = on_progress
                && cb(offset, total_size).is_break()
            {
                cancelled = true;
                break;
            }
        }

        let bytes_written = offset;

        // Release the cached Storage/handle (a mid-window drop self-heals via
        // mtp-rs's TransactionScope; between windows nothing is in flight).
        drop(session);

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

    /// Opens a bounded-window read of a file: resolves the object handle, obtains
    /// the `Storage`, and reads `total_size` ONCE under the device lock, returning
    /// an [`MtpReadSession`] the caller caches and feeds to [`read_window`] per
    /// window. The lock is released before returning, so nothing is held between
    /// windows.
    ///
    /// Takes NO `foreground_guard`: a transfer is a *background* user of the
    /// device gate (it yields TO foreground), so raising `foreground_pending`
    /// here would make the copy contend with itself. See
    /// `mtp/connection/DETAILS.md` § "Bounded-window reads".
    ///
    /// [`read_window`]: Self::read_window
    pub async fn open_read_session(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<MtpReadSession, MtpConnectionError> {
        debug!(
            "MTP open_read_session: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Get the device and resolve path to handle.
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            let handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            (Arc::clone(&entry.device), handle)
        };

        let (storage, total_size) = {
            let device = acquire_device_lock(&device_arc, device_id, "open_read_session").await?;
            let storage = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                device.storage(StorageId(storage_id)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            let object_info = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                storage.get_object_info(object_handle),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            (storage, object_info.size)
        };

        debug!("MTP open_read_session: opened {} bytes for {}", total_size, path);

        Ok(MtpReadSession {
            device_arc,
            storage,
            object_handle,
            total_size,
        })
    }

    /// Reads one bounded window `[offset, offset + len)` of an object opened with
    /// [`open_read_session`]. Acquires the per-device lock for just this one
    /// `GetPartialObject64` (released on return), so the PTP session is free
    /// between windows for a foreground listing/nav.
    ///
    /// Returns the bytes the device actually delivered — possibly fewer than
    /// `len` (a short read mid-file is legal; the caller advances `offset` by the
    /// returned length, and treats a 0-byte read before EOF as a stall, not loop
    /// continuation). Takes NO `foreground_guard` (a transfer is a background gate
    /// user; see `open_read_session`).
    ///
    /// Drop-safety: if this future is dropped mid-flight (task abort, device
    /// disconnect), mtp-rs's `TransactionScope` flags the pipe and the next op
    /// drains it under the operation lock (one ~300 ms self-heal), so an aborted
    /// window never permanently desyncs the session. This is what makes the
    /// buffered-window model safe to abort at any point.
    ///
    /// [`open_read_session`]: Self::open_read_session
    pub async fn read_window(
        &self,
        session: &MtpReadSession,
        device_id: &str,
        offset: u64,
        len: u32,
    ) -> Result<Vec<u8>, MtpConnectionError> {
        let _device = acquire_device_lock(&session.device_arc, device_id, "read_window").await?;
        tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            session.storage.download_partial_64(session.object_handle, offset, len),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))
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
