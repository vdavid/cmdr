//! `Cmdr > Services`: the macOS Services menu, showing the file services Finder shows.
//!
//! AppKit hides every service that takes files from an app that never says it can
//! hand files over, which is why the submenu used to list four Instruments trace
//! templates and nothing else (they declare no send types at all, so they show up
//! everywhere). Three pieces fix that, and all three are needed:
//!
//! 1. [`install`] tells `NSApplication` that Cmdr can send `NSFilenamesPboardType`
//!    and `public.file-url`.
//! 2. [`responder`] answers for those types from the key window's responder chain
//!    and writes the selection onto the pasteboard when a service asks.
//! 3. `menu::macos_appkit`'s services-menu adoption points the Services item in the
//!    installed menu bar at the `NSMenu` AppKit actually fills.
//!
//! What "the selection" means at any moment lives in [`selection`], pushed by the
//! frontend. Details, the measurement behind registering the legacy type, and the
//! responder-chain walk: `DETAILS.md`.

pub mod responder;
pub mod selection;

pub use selection::{SelectedRows, ServicesSelection};

use tauri::{AppHandle, Manager, Wry};

/// Registers Cmdr's send types and splices the selection responder into the main
/// window's responder chain.
///
/// Call on the AppKit main thread, from `setup` and BEFORE the menu bar is built:
/// Apple's guidance is to register from `applicationDidFinishLaunching:`, which is
/// where Tauri's `setup` runs. The main window already exists by then (Tauri creates
/// it before `setup`, hidden), so its content view is there to attach to.
///
/// Idempotent, so a later call after a webview swap is safe.
pub fn install(app: &AppHandle<Wry>) {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        log::warn!(target: "services_menu", "Not on the main thread; the Services menu keeps its short list");
        return;
    };
    responder::register_send_types(mtm);

    let Some(window) = app.get_webview_window("main") else {
        log::warn!(target: "services_menu", "No main window to attach the selection responder to");
        return;
    };
    let ns_window = match window.ns_window() {
        Ok(ptr) => ptr,
        Err(e) => {
            log::warn!(target: "services_menu", "No NSWindow for the main window: {e}");
            return;
        }
    };
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for the main
    // webview, read on the main thread inside this call, so it can't be torn down
    // while `attach_to_window` holds the reference. Null is answered inside.
    unsafe { responder::attach_to_window(mtm, ns_window) };
}
