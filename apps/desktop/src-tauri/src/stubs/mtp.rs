//! MTP stubs for Linux/non-macOS platforms.
//!
//! MTP support is currently macOS-only. This stub allows the app to compile
//! and run on Linux for E2E testing.

use serde::{Deserialize, Serialize};

/// Information about a connected MTP device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceInfo {
    pub id: String,
    pub vendor_id: u16,
    pub product_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
}

/// Information about a storage area on an MTP device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpStorageInfo {
    pub id: u32,
    pub name: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
}

/// Information about a connected device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDeviceInfo {
    pub device: MtpDeviceInfo,
    pub storages: Vec<MtpStorageInfo>,
}

/// Error types for MTP connection operations (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MtpConnectionError {
    NotSupported { message: String },
}

impl std::fmt::Display for MtpConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MtpConnectionError {}

/// Lists connected MTP devices (stub - always returns empty).
#[tauri::command]
pub fn list_mtp_devices() -> Vec<MtpDeviceInfo> {
    // MTP is not supported on non-macOS platforms yet
    Vec::new()
}

/// Connects to an MTP device (stub - returns error).
#[tauri::command]
pub async fn connect_mtp_device(_device_id: String) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Disconnects from an MTP device (stub - returns error).
#[tauri::command]
pub async fn disconnect_mtp_device(_device_id: String) -> Result<(), MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Gets information about a connected MTP device (stub - returns None).
#[tauri::command]
pub fn get_mtp_device_info(_device_id: String) -> Option<ConnectedDeviceInfo> {
    None
}

/// Gets the ptpcamerad workaround command (stub - returns empty string).
#[tauri::command]
pub fn get_ptpcamerad_workaround_command() -> String {
    String::new()
}

/// Gets storage information for a connected device (stub - returns empty).
#[tauri::command]
pub fn get_mtp_storages(_device_id: String) -> Vec<MtpStorageInfo> {
    Vec::new()
}
