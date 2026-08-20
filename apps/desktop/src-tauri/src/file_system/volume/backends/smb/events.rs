//! App-handle registration and SMB session-state event plumbing.
//!
//! Holds the global `AppHandle` set once from `lib.rs::setup` so SMB state
//! transitions can emit `volume-connection-changed` and
//! `smb-fell-back-to-os-mount` events to the frontend.

use crate::network::VolumeConnection;
use log::warn;
use std::sync::{Mutex as StdMutex, OnceLock};
use tauri::AppHandle;

/// Global `AppHandle` for emitting `volume-connection-changed` events. Set once
/// from `lib.rs::setup`. Same pattern as `network::mdns_discovery::APP_HANDLE`.
static APP_HANDLE: OnceLock<StdMutex<Option<AppHandle>>> = OnceLock::new();

/// Stores the `AppHandle` so SMB state transitions can emit events.
pub fn set_app_handle(handle: AppHandle) {
    let storage = APP_HANDLE.get_or_init(|| StdMutex::new(None));
    if let Ok(mut guard) = storage.lock() {
        *guard = Some(handle);
    }
}

fn get_app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().and_then(|m| m.lock().ok()).and_then(|g| g.clone())
}

pub(super) fn emit_state_change(volume_id: &str, state: VolumeConnection) {
    use tauri_specta::Event;
    if let Some(app) = get_app_handle()
        && let Err(e) = (crate::network::VolumeConnectionChanged {
            volume_id: volume_id.to_string(),
            state,
        })
        .emit(&app)
    {
        warn!("Failed to emit volume-connection-changed: {}", e);
    }
}

/// Tells the frontend a share is staying on the macOS kernel mount, so it can offer
/// a retry instead of leaving someone on the slow path with no explanation.
///
/// `network::os_mount_notice` decides WHETHER to speak (once per server per run);
/// this only carries the message. It lives here rather than beside that decision
/// because this is where the `AppHandle` for SMB events already is.
pub fn emit_fell_back_to_os_mount(volume_id: &str, share: &str) {
    use tauri_specta::Event;
    if let Some(app) = get_app_handle()
        && let Err(e) = (crate::network::SmbFellBackToOsMount {
            volume_id: volume_id.to_string(),
            share: share.to_string(),
        })
        .emit(&app)
    {
        warn!("Failed to emit smb-fell-back-to-os-mount: {}", e);
    }
}
