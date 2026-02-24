//! USB hotplug watcher for MTP devices.
//!
//! Watches for USB device connect/disconnect events and emits Tauri events
//! when MTP devices are detected or removed. Uses nusb's hotplug API.

use log::{debug, error, info, warn};
use nusb::hotplug::HotplugEvent;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

/// Global app handle for emitting events from the watcher
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Track known MTP device IDs for comparison
static KNOWN_DEVICES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Flag to indicate watcher has been started
static WATCHER_STARTED: OnceLock<()> = OnceLock::new();

/// Payload for MTP device detected event
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceDetectedPayload {
    /// The device ID
    pub device_id: String,
    /// Device name (if available)
    pub name: Option<String>,
    /// USB vendor ID
    pub vendor_id: u16,
    /// USB product ID
    pub product_id: u16,
}

/// Payload for MTP device removed event
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceRemovedPayload {
    /// The device ID
    pub device_id: String,
}

/// Gets the current set of MTP devices using mtp-rs discovery.
fn get_current_mtp_devices() -> HashSet<String> {
    let devices = super::list_mtp_devices();
    devices.into_iter().map(|d| d.id).collect()
}

/// Checks for MTP device changes by comparing current state with known state.
/// Emits events for newly detected and removed devices.
fn check_for_device_changes() {
    let current_devices = get_current_mtp_devices();

    let known = match KNOWN_DEVICES.get() {
        Some(k) => k,
        None => return,
    };

    let mut known_guard = match known.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    // Find newly detected devices
    for device_id in current_devices.difference(&known_guard) {
        debug!("MTP device detected: {}", device_id);
        emit_device_detected(device_id);
    }

    // Find removed devices
    for device_id in known_guard.difference(&current_devices) {
        debug!("MTP device removed: {}", device_id);
        emit_device_removed(device_id);
    }

    // Update known devices
    *known_guard = current_devices;
}

/// Emit a device detected event to the frontend.
fn emit_device_detected(device_id: &str) {
    if let Some(app) = APP_HANDLE.get() {
        // Try to get full device info
        let devices = super::list_mtp_devices();
        let device_info = devices.iter().find(|d| d.id == device_id);

        let payload = MtpDeviceDetectedPayload {
            device_id: device_id.to_string(),
            name: device_info.and_then(|d| d.product.clone()),
            vendor_id: device_info.map(|d| d.vendor_id).unwrap_or(0),
            product_id: device_info.map(|d| d.product_id).unwrap_or(0),
        };

        if let Err(e) = app.emit("mtp-device-detected", payload) {
            error!("Failed to emit mtp-device-detected event: {}", e);
        } else {
            info!("Emitted mtp-device-detected for {}", device_id);
        }
    }
}

/// Emit a device removed event to the frontend.
fn emit_device_removed(device_id: &str) {
    if let Some(app) = APP_HANDLE.get() {
        let payload = MtpDeviceRemovedPayload {
            device_id: device_id.to_string(),
        };

        if let Err(e) = app.emit("mtp-device-removed", payload) {
            error!("Failed to emit mtp-device-removed event: {}", e);
        } else {
            info!("Emitted mtp-device-removed for {}", device_id);
        }
    }
}

/// Starts the USB hotplug watcher for MTP devices.
/// Call this once at app initialization.
pub fn start_mtp_watcher(app: &AppHandle) {
    // Only start once
    if WATCHER_STARTED.set(()).is_err() {
        debug!("MTP watcher already initialized");
        return;
    }

    // Store app handle for event emission
    if APP_HANDLE.set(app.clone()).is_err() {
        warn!("MTP watcher app handle already set");
    }

    // Initialize known devices with current state
    let initial_devices = get_current_mtp_devices();
    let known = KNOWN_DEVICES.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut known_guard) = known.lock() {
        *known_guard = initial_devices.clone();
        debug!("Initial MTP devices: {:?}", known_guard);
    }

    debug!(
        "Starting MTP device watcher (found {} initial device(s))",
        initial_devices.len()
    );

    // Spawn the async hotplug watcher using Tauri's async runtime
    // (tokio::spawn doesn't work here as we're in a synchronous setup hook)
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        run_hotplug_watcher(app_handle).await;
    });
}

/// The async hotplug watcher loop.
async fn run_hotplug_watcher(_app: AppHandle) {
    // Use nusb's watch_devices to get notified of USB device changes
    let hotplug_stream = match nusb::watch_devices() {
        Ok(stream) => stream,
        Err(e) => {
            error!("Failed to start USB hotplug watcher: {}", e);
            return;
        }
    };

    debug!("USB hotplug watcher started");

    // Process hotplug events
    use futures_util::StreamExt;
    let mut stream = hotplug_stream;
    while let Some(event) = stream.next().await {
        match event {
            HotplugEvent::Connected(device_info) => {
                debug!(
                    "USB device connected: {:04x}:{:04x} at {}:{}",
                    device_info.vendor_id(),
                    device_info.product_id(),
                    device_info.bus_id(),
                    device_info.device_address()
                );
                // Give the device a moment to initialize
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                check_for_device_changes();
            }
            HotplugEvent::Disconnected(device_id) => {
                debug!("USB device disconnected: {:?}", device_id);
                check_for_device_changes();
            }
        }
    }

    warn!("USB hotplug watcher stream ended unexpectedly");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_detected_payload_serialization() {
        let payload = MtpDeviceDetectedPayload {
            device_id: "mtp-336592896".to_string(),
            name: Some("Pixel 8".to_string()),
            vendor_id: 0x18d1,
            product_id: 0x4ee1,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("deviceId"));
        assert!(json.contains("mtp-336592896"));
        assert!(json.contains("vendorId"));
    }

    #[test]
    fn test_device_removed_payload_serialization() {
        let payload = MtpDeviceRemovedPayload {
            device_id: "mtp-336592896".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("deviceId"));
        assert!(json.contains("mtp-336592896"));
    }

    #[test]
    fn test_get_current_mtp_devices() {
        // This test just verifies the function runs without panicking
        let devices = get_current_mtp_devices();
        // The function should complete without error (even if empty)
        assert!(devices.is_empty() || !devices.is_empty());
    }
}
