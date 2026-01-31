//! MTP connection management.
//!
//! Manages device connections with a global registry. Each connected device
//! maintains an active MTP session until disconnected or unplugged.

use log::{debug, error, info, warn};
use mtp_rs::{MtpDevice, MtpDeviceBuilder, ObjectHandle, StorageId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

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
    /// Other connection error.
    Other { device_id: String, message: String },
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
            Self::Other { device_id, message } => {
                write!(f, "Error for {device_id}: {message}")
            }
        }
    }
}

impl std::error::Error for MtpConnectionError {}

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
    device: Arc<tokio::sync::Mutex<MtpDevice>>,
    /// Device metadata.
    info: MtpDeviceInfo,
    /// Cached storage information.
    storages: Vec<MtpStorageInfo>,
    /// Path-to-handle cache per storage.
    path_cache: RwLock<HashMap<u32, PathHandleCache>>,
}

/// Cache for mapping paths to MTP object handles.
#[derive(Default)]
struct PathHandleCache {
    /// Maps virtual path -> MTP object handle.
    path_to_handle: HashMap<PathBuf, ObjectHandle>,
}

/// Global connection manager for MTP devices.
pub struct MtpConnectionManager {
    /// Map of device_id -> connected device entry.
    devices: Mutex<HashMap<String, DeviceEntry>>,
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
            let devices = self.devices.lock().unwrap();
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

        // Find and open the device
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

        debug!("Connected to: {} {}", mtp_info.manufacturer, mtp_info.model);

        // Get storage information
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
            let mut devices = self.devices.lock().unwrap();
            devices.insert(
                device_id.to_string(),
                DeviceEntry {
                    device: Arc::new(tokio::sync::Mutex::new(device)),
                    info: device_info,
                    storages,
                    path_cache: RwLock::new(HashMap::new()),
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
            let mut devices = self.devices.lock().unwrap();
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
    pub fn get_device_info(&self, device_id: &str) -> Option<ConnectedDeviceInfo> {
        let devices = self.devices.lock().unwrap();
        devices.get(device_id).map(|entry| ConnectedDeviceInfo {
            device: entry.info.clone(),
            storages: entry.storages.clone(),
        })
    }

    /// Checks if a device is connected.
    #[allow(dead_code, reason = "Will be used in Phase 3+ for file browsing")]
    pub fn is_connected(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices.contains_key(device_id)
    }

    /// Returns a list of all connected device IDs.
    #[allow(dead_code, reason = "Will be used in Phase 5 for multi-device management")]
    pub fn connected_device_ids(&self) -> Vec<String> {
        let devices = self.devices.lock().unwrap();
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

        // Get the device and resolve path to handle
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().unwrap();
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to parent handle
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, path)?;

            (Arc::clone(&entry.device), parent_handle)
        };

        // Normalize the path for building child paths
        let parent_path = normalize_mtp_path(path);

        // List directory contents (async operation)
        let device = device_arc.lock().await;

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

        // Use list_objects which returns Vec<ObjectInfo> directly
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let object_infos =
            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.list_objects(parent_opt))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;

        debug!("MTP list_directory: found {} objects", object_infos.len());

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
            let devices = self.devices.lock().unwrap();
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

        debug!("MTP list_directory: returning {} entries", entries.len());
        Ok(entries)
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
    pub fn handle_device_disconnected(&self, device_id: &str, app: Option<&AppHandle>) {
        let removed = {
            let mut devices = self.devices.lock().unwrap();
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
    let storage_list = device.storages().await?;
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
        mtp_rs::Error::Protocol { code, operation } => MtpConnectionError::Protocol {
            device_id: device_id.to_string(),
            message: format!("Operation {:?} failed with code {:?}", operation, code),
        },
        _ => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: e.to_string(),
        },
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
}
