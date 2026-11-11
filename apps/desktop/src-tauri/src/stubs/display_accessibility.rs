//! Accessibility > Display options stub for non-macOS platforms.
//!
//! There's no cross-platform equivalent we read today, so every option reports
//! off (the safe default). The frontend still
//! honors the `prefers-contrast: more` media query where the webview reflects it.

use crate::system_events::DisplayAccessibility;

/// Returns every option off on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub fn get_display_accessibility() -> DisplayAccessibility {
    DisplayAccessibility::default()
}
