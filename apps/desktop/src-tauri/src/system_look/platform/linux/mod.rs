//! Linux: the accent color from the XDG desktop portal (`accent_color.rs`), live. The desktop has
//! no Display options, Liquid Glass slider, or Text Size the app reads, so those answer their
//! defaults; the webview still honors `prefers-contrast: more` where it reflects it.

mod accent_color;

use tauri::AppHandle;

use crate::system_events::DisplayAccessibility;

pub(crate) use accent_color::accent_color;

/// Every option off.
pub(crate) async fn display_accessibility(_app: AppHandle) -> DisplayAccessibility {
    DisplayAccessibility::default()
}

/// No slider: the frontend uses its middle-of-the-slider default.
pub(crate) async fn glass_tint_amount() -> Option<f32> {
    None
}

/// No system scaling; the in-app text-size slider still works.
pub(crate) fn text_size_multiplier() -> f32 {
    1.0
}

/// The portal's accent-color signal.
pub(crate) fn observe_changes(app: AppHandle) {
    accent_color::observe_accent_color_changes(app);
}
