//! What the app does as its windows and its process come and go.
//!
//! Two handlers, both wired into the Tauri builder from `lib.rs`: `on_window_event`
//! for per-window signals (focus, close, destroy) and `on_run_event` for
//! process-level ones (ready, exit requested, exit). `lib.rs` names them; the
//! decisions live here.
//!
//! The quit path deliberately runs through `quit::request_quit` in BOTH handlers:
//! closing the main window and quitting the process are separate signals that
//! reach the same gate, and either can be held while work is in flight. See
//! `quit/CLAUDE.md`.

use tauri::{AppHandle, Manager, Window, Wry};

use crate::downloads;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::network;
use crate::{ai, file_viewer, mcp, quit, search, window_state};
#[cfg(target_os = "macos")]
use crate::{drag_image_detection, mtp};

/// Stop the three services that outlive a window: the local LLM, the MCP server, and mDNS.
///
/// Reached from three places (main window closed, main window destroyed, process
/// exiting), because none of them implies the others fire: a `CloseRequested` the
/// gate waves through never becomes a `Destroyed` on some platforms, and an
/// `AppHandle::exit` skips the window events entirely. Every one of these calls is
/// idempotent, so overlapping paths cost nothing.
fn stop_background_services() {
    ai::manager::shutdown();
    mcp::stop_mcp_server();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    network::mdns_discovery::stop_discovery();
}

/// Per-window signals: focus re-checks the FDA gate, closing the main window quits
/// the app through the quit gate, and a destroyed viewer window frees its session.
pub fn on_window_event(window: &Window, event: &tauri::WindowEvent) {
    // Main-window focus re-checks the FDA gate so the Downloads
    // watcher starts/stops on transitions. Covers the "user
    // toggled FDA in System Settings, came back to Cmdr" path
    // without polling. Idempotent when nothing changed.
    if let tauri::WindowEvent::Focused(true) = event
        && window.label() == "main"
    {
        if let Err(err) = downloads::refresh_runtime(window.app_handle()) {
            log::warn!(
                target: "downloads::watcher",
                "Focus-driven gate re-check failed: {err}",
            );
        }
        // Re-evaluate the global-shortcut registration too: if FDA
        // flipped between blur and focus, register/unregister to
        // match. Idempotent when nothing changed.
        downloads::refresh_global_go_to_latest_shortcut(window.app_handle());
    }
    // Closing the main window quits the whole app (settings, debug, and
    // viewer windows included) — but only once the quit gate says so.
    // With work in flight the gate holds the exit, asks the user, and
    // runs its own countdown; the window must STAY OPEN for that, or
    // the dialog it's about to show goes with it. Nothing is torn down
    // on this path until the gate has waved the quit through, so a
    // "Keep working" leaves AI, MCP, and mDNS running. See `quit/`.
    if let tauri::WindowEvent::CloseRequested { api, .. } = event
        && window.label() == "main"
    {
        if quit::request_quit(window.app_handle()) == quit::QuitOutcome::Held {
            api.prevent_close();
        } else {
            stop_background_services();
            window.app_handle().exit(0);
        }
    }
    // Clean up app-wide resources only when the main window is destroyed
    if let tauri::WindowEvent::Destroyed = event
        && window.label() == "main"
    {
        stop_background_services();
    }
    // Free a viewer session when its window is destroyed. Closing a viewer
    // via the titlebar X never fires the FE `viewer_close` IPC (that only
    // runs from the in-app close path), so without this the `ViewerSession`
    // (backend, line index, watcher thread) leaked until app quit.
    // `close_session_for_window` is idempotent: if the FE already closed the
    // session via IPC, the lookup is a no-op.
    if let tauri::WindowEvent::Destroyed = event
        && window.label().starts_with("viewer-")
    {
        file_viewer::close_session_for_window(window.label());
    }
}

/// Process-level signals: the first webview being ready, a quit being requested,
/// and the process actually going away.
pub fn on_run_event(app: &AppHandle<Wry>, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::Ready => {
            // Install drag image detection swizzle. Needs a live webview to
            // discover wry's ObjC class, so it runs at Ready (not setup).
            #[cfg(target_os = "macos")]
            drag_image_detection::install(app.clone());
        }
        // ⌘Q, the app menu's Quit, the Dock's Quit, a logout or
        // restart, and every `AppHandle::exit` in the app all land
        // here. With non-instant work in flight the gate holds the
        // exit and takes the decision over (dialog + its own
        // countdown); with nothing running this is a pass-through and
        // the app quits exactly as it always did.
        //
        // A `restart()` carries `RESTART_EXIT_CODE`, for which Tauri
        // ignores `prevent_exit` outright — asking there would show a
        // dialog nobody could answer, so the gate never sees it.
        tauri::RunEvent::ExitRequested { ref api, code, .. } => {
            if code != Some(tauri::RESTART_EXIT_CODE) && quit::request_quit(app) == quit::QuitOutcome::Held {
                api.prevent_exit();
            }
        }
        tauri::RunEvent::Exit => {
            // Flush window geometry synchronously: the debounced writer
            // may have a pending change the process won't outlive.
            window_state::save_on_exit(app);

            // Stop any live search walk. Coverage stays honest either way
            // (a directory is marked listed only once its rows are
            // written, so a walk cut off mid-flight claims nothing it
            // didn't read), but a walk reading a disk for a window that
            // no longer exists is work nobody asked for.
            search::cancel_all_live_runs();

            // Restore ptpcamerad before exit so we don't leave the system
            // with the daemon disabled after Cmdr closes
            #[cfg(target_os = "macos")]
            if let Err(e) = mtp::macos_workaround::restore_ptpcamerad() {
                log::warn!("Failed to restore ptpcamerad on exit: {}", e);
            }

            stop_background_services();
        }
        _ => {}
    }
}
