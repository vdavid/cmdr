//! Windows: no reader yet (`UISettings` for the accent color and text scale, high contrast and
//! transparency through `SystemParametersInfoW`, `WM_SETTINGCHANGE` for changes), so every answer
//! is the default the other platforms fall back to.

use tauri::AppHandle;

use crate::system_events::DisplayAccessibility;

/// Brand fallback accent (mustard gold from getcmdr.com).
const FALLBACK_ACCENT_HEX: &str = "#d4a006";

/// The brand fallback.
pub(crate) async fn accent_color(_app: AppHandle) -> String {
    FALLBACK_ACCENT_HEX.to_owned()
}

/// Every option off.
pub(crate) async fn display_accessibility(_app: AppHandle) -> DisplayAccessibility {
    DisplayAccessibility::default()
}

/// No slider.
pub(crate) async fn glass_tint_amount() -> Option<f32> {
    None
}

/// No system scaling.
pub(crate) fn text_size_multiplier() -> f32 {
    1.0
}

/// Nothing to observe yet.
pub(crate) fn observe_changes(_app: AppHandle) {}
