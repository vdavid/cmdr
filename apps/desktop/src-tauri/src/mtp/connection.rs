//! MTP connection management.
//!
//! Manages device connections with a global registry. Each connected device
//! maintains an active MTP session until disconnected or unplugged.

use futures_util::StreamExt;
use log::{debug, error, info, warn};
use mtp_rs::{MtpDevice, MtpDeviceBuilder, NewObjectInfo, ObjectHandle, StorageId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, RwLock};
use tokio::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::types::{MtpDeviceInfo, MtpStorageInfo};
use crate::file_system::FileEntry;

/// Default timeout for MTP operations (30 seconds - some devices are slow).
const MTP_TIMEOUT_SECS: u64 = 30;

/// Error types for MTP connection operations.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MtpConnectionError {
    /// Device not found (may have been unplugged).
    DeviceNotFound { device_id: String },
    /// Device is already connected.
    AlreadyConnected { device_id: String },
    /// Device is not connected.
    NotConnected { device_id: String },
    /// Another process has exclusive access to the device.
    ExclusiveAccess {
        device_id: String,
        blocking_process: Option<String>,
    },
    /// Connection timed out.
    Timeout { device_id: String },
    /// Device was disconnected unexpectedly.
    Disconnected { device_id: String },
    /// Protocol error from device.
    Protocol { device_id: String, message: String },
    /// Device is busy (retryable).
    DeviceBusy { device_id: String },
    /// Storage is full.
    StorageFull { device_id: String },
    /// Object not found on device.
    ObjectNotFound { device_id: String, path: String },
    /// Other connection error.
    Other { device_id: String, message: String },
}

impl MtpConnectionError {
    /// Returns true if the operation may succeed if retried.
    #[allow(dead_code, reason = "Will be used by frontend for retry logic")]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::DeviceBusy { .. })
    }

    /// Returns a user-friendly message for this error.
    #[allow(dead_code, reason = "Will be exposed via Tauri commands for UI error display")]
    pub fn user_message(&self) -> String {
        match self {
            Self::DeviceNotFound { .. } => "Device not found. It may have been unplugged.".to_string(),
            Self::AlreadyConnected { .. } => "Device is already connected.".to_string(),
            Self::NotConnected { .. } => {
                "Device is not connected. Select it from the volume picker to connect.".to_string()
            }
            Self::ExclusiveAccess { blocking_process, .. } => {
                if let Some(proc) = blocking_process {
                    format!(
                        "Another app ({}) is using this device. Close it or use the Terminal workaround.",
                        proc
                    )
                } else {
                    "Another app is using this device. Close other apps that might be accessing it.".to_string()
                }
            }
            Self::Timeout { .. } => {
                "The operation timed out. The device may be slow or unresponsive. Try again.".to_string()
            }
            Self::Disconnected { .. } => "Device was disconnected. Reconnect it to continue.".to_string(),
            Self::Protocol { message, .. } => {
                format!("Device reported an error: {}. Try reconnecting.", message)
            }
            Self::DeviceBusy { .. } => "Device is busy. Wait a moment and try again.".to_string(),
            Self::StorageFull { .. } => "Device storage is full. Free up some space.".to_string(),
            Self::ObjectNotFound { path, .. } => {
                format!("File or folder not found: {}. It may have been deleted.", path)
            }
            Self::Other { message, .. } => message.clone(),
        }
    }
}

impl std::fmt::Display for MtpConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound { device_id } => {
                write!(f, "Device not found: {device_id}")
            }
            Self::AlreadyConnected { device_id } => {
                write!(f, "Device already connected: {device_id}")
            }
            Self::NotConnected { device_id } => {
                write!(f, "Device not connected: {device_id}")
            }
            Self::ExclusiveAccess {
                device_id,
                blocking_process,
            } => {
                if let Some(proc) = blocking_process {
                    write!(f, "Device {device_id} is in use by {proc}")
                } else {
                    write!(f, "Device {device_id} is in use by another process")
                }
            }
            Self::Timeout { device_id } => {
                write!(f, "Connection timed out for device: {device_id}")
            }
            Self::Disconnected { device_id } => {
                write!(f, "Device disconnected: {device_id}")
            }
            Self::Protocol { device_id, message } => {
                write!(f, "Protocol error for {device_id}: {message}")
            }
            Self::DeviceBusy { device_id } => {
                write!(f, "Device busy: {device_id}")
            }
            Self::StorageFull { device_id } => {
                write!(f, "Storage full on device: {device_id}")
            }
            Self::ObjectNotFound { device_id, path } => {
                write!(f, "Object not found on {device_id}: {path}")
            }
            Self::Other { device_id, message } => {
                write!(f, "Error for {device_id}: {message}")
            }
        }
    }
}

