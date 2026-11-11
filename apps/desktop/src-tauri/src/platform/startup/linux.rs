//! Linux: the volume watcher, the XDG portal's accent color, and the native title bar.

use tauri::{AppHandle, Manager, TitleBarStyle};

/// Starts every Linux installer.
pub(crate) fn install(app: &AppHandle) {
    crate::volumes::start_volume_watcher(app);

    // Follow the portal's accent color and emit events to the frontend.
    crate::system_look::observe_changes(app.clone());

    // titleBarStyle is "Overlay" in JSON for macOS (needed so trafficLightPosition is applied at
    // window creation time; setting it at runtime resets the position). On Linux/GTK, Overlay
    // hides native window controls, so revert to Visible.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title_bar_style(TitleBarStyle::Visible);
    }
}
