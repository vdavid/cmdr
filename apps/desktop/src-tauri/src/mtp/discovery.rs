//! MTP device discovery.
//!
//! Lists connected MTP devices without opening sessions.
//! Used to populate the volume picker with available Android devices.

use super::types::MtpDeviceInfo;
use log::{debug, warn};
use mtp_rs::MtpDevice;

/// Lists all connected MTP devices.
///
/// This function enumerates USB devices and filters for MTP-capable ones.
/// Device information including friendly names comes directly from mtp-rs.
///
/// # Returns
///
/// A vector of `MtpDeviceInfo` structs describing available devices.
/// Returns an empty vector if no devices are found or if enumeration fails.
///
/// # Example
///
/// ```ignore
/// let devices = list_mtp_devices();
/// for device in devices {
///     println!("Found: {}", device.display_name());
/// }
/// ```
pub fn list_mtp_devices() -> Vec<MtpDeviceInfo> {
    match MtpDevice::list_devices() {
        Ok(devices) => {
            debug!("Found {} MTP device(s)", devices.len());
            devices
                .into_iter()
                .map(|d| {
                    let id = format!("mtp-{}", d.location_id);
                    debug!(
                        "MTP device: id={}, vendor={:04x}, product={:04x}",
                        id, d.vendor_id, d.product_id
                    );

                    if let Some(ref prod) = d.product {
                        debug!("MTP device {} has product name: {}", id, prod);
                    }

                    MtpDeviceInfo {
                        id,
                        location_id: d.location_id,
                        vendor_id: d.vendor_id,
                        product_id: d.product_id,
                        manufacturer: d.manufacturer,
                        product: d.product,
                        serial_number: d.serial_number,
                    }
                })
                .collect()
        }
        Err(e) => {
            // Log the error but return empty list (graceful degradation)
            warn!("Failed to enumerate MTP devices: {}", e);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_mtp_devices_returns_vec() {
        // This test just verifies the function runs without panicking
        // Actual device testing requires hardware
        let devices = list_mtp_devices();
        // The function should complete without error (even if empty)
        // Using is_empty() to avoid useless comparison warning
        let _ = devices.is_empty(); // Just verify it returns a valid vec
    }

    #[test]
    fn test_device_id_format() {
        // Test that our ID format is consistent (location_id based)
        let location_id: u64 = 336592896;
        let id = format!("mtp-{}", location_id);
        assert_eq!(id, "mtp-336592896");
    }
}