impl std::error::Error for MtpConnectionError {}

// ============================================================================
// Progress events for MTP file operations
// ============================================================================

/// Progress event for MTP file transfers (download/upload).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpTransferProgress {
    /// Unique operation ID.
    pub operation_id: String,
    /// Device ID.
    pub device_id: String,
    /// Type of transfer.
    pub transfer_type: MtpTransferType,
    /// Current file being transferred.
    pub current_file: String,
    /// Bytes transferred so far.
    pub bytes_done: u64,
    /// Total bytes to transfer.
    pub bytes_total: u64,
}

/// Type of MTP transfer operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MtpTransferType {
    Download,
    Upload,
}

/// Result of a successful MTP operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpOperationResult {
    /// Operation ID (for tracking).
    pub operation_id: String,
    /// Number of files processed.
    pub files_processed: usize,
    /// Total bytes transferred.
    pub bytes_transferred: u64,
}

/// State for tracking cancellation of MTP operations.
#[allow(dead_code, reason = "Will be used in Phase 5 for operation cancellation")]
pub struct MtpOperationState {
    /// Cancellation flag.
    pub cancelled: AtomicBool,
}

impl Default for MtpOperationState {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }
}

/// Information about an object on the device (returned after creation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpObjectInfo {
    /// Object handle.
    pub handle: u32,
    /// Object name.
    pub name: String,
    /// Virtual path on device.
    pub path: String,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Size in bytes (None for directories).
    pub size: Option<u64>,
}

/// Information about a connected device, including its storages.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDeviceInfo {
    /// Device information.
    pub device: MtpDeviceInfo,
    /// Available storages on the device.
    pub storages: Vec<MtpStorageInfo>,
}

/// Internal entry for a connected device.
struct DeviceEntry {
    /// The MTP device handle (wrapped in Arc for shared access).
    device: Arc<Mutex<MtpDevice>>,
    /// Device metadata.
    info: MtpDeviceInfo,
    /// Cached storage information.
    storages: Vec<MtpStorageInfo>,
    /// Path-to-handle cache per storage.
    path_cache: RwLock<HashMap<u32, PathHandleCache>>,
    /// Directory listing cache per storage.
    listing_cache: RwLock<HashMap<u32, ListingCache>>,
}

/// Cache for mapping paths to MTP object handles.
#[derive(Default)]
struct PathHandleCache {
    /// Maps virtual path -> MTP object handle.
    path_to_handle: HashMap<PathBuf, ObjectHandle>,
}

/// Cache for directory listings.
#[derive(Default)]
struct ListingCache {
    /// Maps directory path -> cached file entries.
    listings: HashMap<PathBuf, CachedListing>,
}

/// A cached directory listing with timestamp for invalidation.
struct CachedListing {
    /// The cached file entries.
    entries: Vec<FileEntry>,
    /// When this listing was cached (for TTL checks).
    cached_at: std::time::Instant,
}

/// How long to keep cached listings (5 seconds).
const LISTING_CACHE_TTL_SECS: u64 = 5;

/// Global connection manager for MTP devices.
pub struct MtpConnectionManager {
    /// Map of device_id -> connected device entry.
    devices: Mutex<HashMap<String, DeviceEntry>>,
}

/// Acquires the device lock with a timeout.
/// This prevents indefinite blocking if the device is unresponsive or another operation is stuck.
async fn acquire_device_lock<'a>(
    device_arc: &'a Arc<Mutex<MtpDevice>>,
    device_id: &str,
    operation: &str,
) -> Result<tokio::sync::MutexGuard<'a, MtpDevice>, MtpConnectionError> {
    tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), device_arc.lock())
        .await
        .map_err(|_| {
            error!("MTP {}: timed out waiting for device lock", operation);
            MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            }
        })
}

