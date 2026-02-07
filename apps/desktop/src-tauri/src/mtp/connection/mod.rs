//! MTP connection management.
//!
//! Manages device connections with a global registry. Each connected device
//! maintains an active MTP session until disconnected or unplugged.
//!
//! ## File watching
//!
//! MTP devices support event notifications via USB interrupt endpoints. When a
//! device is connected, we start a background task that polls for events using
//! `device.next_event()`. Events like ObjectAdded, ObjectRemoved, and ObjectInfoChanged
//! trigger incremental `directory-diff` events to the frontend, using the same
//! unified diff system as local file watching. This provides smooth UI updates
//! without full directory reloads.

mod bulk_ops;
mod cache;
mod directory_ops;
pub(super) mod errors;
mod event_loop;
mod file_ops;
mod mutation_ops;

use cache::{EVENT_DEBOUNCE_MS, EventDebouncer, ListingCache, PathHandleCache};
pub use errors::MtpConnectionError;
use errors::map_mtp_error;

use log::{debug, error, info, warn};
use mtp_rs::ptp::OperationCode;
use mtp_rs::{MtpDevice, MtpDeviceBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, broadcast};

use super::types::{MtpDeviceInfo, MtpStorageInfo};
use crate::file_system::{MtpVolume, get_volume_manager};

/// Default timeout for MTP operations (30 seconds - some devices are slow).
const MTP_TIMEOUT_SECS: u64 = 30;

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
///
/// Fields are private but accessible from child modules (event_loop, directory_ops, etc.).
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

