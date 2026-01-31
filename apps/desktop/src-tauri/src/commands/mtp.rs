//! Tauri commands for MTP (Android device) operations.

use crate::mtp::{self, MtpDeviceInfo};

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
