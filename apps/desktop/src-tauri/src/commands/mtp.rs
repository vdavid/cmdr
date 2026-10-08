//! Tauri commands for MTP (Android device) operations.
//!
//! Browsing and file operations on a phone go through `MtpVolume` like any other
//! volume, and the device list reaches the frontend as volumes; these commands
//! only switch the backend on and off and dial a device.

use crate::mtp::{self, ConnectedDeviceInfo, MtpConnectionError};

/// Enables or disables MTP support at runtime.
///
/// When disabled: disconnects all devices, stops auto-connecting, and restores
/// ptpcamerad (macOS). When enabled: resumes auto-connecting and checks for
/// already-plugged-in devices.
#[tauri::command]
#[specta::specta]
pub async fn set_mtp_enabled(enabled: bool) {
    mtp::set_mtp_enabled(enabled).await;
}

/// Connects to an MTP device by ID.
///
/// Opens an MTP session to the device and retrieves storage information.
/// If another process (like ptpcamerad on macOS) has exclusive access,
/// an `mtp-exclusive-access-error` event is emitted to the frontend.
///
/// # Arguments
///
/// * `device_id` - The device ID (format: "mtp-{bus}-{address}")
///
/// # Returns
///
/// Information about the connected device including available storages.
#[tauri::command]
#[specta::specta]
pub async fn connect_mtp_device(device_id: String) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
    mtp::connection_manager()
        .connect(&device_id, mtp::DeviceWatch::Live)
        .await
}

/// Gets the ptpcamerad workaround command for macOS.
///
/// Returns the Terminal command that users can run to work around
/// ptpcamerad blocking MTP device access.
#[tauri::command]
#[specta::specta]
pub fn get_ptpcamerad_workaround_command() -> String {
    mtp::PTPCAMERAD_WORKAROUND_COMMAND.to_string()
}

/// Forces the virtual MTP device to rescan its backing directories, syncing
/// its in-memory object tree with the actual filesystem. Call after recreating
/// test fixtures to avoid sleeping for the file watcher.
///
/// Only available with `--features virtual-mtp`. Returns (added, removed) counts.
#[cfg(feature = "virtual-mtp")]
#[tauri::command]
#[specta::specta]
pub async fn rescan_virtual_mtp() -> Result<(usize, usize), String> {
    let result =
        mtp::virtual_device::rescan_virtual_device().ok_or_else(|| "Virtual MTP device not found".to_string())?;
    // Clear Cmdr's listing cache so stale entries from before the rescan
    // don't mask the new state (the mtp-rs object tree is updated but Cmdr
    // caches directory listings with a 5s TTL).
    mtp::connection_manager().clear_all_listing_caches().await;
    Ok(result)
}

/// Pauses the virtual device's filesystem watcher. Call before externally
/// manipulating backing dir files to prevent stale events from corrupting state.
#[cfg(feature = "virtual-mtp")]
#[tauri::command]
#[specta::specta]
pub fn pause_virtual_mtp_watcher() -> bool {
    mtp::virtual_device::pause_virtual_watcher()
}

/// Resumes the virtual device's filesystem watcher after a pause.
#[cfg(feature = "virtual-mtp")]
#[tauri::command]
#[specta::specta]
pub fn resume_virtual_mtp_watcher() {
    mtp::virtual_device::resume_virtual_watcher();
}