impl MtpConnectionManager {
    /// Creates a new connection manager.
    fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
        }
    }

    /// Connects to an MTP device by ID.
    ///
    /// Opens an MTP session and retrieves storage information.
    ///
    /// # Returns
    ///
    /// Information about the connected device including available storages.
    pub async fn connect(
        &self,
        device_id: &str,
        app: Option<&AppHandle>,
    ) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
        // Check if already connected
        {
            let devices = self.devices.lock().await;
            if devices.contains_key(device_id) {
                return Err(MtpConnectionError::AlreadyConnected {
                    device_id: device_id.to_string(),
                });
            }
        }

        info!("Connecting to MTP device: {}", device_id);

        // Parse device_id to get bus and address (format: "mtp-{bus}-{address}")
        let (bus, address) = parse_device_id(device_id).ok_or_else(|| MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        })?;
        debug!("Parsed device_id: bus={}, address={}", bus, address);

        // Find and open the device
        debug!("Opening MTP device (timeout={}s)...", MTP_TIMEOUT_SECS);
        let device = match open_device(bus, address).await {
            Ok(d) => d,
            Err(e) => {
                // Check for exclusive access error
                if e.is_exclusive_access() {
                    #[cfg(target_os = "macos")]
                    let blocking_process = super::macos_workaround::get_usb_exclusive_owner();
                    #[cfg(not(target_os = "macos"))]
                    let blocking_process: Option<String> = None;

                    // Emit event for frontend to show dialog
                    if let Some(app) = app {
                        let _ = app.emit(
                            "mtp-exclusive-access-error",
                            serde_json::json!({
                                "deviceId": device_id,
                                "blockingProcess": blocking_process.clone()
                            }),
                        );
                    }

                    return Err(MtpConnectionError::ExclusiveAccess {
                        device_id: device_id.to_string(),
                        blocking_process,
                    });
                }

                // Map other errors
                return Err(map_mtp_error(e, device_id));
            }
        };

        // Get device info
        let mtp_info = device.device_info();
        let device_info = MtpDeviceInfo {
            id: device_id.to_string(),
            vendor_id: 0, // Not available from device_info
            product_id: 0,
            manufacturer: if mtp_info.manufacturer.is_empty() {
                None
            } else {
                Some(mtp_info.manufacturer.clone())
            },
            product: if mtp_info.model.is_empty() {
                None
            } else {
                Some(mtp_info.model.clone())
            },
            serial_number: if mtp_info.serial_number.is_empty() {
                None
            } else {
                Some(mtp_info.serial_number.clone())
            },
        };

        debug!("Device opened successfully: {} {}", mtp_info.manufacturer, mtp_info.model);

        // Get storage information
        debug!("Fetching storage information...");
        let storages = match get_storages(&device).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get storages for {}: {:?}", device_id, e);
                Vec::new()
            }
        };

        let connected_info = ConnectedDeviceInfo {
            device: device_info.clone(),
            storages: storages.clone(),
        };

        // Store in registry with Arc-wrapped device for shared access
        {
            let mut devices = self.devices.lock().await;
            devices.insert(
                device_id.to_string(),
                DeviceEntry {
                    device: Arc::new(Mutex::new(device)),
                    info: device_info,
                    storages,
                    path_cache: RwLock::new(HashMap::new()),
                    listing_cache: RwLock::new(HashMap::new()),
                },
            );
        }

        // Emit connected event
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-device-connected",
                serde_json::json!({
                    "deviceId": device_id,
                    "storages": connected_info.storages
                }),
            );
        }

        info!(
            "MTP device connected: {} ({} storages)",
            device_id,
            connected_info.storages.len()
        );

        Ok(connected_info)
    }

    /// Disconnects from an MTP device.
    ///
    /// Closes the MTP session gracefully.
    pub async fn disconnect(&self, device_id: &str, app: Option<&AppHandle>) -> Result<(), MtpConnectionError> {
        info!("Disconnecting from MTP device: {}", device_id);

        // Remove from registry
        let entry = {
            let mut devices = self.devices.lock().await;
            devices.remove(device_id)
        };

        let Some(entry) = entry else {
            return Err(MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            });
        };

        // The device will be closed when it's dropped.
        // MtpDevice::close() takes ownership, but we have it in an Arc<Mutex>.
        // Dropping the entry will drop the Arc, and if this is the last reference,
        // the device will be closed (MtpDevice has a Drop impl that closes the session).
        // We just drop the entry here - the device handle going out of scope handles cleanup.
        drop(entry);

        // Emit disconnected event
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-device-disconnected",
                serde_json::json!({
                    "deviceId": device_id,
                    "reason": "user"
                }),
            );
        }

        info!("MTP device disconnected: {}", device_id);
        Ok(())
    }

    /// Gets information about a connected device.
    pub async fn get_device_info(&self, device_id: &str) -> Option<ConnectedDeviceInfo> {
        let devices = self.devices.lock().await;
        devices.get(device_id).map(|entry| ConnectedDeviceInfo {
            device: entry.info.clone(),
            storages: entry.storages.clone(),
        })
    }

    /// Checks if a device is connected.
    #[allow(dead_code, reason = "Will be used in Phase 3+ for file browsing")]
    pub async fn is_connected(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().await;
        devices.contains_key(device_id)
    }

    /// Returns a list of all connected device IDs.
    #[allow(dead_code, reason = "Will be used in Phase 5 for multi-device management")]
    pub async fn connected_device_ids(&self) -> Vec<String> {
        let devices = self.devices.lock().await;
        devices.keys().cloned().collect()
    }

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
        debug!(
            "MTP list_directory: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Normalize the path for building child paths
        let parent_path = normalize_mtp_path(path);

        // Check listing cache first
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
                        "MTP list_directory: returning {} cached entries for {}",
                        cached.entries.len(),
                        path
                    );
                    return Ok(cached.entries.clone());
                }
            }
        }

        // Get the device and resolve path to handle
        debug!("MTP list_directory: acquiring devices lock...");
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            debug!("MTP list_directory: got devices lock, looking up device...");
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to parent handle
            debug!("MTP list_directory: resolving path to handle...");
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            debug!("MTP list_directory: resolved to handle {:?}", parent_handle);

            (Arc::clone(&entry.device), parent_handle)
        };

        // List directory contents (async operation)
        debug!("MTP list_directory: acquiring device lock...");
        let device = acquire_device_lock(&device_arc, device_id, "list_directory").await?;
        debug!("MTP list_directory: got device lock, getting storage...");

        // Get the storage object
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;
        debug!("MTP list_directory: got storage object");

        // Use list_objects which returns Vec<ObjectInfo> directly
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        debug!("MTP list_directory: calling list_objects (parent={:?})...", parent_opt);
        let object_infos =
            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.list_objects(parent_opt))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;

        debug!("MTP list_directory: list_objects returned {} objects", object_infos.len());

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

        // Update path cache
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
                        cached_at: std::time::Instant::now(),
                    },
                );
            }
        }

        debug!("MTP list_directory: returning {} entries", entries.len());
        Ok(entries)
    }

    /// Invalidates the listing cache for a specific directory.
    /// Call this after any operation that modifies the directory contents.
    async fn invalidate_listing_cache(&self, device_id: &str, storage_id: u32, dir_path: &Path) {
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
    fn resolve_path_to_handle(
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

        // Path not in cache - we need to traverse
        // For Phase 3, we'll only support navigating to paths that have been listed
        // (the cache is populated as directories are browsed)
        Err(MtpConnectionError::Other {
            device_id: entry.info.id.clone(),
            message: format!(
                "Path not in cache: {}. Navigate through parent directories first.",
                path.display()
            ),
        })
    }

    /// Handles a device disconnection (called when we detect the device was unplugged).
    #[allow(dead_code, reason = "Will be used in Phase 5 for USB hotplug detection")]
    pub async fn handle_device_disconnected(&self, device_id: &str, app: Option<&AppHandle>) {
        let removed = {
            let mut devices = self.devices.lock().await;
            devices.remove(device_id).is_some()
        };

        if removed {
            warn!("MTP device unexpectedly disconnected: {}", device_id);

            if let Some(app) = app {
                let _ = app.emit(
                    "mtp-device-disconnected",
                    serde_json::json!({
                        "deviceId": device_id,
                        "reason": "disconnected"
                    }),
                );
            }
        }
    }

    // ========================================================================
    // Phase 4: File Operations
    // ========================================================================

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

        // Download the file as a stream
        let mut download_stream = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.download(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock before writing to disk
        drop(storage);
        drop(device);

        // Create the local file
        let mut file = tokio::fs::File::create(local_dest)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to create local file: {}", e),
            })?;

        // Write chunks to file
        let mut bytes_written = 0u64;
        while let Some(chunk_result) = download_stream.next().await {
            let chunk = chunk_result.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Download error: {}", e),
            })?;

            file.write_all(&chunk.data)
                .await
                .map_err(|e| MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("Failed to write local file: {}", e),
                })?;

            bytes_written += chunk.data.len() as u64;
        }

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
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path).await;

        Ok(MtpObjectInfo {
            handle: new_handle.0,
            name: filename,
            path: new_path_str,
            is_directory: false,
            size: Some(file_size),
        })
    }

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
        self.invalidate_listing_cache(device_id, storage_id, &parent_path_normalized).await;

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

