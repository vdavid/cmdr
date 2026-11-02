//! IPC for the reveal feature: the Settings row's read/write pair, and the frontend's
//! "I'm listening now" drain.
//!
//! The two registration commands read through to the OS on every call. ❌ Never cache the
//! answer in `settings.json`: the `NSFileViewer` key is machine state anyone can change
//! from outside Cmdr, and a stored flag would show the user a switch that disagrees with
//! their Mac. `DETAILS.md` § "Not a stored setting".

use std::time::Duration;

use cmdr_fs::deadline::blocking_with_timeout;
use tauri::AppHandle;

use super::delivery::{PENDING, spawn_delivery};
use super::finder_extension::{FinderExtensionStatus, for_this_copy};
use super::registration::{GlobalDomain, RevealHandlerBlocker, RevealHandlerStatus, RevealRegistration, own_bundle_id};

/// Build the registration state machine over the real global domain.
fn registration() -> RevealRegistration<GlobalDomain> {
    RevealRegistration::new(GlobalDomain, own_bundle_id(), install_blocker())
}

/// Whether this copy of Cmdr is one we're willing to write into a machine-wide key.
///
/// Nothing runs at uninstall, so a copy in `~/Downloads`, on a mounted disk image, or in
/// Gatekeeper's translocated shadow would take the key and keep it after being deleted.
/// `crate::install_location` holds the rule, shared with the Dock pin.
fn install_blocker() -> Option<RevealHandlerBlocker> {
    (!crate::install_location::running_copy_is_installed()).then_some(RevealHandlerBlocker::NotInApplications)
}

/// Whether reveals from other apps currently land in Cmdr, who holds the key if not, and
/// whether this copy may take it.
#[tauri::command]
#[specta::specta]
pub async fn get_reveal_handler_state() -> RevealHandlerStatus {
    registration().status()
}

/// Take the `NSFileViewer` key, or give it up. Returns the state the OS is left in, so
/// the Settings row renders the truth rather than what it asked for.
#[tauri::command]
#[specta::specta]
pub async fn set_reveal_handler_enabled(enabled: bool) -> RevealHandlerStatus {
    let status = registration().set_enabled(enabled);
    log::info!(target: "reveal::registration", "Reveal handler set to enabled={enabled}: {status:?}");
    status
}

/// How long a `pluginkit` round may take. It's a quick local query, but it's a subprocess.
const FINDER_EXTENSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether the Finder extension is on, read through to macOS, and whether this copy may switch it.
#[tauri::command]
#[specta::specta]
pub async fn get_finder_extension_status() -> Option<FinderExtensionStatus> {
    blocking_with_timeout(FINDER_EXTENSION_TIMEOUT, None, || Some(for_this_copy().status())).await
}

/// Switch the Finder extension. Returns the state macOS is left in, so the row renders the truth.
#[tauri::command]
#[specta::specta]
pub async fn set_finder_extension_enabled(enabled: bool) -> Option<FinderExtensionStatus> {
    let status = blocking_with_timeout(FINDER_EXTENSION_TIMEOUT, None, move || {
        Some(for_this_copy().set_enabled(enabled))
    })
    .await;
    log::info!(target: "reveal", "Finder extension set to enabled={enabled}: {status:?}");
    status
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
    let requests = PENDING.drain();
    if requests.is_empty() {
        return;
    }
    log::info!(target: "reveal", "Frontend is up; delivering {} parked request(s)", requests.len());
    spawn_delivery(&app, requests);
}
