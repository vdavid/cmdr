//! MTP stubs for platforms without MTP support.
//!
//! MTP support is available on macOS and Linux. This stub allows the app to
//! compile and run on other platforms.

use serde::{Deserialize, Serialize};

/// Information about a connected MTP device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceInfo {
    pub id: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

/// Information about a storage area on an MTP device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MtpStorageInfo {
    pub id: u32,
    pub name: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub storage_type: Option<String>,
    /// Whether this storage is read-only (for example, PTP cameras).
    pub is_read_only: bool,
}

/// Information about a connected device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDeviceInfo {
    pub device: MtpDeviceInfo,
    pub storages: Vec<MtpStorageInfo>,
}

/// Error types for MTP connection operations (stub version).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "type")]
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

/// Enables or disables MTP support (stub - no-op).
#[tauri::command]
#[specta::specta]
pub async fn set_mtp_enabled(_enabled: bool) {}

/// Connects to an MTP device (stub - returns error).
#[tauri::command]
#[specta::specta]
pub async fn connect_mtp_device(_device_id: String) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Gets the ptpcamerad workaround command (stub - returns empty string).
#[tauri::command]
#[specta::specta]
pub fn get_ptpcamerad_workaround_command() -> String {
    String::new()
}
