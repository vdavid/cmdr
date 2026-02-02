//! Tauri commands for MTP (Android device) operations.

use log::debug;
use std::path::PathBuf;

use crate::file_system::FileEntry;
use crate::mtp::{
    self, ConnectedDeviceInfo, MtpConnectionError, MtpDeviceInfo, MtpObjectInfo, MtpOperationResult, MtpStorageInfo,
};
use tauri::AppHandle;

/// Lists all connected MTP devices.
///
/// This returns devices detected via USB that support MTP protocol.
/// Use this to populate the "Mobile" section in the volume picker.
///
/// # Returns
///
/// A vector of device info structs. Empty if no devices are connected.
#[tauri::command]
pub fn list_mtp_devices() -> Vec<MtpDeviceInfo> {
    mtp::list_mtp_devices()
}

/// Connects to an MTP device by ID.
///
/// Opens an MTP session to the device and retrieves storage information.
/// If another process (like ptpcamerad on macOS) has exclusive access,
/// an `mtp-exclusive-access-error` event is emitted to the frontend.
///
/// # Arguments
///
/// * `device_id` - The device ID from `list_mtp_devices` (format: "mtp-{bus}-{address}")
///
/// # Returns
///
/// Information about the connected device including available storages.
#[tauri::command]
pub async fn connect_mtp_device(app: AppHandle, device_id: String) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
    mtp::connection_manager().connect(&device_id, Some(&app)).await
}

/// Disconnects from an MTP device.
///
/// Closes the MTP session gracefully. The device remains available in
/// `list_mtp_devices` for reconnection.
///
/// # Arguments
///
/// * `device_id` - The device ID to disconnect from
#[tauri::command]
pub async fn disconnect_mtp_device(app: AppHandle, device_id: String) -> Result<(), MtpConnectionError> {
    mtp::connection_manager().disconnect(&device_id, Some(&app)).await
}

/// Gets information about a connected MTP device.
///
/// Returns device metadata and storage information for a currently connected device.
/// Returns `None` if the device is not connected.
///
/// # Arguments
///
/// * `device_id` - The device ID to query
#[tauri::command]
pub async fn get_mtp_device_info(device_id: String) -> Option<ConnectedDeviceInfo> {
    mtp::connection_manager().get_device_info(&device_id).await
}

/// Gets the ptpcamerad workaround command for macOS.
///
/// Returns the Terminal command that users can run to work around
/// ptpcamerad blocking MTP device access.
#[tauri::command]
pub fn get_ptpcamerad_workaround_command() -> String {
    mtp::PTPCAMERAD_WORKAROUND_COMMAND.to_string()
}

/// Gets storage information for all storages on a connected device.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
///
/// # Returns
///
/// A vector of storage info, or empty if device is not connected.
#[tauri::command]
pub async fn get_mtp_storages(device_id: String) -> Vec<MtpStorageInfo> {
    mtp::connection_manager()
        .get_device_info(&device_id)
        .await
        .map(|info| info.storages)
        .unwrap_or_default()
}

/// Lists the contents of a directory on a connected MTP device.
///
/// Returns file entries in the same format as local directory listings,
/// allowing the frontend to use the same file list components.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `path` - Virtual path to list (for example, "/" or "/DCIM")
///
/// # Returns
///
/// A vector of FileEntry objects, sorted with directories first.
#[tauri::command]
pub async fn list_mtp_directory(
    device_id: String,
    storage_id: u32,
    path: String,
) -> Result<Vec<FileEntry>, MtpConnectionError> {
    debug!(
        "list_mtp_directory: ENTERED - device={}, storage={}, path={}",
        device_id, storage_id, path
    );
    let result = mtp::connection_manager()
        .list_directory(&device_id, storage_id, &path)
        .await;
    match &result {
        Ok(entries) => debug!(
            "list_mtp_directory: SUCCESS - {} entries for {}",
            entries.len(),
            path
        ),
        Err(e) => debug!("list_mtp_directory: ERROR - {:?}", e),
    }
    result
}

// ============================================================================
// Phase 4: File Operations
// ============================================================================

/// Downloads a file from an MTP device to the local filesystem.
///
/// Emits `mtp-transfer-progress` events during the transfer.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `object_path` - Virtual path on the device (for example, "/DCIM/photo.jpg")
/// * `local_dest` - Local destination path
/// * `operation_id` - Unique operation ID for progress tracking
#[tauri::command]
pub async fn download_mtp_file(
    app: AppHandle,
    device_id: String,
    storage_id: u32,
    object_path: String,
    local_dest: String,
    operation_id: String,
) -> Result<MtpOperationResult, MtpConnectionError> {
    let local_path = PathBuf::from(&local_dest);
    mtp::connection_manager()
        .download_file(
            &device_id,
            storage_id,
            &object_path,
            &local_path,
            Some(&app),
            &operation_id,
        )
        .await
}

/// Uploads a file from the local filesystem to an MTP device.
///
/// Emits `mtp-transfer-progress` events during the transfer.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `local_path` - Local file path to upload
/// * `dest_folder` - Destination folder path on device (for example, "/DCIM")
/// * `operation_id` - Unique operation ID for progress tracking
#[tauri::command]
pub async fn upload_to_mtp(
    app: AppHandle,
    device_id: String,
    storage_id: u32,
    local_path: String,
    dest_folder: String,
    operation_id: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    let local = PathBuf::from(&local_path);
    mtp::connection_manager()
        .upload_file(&device_id, storage_id, &local, &dest_folder, Some(&app), &operation_id)
        .await
}

/// Deletes an object (file or folder) from an MTP device.
///
/// For folders, this recursively deletes all contents first since MTP
/// requires folders to be empty before deletion.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `object_path` - Virtual path on the device
#[tauri::command]
pub async fn delete_mtp_object(
    device_id: String,
    storage_id: u32,
    object_path: String,
) -> Result<(), MtpConnectionError> {
    mtp::connection_manager()
        .delete_object(&device_id, storage_id, &object_path)
        .await
}

/// Creates a new folder on an MTP device.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `parent_path` - Parent folder path (for example, "/DCIM")
/// * `folder_name` - Name of the new folder
#[tauri::command]
pub async fn create_mtp_folder(
    device_id: String,
    storage_id: u32,
    parent_path: String,
    folder_name: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    mtp::connection_manager()
        .create_folder(&device_id, storage_id, &parent_path, &folder_name)
        .await
}

/// Renames an object on an MTP device.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `object_path` - Current path of the object
/// * `new_name` - New name for the object
#[tauri::command]
pub async fn rename_mtp_object(
    device_id: String,
    storage_id: u32,
    object_path: String,
    new_name: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    mtp::connection_manager()
        .rename_object(&device_id, storage_id, &object_path, &new_name)
        .await
}

/// Moves an object to a new parent folder on an MTP device.
///
/// May fail if the device doesn't support MoveObject operation.
///
/// # Arguments
///
/// * `device_id` - The connected device ID
/// * `storage_id` - The storage ID within the device
/// * `object_path` - Current path of the object
/// * `new_parent_path` - New parent folder path
#[tauri::command]
pub async fn move_mtp_object(
    device_id: String,
    storage_id: u32,
    object_path: String,
    new_parent_path: String,
) -> Result<MtpObjectInfo, MtpConnectionError> {
    mtp::connection_manager()
        .move_object(&device_id, storage_id, &object_path, &new_parent_path)
        .await
}
