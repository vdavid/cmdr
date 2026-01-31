//! MTP device discovery.
//!
//! Lists connected MTP devices without opening sessions.
//! Used to populate the volume picker with available Android devices.

use super::types::MtpDeviceInfo;
use log::debug;
use mtp_rs::MtpDevice;

/// Lists all connected MTP devices.
///
/// This function enumerates USB devices and filters for MTP-capable ones.
/// It does not open connections to the devices, so it's fast and non-blocking.
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
                    let id = format!("mtp-{}-{}", d.bus, d.address);
                    debug!(
                        "MTP device: id={}, vendor={:04x}, product={:04x}",
                        id, d.vendor_id, d.product_id
                    );
                    MtpDeviceInfo {
                        id,
                        vendor_id: d.vendor_id,
                        product_id: d.product_id,
                        // mtp-rs doesn't expose string descriptors in list_devices() yet
                        // We could get them by opening the device, but that's slower
                        manufacturer: None,
                        product: None,
                        serial_number: None,
                    }
                })
                .collect()
        }
        Err(e) => {
            // Log the error but return empty list (graceful degradation)
            log::warn!("Failed to enumerate MTP devices: {}", e);
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
        // Test that our ID format is consistent
        let id = format!("mtp-{}-{}", 1, 5);
        assert_eq!(id, "mtp-1-5");
    }
}
