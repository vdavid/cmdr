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
/// It also attempts to read USB string descriptors to get friendly device names.
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

                    // Try to get USB string descriptors using nusb
                    let (manufacturer, product, serial) =
                        get_usb_string_descriptors(d.bus, d.address, d.vendor_id, d.product_id);

                    if let Some(ref prod) = product {
                        debug!("MTP device {} has product name: {}", id, prod);
                    }

                    MtpDeviceInfo {
                        id,
                        vendor_id: d.vendor_id,
                        product_id: d.product_id,
                        manufacturer,
                        product,
                        serial_number: serial,
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

/// Attempts to read USB string descriptors from a device using nusb.
///
/// Returns (manufacturer, product, serial) as Options.
/// Falls back to None for any fields that can't be read.
fn get_usb_string_descriptors(
    bus: u8,
    address: u8,
    vendor_id: u16,
    product_id: u16,
) -> (Option<String>, Option<String>, Option<String>) {
    // Find the device in nusb's device list
    let devices = match nusb::list_devices() {
        Ok(d) => d,
        Err(e) => {
            debug!("Failed to list USB devices via nusb: {}", e);
            return (None, None, None);
        }
    };

    // Find the device matching our bus/address or vendor/product ID
    let device_info = devices
        .into_iter()
        .find(|d| d.bus_number() == bus && d.device_address() == address);

    let Some(device_info) = device_info else {
        // Try matching by vendor/product ID as fallback
        let devices = match nusb::list_devices() {
            Ok(d) => d,
            Err(_) => return (None, None, None),
        };
        let device_info = devices
            .into_iter()
            .find(|d| d.vendor_id() == vendor_id && d.product_id() == product_id);
        if device_info.is_none() {
            debug!(
                "Could not find USB device bus={} addr={} in nusb device list",
                bus, address
            );
            return (None, None, None);
        }
        // Continue with the found device
        let device_info = device_info.unwrap();
        return read_descriptors_from_device(&device_info);
    };

    read_descriptors_from_device(&device_info)
}

/// Reads string descriptors from a nusb DeviceInfo.
fn read_descriptors_from_device(device_info: &nusb::DeviceInfo) -> (Option<String>, Option<String>, Option<String>) {
    // nusb provides manufacturer_string, product_string, serial_number directly on DeviceInfo
    // These are read from the device's string descriptors
    let manufacturer = device_info.manufacturer_string().map(|s| s.to_string());
    let product = device_info.product_string().map(|s| s.to_string());
    let serial = device_info.serial_number().map(|s| s.to_string());

    (manufacturer, product, serial)
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