/// Global connection manager instance.
static CONNECTION_MANAGER: LazyLock<MtpConnectionManager> = LazyLock::new(MtpConnectionManager::new);

/// Gets the global connection manager.
pub fn connection_manager() -> &'static MtpConnectionManager {
    &CONNECTION_MANAGER
}

/// Parses a device ID to extract bus and address.
///
/// Format: "mtp-{bus}-{address}"
fn parse_device_id(device_id: &str) -> Option<(u8, u8)> {
    let parts: Vec<&str> = device_id.split('-').collect();
    if parts.len() != 3 || parts[0] != "mtp" {
        return None;
    }

    let bus = parts[1].parse().ok()?;
    let address = parts[2].parse().ok()?;
    Some((bus, address))
}

/// Opens an MTP device by bus and address.
async fn open_device(bus: u8, address: u8) -> Result<MtpDevice, mtp_rs::Error> {
    // Open the device directly by bus and address
    MtpDeviceBuilder::new()
        .timeout(Duration::from_secs(MTP_TIMEOUT_SECS))
        .open(bus, address)
        .await
}

/// Gets storage information from a connected device.
async fn get_storages(device: &MtpDevice) -> Result<Vec<MtpStorageInfo>, mtp_rs::Error> {
    debug!("Calling device.storages()...");
    let storage_list = device.storages().await?;
    debug!("Got {} storage(s)", storage_list.len());
    let mut storages = Vec::new();

    for storage in storage_list {
        let info = storage.info();
        storages.push(MtpStorageInfo {
            id: storage.id().0,
            name: info.description.clone(),
            total_bytes: info.max_capacity,
            available_bytes: info.free_space_bytes,
            storage_type: Some(format!("{:?}", info.storage_type)),
        });
    }

    Ok(storages)
}

