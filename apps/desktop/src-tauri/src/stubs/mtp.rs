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
    /// Whether this storage is read-only (for example, PTP cameras).
    pub is_read_only: bool,
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

/// File entry stub matching the real FileEntry type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
    pub created_at: Option<u64>,
    pub added_at: Option<u64>,
    pub opened_at: Option<u64>,
    pub permissions: u32,
    pub owner: String,
    pub group: String,
    pub icon_id: String,
    pub extended_metadata_loaded: bool,
}

/// Lists MTP directory contents (stub - returns error).
#[tauri::command]
pub async fn list_mtp_directory(
    _device_id: String,
    _storage_id: u32,
    _path: String,
) -> Result<Vec<FileEntry>, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

// ============================================================================
// Phase 4: File Operation stubs
// ============================================================================

/// Result of a successful MTP operation (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpOperationResult {
    pub operation_id: String,
    pub files_processed: usize,
    pub bytes_transferred: u64,
}

/// Information about an object on the device (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpObjectInfo {
    pub handle: u32,
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
}

/// Downloads a file from an MTP device (stub - returns error).
#[tauri::command]
pub async fn download_mtp_file(
    _device_id: String,
    _storage_id: u32,
    _object_path: String,
    _local_dest: String,
    _operation_id: String,
) -> Result<MtpOperationResult, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Uploads a file to an MTP device (stub - returns error).
#[tauri::command]
pub async fn upload_to_mtp(
    _device_id: String,
    _storage_id: u32,
    _local_path: String,
    _dest_folder: String,
    _operation_id: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Deletes an object from an MTP device (stub - returns error).
#[tauri::command]
pub async fn delete_mtp_object(
    _device_id: String,
    _storage_id: u32,
    _object_path: String,
) -> Result<(), MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Creates a folder on an MTP device (stub - returns error).
#[tauri::command]
pub async fn create_mtp_folder(
    _device_id: String,
    _storage_id: u32,
    _parent_path: String,
    _folder_name: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Renames an object on an MTP device (stub - returns error).
#[tauri::command]
pub async fn rename_mtp_object(
    _device_id: String,
    _storage_id: u32,
    _object_path: String,
    _new_name: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

/// Moves an object on an MTP device (stub - returns error).
#[tauri::command]
pub async fn move_mtp_object(
    _device_id: String,
    _storage_id: u32,
    _object_path: String,
    _new_parent_path: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}

// ============================================================================
// Phase 5: Copy/Export Operation stubs
// ============================================================================

/// Result of scanning an MTP path for copy operation (stub version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpScanResult {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
}

/// Scans an MTP path for copy statistics (stub - returns error).
#[tauri::command]
pub async fn scan_mtp_for_copy(
    _device_id: String,
    _storage_id: u32,
    _path: String,
) -> Result<MtpScanResult, MtpConnectionError> {
    Err(MtpConnectionError::NotSupported {
        message: "MTP is not supported on this platform".to_string(),
    })
}
