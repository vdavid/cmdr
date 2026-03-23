//! MTP type definitions for frontend communication.
//!
//! These types are serialized to JSON for Tauri commands.

use serde::{Deserialize, Serialize};

/// Information about a connected MTP device.
///
/// This represents a device detected via USB, before opening an MTP session.
/// Used by the frontend to display available devices in the volume picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceInfo {
    /// Format: "mtp-{location_id}".
    pub id: String,
    /// Stable for a given USB port.
    pub location_id: u64,
    /// For example, 0x18d1 for Google.
    pub vendor_id: u16,
    pub product_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
}

/// Information about a storage area on an MTP device.
///
/// Android devices typically have one or more storages: "Internal Storage", "SD Card", etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpStorageInfo {
    /// MTP storage handle.
    pub id: u32,
    /// For example, "Internal shared storage".
    pub name: String,
    /// In bytes.
    pub total_bytes: u64,
    /// In bytes.
    pub available_bytes: u64,
    /// For example, "FixedROM", "RemovableRAM".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
    pub is_read_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_serialization() {
        let device = MtpDeviceInfo {
            id: "mtp-336592896".to_string(),
            location_id: 336592896,
            vendor_id: 0x18d1,
            product_id: 0x4ee1,
            manufacturer: Some("Google".to_string()),
            product: Some("Pixel".to_string()),
            serial_number: None,
        };
        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("\"vendorId\":"));
        assert!(json.contains("\"productId\":"));
        assert!(json.contains("\"locationId\":"));
        // serialNumber should be omitted when None
        assert!(!json.contains("serialNumber"));
    }

    #[test]
    fn test_storage_serialization() {
        let storage = MtpStorageInfo {
            id: 0x10001,
            name: "Internal Storage".to_string(),
            total_bytes: 128_000_000_000,
            available_bytes: 64_000_000_000,
            storage_type: Some("FixedRAM".to_string()),
            is_read_only: false,
        };
        let json = serde_json::to_string(&storage).unwrap();
        assert!(json.contains("\"totalBytes\":128000000000"));
        assert!(json.contains("\"availableBytes\":64000000000"));
        assert!(json.contains("\"isReadOnly\":false"));
    }

    #[test]
    fn test_storage_read_only_serialization() {
        let storage = MtpStorageInfo {
            id: 0x10001,
            name: "Camera Storage".to_string(),
            total_bytes: 32_000_000_000,
            available_bytes: 16_000_000_000,
            storage_type: Some("FixedRAM".to_string()),
            is_read_only: true,
        };
        let json = serde_json::to_string(&storage).unwrap();
        assert!(json.contains("\"isReadOnly\":true"));
    }
}