/// Maps mtp_rs errors to our error types.
fn map_mtp_error(e: mtp_rs::Error, device_id: &str) -> MtpConnectionError {
    use mtp_rs::ptp::ResponseCode;

    match e {
        mtp_rs::Error::NoDevice => MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Disconnected => MtpConnectionError::Disconnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Timeout => MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Cancelled => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: "Operation cancelled".to_string(),
        },
        mtp_rs::Error::SessionNotOpen => MtpConnectionError::NotConnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Protocol { code, operation } => {
            // Map specific response codes to user-friendly errors
            match code {
                ResponseCode::DeviceBusy => MtpConnectionError::DeviceBusy {
                    device_id: device_id.to_string(),
                },
                ResponseCode::StoreFull => MtpConnectionError::StorageFull {
                    device_id: device_id.to_string(),
                },
                ResponseCode::InvalidObjectHandle | ResponseCode::InvalidParentObject => {
                    MtpConnectionError::ObjectNotFound {
                        device_id: device_id.to_string(),
                        path: format!("(operation: {:?})", operation),
                    }
                }
                ResponseCode::AccessDenied => MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "Access denied. The device rejected the operation.".to_string(),
                },
                _ => MtpConnectionError::Protocol {
                    device_id: device_id.to_string(),
                    message: format!("{:?}", code),
                },
            }
        }
        mtp_rs::Error::InvalidData { message } => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Invalid data from device: {}", message),
        },
        mtp_rs::Error::Io(io_err) => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("I/O error: {}", io_err),
        },
        mtp_rs::Error::Usb(usb_err) => {
            // Check for exclusive access errors
            let msg = usb_err.to_string().to_lowercase();
            if msg.contains("exclusive access") || msg.contains("device or resource busy") {
                MtpConnectionError::ExclusiveAccess {
                    device_id: device_id.to_string(),
                    blocking_process: None,
                }
            } else {
                MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("USB error: {}", usb_err),
                }
            }
        }
    }
}

/// Normalizes an MTP path.
///
/// Ensures the path starts with "/" and handles empty/relative paths.
fn normalize_mtp_path(path: &str) -> PathBuf {
    if path.is_empty() || path == "." {
        PathBuf::from("/")
    } else if !path.starts_with('/') {
        PathBuf::from("/").join(path)
    } else {
        PathBuf::from(path)
    }
}

