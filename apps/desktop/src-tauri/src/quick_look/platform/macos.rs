//! macOS: the shared `QLPreviewPanel`, driven on the AppKit main thread through
//! `QuickLookController`, each hop under `blocking_with_timeout` (2 s) so a wedged AppKit pump
//! never freezes the IPC pool.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::ignore_poison::IgnorePoison;
use crate::quick_look::QuickLookController;

/// The controller, behind the mutex `tauri::State` holds.
pub(crate) type QuickLookState = Mutex<QuickLookController>;

/// The empty controller.
pub(crate) fn init_state() -> QuickLookState {
    Mutex::new(QuickLookController::new())
}

/// Opens the panel on `path`, `false` for a volume macOS can't preview.
pub(crate) async fn open(app: AppHandle, path: String, volume_id: String) -> Result<bool, String> {
    use cmdr_fs::deadline::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping open: volume {volume_id} doesn't support local fs access (path={path})",
            path = cmdr_fs::log_private::LogPrivate::path(&path)
        );
        return Ok(false);
    }

    let app_inner = app.clone();
    let path_inner = path;
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        let path_main = std::path::PathBuf::from(path_inner);
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<QuickLookState>();
                let opened = state.lock_ignore_poison().open_on_main(&app_for_closure, path_main);
                let _ = tx.send(opened);
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())
    })
    .await
}

/// Re-targets the open panel.
pub(crate) async fn set_path(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    use cmdr_fs::deadline::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping set_path: volume {volume_id} doesn't support local fs access (path={path})",
            path = cmdr_fs::log_private::LogPrivate::path(&path)
        );
        return Ok(());
    }

    let app_inner = app.clone();
    let path_inner = path;
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        let path_main = std::path::PathBuf::from(path_inner);
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<QuickLookState>();
                state.lock_ignore_poison().set_path_on_main(path_main);
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

/// Closes the panel.
pub(crate) async fn close(app: AppHandle) -> Result<(), String> {
    use cmdr_fs::deadline::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    let app_inner = app.clone();
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<QuickLookState>();
                QuickLookController::close_on_main(&state);
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

/// Helper: returns true if the named volume's paths are OS-visible (local
/// POSIX, OS-mounted shares, direct SMB while its share is mounted). False for
/// MTP and other protocol-only volumes, and for a direct SMB share whose mount
/// has gone away — those have no NSURL the Quick Look panel can preview.
///
/// ❌ Not `supports_local_fs_access()`: that asks whether CMDR reads through
/// `std::fs`, and direct SMB answers `false` while its `/Volumes/…` paths are
/// exactly the NSURL the panel needs. Keying on it made Quick Look a silent
/// no-op on a direct-SMB pane.
fn volume_supports_local_fs(volume_id: &str) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    match manager.get(volume_id) {
        Some(volume) => volume.paths_are_os_visible(),
        None => {
            // Unknown volume id — assume yes so we don't accidentally silence
            // working previews. The frontend always sends a real id for entries
            // it just listed.
            log::debug!(target: "quick_look", "volume {volume_id} not found; assuming local fs access");
            true
        }
    }
}
