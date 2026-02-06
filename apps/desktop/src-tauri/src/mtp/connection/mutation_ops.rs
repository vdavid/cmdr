//! MTP mutation operations: delete, create folder, rename, and move.

use log::{debug, info, warn};
use mtp_rs::{ObjectHandle, StorageId};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::errors::MtpConnectionError;
use super::{
    MTP_TIMEOUT_SECS, MtpConnectionManager, MtpObjectInfo, acquire_device_lock, map_mtp_error, normalize_mtp_path,
};

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
        debug!(
            "MTP delete_object: device={}, storage={}, path={}",
            device_id, storage_id, object_path
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
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to check if it's a directory
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;

        if is_dir {
            // For directories, we need to recursively delete contents first
            let children = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                storage.list_objects(Some(object_handle)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            drop(storage);
            drop(device);

            // Recursively delete children
            let parent_path = normalize_mtp_path(object_path);
            for child_info in children {
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
                Box::pin(self.delete_object(device_id, storage_id, &child_path_str)).await?;
            }

            // Re-acquire device and storage lock to delete the now-empty folder
            let device = acquire_device_lock(&device_arc, device_id, "delete_object (empty folder)").await?;
            let storage = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                device.storage(StorageId(storage_id)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.delete(object_handle))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;
        } else {
            // For files, just delete directly
            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.delete(object_handle))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
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
                storage_cache.path_to_handle.remove(&object_path_normalized);
            }
        }

        // Invalidate the parent directory's listing cache
        if let Some(parent) = object_path_normalized.parent() {
            self.invalidate_listing_cache(device_id, storage_id, parent).await;
        }

        info!("MTP delete complete: {}", object_path);
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
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create the folder
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.create_folder(parent_opt, folder_name),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
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
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let parent_path_normalized = normalize_mtp_path(parent_path);
        self.invalidate_listing_cache(device_id, storage_id, &parent_path_normalized)
            .await;

        info!("MTP folder created: {}", new_path_str);

        Ok(MtpObjectInfo {
            handle: new_handle.0,
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
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine if it's a directory
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;
        let old_size = object_info.size;

        // Set the new filename using storage.rename()
        tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.rename(object_handle, new_name),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
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
                storage_cache.path_to_handle.remove(&old_path);
                storage_cache.path_to_handle.insert(new_path.clone(), object_handle);
            }
        }

        // Invalidate the parent directory's listing cache (rename affects the parent listing)
        self.invalidate_listing_cache(device_id, storage_id, parent).await;

        info!("MTP rename complete: {} -> {}", object_path, new_path_str);

        Ok(MtpObjectInfo {
            handle: object_handle.0,
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
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;
        let object_size = object_info.size;
        let object_name = object_info.filename.clone();

        // Try to use MoveObject operation
        // storage.move_object expects the new parent handle directly, not Option
        let new_parent_for_move = if new_parent_handle == ObjectHandle::ROOT {
            ObjectHandle::ROOT
        } else {
            new_parent_handle
        };

        let move_result = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.move_object(object_handle, new_parent_for_move, None),
        )
        .await;

        // Release device and storage lock
        drop(storage);
        drop(device);

        match move_result {
            Ok(Ok(())) => {
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
                        storage_cache.path_to_handle.remove(&old_path);
                        storage_cache.path_to_handle.insert(new_path.clone(), object_handle);
                    }
                }

                // Invalidate listing cache for both old and new parent directories
                let old_parent = old_path.parent().unwrap_or(Path::new("/"));
                self.invalidate_listing_cache(device_id, storage_id, old_parent).await;
                let new_parent = normalize_mtp_path(new_parent_path);
                self.invalidate_listing_cache(device_id, storage_id, &new_parent).await;

                info!("MTP move complete: {} -> {}", object_path, new_path_str);

                Ok(MtpObjectInfo {
                    handle: object_handle.0,
                    name: object_name,
                    path: new_path_str,
                    is_directory: is_dir,
                    size: if is_dir { None } else { Some(object_size) },
                })
            }
            Ok(Err(e)) => {
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
            Err(_) => Err(MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            }),
        }
    }
}