/// Global connection manager for MTP devices.
///
/// Fields are private but accessible from child modules (event_loop, directory_ops, etc.).
pub struct MtpConnectionManager {
    /// Map of device_id -> connected device entry.
    devices: Mutex<HashMap<String, DeviceEntry>>,
    /// Channels to signal event loop shutdown per device.
    event_loop_shutdown: RwLock<HashMap<String, broadcast::Sender<()>>>,
    /// Debouncer for directory change events.
    event_debouncer: EventDebouncer,
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
            event_loop_shutdown: RwLock::new(HashMap::new()),
            event_debouncer: EventDebouncer::new(Duration::from_millis(EVENT_DEBOUNCE_MS)),
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
        // Check if already connected - if so, return existing connection info (idempotent)
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id) {
                debug!(
                    "connect: {} already connected, returning existing connection info",
                    device_id
                );
                return Ok(ConnectedDeviceInfo {
                    device: entry.info.clone(),
                    storages: entry.storages.clone(),
                });
            }
        }

        info!("Connecting to MTP device: {}", device_id);

        // Parse device_id to get location_id (format: "mtp-{location_id}")
        let location_id = parse_device_id(device_id).ok_or_else(|| MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        })?;
        debug!("Parsed device_id: location_id={}", location_id);

        // Find and open the device
        debug!("Opening MTP device (timeout={}s)...", MTP_TIMEOUT_SECS);
        let device = match open_device(location_id).await {
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
            location_id,
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

        debug!(
            "Device opened successfully: {} {}",
            mtp_info.manufacturer, mtp_info.model
        );

        // Check if device supports write operations (SendObjectInfo is required for uploads)
        // PTP cameras often don't support this, making them effectively read-only
        let device_supports_write = mtp_info.supports_operation(OperationCode::SendObjectInfo);
        info!(
            "Device '{}' write support: {} (operations: {:?})",
            mtp_info.model,
            device_supports_write,
            mtp_info
                .operations_supported
                .iter()
                .filter(|op| matches!(
                    op,
                    OperationCode::SendObjectInfo | OperationCode::SendObject | OperationCode::DeleteObject
                ))
                .collect::<Vec<_>>()
        );

        // Get storage information
        debug!("Fetching storage information...");
        let storages = match get_storages(&device, device_supports_write).await {
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

        // Wrap device in Arc for shared access
        let device_arc = Arc::new(Mutex::new(device));

        // Store in registry
        {
            let mut devices = self.devices.lock().await;
            devices.insert(
                device_id.to_string(),
                DeviceEntry {
                    device: Arc::clone(&device_arc),
                    info: device_info,
                    storages,
                    path_cache: RwLock::new(HashMap::new()),
                    listing_cache: RwLock::new(HashMap::new()),
                },
            );
        }

        // Register MTP volumes for each storage with the global VolumeManager
        // This enables MTP browsing through the standard file listing pipeline
        for storage in &connected_info.storages {
            let volume_id = format!("{}:{}", device_id, storage.id);
            let volume = Arc::new(MtpVolume::new(device_id, storage.id, &storage.name));
            get_volume_manager().register(&volume_id, volume);
            debug!("Registered MTP volume: {} ({})", volume_id, storage.name);
        }

        // Start the event loop for file watching (requires AppHandle)
        if let Some(app) = app {
            self.start_event_loop(device_id.to_string(), device_arc, app.clone());
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

        // Stop the event loop first
        self.stop_event_loop(device_id);

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

        // Unregister MTP volumes from the VolumeManager
        for storage in &entry.storages {
            let volume_id = format!("{}:{}", device_id, storage.id);
            get_volume_manager().unregister(&volume_id);
            debug!("Unregistered MTP volume: {}", volume_id);
        }

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
}

// Remaining impl blocks are in submodules:
// - directory_ops.rs: list_directory, resolve_path_to_handle, handle_device_disconnected
// - event_loop.rs: start_event_loop, stop_event_loop, event handling
// - file_ops.rs: download_file, upload_file, open_download_stream, upload_from_chunks
// - mutation_ops.rs: delete_object, create_folder, rename_object, move_object
// - bulk_ops.rs: scan_for_copy, download_recursive, upload_recursive

/// Global connection manager instance.
static CONNECTION_MANAGER: LazyLock<MtpConnectionManager> = LazyLock::new(MtpConnectionManager::new);

/// Gets the global connection manager.
pub fn connection_manager() -> &'static MtpConnectionManager {
    &CONNECTION_MANAGER
}

/// Parses a device ID to extract location_id.
///
/// Format: "mtp-{location_id}"
fn parse_device_id(device_id: &str) -> Option<u64> {
    let prefix = "mtp-";
    if !device_id.starts_with(prefix) {
        return None;
    }

    device_id[prefix.len()..].parse().ok()
}

/// Opens an MTP device by location_id.
async fn open_device(location_id: u64) -> Result<MtpDevice, mtp_rs::Error> {
    MtpDeviceBuilder::new()
        .timeout(Duration::from_secs(MTP_TIMEOUT_SECS))
        .open_by_location(location_id)
        .await
}

/// Probes whether a storage actually supports writes by attempting to create
/// and delete a hidden test folder.
///
/// Some devices (especially cameras) report ReadWrite capability but actually
/// reject writes at runtime with `StoreReadOnly`. This probe detects such cases early.
///
/// Returns `true` if writes are supported (or probe was inconclusive), `false` only
/// if the device explicitly rejected writes with `StoreReadOnly` or `AccessDenied`.
async fn probe_write_capability(storage: &mtp_rs::Storage, storage_name: &str) -> bool {
    use mtp_rs::ptp::ResponseCode;

    const PROBE_FOLDER_NAME: &str = ".cmdr_write_probe";
    const PROBE_TIMEOUT_SECS: u64 = 3;

    // Try to create a hidden probe folder at the root
    match tokio::time::timeout(
        Duration::from_secs(PROBE_TIMEOUT_SECS),
        storage.create_folder(None, PROBE_FOLDER_NAME),
    )
    .await
    {
        Ok(Ok(handle)) => {
            // Success! Clean up by deleting the probe folder
            debug!("Storage '{}': write probe succeeded, cleaning up", storage_name);
            if let Err(e) = storage.delete(handle).await {
                warn!("Storage '{}': failed to clean up probe folder: {:?}", storage_name, e);
            }
            true
        }
        Ok(Err(e)) => {
            // Check the specific error code to determine if this is a read-only issue
            // or just a restriction on where we can create folders
            let is_read_only_error = match &e {
                mtp_rs::Error::Protocol { code, .. } => {
                    matches!(code, ResponseCode::StoreReadOnly | ResponseCode::AccessDenied)
                }
                _ => false,
            };

            if is_read_only_error {
                debug!(
                    "Storage '{}': write probe failed with read-only error: {:?}",
                    storage_name, e
                );
                false
            } else {
                // Other errors (InvalidObjectHandle, InvalidParentObject, etc.) likely mean
                // we just can't create at root, not that the device is read-only.
                // Android devices often don't allow creating at root but are still writable.
                debug!(
                    "Storage '{}': write probe failed with non-fatal error (assuming writable): {:?}",
                    storage_name, e
                );
                true
            }
        }
        Err(_) => {
            // Timeout - assume writable (benefit of the doubt)
            debug!("Storage '{}': write probe timed out (assuming writable)", storage_name);
            true
        }
    }
}

/// Gets storage information from a connected device.
///
/// # Arguments
/// * `device` - The connected MTP device
/// * `device_supports_write` - Whether the device supports write operations (SendObjectInfo)
async fn get_storages(device: &MtpDevice, device_supports_write: bool) -> Result<Vec<MtpStorageInfo>, mtp_rs::Error> {
    use mtp_rs::ptp::AccessCapability;

    debug!("Calling device.storages()...");
    let storage_list = device.storages().await?;
    debug!("Got {} storage(s)", storage_list.len());
    let mut storages = Vec::new();

    for storage in storage_list {
        let info = storage.info();
        // Check if storage reports read-only capability
        let storage_reports_read_only = !matches!(info.access_capability, AccessCapability::ReadWrite);

        // Determine actual read-only status
        let is_read_only = if !device_supports_write || storage_reports_read_only {
            // Device/storage claims no write support - trust it
            true
        } else {
            // Device claims write support - probe to verify
            // This catches cameras that advertise write support but reject writes at runtime
            let probe_ok = probe_write_capability(&storage, &info.description).await;
            if !probe_ok {
                info!(
                    "Storage '{}' claims write support but probe failed - marking read-only",
                    info.description
                );
            }
            !probe_ok // read-only if probe failed
        };

        // Log final determination
        info!(
            "Storage '{}': access_capability={:?}, device_supports_write={}, is_read_only={}",
            info.description, info.access_capability, device_supports_write, is_read_only
        );

        storages.push(MtpStorageInfo {
            id: storage.id().0,
            name: info.description.clone(),
            total_bytes: info.max_capacity,
            available_bytes: info.free_space_bytes,
            storage_type: Some(format!("{:?}", info.storage_type)),
            is_read_only,
        });
    }

    Ok(storages)
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
        assert_eq!(parse_device_id("mtp-336592896"), Some(336592896));
        assert_eq!(parse_device_id("mtp-12345"), Some(12345));
        assert_eq!(parse_device_id("mtp-0"), Some(0));
    }

    #[test]
    fn test_parse_device_id_invalid() {
        assert_eq!(parse_device_id("usb-336592896"), None);
        assert_eq!(parse_device_id("mtp-abc"), None);
        assert_eq!(parse_device_id("mtp-"), None);
        assert_eq!(parse_device_id("mtp"), None);
        assert_eq!(parse_device_id(""), None);
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
                id: "mtp-336592896".to_string(),
                location_id: 336592896,
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
                is_read_only: false,
            }],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":\"mtp-336592896\""));
        assert!(json.contains("\"locationId\":336592896"));
        assert!(json.contains("\"manufacturer\":\"Google\""));
        assert!(json.contains("\"product\":\"Pixel 8\""));
        assert!(json.contains("\"Internal shared storage\""));
        assert!(json.contains("\"isReadOnly\":false"));
    }

    // ========================================================================
    // Edge cases for parse_device_id
    // ========================================================================

    #[test]
    fn test_parse_device_id_edge_cases() {
        // Maximum u64 value
        assert_eq!(parse_device_id("mtp-18446744073709551615"), Some(u64::MAX));

        // Zero value
        assert_eq!(parse_device_id("mtp-0"), Some(0));

        // Typical macOS location_id values
        assert_eq!(parse_device_id("mtp-336592896"), Some(336592896));

        // Wrong prefix (case sensitive)
        assert_eq!(parse_device_id("MTP-336592896"), None);

        // Whitespace
        assert_eq!(parse_device_id(" mtp-336592896"), None);
        assert_eq!(parse_device_id("mtp-336592896 "), None);

        // Negative numbers (not valid for u64)
        assert_eq!(parse_device_id("mtp--1"), None);
    }
}
