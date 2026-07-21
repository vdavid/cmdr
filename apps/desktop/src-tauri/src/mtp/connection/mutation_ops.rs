//! MTP mutation operations: delete, create folder, rename, and move.

use log::{debug, warn};
use mtp_rs::{CancelToken, ObjectHandle, StorageId};
use std::path::Path;
use std::sync::Arc;

use super::errors::MtpConnectionError;
use super::{MtpConnectionManager, MtpObjectInfo, acquire_device_lock, map_mtp_error, normalize_mtp_path};

impl MtpConnectionManager {
    /// Deletes an object (file or folder) from the MTP device.
    ///
    /// For folders, this recursively deletes all contents first since MTP
    /// requires folders to be empty before deletion.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device
    pub async fn delete_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
    ) -> Result<(), MtpConnectionError> {
        self.delete_object_with_cancel(device_id, storage_id, object_path, None)
            .await
    }

    /// Like [`delete_object`](Self::delete_object) but accepts a cooperative
    /// cancel token. Cancellation is checked:
    /// 1. Before each recursive child delete (between iterations of the
    ///    children loop).
    /// 2. Before each per-handle `GetObjectInfo` USB roundtrip inside
    ///    `list_objects` (via `mtp-rs`'s `list_objects_with_cancel`).
    /// 3. Before issuing the final `DeleteObject` PTP request (via
    ///    `delete_with_cancel`).
    ///
    /// A flipped token bails with `MtpConnectionError::Cancelled` within ≈one
    /// USB roundtrip's latency, so `OperationIntent::Stopped` actually stops
    /// the wire activity instead of just the loop above it.
    pub async fn delete_object_with_cancel(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        cancel: Option<&CancelToken>,
    ) -> Result<(), MtpConnectionError> {
        // Fast bail before any I/O if cancel is already set. Cheap and avoids
        // taking the devices registry lock on a doomed call.
        if let Some(t) = cancel
            && t.is_cancelled()
        {
            return Err(MtpConnectionError::Cancelled {
                device_id: device_id.to_string(),
                message: "Delete cancelled before start".to_string(),
            });
        }

        // Foreground priority: a user delete preempts the background scan. The
        // recursive child deletes nest the guard count (harmless — it just keeps
        // the scan yielded for the whole subtree delete).
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP delete_object: device={}, storage={}, path={}, cancel={}",
            device_id,
            storage_id,
            object_path,
            cancel.is_some()
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "delete_object").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to check if it's a directory
        let object_info = storage
            .get_object_info(object_handle)
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.is_folder();

        if is_dir {
            // For directories, recursively delete contents first. Threading the
            // cancel token into `list_objects_with_cancel` is what makes the
            // 950-entry `/DCIM/Camera` listing bail at the next per-handle USB
            // boundary instead of running all 950 GetObjectInfo roundtrips to
            // completion.
            let children = storage
                .list_objects_with_cancel(Some(object_handle), cancel)
                .await
                .map_err(|e| map_mtp_error(e, device_id))?;

            drop(storage);
            drop(device);

            // Recursively delete children. The token is checked between
            // iterations and inside each child's own list/delete USB call.
            let parent_path = normalize_mtp_path(object_path);
            for child_info in children {
                if let Some(t) = cancel
                    && t.is_cancelled()
                {
                    return Err(MtpConnectionError::Cancelled {
                        device_id: device_id.to_string(),
                        message: "Delete cancelled between children".to_string(),
                    });
                }

                let child_path = parent_path.join(&child_info.filename);
                let child_path_str = child_path.to_string_lossy().to_string();

                // Cache the child handle for the recursive call
                {
                    let devices = self.devices.lock().await;
                    if let Some(entry) = devices.get(device_id)
                        && let Ok(mut cache_map) = entry.path_cache.write()
                    {
                        let storage_cache = cache_map.entry(storage_id).or_default();
                        storage_cache
                            .path_to_handle
                            .insert(child_path.clone(), child_info.handle);
                    }
                }

                // Use Box::pin for recursive async call
                Box::pin(self.delete_object_with_cancel(device_id, storage_id, &child_path_str, cancel)).await?;
            }

            // Re-acquire device and storage lock to delete the now-empty folder
            let device = acquire_device_lock(&device_arc, device_id, "delete_object (empty folder)").await?;
            let storage = device
                .storage(StorageId(u64::from(storage_id)))
                .await
                .map_err(|e| map_mtp_error(e, device_id))?;

            storage
                .delete_with_cancel(object_handle, cancel)
                .await
                .map_err(|e| map_mtp_error(e, device_id))?;
        } else {
            // For files, just delete directly. The cancel check inside
            // delete_with_cancel bails before the PTP `DeleteObject` request
            // is issued, so a cancelled token never touches the wire here.
            storage
                .delete_with_cancel(object_handle, cancel)
                .await
                .map_err(|e| map_mtp_error(e, device_id))?;
        }

        // Remove from path cache
        let object_path_normalized = normalize_mtp_path(object_path);
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
                && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            {
                storage_cache.remove_path(&object_path_normalized);
            }
        }

        // Invalidate the parent directory's listing cache
        if let Some(parent) = object_path_normalized.parent() {
            self.invalidate_listing_cache(device_id, storage_id, parent).await;
        }

        debug!("MTP delete complete: {}", object_path);
        Ok(())
    }

    /// Creates a new folder on the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `parent_path` - Parent folder path (for example, "/DCIM")
    /// * `folder_name` - Name of the new folder
    pub async fn create_folder(
        &self,
        device_id: &str,
        storage_id: u32,
        parent_path: &str,
        folder_name: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        // Foreground priority: a user create preempts the background scan.
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP create_folder: device={}, storage={}, parent={}, name={}",
            device_id, storage_id, parent_path, folder_name
        );

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = self.resolve_path_to_handle(entry, storage_id, parent_path)?;
            (Arc::clone(&entry.device), parent)
        };

        let device = acquire_device_lock(&device_arc, device_id, "create_folder").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Create the folder
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let new_handle = storage
            .create_folder(parent_opt, folder_name)
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Build the new folder path
        let new_path = normalize_mtp_path(parent_path).join(folder_name);
        let new_path_str = new_path.to_string_lossy().to_string();

        // Update path cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let parent_path_normalized = normalize_mtp_path(parent_path);
        self.invalidate_listing_cache(device_id, storage_id, &parent_path_normalized)
            .await;

        debug!("MTP folder created: {}", new_path_str);

        Ok(MtpObjectInfo {
            handle: new_handle.0 as u32,
            name: folder_name.to_string(),
            path: new_path_str,
            is_directory: true,
            size: None,
        })
    }

    /// Renames an object on the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Current path of the object
    /// * `new_name` - New name for the object
    pub async fn rename_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        new_name: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        // Foreground priority: a user rename preempts the background scan.
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP rename_object: device={}, storage={}, path={}, new_name={}",
            device_id, storage_id, object_path, new_name
        );

        // Get device and resolve object handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "rename_object").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine if it's a directory
        let object_info = storage
            .get_object_info(object_handle)
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.is_folder();
        let old_size = object_info.size;

        // Set the new filename using storage.rename()
        storage
            .rename(object_handle, new_name)
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device and storage lock
        drop(storage);
        drop(device);

        // Update path cache
        let old_path = normalize_mtp_path(object_path);
        let parent = old_path.parent().unwrap_or(Path::new("/"));
        let new_path = parent.join(new_name);
        let new_path_str = new_path.to_string_lossy().to_string();

        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
                && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            {
                storage_cache.remove_path(&old_path);
                storage_cache.insert(new_path.clone(), object_handle);
            }
        }

        // Invalidate the parent directory's listing cache (rename affects the parent listing)
        self.invalidate_listing_cache(device_id, storage_id, parent).await;

        debug!("MTP rename complete: {} -> {}", object_path, new_path_str);

        Ok(MtpObjectInfo {
            handle: object_handle.0 as u32,
            name: new_name.to_string(),
            path: new_path_str,
            is_directory: is_dir,
            size: if is_dir { None } else { Some(old_size) },
        })
    }

    /// Moves an object to a new parent folder on the MTP device.
    ///
    /// Falls back to copy+delete if the device doesn't support MoveObject.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Current path of the object
    /// * `new_parent_path` - New parent folder path
    pub async fn move_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        new_parent_path: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        // Foreground priority: a user move preempts the background scan.
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP move_object: device={}, storage={}, path={}, new_parent={}",
            device_id, storage_id, object_path, new_parent_path
        );

        // Get device and resolve both handles
        let (device_arc, object_handle, new_parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let obj_handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, new_parent_path)?;
            (Arc::clone(&entry.device), obj_handle, parent_handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "move_object").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info
        let object_info = storage
            .get_object_info(object_handle)
            .await
            .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.is_folder();
        let object_size = object_info.size;
        let object_name = object_info.filename.clone();

        // Try to use MoveObject operation
        // storage.move_object expects the new parent handle directly, not Option
        let new_parent_for_move = if new_parent_handle == ObjectHandle::ROOT {
            ObjectHandle::ROOT
        } else {
            new_parent_handle
        };

        let move_result = storage.move_object(object_handle, new_parent_for_move, None).await;

        // Release device and storage lock
        drop(storage);
        drop(device);

        match move_result {
            Ok(()) => {
                // Move succeeded
                let old_path = normalize_mtp_path(object_path);
                let new_path = normalize_mtp_path(new_parent_path).join(&object_name);
                let new_path_str = new_path.to_string_lossy().to_string();

                // Update path cache
                {
                    let devices = self.devices.lock().await;
                    if let Some(entry) = devices.get(device_id)
                        && let Ok(mut cache_map) = entry.path_cache.write()
                        && let Some(storage_cache) = cache_map.get_mut(&storage_id)
                    {
                        storage_cache.remove_path(&old_path);
                        storage_cache.insert(new_path.clone(), object_handle);
                    }
                }

                // Invalidate listing cache for both old and new parent directories
                let old_parent = old_path.parent().unwrap_or(Path::new("/"));
                self.invalidate_listing_cache(device_id, storage_id, old_parent).await;
                let new_parent = normalize_mtp_path(new_parent_path);
                self.invalidate_listing_cache(device_id, storage_id, &new_parent).await;

                debug!("MTP move complete: {} -> {}", object_path, new_path_str);

                Ok(MtpObjectInfo {
                    handle: object_handle.0 as u32,
                    name: object_name,
                    path: new_path_str,
                    is_directory: is_dir,
                    size: if is_dir { None } else { Some(object_size) },
                })
            }
            Err(e) => {
                // Move operation returned an error - might not be supported
                warn!(
                    "MTP MoveObject failed for {}: {:?}. Device may not support this operation.",
                    object_path, e
                );
                Err(MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("Move operation not supported by device: {}", e),
                })
            }
        }
    }
}
