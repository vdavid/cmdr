//! MTP (Media Transfer Protocol) support for Android devices.
//!
//! This module provides device discovery and file operations for Android devices
//! connected via USB in "File transfer / Android Auto" mode.
//!
//! # Architecture
//!
//! - `types`: Type definitions for frontend communication
//! - `discovery`: Device detection using mtp-rs
//! - `connection`: the session layer (`MtpConnectionManager`), and where this module parks the app's one instance
//! - `events`: the `tauri_specta` event payloads, and the adapter that turns the session layer's typed device events into them
//! - `volume_wiring`: Registers an attached storage as an `MtpVolume` (the session layer never does)
//! - `macos_workaround`: Handles ptpcamerad interference on macOS (macOS only)
//!
//! # Platform Support
//!
//! MTP support works on macOS and Linux. The underlying crate (`mtp-rs`) is pure
//! Rust and supports both platforms.
//! On macOS, the system daemon `ptpcamerad` may claim devices first;
//! see `macos_workaround` module for handling this.
//! On Linux, USB device permissions may require udev rules.

pub mod connection;
mod discovery;
pub mod events;
#[cfg(target_os = "macos")]
pub mod macos_workaround;
pub mod types;
#[cfg(feature = "virtual-mtp")]
pub mod virtual_device;
pub mod volume_wiring;
pub mod watcher;

pub use connection::{ConnectedDeviceInfo, MtpConnectionError, MtpDisconnectReason, MtpObjectInfo};
pub use discovery::list_mtp_devices;
pub use events::{
    MtpDeviceConnected, MtpDeviceDisconnected, MtpExclusiveAccessError, MtpPermissionError, MtpPtpcameradRestored,
    MtpPtpcameradSuppressed, MtpStorageRemoved,
};
pub use types::{MtpDeviceInfo, MtpStorageInfo};
pub use watcher::{set_mtp_enabled, set_mtp_enabled_flag, start_mtp_watcher};

/// The manager [`install_connection_manager`] built.
static CONNECTION_MANAGER: std::sync::OnceLock<std::sync::Arc<connection::MtpConnectionManager>> =
    std::sync::OnceLock::new();

/// Builds a manager over this app: the real volume host, the real registrar, and
/// wherever device lifecycle should be reported.
fn build_connection_manager(
    events: std::sync::Arc<dyn connection::events::MtpDeviceEvents>,
) -> std::sync::Arc<connection::MtpConnectionManager> {
    connection::MtpConnectionManager::new(crate::volume_host::host(), events, volume_wiring::volume_registrar())
}

/// Parks the app's connection manager, reporting device lifecycle into `app`.
///
/// Call once at startup, before anything can connect a device: the hotplug
/// watcher, the virtual device, and every IPC command reach the manager through
/// [`connection_manager`]. A second call keeps the first manager and is ignored,
/// so a test fixture and the app wiring can both call it without fighting.
pub(crate) fn install_connection_manager(app: &tauri::AppHandle) {
    let events = std::sync::Arc::new(events::TauriMtpDeviceEvents::new(app.clone()));
    if CONNECTION_MANAGER.set(build_connection_manager(events)).is_err() {
        log::debug!("MTP connection manager was already installed; keeping the first one");
    }
}

/// The app's connection manager.
///
/// ❗ This is where the APP parks the one it built, ❌ not how a backend finds a
/// manager: `MtpVolume` carries the manager that attached it, and a test builds
/// its own. Falls back to a detached-events manager (real host, real registrar,
/// nowhere to report) so a test binary that never runs `setup()` still browses a
/// virtual device.
pub fn connection_manager() -> &'static std::sync::Arc<connection::MtpConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| build_connection_manager(connection::events::no_device_events()))
}

/// A manager over this app's real host, reporting into `events` and registering
/// through `registrar`.
///
/// For a test that asserts on what the session layer TOLD its surroundings: the
/// lifecycle sequence a user would have seen, or how a storage got attached.
/// It's a SECOND manager, so hold `virtual_device_test_lock` across its whole
/// connect-use-disconnect span the way every virtual-device test already does.
#[cfg(all(test, feature = "virtual-mtp"))]
pub(crate) fn connection_manager_for_test(
    events: std::sync::Arc<dyn connection::events::MtpDeviceEvents>,
    registrar: connection::MtpVolumeRegistrar,
) -> std::sync::Arc<connection::MtpConnectionManager> {
    connection::MtpConnectionManager::new(crate::volume_host::host(), events, registrar)
}

/// The Terminal command that users can run to work around ptpcamerad on macOS.
/// Returns an empty string on non-macOS platforms (ptpcamerad doesn't exist there).
#[cfg(target_os = "macos")]
pub use macos_workaround::PTPCAMERAD_WORKAROUND_COMMAND;

#[cfg(not(target_os = "macos"))]
pub const PTPCAMERAD_WORKAROUND_COMMAND: &str = "";
