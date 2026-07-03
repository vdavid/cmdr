//! Window z-ordering commands, used only by E2E test runs to keep a test's
//! windows out of the developer's way (order them to the back without focusing).
//! On macOS these hop to the AppKit main thread; off macOS / outside E2E they
//! are no-ops.

use tauri::{AppHandle, Runtime, Window};
// `get_webview_window` / `run_on_main_thread` come from `Manager`, used only on
// the macOS ordering path; off macOS the commands are no-ops.
#[cfg(target_os = "macos")]
use tauri::Manager;

/// macOS: send the given NSWindow to the back of the window list without focusing
/// it. `orderBack:` still makes the window visible (just behind everything), so
/// the webview keeps rendering and the E2E tests can drive it over the socket.
///
/// MUST run on the AppKit main thread (AppKit window ordering is main-thread-only).
/// Callers hop via `run_on_main_thread`; the marker check below enforces it.
#[cfg(target_os = "macos")]
fn order_ns_window_back(ns_window: *mut objc2::runtime::AnyObject) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    if MainThreadMarker::new().is_none() {
        return Err("order_ns_window_back must run on the AppKit main thread".into());
    }
    if ns_window.is_null() {
        return Err("NSWindow pointer is null".into());
    }
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for this webview (null-checked
    // above), and we are on the AppKit main thread (`MainThreadMarker` checked above), as `-orderBack:`
    // requires. It takes an `id` sender; we pass nil and it returns void, so there's no ownership to
    // manage. E2E-only window plumbing (gated by `is_e2e_mode`), never on a user path.
    unsafe {
        let _: () = msg_send![ns_window, orderBack: std::ptr::null_mut::<AnyObject>()];
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn show_main_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    // E2E: on macOS, order the window to the back without focusing it instead of
    // `window.show()` (which calls `makeKeyAndOrderFront:`, always grabbing OS
    // focus AND raising the window to the front). This keeps a test run's windows
    // out of the developer's way. Linux/Windows test runs happen in headless
    // containers, so the standard show is fine there.
    #[cfg(target_os = "macos")]
    if crate::test_mode::is_e2e_mode() {
        use objc2::runtime::AnyObject;
        // AppKit window ordering must run on the main thread; the raw NSWindow pointer
        // isn't `Send`, so obtain it INSIDE the main-thread closure rather than capturing it.
        let window_for_main = window.clone();
        window
            .run_on_main_thread(move || match window_for_main.ns_window() {
                Ok(ptr) => {
                    if let Err(e) = order_ns_window_back(ptr as *mut AnyObject) {
                        log::warn!(target: "ui", "show_main_window: order_ns_window_back failed: {e}");
                    }
                }
                Err(e) => log::warn!(target: "ui", "show_main_window: ns_window() failed: {e}"),
            })
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    window.show().map_err(|e| e.to_string())
}

/// E2E-only: order a freshly created child window (Settings, file viewer,
/// shortcuts) to the back without focusing it, so a test run's windows don't pop
/// in front of the developer's work. Invoked by the opener (the main window) right
/// after the child window is created, resolving it by label. No-op off macOS or
/// outside E2E; pairs with the `Prohibited` activation policy (`crate::run`) and
/// the child windows' own `focus: false`. See `crate::test_mode::is_e2e_mode`.
#[tauri::command]
#[specta::specta]
pub fn order_window_to_back<R: Runtime>(app: AppHandle<R>, label: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if crate::test_mode::is_e2e_mode() {
        use objc2::runtime::AnyObject;
        // AppKit window ordering must run on the main thread; the raw NSWindow pointer
        // isn't `Send`, so resolve the window and obtain it INSIDE the main-thread closure.
        let app_for_main = app.clone();
        app.run_on_main_thread(move || {
            let Some(window) = app_for_main.get_webview_window(&label) else {
                log::warn!(target: "ui", "order_window_to_back: no window with label {label}");
                return;
            };
            match window.ns_window() {
                Ok(ptr) => {
                    if let Err(e) = order_ns_window_back(ptr as *mut AnyObject) {
                        log::warn!(target: "ui", "order_window_to_back: order_ns_window_back failed: {e}");
                    }
                }
                Err(e) => log::warn!(target: "ui", "order_window_to_back: ns_window() failed: {e}"),
            }
        })
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, label);
    Ok(())
}
