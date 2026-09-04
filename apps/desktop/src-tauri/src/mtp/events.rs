//! The MTP events the frontend subscribes to, and the adapter that produces
//! them.
//!
//! Every `tauri_specta::Event` payload MTP has lives here: the five the session
//! layer reports through `connection::events::MtpDeviceEvents`, plus the two
//! `ptpcamerad` ones the hotplug watcher emits itself. The session layer speaks
//! typed values; this is the one place that turns them into wire events, which
//! is what keeps `specta` derives and English words out of the code that talks
//! to the device.
//!
//! ❗ A struct name kebab-cases to its wire event name, so renaming one here
//! silently renames the event the frontend listens for. `ipc.rs` registers all
//! seven in `collect_events!`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use super::connection::MtpDisconnectReason;
use super::connection::events::{MtpDeviceEvent, MtpDeviceEvents};
use super::types::MtpStorageInfo;

/// Emitted when an MTP device connects, or when a late-arriving storage is
/// registered on an already-connected device (in which case `device_name` is
/// empty and `storages` carries only the new storage).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceConnected {
    pub device_id: String,
    pub device_name: String,
    pub storages: Vec<MtpStorageInfo>,
}

/// Emitted when an MTP device disconnects (user toggle or USB removal).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MtpDeviceDisconnected {
    pub device_id: String,
    pub reason: MtpDisconnectReason,
}

/// Emitted when a storage area is removed from a connected device.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MtpStorageRemoved {
    pub device_id: String,
    pub storage_id: u32,
}

/// Emitted when opening a device fails because another process holds exclusive
/// access (typically `ptpcamerad` on macOS). The frontend shows the workaround
/// dialog with `blocking_process` (the claiming process name, from `ioreg`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MtpExclusiveAccessError {
    pub device_id: String,
    pub blocking_process: Option<String>,
}

/// Emitted when opening a device fails for lack of USB permission (Linux:
/// missing udev rules). The frontend shows a copyable udev install command.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MtpPermissionError {
    pub device_id: String,
}

/// Emitted (macOS) when Cmdr suppresses `ptpcamerad` to claim a device.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct MtpPtpcameradSuppressed;

/// Emitted (macOS) when Cmdr restores `ptpcamerad` (MTP disabled or no devices
/// remain connected).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct MtpPtpcameradRestored;

/// Turns the session layer's typed device events into the frontend's.
///
/// One match, in one place. An emit failure is logged by Tauri and dropped here:
/// a device's lifecycle can't be held up because a window went away mid-event.
pub struct TauriMtpDeviceEvents {
    app: AppHandle,
}

impl TauriMtpDeviceEvents {
    /// An events sink that emits into `app`.
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl MtpDeviceEvents for TauriMtpDeviceEvents {
    fn device_event(&self, event: MtpDeviceEvent) {
        let emitted = match event {
            MtpDeviceEvent::Connected {
                device_id,
                device_name,
                storages,
            } => MtpDeviceConnected {
                device_id,
                device_name,
                storages,
            }
            .emit(&self.app),
            MtpDeviceEvent::Disconnected { device_id, reason } => {
                MtpDeviceDisconnected { device_id, reason }.emit(&self.app)
            }
            MtpDeviceEvent::StorageRemoved { device_id, storage_id } => {
                MtpStorageRemoved { device_id, storage_id }.emit(&self.app)
            }
            MtpDeviceEvent::ExclusiveAccess {
                device_id,
                blocking_process,
            } => MtpExclusiveAccessError {
                device_id,
                blocking_process,
            }
            .emit(&self.app),
            MtpDeviceEvent::PermissionDenied { device_id } => MtpPermissionError { device_id }.emit(&self.app),
        };
        if let Err(e) = emitted {
            log::debug!(target: "mtp", "couldn't emit an MTP device event: {e}");
        }
    }
}

/// Where MTP background work reports, given whatever the app has wired so far.
///
/// The hotplug watcher stores the app handle at startup; before that (and in
/// every test binary) there is no window to emit into, so this answers with the
/// detached sink rather than an `Option` every caller has to unwrap.
pub(crate) fn device_events() -> Arc<dyn MtpDeviceEvents> {
    match super::watcher::app_handle() {
        Some(app) => Arc::new(TauriMtpDeviceEvents::new(app)),
        None => super::connection::events::no_device_events(),
    }
}

/// Where an IPC command reports: it already holds the handle the call came in
/// on, so it never has to wait for the watcher's.
pub(crate) fn device_events_for(app: &AppHandle) -> Arc<dyn MtpDeviceEvents> {
    Arc::new(TauriMtpDeviceEvents::new(app.clone()))
}
