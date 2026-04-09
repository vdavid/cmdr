//! USB hotplug watcher for MTP devices.
//!
//! Watches for USB device connect/disconnect events via nusb's hotplug API.
//! On detection, auto-connects devices and emits `mtp-device-connected` /
//! `mtp-device-disconnected` events (via the connection manager). The frontend
//! is a passive consumer — it never orchestrates connections.

use log::{debug, error, info, warn};
use nusb::hotplug::HotplugEvent;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// Global app handle for emitting events from the watcher
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Track known MTP device IDs for comparison
static KNOWN_DEVICES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Flag to indicate watcher has been started
static WATCHER_STARTED: OnceLock<()> = OnceLock::new();

/// Whether MTP support is enabled. When false, the watcher loop still runs
/// but `check_for_device_changes()` returns early and no auto-connects happen.
static MTP_ENABLED: AtomicBool = AtomicBool::new(true);

/// Sets the MTP enabled flag without side effects. Used at startup before the
/// watcher starts, so the initial auto-connect respects the persisted setting.
pub fn set_mtp_enabled_flag(enabled: bool) {
    MTP_ENABLED.store(enabled, Ordering::SeqCst);
    debug!("MTP enabled flag set to {}", enabled);
}

/// Enables or disables MTP support at runtime.
///
/// When disabling: disconnects all connected devices, clears known devices,
/// and restores ptpcamerad (macOS). When enabling: re-checks for plugged-in
/// devices so they get auto-connected.
pub async fn set_mtp_enabled(enabled: bool) {
    let was_enabled = MTP_ENABLED.swap(enabled, Ordering::SeqCst);
    if was_enabled == enabled {
        debug!("MTP enabled unchanged ({})", enabled);
        return;
    }

    info!("MTP support {}", if enabled { "enabled" } else { "disabled" });

    if enabled {
        check_for_device_changes();
    } else {
        // Disconnect all connected devices
        let cm = super::connection_manager();
        let connected = cm.get_all_connected_devices().await;
        for device in &connected {
            let device_id = device.device.id.clone();
            auto_disconnect_device(device_id);
        }

        // Clear known devices so re-enable detects everything as new
        if let Some(known) = KNOWN_DEVICES.get()
            && let Ok(mut guard) = known.lock()
        {
            guard.clear();
        }

        // Restore ptpcamerad on macOS
        #[cfg(target_os = "macos")]
        restore_ptpcamerad_unconditionally();
    }
}

/// Gets the current set of MTP devices using mtp-rs discovery.
fn get_current_mtp_devices() -> HashSet<String> {
    let devices = super::list_mtp_devices();
    devices.into_iter().map(|d| d.id).collect()
}

/// Checks for MTP device changes by comparing current state with known state.
/// Auto-connects newly detected devices and disconnects removed ones.
/// Returns early if MTP is disabled.
fn check_for_device_changes() {
    if !MTP_ENABLED.load(Ordering::SeqCst) {
        return;
    }

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
    if !new_devices.is_empty() {
        #[cfg(target_os = "macos")]
        suppress_ptpcamerad_if_needed();

        for device_id in new_devices {
            info!("MTP device detected, auto-connecting: {}", device_id);
            auto_connect_device(device_id);
        }
    }

    // Disconnect removed devices
    if !removed_devices.is_empty() {
        for device_id in &removed_devices {
            info!("MTP device removed, disconnecting: {}", device_id);
            auto_disconnect_device(device_id.clone());
        }

        #[cfg(target_os = "macos")]
        restore_ptpcamerad_if_no_devices();
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

    // Auto-connect any devices already plugged in at startup (skip if MTP is disabled)
    if !initial_devices.is_empty() && MTP_ENABLED.load(Ordering::SeqCst) {
        #[cfg(target_os = "macos")]
        suppress_ptpcamerad_if_needed();

        for device_id in &initial_devices {
            auto_connect_device(device_id.clone());
        }
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

/// Suppresses ptpcamerad before connecting to MTP devices.
/// Emits `mtp-ptpcamerad-suppressed` event on success so the frontend can show a toast.
#[cfg(target_os = "macos")]
fn suppress_ptpcamerad_if_needed() {
    use tauri::Emitter;

    match super::macos_workaround::suppress_ptpcamerad() {
        Ok(true) => {
            info!("Suppressed ptpcamerad for MTP device access");
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit("mtp-ptpcamerad-suppressed", ());
            }
            // Give the daemon time to die before we try to claim the USB device
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Ok(false) => {} // Already suppressed
        Err(e) => warn!(
            "Failed to suppress ptpcamerad: {} — falling back to manual workaround dialog",
            e
        ),
    }
}

/// Restores ptpcamerad unconditionally (used when MTP is disabled).
/// Emits `mtp-ptpcamerad-restored` event on success.
#[cfg(target_os = "macos")]
fn restore_ptpcamerad_unconditionally() {
    use tauri::Emitter;

    match super::macos_workaround::restore_ptpcamerad() {
        Ok(true) => {
            info!("Restored ptpcamerad (MTP disabled)");
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit("mtp-ptpcamerad-restored", ());
            }
        }
        Ok(false) => {} // Wasn't suppressed
        Err(e) => warn!("Failed to restore ptpcamerad: {}", e),
    }
}

/// Restores ptpcamerad when no MTP devices remain connected.
/// Emits `mtp-ptpcamerad-restored` event on success.
#[cfg(target_os = "macos")]
fn restore_ptpcamerad_if_no_devices() {
    use tauri::Emitter;

    let remaining = get_current_mtp_devices();
    if !remaining.is_empty() {
        return;
    }

    match super::macos_workaround::restore_ptpcamerad() {
        Ok(true) => {
            info!("Restored ptpcamerad (no MTP devices remaining)");
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit("mtp-ptpcamerad-restored", ());
            }
        }
        Ok(false) => {} // Wasn't suppressed
        Err(e) => warn!("Failed to restore ptpcamerad: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_mtp_devices() {
        // This test verifies the function runs without panicking
        let devices = get_current_mtp_devices();
        // The function should complete without error (even if empty)
        assert!(devices.is_empty() || !devices.is_empty());
    }

    #[test]
    fn test_mtp_enabled_flag_defaults_to_true() {
        assert!(MTP_ENABLED.load(Ordering::SeqCst));
    }

    #[test]
    fn test_set_mtp_enabled_flag() {
        let original = MTP_ENABLED.load(Ordering::SeqCst);

        set_mtp_enabled_flag(false);
        assert!(!MTP_ENABLED.load(Ordering::SeqCst));

        set_mtp_enabled_flag(true);
        assert!(MTP_ENABLED.load(Ordering::SeqCst));

        // Restore original state
        MTP_ENABLED.store(original, Ordering::SeqCst);
    }
}
