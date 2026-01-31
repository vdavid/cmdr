//! MTP connection management.
//!
//! Manages device connections with a global registry. Each connected device
//! maintains an active MTP session until disconnected or unplugged.

use log::{debug, error, info, warn};
use mtp_rs::{MtpDevice, MtpDeviceBuilder};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use super::types::{MtpDeviceInfo, MtpStorageInfo};

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
    /// The MTP device handle.
    device: MtpDevice,
    /// Device metadata.
    info: MtpDeviceInfo,
    /// Cached storage information.
    storages: Vec<MtpStorageInfo>,
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

        // Store in registry
        {
            let mut devices = self.devices.lock().unwrap();
            devices.insert(
                device_id.to_string(),
                DeviceEntry {
                    device,
                    info: device_info,
                    storages,
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

        // Close the device gracefully
        if let Err(e) = entry.device.close().await {
            warn!("Error closing MTP device {}: {:?}", device_id, e);
            // Continue anyway - device might have been unplugged
        }

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
