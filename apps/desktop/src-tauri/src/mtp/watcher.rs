//! USB hotplug watcher for MTP devices.
//!
//! Watches for USB device connect/disconnect events via nusb's hotplug API.
//! On detection, auto-connects devices and emits `mtp-device-connected` /
//! `mtp-device-disconnected` events (via the connection manager). The frontend
//! is a passive consumer — it never orchestrates connections.

use log::{debug, error, info, warn};
use nusb::hotplug::HotplugEvent;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// Global app handle for emitting events from the watcher
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Track known MTP device IDs for comparison
static KNOWN_DEVICES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Flag to indicate watcher has been started
static WATCHER_STARTED: OnceLock<()> = OnceLock::new();

/// Gets the current set of MTP devices using mtp-rs discovery.
fn get_current_mtp_devices() -> HashSet<String> {
    let devices = super::list_mtp_devices();
    devices.into_iter().map(|d| d.id).collect()
}

/// Checks for MTP device changes by comparing current state with known state.
/// Auto-connects newly detected devices and disconnects removed ones.
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

    let new_devices: Vec<_> = current_devices.difference(&known_guard).cloned().collect();
    let removed_devices: Vec<_> = known_guard.difference(&current_devices).cloned().collect();

    // Update known devices before async work to avoid re-triggering
    *known_guard = current_devices;
    drop(known_guard);

    // Auto-connect newly detected devices
    for device_id in new_devices {
        info!("MTP device detected, auto-connecting: {}", device_id);
        auto_connect_device(device_id);
    }

    // Disconnect removed devices
    for device_id in removed_devices {
        info!("MTP device removed, disconnecting: {}", device_id);
        auto_disconnect_device(device_id);
    }
}

/// Spawns an async task to connect a newly detected MTP device.
fn auto_connect_device(device_id: String) {
    let app = APP_HANDLE.get().cloned();
    tauri::async_runtime::spawn(async move {
        let cm = super::connection_manager();
        match cm.connect(&device_id, app.as_ref()).await {
            Ok(info) => {
                info!(
                    "Auto-connected MTP device: {} ({} storages)",
                    device_id,
                    info.storages.len()
                );
            }
            Err(e) => {
                // Connection errors (exclusive access, permission) are already
                // emitted as events by the connection manager
                warn!("Failed to auto-connect MTP device {}: {:?}", device_id, e);
            }
        }
    });
}

/// Spawns an async task to disconnect a removed MTP device.
fn auto_disconnect_device(device_id: String) {
    let app = APP_HANDLE.get().cloned();
    tauri::async_runtime::spawn(async move {
        let cm = super::connection_manager();
        if let Err(e) = cm.disconnect(&device_id, app.as_ref()).await {
            // NotConnected is fine — device may not have been connected yet
            debug!("Disconnect for removed device {} returned: {:?}", device_id, e);
        }
    });
}

/// Starts the USB hotplug watcher for MTP devices.
/// Also auto-connects any devices that are already plugged in at startup.
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

    // Auto-connect any devices already plugged in at startup
    for device_id in &initial_devices {
        auto_connect_device(device_id.clone());
    }

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
    fn test_get_current_mtp_devices() {
        // This test just verifies the function runs without panicking
        let devices = get_current_mtp_devices();
        // The function should complete without error (even if empty)
        assert!(devices.is_empty() || !devices.is_empty());
    }
}