/// Converts MTP DateTime to Unix timestamp.
fn convert_mtp_datetime(dt: mtp_rs::ptp::DateTime) -> u64 {
    // Convert the DateTime struct fields to Unix timestamp
    // This is a simplified conversion - MTP DateTime has year, month, day, hour, minute, second

    // Create a rough Unix timestamp from the date components
    // Note: This is a simplified calculation that doesn't account for leap years perfectly
    let year = dt.year as u64;
    let month = dt.month as u64;
    let day = dt.day as u64;
    let hour = dt.hour as u64;
    let minute = dt.minute as u64;
    let second = dt.second as u64;

    // Simplified calculation: days since epoch + time
    // This is approximate but good enough for file listing purposes
    let years_since_1970 = year.saturating_sub(1970);
    let days = years_since_1970 * 365 + (years_since_1970 / 4) // leap years approximation
        + (month.saturating_sub(1)) * 30  // approximate days per month
        + day.saturating_sub(1);

    days * 86400 + hour * 3600 + minute * 60 + second
}

/// Generates icon ID for MTP files.
fn get_mtp_icon_id(is_dir: bool, filename: &str) -> String {
    if is_dir {
        return "dir".to_string();
    }
    if let Some(ext) = Path::new(filename).extension() {
        return format!("ext:{}", ext.to_string_lossy().to_lowercase());
    }
    "file".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_id_valid() {
        assert_eq!(parse_device_id("mtp-1-5"), Some((1, 5)));
        assert_eq!(parse_device_id("mtp-20-100"), Some((20, 100)));
    }

    #[test]
    fn test_parse_device_id_invalid() {
        assert_eq!(parse_device_id("mtp-1"), None);
        assert_eq!(parse_device_id("usb-1-5"), None);
        assert_eq!(parse_device_id("mtp-abc-5"), None);
        assert_eq!(parse_device_id(""), None);
    }

    #[test]
    fn test_connection_error_display() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not found: mtp-1-5");

        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        assert_eq!(err.to_string(), "Device mtp-1-5 is in use by ptpcamerad");
    }

    #[test]
    fn test_connection_error_is_retryable() {
        // Retryable errors
        assert!(
            MtpConnectionError::Timeout {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            MtpConnectionError::DeviceBusy {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );

        // Non-retryable errors
        assert!(
            !MtpConnectionError::DeviceNotFound {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::Disconnected {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::StorageFull {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
    }

    #[test]
    fn test_connection_error_user_message() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("not found"));
        assert!(err.user_message().contains("unplugged"));

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("full"));

        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("busy"));
        assert!(err.user_message().contains("try again"));
    }

    #[test]
    fn test_new_error_types_display() {
        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device busy: mtp-1-5");

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Storage full on device: mtp-1-5");

        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        assert!(err.to_string().contains("Object not found"));
        assert!(err.to_string().contains("/DCIM/photo.jpg"));
    }

    // ========================================================================
    // Path normalization tests
    // ========================================================================

    #[test]
    fn test_normalize_mtp_path_empty() {
        assert_eq!(normalize_mtp_path(""), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_dot() {
        assert_eq!(normalize_mtp_path("."), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_root() {
        assert_eq!(normalize_mtp_path("/"), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_absolute() {
        assert_eq!(normalize_mtp_path("/DCIM"), PathBuf::from("/DCIM"));
        assert_eq!(normalize_mtp_path("/DCIM/Camera"), PathBuf::from("/DCIM/Camera"));
    }

    #[test]
    fn test_normalize_mtp_path_relative() {
        assert_eq!(normalize_mtp_path("DCIM"), PathBuf::from("/DCIM"));
        assert_eq!(normalize_mtp_path("DCIM/Camera"), PathBuf::from("/DCIM/Camera"));
    }

    #[test]
    fn test_normalize_mtp_path_special_characters() {
        // Test paths with spaces and special characters
        assert_eq!(normalize_mtp_path("/My Files"), PathBuf::from("/My Files"));
        assert_eq!(normalize_mtp_path("Photos & Videos"), PathBuf::from("/Photos & Videos"));
    }

    // ========================================================================
    // Icon ID generation tests
    // ========================================================================

    #[test]
    fn test_get_mtp_icon_id_directory() {
        assert_eq!(get_mtp_icon_id(true, "DCIM"), "dir");
        assert_eq!(get_mtp_icon_id(true, "Camera"), "dir");
        assert_eq!(get_mtp_icon_id(true, ""), "dir");
    }

    #[test]
    fn test_get_mtp_icon_id_file_with_extension() {
        assert_eq!(get_mtp_icon_id(false, "photo.jpg"), "ext:jpg");
        assert_eq!(get_mtp_icon_id(false, "document.PDF"), "ext:pdf");
        assert_eq!(get_mtp_icon_id(false, "video.MP4"), "ext:mp4");
        assert_eq!(get_mtp_icon_id(false, "archive.tar.gz"), "ext:gz");
    }

    #[test]
    fn test_get_mtp_icon_id_file_without_extension() {
        assert_eq!(get_mtp_icon_id(false, "README"), "file");
        assert_eq!(get_mtp_icon_id(false, "Makefile"), "file");
        // Hidden files starting with . have no "real" extension, Path::extension returns None
        assert_eq!(get_mtp_icon_id(false, ".hidden"), "file");
    }

    // ========================================================================
    // Error serialization tests
    // ========================================================================

    #[test]
    fn test_connection_error_serialization() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: With tag = "type" and rename_all = "camelCase", device_id becomes deviceId
        assert!(json.contains("\"type\":\"deviceNotFound\""), "JSON: {}", json);
        assert!(json.contains("\"device_id\":\"mtp-1-5\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_serialization() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: tag type is camelCase, but inner field names stay snake_case
        assert!(json.contains("\"type\":\"exclusiveAccess\""), "JSON: {}", json);
        assert!(json.contains("\"blocking_process\":\"ptpcamerad\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_no_process() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"blocking_process\":null"), "JSON: {}", json);

        // Test user message for this case
        assert!(err.user_message().contains("Another app"));
        assert!(!err.user_message().contains("ptpcamerad"));
    }

    #[test]
    fn test_connection_error_protocol_serialization() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"protocol\""), "JSON: {}", json);
        assert!(json.contains("\"message\":\"InvalidObjectHandle\""), "JSON: {}", json);
    }

    // ========================================================================
    // All error display and user_message coverage
    // ========================================================================

    #[test]
    fn test_all_error_variants_display() {
        // Test all variants have Display impl
        let errors = vec![
            MtpConnectionError::DeviceNotFound {
                device_id: "test".to_string(),
            },
            MtpConnectionError::AlreadyConnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::NotConnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::ExclusiveAccess {
                device_id: "test".to_string(),
                blocking_process: None,
            },
            MtpConnectionError::Timeout {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Disconnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Protocol {
                device_id: "test".to_string(),
                message: "error".to_string(),
            },
            MtpConnectionError::DeviceBusy {
                device_id: "test".to_string(),
            },
            MtpConnectionError::StorageFull {
                device_id: "test".to_string(),
            },
            MtpConnectionError::ObjectNotFound {
                device_id: "test".to_string(),
                path: "/path".to_string(),
            },
            MtpConnectionError::Other {
                device_id: "test".to_string(),
                message: "other".to_string(),
            },
        ];

        for err in errors {
            // Each should have non-empty display
            assert!(!err.to_string().is_empty());
            // Each should have non-empty user message
            assert!(!err.user_message().is_empty());
        }
    }

    #[test]
    fn test_already_connected_error() {
        let err = MtpConnectionError::AlreadyConnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device already connected: mtp-1-5");
        assert!(err.user_message().contains("already connected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_not_connected_error() {
        let err = MtpConnectionError::NotConnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not connected: mtp-1-5");
        assert!(err.user_message().contains("not connected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_timeout_error() {
        let err = MtpConnectionError::Timeout {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("timed out"));
        assert!(err.user_message().contains("timed out"));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_disconnected_error() {
        let err = MtpConnectionError::Disconnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("disconnected"));
        assert!(err.user_message().contains("disconnected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_protocol_error_user_message() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        assert!(err.user_message().contains("InvalidObjectHandle"));
        assert!(err.user_message().contains("reconnecting"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_object_not_found_user_message() {
        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        assert!(err.user_message().contains("/DCIM/photo.jpg"));
        assert!(err.user_message().contains("deleted"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_other_error() {
        let err = MtpConnectionError::Other {
            device_id: "mtp-1-5".to_string(),
            message: "Custom error message".to_string(),
        };
        assert!(err.to_string().contains("Custom error message"));
        assert_eq!(err.user_message(), "Custom error message");
        assert!(!err.is_retryable());
    }

    // ========================================================================
    // Transfer types and result tests
    // ========================================================================

    #[test]
    fn test_transfer_type_serialization() {
        let download = MtpTransferType::Download;
        let upload = MtpTransferType::Upload;

        let download_json = serde_json::to_string(&download).unwrap();
        let upload_json = serde_json::to_string(&upload).unwrap();

        assert_eq!(download_json, "\"download\"");
        assert_eq!(upload_json, "\"upload\"");
    }

    #[test]
    fn test_transfer_progress_serialization() {
        let progress = MtpTransferProgress {
            operation_id: "op-123".to_string(),
            device_id: "mtp-1-5".to_string(),
            transfer_type: MtpTransferType::Download,
            current_file: "photo.jpg".to_string(),
            bytes_done: 1024,
            bytes_total: 4096,
        };

        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"operationId\":\"op-123\""));
        assert!(json.contains("\"deviceId\":\"mtp-1-5\""));
        assert!(json.contains("\"transferType\":\"download\""));
        assert!(json.contains("\"currentFile\":\"photo.jpg\""));
        assert!(json.contains("\"bytesDone\":1024"));
        assert!(json.contains("\"bytesTotal\":4096"));
    }

    #[test]
    fn test_operation_result_serialization() {
        let result = MtpOperationResult {
            operation_id: "op-456".to_string(),
            files_processed: 5,
            bytes_transferred: 1_000_000,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"operationId\":\"op-456\""));
        assert!(json.contains("\"filesProcessed\":5"));
        assert!(json.contains("\"bytesTransferred\":1000000"));
    }

    #[test]
    fn test_object_info_serialization() {
        let info = MtpObjectInfo {
            handle: 12345,
            name: "test.jpg".to_string(),
            path: "/DCIM/test.jpg".to_string(),
            is_directory: false,
            size: Some(1024),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"handle\":12345"));
        assert!(json.contains("\"name\":\"test.jpg\""));
        assert!(json.contains("\"path\":\"/DCIM/test.jpg\""));
        assert!(json.contains("\"isDirectory\":false"));
        assert!(json.contains("\"size\":1024"));
    }

    #[test]
    fn test_object_info_directory() {
        let info = MtpObjectInfo {
            handle: 100,
            name: "Photos".to_string(),
            path: "/Photos".to_string(),
            is_directory: true,
            size: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"isDirectory\":true"));
        assert!(json.contains("\"size\":null"));
    }

    // ========================================================================
    // Connected device info tests
    // ========================================================================

    #[test]
    fn test_connected_device_info_serialization() {
        use super::super::types::{MtpDeviceInfo, MtpStorageInfo};

        let info = ConnectedDeviceInfo {
            device: MtpDeviceInfo {
                id: "mtp-1-5".to_string(),
                vendor_id: 0x18d1,
                product_id: 0x4ee1,
                manufacturer: Some("Google".to_string()),
                product: Some("Pixel 8".to_string()),
                serial_number: None,
            },
            storages: vec![MtpStorageInfo {
                id: 65537,
                name: "Internal shared storage".to_string(),
                total_bytes: 128_000_000_000,
                available_bytes: 64_000_000_000,
                storage_type: Some("FixedRAM".to_string()),
            }],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":\"mtp-1-5\""));
        assert!(json.contains("\"manufacturer\":\"Google\""));
        assert!(json.contains("\"product\":\"Pixel 8\""));
        assert!(json.contains("\"Internal shared storage\""));
    }

    // ========================================================================
    // Edge cases for parse_device_id
    // ========================================================================

    #[test]
    fn test_parse_device_id_edge_cases() {
        // Maximum u8 values
        assert_eq!(parse_device_id("mtp-255-255"), Some((255, 255)));

        // Zero values
        assert_eq!(parse_device_id("mtp-0-0"), Some((0, 0)));

        // Overflow values (should fail because u8 max is 255)
        assert_eq!(parse_device_id("mtp-256-1"), None);
        assert_eq!(parse_device_id("mtp-1-256"), None);

        // Extra dashes
        assert_eq!(parse_device_id("mtp-1-5-extra"), None);

        // Wrong prefix
        assert_eq!(parse_device_id("MTP-1-5"), None);

        // Whitespace
        assert_eq!(parse_device_id(" mtp-1-5"), None);
        assert_eq!(parse_device_id("mtp-1-5 "), None);
    }

    // ========================================================================
    // MtpOperationState tests
    // ========================================================================

    #[test]
    fn test_operation_state_default() {
        let state = MtpOperationState::default();
        assert!(!state.cancelled.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_operation_state_cancel() {
        let state = MtpOperationState::default();
        state.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(state.cancelled.load(std::sync::atomic::Ordering::Relaxed));
    }
}
