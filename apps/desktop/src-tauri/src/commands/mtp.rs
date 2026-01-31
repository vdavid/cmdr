//! Tauri commands for MTP (Android device) operations.

use crate::mtp::{self, ConnectedDeviceInfo, MtpConnectionError, MtpDeviceInfo, MtpStorageInfo};
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
pub fn get_mtp_device_info(device_id: String) -> Option<ConnectedDeviceInfo> {
    mtp::connection_manager().get_device_info(&device_id)
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
pub fn get_mtp_storages(device_id: String) -> Vec<MtpStorageInfo> {
    mtp::connection_manager()
        .get_device_info(&device_id)
        .map(|info| info.storages)
        .unwrap_or_default()
}
