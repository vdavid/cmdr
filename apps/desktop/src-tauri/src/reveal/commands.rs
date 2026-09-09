//! IPC for the reveal feature: the Settings row's read/write pair, and the frontend's
//! "I'm listening now" drain.
//!
//! The two registration commands read through to the OS on every call. ❌ Never cache the
//! answer in `settings.json`: the `NSFileViewer` key is machine state anyone can change
//! from outside Cmdr, and a stored flag would show the user a switch that disagrees with
//! their Mac. `DETAILS.md` § "Not a stored setting".

use tauri::AppHandle;

use super::delivery::{PENDING, spawn_delivery};
use super::registration::{GlobalDomain, RevealHandlerState, RevealRegistration, own_bundle_id};

/// Build the registration state machine over the real global domain.
fn registration() -> RevealRegistration<GlobalDomain> {
    RevealRegistration::new(GlobalDomain, own_bundle_id())
}

/// Whether reveals from other apps currently land in Cmdr, and who holds the key if not.
#[tauri::command]
#[specta::specta]
pub async fn get_reveal_handler_state() -> RevealHandlerState {
    registration().state()
}

/// Take the `NSFileViewer` key, or give it up. Returns the state the OS is left in, so
/// the Settings row renders the truth rather than what it asked for.
#[tauri::command]
#[specta::specta]
pub async fn set_reveal_handler_enabled(enabled: bool) -> RevealHandlerState {
    let state = registration().set_enabled(enabled);
    log::info!(target: "reveal::registration", "Reveal handler set to enabled={enabled}: {state:?}");
    state
}

/// Deliver any reveal that arrived before this window could act on it.
///
/// Called once per main-window lifetime, after the `mcp-*` listeners are up. It also
/// arms the direct path: from here on a reveal is delivered as it arrives instead of
/// being parked.
///
/// Returns immediately; the pane move runs on the async runtime, because the frontend
/// that has to answer it is the same frontend that would be waiting on this call.
#[tauri::command]
#[specta::specta]
pub async fn drain_pending_reveals(app: AppHandle) {
    let paths = PENDING.drain();
    if paths.is_empty() {
        return;
    }
    log::info!(target: "reveal", "Frontend is up; delivering {} parked path(s)", paths.len());
    spawn_delivery(&app, paths);
}
