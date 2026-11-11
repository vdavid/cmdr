//! Linux: the volume watcher, the XDG portal's accent color, and the native title bar.

use tauri::{AppHandle, Manager, TitleBarStyle};

/// Starts every Linux installer.
pub(crate) fn install(app: &AppHandle) {
    crate::volumes_linux::watcher::start_volume_watcher(app);

    // Observe system accent color changes and emit events to frontend.
    crate::accent_color_linux::observe_accent_color_changes(app.clone());

    // titleBarStyle is "Overlay" in JSON for macOS (needed so trafficLightPosition is applied at
    // window creation time; setting it at runtime resets the position). On Linux/GTK, Overlay
    // hides native window controls, so revert to Visible.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title_bar_style(TitleBarStyle::Visible);
    }
}
