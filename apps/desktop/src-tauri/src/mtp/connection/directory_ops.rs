//! MTP directory listing and path resolution.

use log::{debug, error, info};
use mtp_rs::{ObjectHandle, StorageId};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use super::cache::{CachedListing, LISTING_CACHE_TTL_SECS};
use super::errors::MtpConnectionError;
use super::{
    DeviceEntry, MTP_TIMEOUT_SECS, MtpConnectionManager, acquire_device_lock, convert_mtp_datetime, get_mtp_icon_id,
    map_mtp_error, normalize_mtp_path,
};
use crate::file_system::FileEntry;

/// Global counter for generating unique request IDs for debugging.
static REQUEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Tracks concurrent list_directory calls for debugging lock contention.
static CONCURRENT_LIST_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl MtpConnectionManager {
    /// Lists the contents of a directory on an MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path to list (for example, "/" or "/DCIM")
    ///
    /// # Returns
    ///
    /// A vector of FileEntry objects suitable for the file browser.
    pub async fn list_directory(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<Vec<FileEntry>, MtpConnectionError> {
        use std::sync::atomic::Ordering;

        // Generate unique request ID for tracing this call
        let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let call_start = Instant::now();

        // Track concurrent calls
        let concurrent_before = CONCURRENT_LIST_CALLS.fetch_add(1, Ordering::Relaxed);
        debug!(
            "MTP list_directory [req#{}]: START device={}, storage={}, path={}, concurrent_calls={}",
            request_id,
            device_id,
            storage_id,
            path,
            concurrent_before + 1
        );

        // Wrap the entire operation to ensure we decrement the counter on exit
        let result = self
            .list_directory_inner(request_id, device_id, storage_id, path, call_start)
            .await;

        let concurrent_after = CONCURRENT_LIST_CALLS.fetch_sub(1, Ordering::Relaxed);
        debug!(
            "MTP list_directory [req#{}]: END total_time={:?}, concurrent_calls_remaining={}",
            request_id,
            call_start.elapsed(),
            concurrent_after - 1
        );

        result
    }

    /// Inner implementation of list_directory with detailed phase logging.
    async fn list_directory_inner(
        &self,
        request_id: u64,
        device_id: &str,
        storage_id: u32,
        path: &str,
        call_start: Instant,
    ) -> Result<Vec<FileEntry>, MtpConnectionError> {
        // Normalize the path for building child paths
        let parent_path = normalize_mtp_path(path);

        // Check listing cache first
        let cache_check_start = Instant::now();
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(cache_map) = entry.listing_cache.read()
                && let Some(storage_cache) = cache_map.get(&storage_id)
                && let Some(cached) = storage_cache.listings.get(&parent_path)
            {
                // Check if cache is still valid (within TTL)
                if cached.cached_at.elapsed().as_secs() < LISTING_CACHE_TTL_SECS {
                    debug!(
                        "MTP list_directory [req#{}]: cache HIT, returning {} entries, cache_check_time={:?}, elapsed_since_start={:?}",
                        request_id,
                        cached.entries.len(),
                        cache_check_start.elapsed(),
                        call_start.elapsed()
                    );
                    return Ok(cached.entries.clone());
                } else {
                    debug!(
                        "MTP list_directory [req#{}]: cache STALE (age={}s > TTL={}s)",
                        request_id,
                        cached.cached_at.elapsed().as_secs(),
                        LISTING_CACHE_TTL_SECS
                    );
                }
            } else {
                debug!("MTP list_directory [req#{}]: cache MISS for path={}", request_id, path);
            }
        }
        debug!(
            "MTP list_directory [req#{}]: cache check complete, time={:?}",
            request_id,
            cache_check_start.elapsed()
        );

        // Get the device and resolve path to handle
        let path_resolve_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: acquiring devices registry lock...",
            request_id
        );
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            debug!(
                "MTP list_directory [req#{}]: got devices registry lock in {:?}, looking up device...",
                request_id,
                path_resolve_start.elapsed()
            );
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to parent handle
            debug!("MTP list_directory [req#{}]: resolving path to handle...", request_id);
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            debug!(
                "MTP list_directory [req#{}]: resolved to handle {:?} in {:?}",
                request_id,
                parent_handle,
                path_resolve_start.elapsed()
            );

            (Arc::clone(&entry.device), parent_handle)
        };
        debug!(
            "MTP list_directory [req#{}]: path resolution complete, total_time={:?}",
            request_id,
            path_resolve_start.elapsed()
        );

        // List directory contents (async operation)
        let device_lock_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: waiting to acquire device USB lock...",
            request_id
        );
        let device =
            acquire_device_lock(&device_arc, device_id, &format!("list_directory[req#{}]", request_id)).await?;
        let device_lock_acquired_at = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: acquired device USB lock after {:?} wait, getting storage...",
            request_id,
            device_lock_start.elapsed()
        );

        // Get the storage object
        let usb_io_start = Instant::now();
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;
        debug!(
            "MTP list_directory [req#{}]: got storage object in {:?}",
            request_id,
            usb_io_start.elapsed()
        );

        // Use list_objects which returns Vec<ObjectInfo> directly
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let list_objects_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: calling list_objects (parent={:?})...",
            request_id, parent_opt
        );
        let object_infos =
            match tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.list_objects(parent_opt)).await {
                Ok(Ok(infos)) => infos,
                Ok(Err(e)) => {
                    let mapped_err = map_mtp_error(e, device_id);
                    error!(
                        "MTP list_directory [req#{}]: list_objects failed after {:?}: {:?}",
                        request_id,
                        list_objects_start.elapsed(),
                        mapped_err
                    );
                    return Err(mapped_err);
                }
                Err(_) => {
                    error!(
                        "MTP list_directory [req#{}]: list_objects timed out after {:?}",
                        request_id,
                        list_objects_start.elapsed()
                    );
                    return Err(MtpConnectionError::Timeout {
                        device_id: device_id.to_string(),
                    });
                }
            };

        debug!(
            "MTP list_directory [req#{}]: list_objects returned {} objects in {:?}, total USB I/O time={:?}",
            request_id,
            object_infos.len(),
            list_objects_start.elapsed(),
            usb_io_start.elapsed()
        );

        let mut entries = Vec::with_capacity(object_infos.len());
        let mut cache_updates: Vec<(PathBuf, ObjectHandle)> = Vec::new();

        for info in object_infos {
            let is_dir = info.format == mtp_rs::ptp::ObjectFormatCode::Association;
            let child_path = parent_path.join(&info.filename);

            // Queue cache update
            cache_updates.push((child_path.clone(), info.handle));

            // Convert MTP timestamps
            let modified_at = info.modified.map(convert_mtp_datetime);
            let created_at = info.created.map(convert_mtp_datetime);

            entries.push(FileEntry {
                name: info.filename.clone(),
                path: child_path.to_string_lossy().to_string(),
                is_directory: is_dir,
                is_symlink: false,
                size: if is_dir { None } else { Some(info.size) },
                modified_at,
                created_at,
                added_at: None,
                opened_at: None,
                permissions: if is_dir { 0o755 } else { 0o644 },
                owner: String::new(),
                group: String::new(),
                icon_id: get_mtp_icon_id(is_dir, &info.filename),
                extended_metadata_loaded: true,
            });
        }

        // Release device lock before updating cache
        drop(storage);
        drop(device);
        let lock_held_duration = device_lock_acquired_at.elapsed();
        debug!(
            "MTP list_directory [req#{}]: released device USB lock after holding for {:?}",
            request_id, lock_held_duration
        );

        // Update path cache
        let cache_update_start = Instant::now();
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                for (path, handle) in cache_updates {
                    storage_cache.path_to_handle.insert(path, handle);
                }
            }
        }

        // Sort: directories first, then files, both alphabetically
        entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        // Store in listing cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.listing_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.listings.insert(
                    parent_path,
                    CachedListing {
                        entries: entries.clone(),
                        cached_at: Instant::now(),
                    },
                );
            }
        }
        debug!(
            "MTP list_directory [req#{}]: cache update complete in {:?}",
            request_id,
            cache_update_start.elapsed()
        );

        debug!(
            "MTP list_directory [req#{}]: returning {} entries, total_time={:?}",
            request_id,
            entries.len(),
            call_start.elapsed()
        );
        Ok(entries)
    }

    /// Invalidates the listing cache for a specific directory.
    /// Call this after any operation that modifies the directory contents.
    pub(super) async fn invalidate_listing_cache(&self, device_id: &str, storage_id: u32, dir_path: &Path) {
        let devices = self.devices.lock().await;
        if let Some(entry) = devices.get(device_id)
            && let Ok(mut cache_map) = entry.listing_cache.write()
            && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            && storage_cache.listings.remove(dir_path).is_some()
        {
            debug!(
                "Invalidated listing cache for {} on device {}",
                dir_path.display(),
                device_id
            );
        }
    }

    /// Resolves a virtual path to an MTP object handle.
    pub(super) fn resolve_path_to_handle(
        &self,
        entry: &DeviceEntry,
        storage_id: u32,
        path: &str,
    ) -> Result<ObjectHandle, MtpConnectionError> {
        let path = normalize_mtp_path(path);

        // Root is always ObjectHandle::ROOT
        if path.as_os_str() == "/" || path.as_os_str().is_empty() {
            return Ok(ObjectHandle::ROOT);
        }

        // Check cache
        if let Ok(cache_map) = entry.path_cache.read()
            && let Some(storage_cache) = cache_map.get(&storage_id)
            && let Some(handle) = storage_cache.path_to_handle.get(&path)
        {
            return Ok(*handle);
        }

        // Path not in cache — only paths that have been listed (browsed) are cached
        Err(MtpConnectionError::Other {
            device_id: entry.info.id.clone(),
            message: format!(
                "Path not in cache: {}. Navigate through parent directories first.",
                path.display()
            ),
        })
    }

    /// Handles a device disconnection (called when we detect the device was unplugged).
    ///
    /// This cleans up the devices registry and emits a disconnection event.
    /// Called from the event loop when MTP reports a disconnect, ensuring that
    /// subsequent reconnection attempts don't fail with "already connected".
    pub(super) async fn handle_device_disconnected(&self, device_id: &str, app: Option<&AppHandle>) {
        debug!(
            "handle_device_disconnected: cleaning up device {} from registry",
            device_id
        );

        let removed = {
            let mut devices = self.devices.lock().await;
            let was_present = devices.remove(device_id).is_some();
            debug!(
                "handle_device_disconnected: device {} was {} in registry, {} devices remaining",
                device_id,
                if was_present { "found" } else { "NOT found" },
                devices.len()
            );
            was_present
        };

        // Stop the event loop for this device
        self.stop_event_loop(device_id);

        if removed {
            info!("MTP device disconnected and removed from registry: {}", device_id);

            if let Some(app) = app {
                let _ = app.emit(
                    "mtp-device-disconnected",
                    serde_json::json!({
                        "deviceId": device_id,
                        "reason": "disconnected"
                    }),
                );
                debug!(
                    "handle_device_disconnected: emitted mtp-device-disconnected event for {}",
                    device_id
                );
            }
        } else {
            debug!(
                "handle_device_disconnected: device {} was not in registry (already cleaned up?)",
                device_id
            );
        }
    }
}
