//! What the app adds around `cmdr-mtp`.
//!
//! The protocol, the session layer, and the `Volume` over one storage live in
//! the crate; this module is the app half of the backend, plus the door every
//! call site walks through (the crate's names are re-exported here, so a caller
//! writes `crate::mtp::…` whichever side the item is on).
//!
//! - `watcher`: the USB hotplug task, the `MTP_ENABLED` gate, auto-connect, and
//!   the ptpcamerad calls. ADB's tracker twin, and app-side for the same reason:
//!   it owns a policy, not a protocol.
//! - `events`: the `tauri_specta` payloads the frontend subscribes to, and the
//!   adapter that maps the crate's typed device events onto them.
//! - `volume_wiring`: the registrar that turns an attached storage into a
//!   registered `MtpVolume`, and the `DeviceVolumeProvider`.
//! - `macos_workaround`: suppressing and restoring `ptpcamerad` (macOS only).
//!
//! MTP works on macOS and Linux; `stubs::mtp` serves the commands elsewhere.

pub mod events;
#[cfg(target_os = "macos")]
pub mod macos_workaround;
/// What the app-side MTP suites reach a virtual device through: the crate's
/// fixtures, over the manager THIS app parked.
#[cfg(all(test, feature = "virtual-mtp"))]
pub(crate) mod test_support;
pub mod volume_wiring;
/// The registrar this module hands the session layer, asserted through the app's
/// own volume registry.
#[cfg(all(test, feature = "virtual-mtp"))]
mod volume_wiring_test;
pub mod watcher;

#[cfg(feature = "virtual-mtp")]
pub use cmdr_mtp::virtual_device;
pub use cmdr_mtp::{
    ConnectedDeviceInfo, DeviceWatch, MtpConnectionError, MtpConnectionManager, MtpDeleteScope, MtpDeviceInfo,
    MtpDisconnectReason, MtpObjectInfo, MtpStorageInfo, MtpVolumeRegistrar, list_mtp_devices,
};
pub use events::{
    MtpDeviceConnected, MtpDeviceDisconnected, MtpExclusiveAccessError, MtpPermissionError, MtpPtpcameradRestored,
    MtpPtpcameradSuppressed, MtpStorageRemoved,
};
pub use watcher::{set_mtp_enabled, set_mtp_enabled_flag, start_mtp_watcher};

/// The manager [`install_connection_manager`] built.
static CONNECTION_MANAGER: std::sync::OnceLock<std::sync::Arc<MtpConnectionManager>> = std::sync::OnceLock::new();

/// Builds a manager over this app: the real volume host, the real registrar, and
/// wherever device lifecycle should be reported.
fn build_connection_manager(
    events: std::sync::Arc<dyn cmdr_mtp::MtpDeviceEvents>,
) -> std::sync::Arc<MtpConnectionManager> {
    MtpConnectionManager::new(crate::volume_host::host(), events, volume_wiring::volume_registrar())
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
pub fn connection_manager() -> &'static std::sync::Arc<MtpConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| build_connection_manager(cmdr_mtp::no_device_events()))
}

/// The Terminal command that users can run to work around ptpcamerad on macOS.
/// Returns an empty string on non-macOS platforms (ptpcamerad doesn't exist there).
#[cfg(target_os = "macos")]
pub use macos_workaround::PTPCAMERAD_WORKAROUND_COMMAND;

#[cfg(not(target_os = "macos"))]
pub const PTPCAMERAD_WORKAROUND_COMMAND: &str = "";
