//! App-handle registration and `smb-connection-changed` event plumbing.
//!
//! Holds the global `AppHandle` set once from `lib.rs::setup` so SMB state
//! transitions can emit `smb-connection-changed` events to the frontend.

use super::*;

/// Global `AppHandle` for emitting `smb-connection-changed` events. Set once
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

pub(super) fn emit_state_change(volume_id: &str, state: &'static str) {
    use tauri_specta::Event;
    if let Some(app) = get_app_handle()
        && let Err(e) = (crate::network::SmbConnectionChanged {
            volume_id: volume_id.to_string(),
            state: state.to_string(),
        })
        .emit(&app)
    {
        warn!("Failed to emit smb-connection-changed: {}", e);
    }
}
