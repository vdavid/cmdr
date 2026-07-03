//! Quick Look commands (native `QLPreviewPanel` on macOS, no-op stubs elsewhere).
//!
//! Three commands rather than one because the panel is a process-wide singleton
//! owned by AppKit — we can `open` it, re-target it via `set_path`, or `close`
//! it, but we don't get to construct fresh instances. The frontend tracks
//! `isOpen` and picks the right call. See `crate::quick_look` for the full
//! design (and the why-singleton, why-main-thread, why-events arguments).
//!
//! All three commands wrap their main-thread hop in `blocking_with_timeout` (2 s)
//! so a wedged AppKit pump never freezes the IPC blocking pool.

use tauri::AppHandle;
// `run_on_main_thread` / `state` come from `Manager`, used only on macOS where
// the real panel lives; the other platforms have no-op stubs.
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Open (or re-open) Quick Look on the given path.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_open(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping open: volume {volume_id} doesn't support local fs access (path={path})"
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
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.open_on_main(&app_for_closure, path_main);
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_set_path(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    if !volume_supports_local_fs(&volume_id) {
        log::debug!(
            target: "quick_look",
            "skipping set_path: volume {volume_id} doesn't support local fs access (path={path})"
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
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.set_path_on_main(path_main);
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn quick_look_close(app: AppHandle) -> Result<(), String> {
    use crate::commands::util::blocking_with_timeout;
    use std::sync::mpsc::channel;
    use tokio::time::Duration;

    let app_inner = app.clone();
    blocking_with_timeout(Duration::from_secs(2), Err("timed out".to_string()), move || {
        let (tx, rx) = channel();
        let app_for_closure = app_inner.clone();
        app_inner
            .run_on_main_thread(move || {
                let state = app_for_closure.state::<crate::quick_look::QuickLookState>();
                if let Ok(mut ctrl) = state.lock() {
                    ctrl.close_on_main();
                }
                let _ = tx.send(());
            })
            .map_err(|e| format!("run_on_main_thread failed: {e}"))?;
        rx.recv().map_err(|_| "main-thread reply lost".to_string())?;
        Ok::<(), String>(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_open(_app: AppHandle, _path: String, _volume_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_set_path(_app: AppHandle, _path: String, _volume_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub async fn quick_look_close(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// Helper: returns true if the named volume supports `std::fs`-style access
/// (local POSIX, OS-mounted SMB). False for MTP and other protocol-only
/// volumes — those have no NSURL the Quick Look panel can preview.
#[cfg(target_os = "macos")]
fn volume_supports_local_fs(volume_id: &str) -> bool {
    let manager = crate::file_system::get_volume_manager();
    match manager.get(volume_id) {
        Some(volume) => volume.supports_local_fs_access(),
        None => {
            // Unknown volume id — assume yes so we don't accidentally silence
            // working previews. The frontend always sends a real id for entries
            // it just listed.
            log::debug!(target: "quick_look", "volume {volume_id} not found; assuming local fs access");
            true
        }
    }
}
