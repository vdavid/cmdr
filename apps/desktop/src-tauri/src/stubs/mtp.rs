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

/// Lists connected MTP devices (stub - always returns empty).
#[tauri::command]
pub fn list_mtp_devices() -> Vec<MtpDeviceInfo> {
    // MTP is not supported on non-macOS platforms yet
    Vec::new()
}
